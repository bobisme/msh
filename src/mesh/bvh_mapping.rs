//! Map BVH motion data onto a glTF skeleton and convert to AnimationClip.
//!
//! This module handles:
//! - Joint name normalization (strip prefixes, case-insensitive compare, alias table)
//! - Skeleton matching (BVH joint index -> skeleton joint index)
//! - Euler-to-quaternion conversion (respecting declared channel order)
//! - BVH frame data -> AnimationClip conversion

use nalgebra::{UnitQuaternion, Vector3};

use super::animation::{AnimationChannel, AnimationClip, AnimationProperty, Interpolation, Keyframe};
use super::bvh::{BvhClip, ChannelType};
use super::skeleton::Skeleton;

// ---------------------------------------------------------------------------
// Name normalization
// ---------------------------------------------------------------------------

/// Known prefixes to strip from joint names before matching.
const STRIP_PREFIXES: &[&str] = &["mixamorig:", "mixamorig_"];

/// Alias table: (alias, canonical). Case-insensitive lookup.
const ALIASES: &[(&str, &str)] = &[
    ("lowerback", "spine"),
    ("chest", "spine1"),
    ("chest1", "spine1"),
    ("chest2", "spine2"),
    ("neck1", "neck"),
    ("leftshoulder", "leftshoulder"),
    ("rightshoulder", "rightshoulder"),
];

/// Normalize a joint name for matching: strip known prefixes, lowercase, apply alias table.
fn normalize_name(name: &str) -> String {
    let mut n = name.to_string();
    for prefix in STRIP_PREFIXES {
        if let Some(rest) = n.strip_prefix(prefix) {
            n = rest.to_string();
            break;
        }
        // Also try case-insensitive prefix strip
        let lower = n.to_lowercase();
        let prefix_lower = prefix.to_lowercase();
        if let Some(rest) = lower.strip_prefix(&prefix_lower) {
            n = name[prefix.len()..].to_string();
            let _ = rest; // used only for the check
            break;
        }
    }
    let lowered = n.to_lowercase();
    // Check alias table
    for &(alias, canonical) in ALIASES {
        if lowered == alias {
            return canonical.to_lowercase();
        }
    }
    lowered
}

// ---------------------------------------------------------------------------
// Skeleton matching
// ---------------------------------------------------------------------------

/// Match BVH joints to skeleton joints by normalized name.
///
/// Returns a Vec where `result[bvh_joint_index]` is `Some(skeleton_joint_index)` if matched,
/// or `None` if no match was found. End-site joints are always `None`.
///
/// Warns on unmatched joints but only fails if the root joint (index 0) is unmatched.
pub fn match_bvh_to_skeleton(
    bvh: &BvhClip,
    skeleton: &Skeleton,
) -> Result<Vec<Option<usize>>, String> {
    // Build a lookup: normalized skeleton name -> joint index
    let skel_lookup: Vec<(String, usize)> = skeleton
        .joints
        .iter()
        .filter_map(|j| {
            j.name
                .as_ref()
                .map(|name| (normalize_name(name), j.index))
        })
        .collect();

    let mut mapping = Vec::with_capacity(bvh.joints.len());
    let mut unmatched = Vec::new();

    for (_bvh_idx, bvh_joint) in bvh.joints.iter().enumerate() {
        if bvh_joint.is_end_site {
            mapping.push(None);
            continue;
        }

        let bvh_norm = normalize_name(&bvh_joint.name);
        let found = skel_lookup
            .iter()
            .find(|(skel_norm, _)| *skel_norm == bvh_norm)
            .map(|(_, idx)| *idx);

        if found.is_none() {
            unmatched.push(bvh_joint.name.clone());
        }
        mapping.push(found);
    }

    // Check root is matched
    if !bvh.joints.is_empty() && !bvh.joints[0].is_end_site && mapping[0].is_none() {
        return Err(format!(
            "BVH root joint '{}' does not match any skeleton joint. \
             Cannot apply BVH motion without a root match.",
            bvh.joints[0].name
        ));
    }

    if !unmatched.is_empty() {
        eprintln!(
            "Warning: {} BVH joint(s) unmatched (will use rest pose): {}",
            unmatched.len(),
            unmatched.join(", ")
        );
    }

    let matched_count = mapping.iter().filter(|m| m.is_some()).count();
    println!(
        "BVH skeleton mapping: {}/{} joints matched",
        matched_count,
        bvh.joints.iter().filter(|j| !j.is_end_site).count()
    );

    Ok(mapping)
}

// ---------------------------------------------------------------------------
// Euler-to-quaternion conversion
// ---------------------------------------------------------------------------

/// Given a BVH joint's channel types and the float values for one frame,
/// extract rotation angles and convert to a unit quaternion.
///
/// The channel order in BVH declares the application order (first listed = first applied).
/// For intrinsic rotations, the matrix multiplication order is reversed:
/// channels [Z, X, Y] -> matrix R_Y * R_X * R_Z
///
/// Returns quaternion as [x, y, z, w] to match glTF/nalgebra convention.
pub fn euler_to_quat(channels: &[ChannelType], values: &[f32]) -> [f32; 4] {
    // Collect rotation channels in application order (as listed in BVH)
    let mut rot_quats: Vec<UnitQuaternion<f32>> = Vec::new();

    for (ch, &val) in channels.iter().zip(values.iter()) {
        let rad = val.to_radians();
        let q = match ch {
            ChannelType::Xrotation => {
                UnitQuaternion::from_axis_angle(&Vector3::x_axis(), rad)
            }
            ChannelType::Yrotation => {
                UnitQuaternion::from_axis_angle(&Vector3::y_axis(), rad)
            }
            ChannelType::Zrotation => {
                UnitQuaternion::from_axis_angle(&Vector3::z_axis(), rad)
            }
            _ => continue, // skip position channels
        };
        rot_quats.push(q);
    }

    // Compose: intrinsic rotations -> reverse multiply order
    // channels [first, second, third] -> result = q_third * q_second * q_first
    let mut result = UnitQuaternion::identity();
    for q in rot_quats.iter().rev() {
        result = result * q;
    }

    let qi = result.into_inner();
    [qi.i, qi.j, qi.k, qi.w]
}

/// Extract translation [x, y, z] from a joint's channels and frame values.
/// Returns None if the joint has no position channels.
pub fn extract_translation(channels: &[ChannelType], values: &[f32]) -> Option<[f32; 3]> {
    let mut tx = None;
    let mut ty = None;
    let mut tz = None;

    for (ch, &val) in channels.iter().zip(values.iter()) {
        match ch {
            ChannelType::Xposition => tx = Some(val),
            ChannelType::Yposition => ty = Some(val),
            ChannelType::Zposition => tz = Some(val),
            _ => {}
        }
    }

    // Only return translation if at least one position channel exists
    if tx.is_some() || ty.is_some() || tz.is_some() {
        Some([tx.unwrap_or(0.0), ty.unwrap_or(0.0), tz.unwrap_or(0.0)])
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// BVH -> AnimationClip conversion
// ---------------------------------------------------------------------------

/// Convert a BVH clip into an AnimationClip using the given joint mapping.
///
/// For each mapped joint, creates AnimationChannels for rotation (and translation
/// for joints with position channels, typically just the root).
/// Each BVH frame becomes a Linear-interpolated keyframe.
pub fn bvh_to_animation_clip(
    bvh: &BvhClip,
    joint_mapping: &[Option<usize>],
) -> AnimationClip {
    let mut channels: Vec<AnimationChannel> = Vec::new();

    for (bvh_idx, bvh_joint) in bvh.joints.iter().enumerate() {
        if bvh_joint.is_end_site || bvh_joint.channels.is_empty() {
            continue;
        }

        let skel_idx = match joint_mapping.get(bvh_idx).copied().flatten() {
            Some(idx) => idx,
            None => continue, // unmatched joint, skip
        };

        let ch_offset = bvh.joint_channel_offset(bvh_idx);
        let ch_count = bvh_joint.channels.len();

        // Check if this joint has rotation channels
        let has_rotation = bvh_joint.channels.iter().any(|c| matches!(c,
            ChannelType::Xrotation | ChannelType::Yrotation | ChannelType::Zrotation
        ));

        // Check if this joint has position channels
        let has_translation = bvh_joint.channels.iter().any(|c| matches!(c,
            ChannelType::Xposition | ChannelType::Yposition | ChannelType::Zposition
        ));

        // Build rotation keyframes
        if has_rotation {
            let mut keyframes = Vec::with_capacity(bvh.frame_count);
            for frame_idx in 0..bvh.frame_count {
                let frame_values = &bvh.frames[frame_idx][ch_offset..ch_offset + ch_count];
                let quat = euler_to_quat(&bvh_joint.channels, frame_values);
                keyframes.push(Keyframe {
                    time: frame_idx as f32 * bvh.frame_time,
                    value: quat.to_vec(),
                    in_tangent: None,
                    out_tangent: None,
                });
            }
            channels.push(AnimationChannel {
                joint_index: skel_idx,
                property: AnimationProperty::Rotation,
                interpolation: Interpolation::Linear,
                keyframes,
            });
        }

        // Build translation keyframes (typically only root)
        if has_translation {
            let mut keyframes = Vec::with_capacity(bvh.frame_count);
            for frame_idx in 0..bvh.frame_count {
                let frame_values = &bvh.frames[frame_idx][ch_offset..ch_offset + ch_count];
                if let Some(trans) = extract_translation(&bvh_joint.channels, frame_values) {
                    keyframes.push(Keyframe {
                        time: frame_idx as f32 * bvh.frame_time,
                        value: trans.to_vec(),
                        in_tangent: None,
                        out_tangent: None,
                    });
                }
            }
            if !keyframes.is_empty() {
                channels.push(AnimationChannel {
                    joint_index: skel_idx,
                    property: AnimationProperty::Translation,
                    interpolation: Interpolation::Linear,
                    keyframes,
                });
            }
        }
    }

    let duration = bvh.duration();

    AnimationClip {
        name: Some("BVH Motion".to_string()),
        channels,
        duration,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_name_strips_mixamo_prefix() {
        assert_eq!(normalize_name("mixamorig:Hips"), "hips");
        assert_eq!(normalize_name("mixamorig_LeftArm"), "leftarm");
    }

    #[test]
    fn test_normalize_name_lowercase() {
        assert_eq!(normalize_name("LeftForeArm"), "leftforearm");
        assert_eq!(normalize_name("Hips"), "hips");
    }

    #[test]
    fn test_normalize_name_aliases() {
        assert_eq!(normalize_name("LowerBack"), "spine");
        assert_eq!(normalize_name("Chest"), "spine1");
        assert_eq!(normalize_name("Neck1"), "neck");
    }

    #[test]
    fn test_euler_to_quat_identity() {
        let channels = vec![
            ChannelType::Zrotation,
            ChannelType::Xrotation,
            ChannelType::Yrotation,
        ];
        let values = [0.0, 0.0, 0.0];
        let q = euler_to_quat(&channels, &values);
        // Should be identity quaternion: [0, 0, 0, 1]
        assert!((q[0]).abs() < 1e-6);
        assert!((q[1]).abs() < 1e-6);
        assert!((q[2]).abs() < 1e-6);
        assert!((q[3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_euler_to_quat_90_deg_y() {
        let channels = vec![
            ChannelType::Zrotation,
            ChannelType::Xrotation,
            ChannelType::Yrotation,
        ];
        // Only Y rotation = 90 degrees
        let values = [0.0, 0.0, 90.0];
        let q = euler_to_quat(&channels, &values);
        // 90 deg around Y: quat should be [0, sin(45), 0, cos(45)]
        let expected_sin = std::f32::consts::FRAC_PI_4.sin();
        let expected_cos = std::f32::consts::FRAC_PI_4.cos();
        assert!((q[0]).abs() < 1e-5);
        assert!((q[1] - expected_sin).abs() < 1e-5);
        assert!((q[2]).abs() < 1e-5);
        assert!((q[3] - expected_cos).abs() < 1e-5);
    }

    #[test]
    fn test_extract_translation() {
        let channels = vec![
            ChannelType::Xposition,
            ChannelType::Yposition,
            ChannelType::Zposition,
            ChannelType::Zrotation,
            ChannelType::Xrotation,
            ChannelType::Yrotation,
        ];
        let values = [1.0, 2.0, 3.0, 10.0, 20.0, 30.0];
        let t = extract_translation(&channels, &values);
        assert_eq!(t, Some([1.0, 2.0, 3.0]));
    }

    #[test]
    fn test_extract_translation_none_for_rotation_only() {
        let channels = vec![
            ChannelType::Zrotation,
            ChannelType::Xrotation,
            ChannelType::Yrotation,
        ];
        let values = [10.0, 20.0, 30.0];
        let t = extract_translation(&channels, &values);
        assert_eq!(t, None);
    }

    #[test]
    fn test_bvh_to_clip_basic() {
        use super::super::bvh::parse_bvh;

        let bvh_text = "\
HIERARCHY
ROOT Hips
{
    OFFSET 0.0 0.0 0.0
    CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation
    JOINT Chest
    {
        OFFSET 0.0 5.21 0.0
        CHANNELS 3 Zrotation Xrotation Yrotation
        End Site
        {
            OFFSET 0.0 4.0 0.0
        }
    }
}
MOTION
Frames: 2
Frame Time: 0.033333
0.0 35.0 0.0 -2.1 0.5 1.3 0.1 -0.2 0.3
0.0 35.1 0.0 -2.0 0.4 1.2 0.2 -0.1 0.4
";
        let bvh = parse_bvh(bvh_text).unwrap();
        // Mapping: Hips -> 0, Chest -> 1, End Site -> None
        let mapping = vec![Some(0), Some(1), None];
        let clip = bvh_to_animation_clip(&bvh, &mapping);

        assert_eq!(clip.name.as_deref(), Some("BVH Motion"));
        assert!((clip.duration - 0.033333).abs() < 1e-4);

        // Should have: Hips rotation, Hips translation, Chest rotation = 3 channels
        assert_eq!(clip.channels.len(), 3);

        // Check that we have both rotation and translation for joint 0 (Hips)
        let hips_rot = clip.channels.iter().find(|c| c.joint_index == 0 && c.property == AnimationProperty::Rotation);
        let hips_trans = clip.channels.iter().find(|c| c.joint_index == 0 && c.property == AnimationProperty::Translation);
        let chest_rot = clip.channels.iter().find(|c| c.joint_index == 1 && c.property == AnimationProperty::Rotation);

        assert!(hips_rot.is_some());
        assert!(hips_trans.is_some());
        assert!(chest_rot.is_some());

        // Each should have 2 keyframes
        assert_eq!(hips_rot.unwrap().keyframes.len(), 2);
        assert_eq!(hips_trans.unwrap().keyframes.len(), 2);
        assert_eq!(chest_rot.unwrap().keyframes.len(), 2);

        // Translation values should match BVH data
        let t0 = &hips_trans.unwrap().keyframes[0].value;
        assert!((t0[0] - 0.0).abs() < 1e-5);
        assert!((t0[1] - 35.0).abs() < 1e-5);
        assert!((t0[2] - 0.0).abs() < 1e-5);
    }
}
