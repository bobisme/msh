//! Shared clips can omit geometry. Matching is by unique joint name, with hierarchy
//! and rest-axis checks; translations are transferred as rest-relative deltas so
//! body proportions are preserved.
use super::{
    animation::{self, AnimationClip, AnimationProperty},
    skeleton::Skeleton,
};
use std::collections::HashMap;
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn load(path: &Path, target: &Skeleton) -> Result<Vec<AnimationClip>> {
    let (document, buffers, _) = super::gltf_import::import(path)?;
    let skins: Vec<_> = document.skins().collect();
    if skins.len() != 1 {
        return Err("External clips file must contain exactly one unambiguous skin".into());
    }
    let source = super::loader::extract_skeleton(&skins[0], &document, &buffers)?;
    let mut target_names = HashMap::new();
    for joint in &target.joints {
        let name = joint
            .name
            .as_deref()
            .ok_or("Target skin contains an unnamed joint")?;
        if target_names.insert(name, joint.index).is_some() {
            return Err(format!("Ambiguous target joint name '{name}'").into());
        }
    }
    let mut source_names = HashMap::new();
    let mut mapping = Vec::new();
    for joint in &source.joints {
        let name = joint
            .name
            .as_deref()
            .ok_or("Clips skin contains an unnamed joint")?;
        if source_names.insert(name, joint.index).is_some() {
            return Err(format!("Ambiguous clips joint name '{name}'").into());
        }
        mapping.push(
            *target_names
                .get(name)
                .ok_or_else(|| format!("Clips joint '{name}' is missing from target skin"))?,
        );
    }
    for joint in &source.joints {
        let destination = &target.joints[mapping[joint.index]];
        let expected = joint.parent.and_then(|i| source.joints[i].name.as_deref());
        let mut parent = destination.parent;
        while let Some(index) = parent {
            if target.joints[index]
                .name
                .as_deref()
                .is_some_and(|n| source_names.contains_key(n))
            {
                break;
            }
            parent = target.joints[index].parent;
        }
        let actual = parent.and_then(|i| target.joints[i].name.as_deref());
        if expected != actual {
            return Err(format!("Incompatible hierarchy for joint {:?}", joint.name).into());
        }
        let (_, r, s) = joint.local_transform.decompose();
        let (_, tr, ts) = destination.local_transform.decompose();
        let dot: f32 = r.iter().zip(tr).map(|(a, b)| a * b).sum();
        if (dot.abs() - 1.0).abs() > 0.001 || s.iter().zip(ts).any(|(a, b)| (a - b).abs() > 0.001) {
            return Err(format!(
                "Incompatible rest rotation/scale for joint {:?}",
                joint.name
            )
            .into());
        }
        for column in 0..3 {
            for row in 0..3 {
                if (joint.parent_transform[column][row] - destination.parent_transform[column][row])
                    .abs()
                    > 0.001
                {
                    return Err(
                        format!("Incompatible ancestor basis for joint {:?}", joint.name).into(),
                    );
                }
            }
        }
    }
    let nodes: Vec<_> = source.joints.iter().map(|j| j.node_index).collect();
    let mut clips = animation::extract_animations(&document, &buffers, Some(&nodes));
    if clips.is_empty() {
        return Err("External clips file contains no animations".into());
    }
    for clip in &mut clips {
        if !clip.duration.is_finite() || clip.duration < 0.0 {
            return Err("Invalid clip duration".into());
        }
        for channel in &mut clip.channels {
            let original = channel.joint_index;
            let destination = mapping[original];
            let (source_t, _, source_s) = source.joints[original].local_transform.decompose();
            let (target_t, _, target_s) = target.joints[destination].local_transform.decompose();
            let mut previous = -f32::INFINITY;
            for key in &mut channel.keyframes {
                if !key.time.is_finite()
                    || key.time < 0.0
                    || key.time < previous
                    || !key.value.iter().all(|v| v.is_finite())
                {
                    return Err("Invalid animation keyframe".into());
                }
                previous = key.time;
                match channel.property {
                    AnimationProperty::Translation => {
                        for i in 0..3 {
                            key.value[i] += target_t[i] - source_t[i];
                        }
                    }
                    AnimationProperty::Scale => {
                        for i in 0..3 {
                            if source_s[i].abs() < 1e-6 {
                                return Err("Zero rest scale in clips skeleton".into());
                            }
                            let ratio = target_s[i] / source_s[i];
                            key.value[i] *= ratio;
                            if let Some(t) = &mut key.in_tangent {
                                t[i] *= ratio;
                            }
                            if let Some(t) = &mut key.out_tangent {
                                t[i] *= ratio;
                            }
                        }
                    }
                    AnimationProperty::Rotation => {}
                }
            }
            channel.joint_index = destination;
        }
    }
    Ok(clips)
}

pub fn attach(data: &mut super::loader::MeshWithColors, path: &Path) -> Result<()> {
    let target = data
        .skeleton
        .as_ref()
        .ok_or("Cannot apply clips to a mesh without a skin")?;
    data.animations = load(path, target)?;
    Ok(())
}
