use gltf::animation::util::ReadOutputs;
use nalgebra::{Matrix4, Quaternion, UnitQuaternion, Vector3};

/// Interpolation mode for animation keyframes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    Step,
    Linear,
    CubicSpline,
}

/// Which transform property a channel animates
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationProperty {
    Translation,
    Rotation,
    Scale,
}

/// A single keyframe with optional cubic spline tangents
#[derive(Debug, Clone)]
pub struct Keyframe {
    pub time: f32,
    /// 3 floats for Translation/Scale, 4 for Rotation (quaternion XYZW)
    pub value: Vec<f32>,
    /// In-tangent (CubicSpline only)
    pub in_tangent: Option<Vec<f32>>,
    /// Out-tangent (CubicSpline only)
    pub out_tangent: Option<Vec<f32>>,
}

/// A single animated channel targeting one joint's property
#[derive(Debug, Clone)]
pub struct AnimationChannel {
    /// Index into the skeleton's joint list (NOT the glTF node index)
    pub joint_index: usize,
    pub property: AnimationProperty,
    pub interpolation: Interpolation,
    pub keyframes: Vec<Keyframe>,
}

/// A complete animation clip containing all channels
#[derive(Debug, Clone)]
pub struct AnimationClip {
    pub name: Option<String>,
    pub channels: Vec<AnimationChannel>,
    /// Maximum timestamp across all channels
    pub duration: f32,
}

/// Extract all animation clips from a glTF document.
///
/// `joint_node_indices` is the list of node indices from the skin's joints array,
/// used to map channel target nodes to joint indices. If None, channels that
/// target nodes not in a skin are skipped.
pub fn extract_animations(
    document: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    joint_node_indices: Option<&[usize]>,
) -> Vec<AnimationClip> {
    document
        .animations()
        .map(|anim| extract_one_animation(&anim, buffers, joint_node_indices))
        .collect()
}

fn extract_one_animation(
    anim: &gltf::Animation<'_>,
    buffers: &[gltf::buffer::Data],
    joint_node_indices: Option<&[usize]>,
) -> AnimationClip {
    let name = anim.name().map(String::from);
    let mut duration: f32 = 0.0;
    let mut channels = Vec::new();

    for channel in anim.channels() {
        let target = channel.target();
        let node_index = target.node().index();

        // Map node index to joint index
        let joint_index = match joint_node_indices {
            Some(joints) => match joints.iter().position(|&n| n == node_index) {
                Some(idx) => idx,
                None => continue, // node is not a joint, skip
            },
            None => node_index, // no skin info, use node index as-is
        };

        let property = match target.property() {
            gltf::animation::Property::Translation => AnimationProperty::Translation,
            gltf::animation::Property::Rotation => AnimationProperty::Rotation,
            gltf::animation::Property::Scale => AnimationProperty::Scale,
            gltf::animation::Property::MorphTargetWeights => continue, // not supported
        };

        let sampler = channel.sampler();
        let interpolation = match sampler.interpolation() {
            gltf::animation::Interpolation::Step => Interpolation::Step,
            gltf::animation::Interpolation::Linear => Interpolation::Linear,
            gltf::animation::Interpolation::CubicSpline => Interpolation::CubicSpline,
        };

        let reader = channel.reader(|buf| Some(&buffers[buf.index()]));

        let timestamps: Vec<f32> = match reader.read_inputs() {
            Some(iter) => iter.collect(),
            None => continue,
        };

        // Track max timestamp for duration
        if let Some(&last) = timestamps.last() {
            if last > duration {
                duration = last;
            }
        }

        let outputs = match reader.read_outputs() {
            Some(o) => o,
            None => continue,
        };

        let raw_values = read_output_values(outputs);
        let is_cubic = interpolation == Interpolation::CubicSpline;
        let component_count = match property {
            AnimationProperty::Translation | AnimationProperty::Scale => 3,
            AnimationProperty::Rotation => 4,
        };

        let keyframes = build_keyframes(
            &timestamps,
            &raw_values,
            component_count,
            is_cubic,
        );

        channels.push(AnimationChannel {
            joint_index,
            property,
            interpolation,
            keyframes,
        });
    }

    AnimationClip {
        name,
        channels,
        duration,
    }
}

/// Read output values into a flat Vec<Vec<f32>>, one entry per output element.
fn read_output_values(outputs: ReadOutputs<'_>) -> Vec<Vec<f32>> {
    match outputs {
        ReadOutputs::Translations(iter) => iter.map(|v| v.to_vec()).collect(),
        ReadOutputs::Rotations(iter) => iter.into_f32().map(|v| v.to_vec()).collect(),
        ReadOutputs::Scales(iter) => iter.map(|v| v.to_vec()).collect(),
        ReadOutputs::MorphTargetWeights(_) => Vec::new(), // not supported
    }
}

/// Build keyframes from timestamps and raw output values.
///
/// For CubicSpline, the raw values are packed as triplets:
/// [in_tangent_0, value_0, out_tangent_0, in_tangent_1, value_1, out_tangent_1, ...]
fn build_keyframes(
    timestamps: &[f32],
    raw_values: &[Vec<f32>],
    _component_count: usize,
    is_cubic: bool,
) -> Vec<Keyframe> {
    if is_cubic {
        // CubicSpline: 3 output elements per keyframe
        timestamps
            .iter()
            .enumerate()
            .map(|(i, &time)| {
                let base = i * 3;
                Keyframe {
                    time,
                    value: raw_values[base + 1].clone(),
                    in_tangent: Some(raw_values[base].clone()),
                    out_tangent: Some(raw_values[base + 2].clone()),
                }
            })
            .collect()
    } else {
        // Step or Linear: 1 output element per keyframe
        timestamps
            .iter()
            .enumerate()
            .map(|(i, &time)| Keyframe {
                time,
                value: raw_values[i].clone(),
                in_tangent: None,
                out_tangent: None,
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Interpolation helpers
// ---------------------------------------------------------------------------

/// Linear interpolation for translation/scale vectors.
pub fn lerp_vec(a: &[f32], b: &[f32], t: f32) -> Vec<f32> {
    a.iter()
        .zip(b.iter())
        .map(|(&ai, &bi)| ai + (bi - ai) * t)
        .collect()
}

/// Spherical linear interpolation for rotation quaternions (XYZW layout).
pub fn slerp_quat(a: &[f32; 4], b: &[f32; 4], t: f32) -> [f32; 4] {
    let qa = UnitQuaternion::new_normalize(Quaternion::new(a[3], a[0], a[1], a[2]));
    let qb = UnitQuaternion::new_normalize(Quaternion::new(b[3], b[0], b[1], b[2]));
    let result = qa.slerp(&qb, t);
    let q = result.into_inner();
    [q.i, q.j, q.k, q.w]
}

/// Cubic hermite spline interpolation.
///
/// `v0` / `v1` are the values at the two keyframes, `b0` is the out-tangent of
/// keyframe 0, `a1` is the in-tangent of keyframe 1, `t_delta` is the time span
/// between the two keyframes, and `t` is the normalised interpolation factor
/// in [0, 1].
pub fn cubic_hermite(
    v0: &[f32],
    b0: &[f32],
    v1: &[f32],
    a1: &[f32],
    t_delta: f32,
    t: f32,
) -> Vec<f32> {
    let t2 = t * t;
    let t3 = t2 * t;
    // Hermite basis functions
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;

    v0.iter()
        .zip(b0.iter())
        .zip(v1.iter().zip(a1.iter()))
        .map(|((&p0, &m0), (&p1, &m1))| {
            h00 * p0 + h10 * t_delta * m0 + h01 * p1 + h11 * t_delta * m1
        })
        .collect()
}

/// Normalise a quaternion stored as [x, y, z, w].
fn normalize_quat(q: &mut [f32]) {
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if len > 1e-12 {
        let inv = 1.0 / len;
        q[0] *= inv;
        q[1] *= inv;
        q[2] *= inv;
        q[3] *= inv;
    }
}

/// Sample a single animation channel at the given `time`, returning the
/// interpolated value (3 floats for translation/scale, 4 for rotation).
pub fn sample_channel(channel: &AnimationChannel, time: f32) -> Vec<f32> {
    let kfs = &channel.keyframes;
    if kfs.is_empty() {
        return Vec::new();
    }
    // Before first keyframe
    if time <= kfs[0].time {
        return kfs[0].value.clone();
    }
    // After last keyframe
    if time >= kfs[kfs.len() - 1].time {
        return kfs[kfs.len() - 1].value.clone();
    }
    // Find the pair of keyframes that bracket `time`
    let mut idx = 0;
    for i in 0..kfs.len() - 1 {
        if time >= kfs[i].time && time < kfs[i + 1].time {
            idx = i;
            break;
        }
    }
    let k0 = &kfs[idx];
    let k1 = &kfs[idx + 1];
    let t_delta = k1.time - k0.time;
    let t = if t_delta > 0.0 {
        (time - k0.time) / t_delta
    } else {
        0.0
    };

    let mut result = match channel.interpolation {
        Interpolation::Step => k0.value.clone(),
        Interpolation::Linear => {
            if channel.property == AnimationProperty::Rotation {
                let a: [f32; 4] = [k0.value[0], k0.value[1], k0.value[2], k0.value[3]];
                let b: [f32; 4] = [k1.value[0], k1.value[1], k1.value[2], k1.value[3]];
                slerp_quat(&a, &b, t).to_vec()
            } else {
                lerp_vec(&k0.value, &k1.value, t)
            }
        }
        Interpolation::CubicSpline => {
            let b0 = k0.out_tangent.as_deref().unwrap_or(&k0.value);
            let a1 = k1.in_tangent.as_deref().unwrap_or(&k1.value);
            cubic_hermite(&k0.value, b0, &k1.value, a1, t_delta, t)
        }
    };

    // Normalise quaternion results
    if channel.property == AnimationProperty::Rotation && result.len() == 4 {
        normalize_quat(&mut result);
    }

    result
}

/// Compose translation, rotation (quaternion XYZW), and scale into a column-major 4x4 matrix.
pub fn compose_trs(
    translation: &[f32; 3],
    rotation: &[f32; 4],
    scale: &[f32; 3],
) -> [[f32; 4]; 4] {
    let t = Matrix4::new_translation(&Vector3::new(
        translation[0],
        translation[1],
        translation[2],
    ));
    let r = UnitQuaternion::new_normalize(Quaternion::new(
        rotation[3],
        rotation[0],
        rotation[1],
        rotation[2],
    ))
    .to_homogeneous();
    let s = Matrix4::new_nonuniform_scaling(&Vector3::new(scale[0], scale[1], scale[2]));
    (t * r * s).into()
}

/// Evaluate an animation clip at a given time and return per-joint local transform matrices.
/// Returns a Vec of mat4 in column-major [[f32;4];4] format, one per joint in the skeleton.
/// Joints without animation channels get their rest-pose transform from the skeleton.
pub fn evaluate_animation(
    clip: &AnimationClip,
    skeleton: &crate::mesh::skeleton::Skeleton,
    time: f32,
) -> Vec<[[f32; 4]; 4]> {
    let joint_count = skeleton.joints.len();

    // Start with rest-pose T/R/S per joint
    let mut translations: Vec<Option<[f32; 3]>> = vec![None; joint_count];
    let mut rotations: Vec<Option<[f32; 4]>> = vec![None; joint_count];
    let mut scales: Vec<Option<[f32; 3]>> = vec![None; joint_count];

    for channel in &clip.channels {
        if channel.joint_index >= joint_count {
            continue;
        }
        let sampled = sample_channel(channel, time);
        match channel.property {
            AnimationProperty::Translation if sampled.len() >= 3 => {
                translations[channel.joint_index] =
                    Some([sampled[0], sampled[1], sampled[2]]);
            }
            AnimationProperty::Rotation if sampled.len() >= 4 => {
                rotations[channel.joint_index] =
                    Some([sampled[0], sampled[1], sampled[2], sampled[3]]);
            }
            AnimationProperty::Scale if sampled.len() >= 3 => {
                scales[channel.joint_index] =
                    Some([sampled[0], sampled[1], sampled[2]]);
            }
            _ => {}
        }
    }

    // Compose local transform matrices
    (0..joint_count)
        .map(|i| {
            let rest = &skeleton.joints[i].local_transform;
            let (rest_t, rest_r, rest_s) = rest.decompose();
            let t = translations[i].unwrap_or(rest_t);
            let r = rotations[i].unwrap_or(rest_r);
            let s = scales[i].unwrap_or(rest_s);
            compose_trs(&t, &r, &s)
        })
        .collect()
}

/// Convert a frame index to a time within an animation clip.
/// `frame` is the zero-based frame index, `total_frames` is the total number of frames.
/// Returns `frame as f32 / total_frames as f32 * clip.duration`.
pub fn frame_to_time(clip: &AnimationClip, frame: usize, total_frames: usize) -> f32 {
    if total_frames == 0 {
        return 0.0;
    }
    frame as f32 / total_frames as f32 * clip.duration
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_keyframes_linear() {
        let timestamps = vec![0.0, 1.0, 2.0];
        let values: Vec<Vec<f32>> = vec![
            vec![1.0, 0.0, 0.0],
            vec![2.0, 0.0, 0.0],
            vec![3.0, 0.0, 0.0],
        ];
        let keyframes = build_keyframes(&timestamps, &values, 3, false);
        assert_eq!(keyframes.len(), 3);
        assert_eq!(keyframes[0].time, 0.0);
        assert_eq!(keyframes[0].value, vec![1.0, 0.0, 0.0]);
        assert!(keyframes[0].in_tangent.is_none());
        assert!(keyframes[0].out_tangent.is_none());
    }

    #[test]
    fn test_build_keyframes_cubic_spline() {
        let timestamps = vec![0.0, 1.0];
        // CubicSpline: [in_tan_0, val_0, out_tan_0, in_tan_1, val_1, out_tan_1]
        let values: Vec<Vec<f32>> = vec![
            vec![0.0, 0.0, 0.0], // in_tangent_0
            vec![1.0, 2.0, 3.0], // value_0
            vec![0.5, 0.5, 0.5], // out_tangent_0
            vec![0.1, 0.1, 0.1], // in_tangent_1
            vec![4.0, 5.0, 6.0], // value_1
            vec![0.0, 0.0, 0.0], // out_tangent_1
        ];
        let keyframes = build_keyframes(&timestamps, &values, 3, true);
        assert_eq!(keyframes.len(), 2);
        assert_eq!(keyframes[0].value, vec![1.0, 2.0, 3.0]);
        assert_eq!(
            keyframes[0].in_tangent.as_ref().unwrap(),
            &vec![0.0, 0.0, 0.0]
        );
        assert_eq!(
            keyframes[0].out_tangent.as_ref().unwrap(),
            &vec![0.5, 0.5, 0.5]
        );
        assert_eq!(keyframes[1].value, vec![4.0, 5.0, 6.0]);
    }
}
