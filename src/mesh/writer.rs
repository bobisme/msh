use gltf::json::{self, validation::Checked::Valid};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::PathBuf;

use super::animation::{AnimationClip, AnimationProperty, Interpolation};
use super::skeleton::Skeleton;

/// Optional skin data to embed in the output GLB.
pub struct SkinData<'a> {
    pub skeleton: &'a Skeleton,
    pub joint_indices: &'a [[u16; 4]],
    pub joint_weights: &'a [[f32; 4]],
    pub animations: &'a [AnimationClip],
}

/// Write a mesh to a file, choosing format based on file extension.
/// Supports .obj and .glb.
pub fn write_mesh(
    path: &PathBuf,
    positions: &[[f32; 3]],
    face_indices: &[[u32; 3]],
    normals: Option<&[[f32; 3]]>,
    texcoords: Option<&[[f32; 2]]>,
    skin_data: Option<&SkinData>,
) -> Result<(), Box<dyn std::error::Error>> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "glb" => write_glb(path, positions, face_indices, normals, texcoords, skin_data),
        "obj" => write_obj(path, positions, face_indices),
        _ => Err(format!(
            "Unsupported output format '.{}'. Use .obj or .glb",
            ext
        )
        .into()),
    }
}

fn write_obj(
    path: &PathBuf,
    positions: &[[f32; 3]],
    face_indices: &[[u32; 3]],
) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    for pos in positions {
        writeln!(file, "v {} {} {}", pos[0], pos[1], pos[2])?;
    }
    for tri in face_indices {
        writeln!(file, "f {} {} {}", tri[0] + 1, tri[1] + 1, tri[2] + 1)?;
    }
    Ok(())
}

fn write_glb(
    path: &PathBuf,
    positions: &[[f32; 3]],
    face_indices: &[[u32; 3]],
    normals: Option<&[[f32; 3]]>,
    texcoords: Option<&[[f32; 2]]>,
    skin_data: Option<&SkinData>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Pack binary buffer: positions | normals | texcoords | indices | joints | weights | ibm | anim...
    let mut bin_data = Vec::new();

    // Positions
    let pos_offset = bin_data.len();
    for p in positions {
        for &f in p {
            bin_data.extend_from_slice(&f.to_le_bytes());
        }
    }
    let pos_byte_length = bin_data.len() - pos_offset;

    // Normals (optional, compute from geometry if not provided)
    let computed_normals;
    let normals = if let Some(n) = normals {
        n
    } else {
        computed_normals = compute_smooth_normals(positions, face_indices);
        &computed_normals
    };
    let norm_offset = bin_data.len();
    for n in normals {
        for &f in n {
            bin_data.extend_from_slice(&f.to_le_bytes());
        }
    }
    let norm_byte_length = bin_data.len() - norm_offset;

    // Texcoords (optional)
    let uv_offset = bin_data.len();
    let mut uv_byte_length = 0;
    if let Some(uvs) = texcoords {
        if !uvs.is_empty() {
            for uv in uvs {
                for &f in uv {
                    bin_data.extend_from_slice(&f.to_le_bytes());
                }
            }
            uv_byte_length = bin_data.len() - uv_offset;
        }
    }

    // Indices (flatten [u32; 3] to u32)
    let idx_offset = bin_data.len();
    for tri in face_indices {
        for &i in tri {
            bin_data.extend_from_slice(&i.to_le_bytes());
        }
    }
    let idx_byte_length = bin_data.len() - idx_offset;
    let idx_count = face_indices.len() * 3;

    // Skin binary data offsets (computed later if skin_data is present)
    let mut joints_offset = 0;
    let mut joints_byte_length = 0;
    let mut weights_offset = 0;
    let mut weights_byte_length = 0;
    let mut ibm_offset = 0;
    let mut ibm_byte_length = 0;

    if let Some(skin) = skin_data {
        // JOINTS_0: u16 vec4 per vertex
        joints_offset = bin_data.len();
        for j in skin.joint_indices {
            for &idx in j {
                bin_data.extend_from_slice(&idx.to_le_bytes());
            }
        }
        joints_byte_length = bin_data.len() - joints_offset;

        // WEIGHTS_0: f32 vec4 per vertex
        weights_offset = bin_data.len();
        for w in skin.joint_weights {
            for &val in w {
                bin_data.extend_from_slice(&val.to_le_bytes());
            }
        }
        weights_byte_length = bin_data.len() - weights_offset;

        // Inverse bind matrices: mat4 per joint (column-major, 16 floats each)
        ibm_offset = bin_data.len();
        for joint in &skin.skeleton.joints {
            for col in &joint.inverse_bind_matrix {
                for &val in col {
                    bin_data.extend_from_slice(&val.to_le_bytes());
                }
            }
        }
        ibm_byte_length = bin_data.len() - ibm_offset;
    }

    // Animation binary data
    struct AnimBinaryData {
        timestamps_offset: usize,
        timestamps_byte_length: usize,
        timestamps_count: usize,
        timestamps_min: f32,
        timestamps_max: f32,
        values_offset: usize,
        values_byte_length: usize,
        values_count: usize,
        values_component_type: json::accessor::ComponentType,
        values_type: json::accessor::Type,
    }

    let mut anim_binary: Vec<Vec<AnimBinaryData>> = Vec::new(); // per-clip, per-channel

    if let Some(skin) = skin_data {
        for clip in skin.animations {
            let mut channel_data = Vec::new();
            for channel in &clip.channels {
                let is_cubic = channel.interpolation == Interpolation::CubicSpline;

                // Timestamps
                let ts_offset = bin_data.len();
                let mut ts_min = f32::INFINITY;
                let mut ts_max = f32::NEG_INFINITY;
                let ts_count = channel.keyframes.len();
                for kf in &channel.keyframes {
                    bin_data.extend_from_slice(&kf.time.to_le_bytes());
                    ts_min = ts_min.min(kf.time);
                    ts_max = ts_max.max(kf.time);
                }
                let ts_byte_length = bin_data.len() - ts_offset;

                // Output values
                let val_offset = bin_data.len();
                let (component_count, accessor_type) = match channel.property {
                    AnimationProperty::Translation | AnimationProperty::Scale => {
                        (3, json::accessor::Type::Vec3)
                    }
                    AnimationProperty::Rotation => (4, json::accessor::Type::Vec4),
                };

                let mut val_count = 0;
                for kf in &channel.keyframes {
                    if is_cubic {
                        // in_tangent
                        if let Some(ref tan) = kf.in_tangent {
                            for i in 0..component_count {
                                bin_data.extend_from_slice(&tan[i].to_le_bytes());
                            }
                        }
                        val_count += 1;
                    }
                    // value
                    for i in 0..component_count {
                        bin_data.extend_from_slice(&kf.value[i].to_le_bytes());
                    }
                    val_count += 1;
                    if is_cubic {
                        // out_tangent
                        if let Some(ref tan) = kf.out_tangent {
                            for i in 0..component_count {
                                bin_data.extend_from_slice(&tan[i].to_le_bytes());
                            }
                        }
                        val_count += 1;
                    }
                }
                let val_byte_length = bin_data.len() - val_offset;

                channel_data.push(AnimBinaryData {
                    timestamps_offset: ts_offset,
                    timestamps_byte_length: ts_byte_length,
                    timestamps_count: ts_count,
                    timestamps_min: ts_min,
                    timestamps_max: ts_max,
                    values_offset: val_offset,
                    values_byte_length: val_byte_length,
                    values_count: val_count,
                    values_component_type: json::accessor::ComponentType::F32,
                    values_type: accessor_type,
                });
            }
            anim_binary.push(channel_data);
        }
    }

    // Pad binary to 4-byte alignment
    while bin_data.len() % 4 != 0 {
        bin_data.push(0);
    }

    // Compute bounding box (required by spec for POSITION accessor)
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for p in positions {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }

    // Build JSON document
    let mut root = json::Root::default();

    let buffer_idx = root.push(json::Buffer {
        byte_length: json::validation::USize64::from(bin_data.len()),
        uri: None,
        extensions: None,
        extras: Default::default(),
        name: None,
    });

    // Buffer views
    let pos_view_idx = root.push(json::buffer::View {
        buffer: buffer_idx,
        byte_length: json::validation::USize64::from(pos_byte_length),
        byte_offset: Some(json::validation::USize64::from(pos_offset)),
        byte_stride: Some(json::buffer::Stride(12)),
        target: Some(Valid(json::buffer::Target::ArrayBuffer)),
        extensions: None,
        extras: Default::default(),
        name: None,
    });

    let norm_view_idx = root.push(json::buffer::View {
        buffer: buffer_idx,
        byte_length: json::validation::USize64::from(norm_byte_length),
        byte_offset: Some(json::validation::USize64::from(norm_offset)),
        byte_stride: Some(json::buffer::Stride(12)),
        target: Some(Valid(json::buffer::Target::ArrayBuffer)),
        extensions: None,
        extras: Default::default(),
        name: None,
    });

    let uv_view_idx = if uv_byte_length > 0 {
        Some(root.push(json::buffer::View {
            buffer: buffer_idx,
            byte_length: json::validation::USize64::from(uv_byte_length),
            byte_offset: Some(json::validation::USize64::from(uv_offset)),
            byte_stride: Some(json::buffer::Stride(8)),
            target: Some(Valid(json::buffer::Target::ArrayBuffer)),
            extensions: None,
            extras: Default::default(),
            name: None,
        }))
    } else {
        None
    };

    let idx_view_idx = root.push(json::buffer::View {
        buffer: buffer_idx,
        byte_length: json::validation::USize64::from(idx_byte_length),
        byte_offset: Some(json::validation::USize64::from(idx_offset)),
        byte_stride: None,
        target: Some(Valid(json::buffer::Target::ElementArrayBuffer)),
        extensions: None,
        extras: Default::default(),
        name: None,
    });

    // Skin buffer views + accessors (if present)
    let skin_accessors = if let Some(skin) = skin_data {
        let joints_view_idx = root.push(json::buffer::View {
            buffer: buffer_idx,
            byte_length: json::validation::USize64::from(joints_byte_length),
            byte_offset: Some(json::validation::USize64::from(joints_offset)),
            byte_stride: Some(json::buffer::Stride(8)), // 4 x u16 = 8 bytes
            target: Some(Valid(json::buffer::Target::ArrayBuffer)),
            extensions: None,
            extras: Default::default(),
            name: None,
        });

        let weights_view_idx = root.push(json::buffer::View {
            buffer: buffer_idx,
            byte_length: json::validation::USize64::from(weights_byte_length),
            byte_offset: Some(json::validation::USize64::from(weights_offset)),
            byte_stride: Some(json::buffer::Stride(16)), // 4 x f32 = 16 bytes
            target: Some(Valid(json::buffer::Target::ArrayBuffer)),
            extensions: None,
            extras: Default::default(),
            name: None,
        });

        let ibm_view_idx = root.push(json::buffer::View {
            buffer: buffer_idx,
            byte_length: json::validation::USize64::from(ibm_byte_length),
            byte_offset: Some(json::validation::USize64::from(ibm_offset)),
            byte_stride: None,
            target: None,
            extensions: None,
            extras: Default::default(),
            name: None,
        });

        let joints_accessor_idx = root.push(json::Accessor {
            buffer_view: Some(joints_view_idx),
            byte_offset: Some(json::validation::USize64::from(0usize)),
            count: json::validation::USize64::from(skin.joint_indices.len()),
            component_type: Valid(json::accessor::GenericComponentType(
                json::accessor::ComponentType::U16,
            )),
            type_: Valid(json::accessor::Type::Vec4),
            min: None,
            max: None,
            normalized: false,
            sparse: None,
            extensions: None,
            extras: Default::default(),
            name: None,
        });

        let weights_accessor_idx = root.push(json::Accessor {
            buffer_view: Some(weights_view_idx),
            byte_offset: Some(json::validation::USize64::from(0usize)),
            count: json::validation::USize64::from(skin.joint_weights.len()),
            component_type: Valid(json::accessor::GenericComponentType(
                json::accessor::ComponentType::F32,
            )),
            type_: Valid(json::accessor::Type::Vec4),
            min: None,
            max: None,
            normalized: false,
            sparse: None,
            extensions: None,
            extras: Default::default(),
            name: None,
        });

        let ibm_accessor_idx = root.push(json::Accessor {
            buffer_view: Some(ibm_view_idx),
            byte_offset: Some(json::validation::USize64::from(0usize)),
            count: json::validation::USize64::from(skin.skeleton.joints.len()),
            component_type: Valid(json::accessor::GenericComponentType(
                json::accessor::ComponentType::F32,
            )),
            type_: Valid(json::accessor::Type::Mat4),
            min: None,
            max: None,
            normalized: false,
            sparse: None,
            extensions: None,
            extras: Default::default(),
            name: None,
        });

        Some((joints_accessor_idx, weights_accessor_idx, ibm_accessor_idx))
    } else {
        None
    };

    // Accessors
    let pos_accessor_idx = root.push(json::Accessor {
        buffer_view: Some(pos_view_idx),
        byte_offset: Some(json::validation::USize64::from(0usize)),
        count: json::validation::USize64::from(positions.len()),
        component_type: Valid(json::accessor::GenericComponentType(
            json::accessor::ComponentType::F32,
        )),
        type_: Valid(json::accessor::Type::Vec3),
        min: Some(json::Value::from(min.to_vec())),
        max: Some(json::Value::from(max.to_vec())),
        normalized: false,
        sparse: None,
        extensions: None,
        extras: Default::default(),
        name: None,
    });

    let norm_accessor_idx = root.push(json::Accessor {
        buffer_view: Some(norm_view_idx),
        byte_offset: Some(json::validation::USize64::from(0usize)),
        count: json::validation::USize64::from(normals.len()),
        component_type: Valid(json::accessor::GenericComponentType(
            json::accessor::ComponentType::F32,
        )),
        type_: Valid(json::accessor::Type::Vec3),
        min: None,
        max: None,
        normalized: false,
        sparse: None,
        extensions: None,
        extras: Default::default(),
        name: None,
    });

    let uv_accessor_idx = uv_view_idx.map(|view_idx| {
        root.push(json::Accessor {
            buffer_view: Some(view_idx),
            byte_offset: Some(json::validation::USize64::from(0usize)),
            count: json::validation::USize64::from(texcoords.unwrap().len()),
            component_type: Valid(json::accessor::GenericComponentType(
                json::accessor::ComponentType::F32,
            )),
            type_: Valid(json::accessor::Type::Vec2),
            min: None,
            max: None,
            normalized: false,
            sparse: None,
            extensions: None,
            extras: Default::default(),
            name: None,
        })
    });

    let idx_accessor_idx = root.push(json::Accessor {
        buffer_view: Some(idx_view_idx),
        byte_offset: Some(json::validation::USize64::from(0usize)),
        count: json::validation::USize64::from(idx_count),
        component_type: Valid(json::accessor::GenericComponentType(
            json::accessor::ComponentType::U32,
        )),
        type_: Valid(json::accessor::Type::Scalar),
        min: None,
        max: None,
        normalized: false,
        sparse: None,
        extensions: None,
        extras: Default::default(),
        name: None,
    });

    // Mesh primitive
    let mut attributes = BTreeMap::new();
    attributes.insert(Valid(json::mesh::Semantic::Positions), pos_accessor_idx);
    attributes.insert(Valid(json::mesh::Semantic::Normals), norm_accessor_idx);
    if let Some(uv_idx) = uv_accessor_idx {
        attributes.insert(
            Valid(json::mesh::Semantic::TexCoords(0)),
            uv_idx,
        );
    }
    if let Some((joints_acc, weights_acc, _)) = skin_accessors {
        attributes.insert(Valid(json::mesh::Semantic::Joints(0)), joints_acc);
        attributes.insert(Valid(json::mesh::Semantic::Weights(0)), weights_acc);
    }

    let mesh_idx = root.push(json::Mesh {
        primitives: vec![json::mesh::Primitive {
            attributes,
            indices: Some(idx_accessor_idx),
            material: None,
            mode: Valid(json::mesh::Mode::Triangles),
            targets: None,
            extensions: None,
            extras: Default::default(),
        }],
        extensions: None,
        extras: Default::default(),
        weights: None,
        name: None,
    });

    // Build nodes: joint nodes first, then mesh node
    let skin_idx = if let Some((_, _, ibm_acc)) = skin_accessors {
        let skin = skin_data.unwrap();
        let joint_count = skin.skeleton.joints.len();

        // Create joint nodes. Node indices: 0..joint_count are joint nodes.
        let mut joint_node_indices: Vec<json::Index<json::scene::Node>> = Vec::new();

        // First pass: create all joint nodes (without children — we'll fill those in)
        for joint in &skin.skeleton.joints {
            let (t, r, s) = joint.local_transform.decompose();

            let node_idx = root.push(json::Node {
                mesh: None,
                name: joint.name.clone(),
                camera: None,
                children: None,
                matrix: None,
                rotation: Some(json::scene::UnitQuaternion(r)),
                scale: Some(s),
                translation: Some(t),
                skin: None,
                weights: None,
                extensions: None,
                extras: Default::default(),
            });
            joint_node_indices.push(node_idx);
        }

        // Second pass: set up parent-child relationships
        // Collect children per parent
        let mut children_map: Vec<Vec<json::Index<json::scene::Node>>> = vec![Vec::new(); joint_count];
        for joint in &skin.skeleton.joints {
            if let Some(parent_idx) = joint.parent {
                children_map[parent_idx].push(joint_node_indices[joint.index]);
            }
        }
        for (i, children) in children_map.into_iter().enumerate() {
            if !children.is_empty() {
                root.nodes[joint_node_indices[i].value()].children = Some(children);
            }
        }

        // Create skin
        let skin_idx = root.push(json::Skin {
            extensions: None,
            extras: Default::default(),
            inverse_bind_matrices: Some(ibm_acc),
            joints: joint_node_indices.clone(),
            name: None,
            skeleton: None,
        });

        Some((skin_idx, joint_node_indices))
    } else {
        None
    };

    // Mesh node
    let mesh_node = json::Node {
        mesh: Some(mesh_idx),
        name: None,
        camera: None,
        children: None,
        matrix: None,
        rotation: None,
        scale: None,
        translation: None,
        skin: skin_idx.as_ref().map(|(s, _)| *s),
        weights: None,
        extensions: None,
        extras: Default::default(),
    };
    let mesh_node_idx = root.push(mesh_node);

    // Scene: include root joint nodes and mesh node
    let mut scene_nodes = Vec::new();
    if let Some((_, ref joint_node_indices)) = skin_idx {
        let skin = skin_data.unwrap();
        // Add root joints (joints with no parent) to the scene
        for joint in &skin.skeleton.joints {
            if joint.parent.is_none() {
                scene_nodes.push(joint_node_indices[joint.index]);
            }
        }
    }
    scene_nodes.push(mesh_node_idx);

    root.push(json::Scene {
        nodes: scene_nodes,
        extensions: None,
        extras: Default::default(),
        name: None,
    });
    root.scene = Some(json::Index::new(0));

    // Animations
    if let Some(skin) = skin_data {
        if let Some((_, ref joint_node_indices)) = skin_idx {
            for (clip_idx, clip) in skin.animations.iter().enumerate() {
                let mut samplers = Vec::new();
                let mut channels = Vec::new();

                for (ch_idx, channel) in clip.channels.iter().enumerate() {
                    let binary = &anim_binary[clip_idx][ch_idx];

                    // Timestamp buffer view + accessor
                    let ts_view_idx = root.push(json::buffer::View {
                        buffer: buffer_idx,
                        byte_length: json::validation::USize64::from(binary.timestamps_byte_length),
                        byte_offset: Some(json::validation::USize64::from(binary.timestamps_offset)),
                        byte_stride: None,
                        target: None,
                        extensions: None,
                        extras: Default::default(),
                        name: None,
                    });
                    let ts_accessor_idx = root.push(json::Accessor {
                        buffer_view: Some(ts_view_idx),
                        byte_offset: Some(json::validation::USize64::from(0usize)),
                        count: json::validation::USize64::from(binary.timestamps_count),
                        component_type: Valid(json::accessor::GenericComponentType(
                            json::accessor::ComponentType::F32,
                        )),
                        type_: Valid(json::accessor::Type::Scalar),
                        min: Some(json::Value::from(vec![binary.timestamps_min])),
                        max: Some(json::Value::from(vec![binary.timestamps_max])),
                        normalized: false,
                        sparse: None,
                        extensions: None,
                        extras: Default::default(),
                        name: None,
                    });

                    // Values buffer view + accessor
                    let val_view_idx = root.push(json::buffer::View {
                        buffer: buffer_idx,
                        byte_length: json::validation::USize64::from(binary.values_byte_length),
                        byte_offset: Some(json::validation::USize64::from(binary.values_offset)),
                        byte_stride: None,
                        target: None,
                        extensions: None,
                        extras: Default::default(),
                        name: None,
                    });
                    let val_accessor_idx = root.push(json::Accessor {
                        buffer_view: Some(val_view_idx),
                        byte_offset: Some(json::validation::USize64::from(0usize)),
                        count: json::validation::USize64::from(binary.values_count),
                        component_type: Valid(json::accessor::GenericComponentType(
                            binary.values_component_type,
                        )),
                        type_: Valid(binary.values_type),
                        min: None,
                        max: None,
                        normalized: false,
                        sparse: None,
                        extensions: None,
                        extras: Default::default(),
                        name: None,
                    });

                    let sampler_idx = json::Index::new(samplers.len() as u32);
                    samplers.push(json::animation::Sampler {
                        extensions: None,
                        extras: Default::default(),
                        input: ts_accessor_idx,
                        interpolation: Valid(match channel.interpolation {
                            Interpolation::Linear => json::animation::Interpolation::Linear,
                            Interpolation::Step => json::animation::Interpolation::Step,
                            Interpolation::CubicSpline => json::animation::Interpolation::CubicSpline,
                        }),
                        output: val_accessor_idx,
                    });

                    let property = match channel.property {
                        AnimationProperty::Translation => json::animation::Property::Translation,
                        AnimationProperty::Rotation => json::animation::Property::Rotation,
                        AnimationProperty::Scale => json::animation::Property::Scale,
                    };

                    channels.push(json::animation::Channel {
                        sampler: sampler_idx,
                        target: json::animation::Target {
                            extensions: None,
                            extras: Default::default(),
                            node: joint_node_indices[channel.joint_index],
                            path: Valid(property),
                        },
                        extensions: None,
                        extras: Default::default(),
                    });
                }

                root.push(json::Animation {
                    extensions: None,
                    extras: Default::default(),
                    channels,
                    name: clip.name.clone(),
                    samplers,
                });
            }
        }
    }

    // Write GLB
    let json_bytes = root.to_vec()?;
    let glb = gltf::binary::Glb {
        header: gltf::binary::Header {
            magic: *b"glTF",
            version: 2,
            length: 0, // to_writer computes this
        },
        json: Cow::Borrowed(&json_bytes),
        bin: Some(Cow::Borrowed(&bin_data)),
    };

    let file = std::fs::File::create(path)?;
    let writer = std::io::BufWriter::new(file);
    glb.to_writer(writer)?;
    Ok(())
}

fn compute_smooth_normals(
    positions: &[[f32; 3]],
    face_indices: &[[u32; 3]],
) -> Vec<[f32; 3]> {
    let mut normals = vec![[0.0f32; 3]; positions.len()];

    for tri in face_indices {
        let a = positions[tri[0] as usize];
        let b = positions[tri[1] as usize];
        let c = positions[tri[2] as usize];
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        // Accumulate (area-weighted by cross product magnitude)
        for &vi in tri {
            let v = &mut normals[vi as usize];
            v[0] += n[0];
            v[1] += n[1];
            v[2] += n[2];
        }
    }

    // Normalize
    for n in &mut normals {
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len > 1e-10 {
            n[0] /= len;
            n[1] /= len;
            n[2] /= len;
        }
    }

    normals
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::skeleton::{Joint, JointTransform, Skeleton};

    /// Round-trip test: write a GLB with skin data, load it back, verify skin data survived.
    #[test]
    fn test_glb_round_trip_with_skin() {
        let positions: Vec<[f32; 3]> = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        let face_indices: Vec<[u32; 3]> = vec![[0, 1, 2], [0, 2, 3]];

        let skeleton = Skeleton {
            joints: vec![
                Joint {
                    index: 0,
                    node_index: 0,
                    name: Some("root".into()),
                    parent: None,
                    inverse_bind_matrix: [
                        [1.0, 0.0, 0.0, 0.0],
                        [0.0, 1.0, 0.0, 0.0],
                        [0.0, 0.0, 1.0, 0.0],
                        [0.0, 0.0, 0.0, 1.0],
                    ],
                    local_transform: JointTransform::Decomposed {
                        translation: [0.0, 0.0, 0.0],
                        rotation: [0.0, 0.0, 0.0, 1.0],
                        scale: [1.0, 1.0, 1.0],
                    },
                },
                Joint {
                    index: 1,
                    node_index: 1,
                    name: Some("child".into()),
                    parent: Some(0),
                    inverse_bind_matrix: [
                        [1.0, 0.0, 0.0, 0.0],
                        [0.0, 1.0, 0.0, 0.0],
                        [0.0, 0.0, 1.0, 0.0],
                        [0.0, -1.0, 0.0, 1.0],
                    ],
                    local_transform: JointTransform::Decomposed {
                        translation: [0.0, 1.0, 0.0],
                        rotation: [0.0, 0.0, 0.0, 1.0],
                        scale: [1.0, 1.0, 1.0],
                    },
                },
            ],
        };

        let joint_indices: Vec<[u16; 4]> = vec![
            [0, 0, 0, 0],
            [0, 1, 0, 0],
            [1, 0, 0, 0],
            [0, 1, 0, 0],
        ];
        let joint_weights: Vec<[f32; 4]> = vec![
            [1.0, 0.0, 0.0, 0.0],
            [0.6, 0.4, 0.0, 0.0],
            [0.9, 0.1, 0.0, 0.0],
            [0.3, 0.7, 0.0, 0.0],
        ];

        let skin_data = SkinData {
            skeleton: &skeleton,
            joint_indices: &joint_indices,
            joint_weights: &joint_weights,
            animations: &[],
        };

        let tmp = std::env::temp_dir().join("msh_test_skin_roundtrip.glb");
        write_mesh(
            &tmp,
            &positions,
            &face_indices,
            None,
            None,
            Some(&skin_data),
        )
        .expect("write_mesh failed");

        // Load it back
        let loaded = crate::mesh::loader::load_mesh_with_colors(&tmp, None)
            .expect("load_mesh_with_colors failed");
        let _ = std::fs::remove_file(&tmp);

        // Verify skeleton
        let skel = loaded.skeleton.as_ref().expect("skeleton missing");
        assert_eq!(skel.joints.len(), 2);
        assert_eq!(skel.joints[0].name.as_deref(), Some("root"));
        assert_eq!(skel.joints[1].name.as_deref(), Some("child"));
        assert_eq!(skel.joints[1].parent, Some(0));

        // Verify joint indices and weights survived
        assert_eq!(loaded.joint_indices.len(), 4);
        assert_eq!(loaded.joint_weights.len(), 4);
        assert_eq!(loaded.joint_indices[0], [0, 0, 0, 0]);
        assert_eq!(loaded.joint_indices[1], [0, 1, 0, 0]);

        // Weights should be approximately equal (f32 round-trip)
        for i in 0..4 {
            for j in 0..4 {
                assert!(
                    (loaded.joint_weights[i][j] - joint_weights[i][j]).abs() < 1e-6,
                    "weight mismatch at [{i}][{j}]"
                );
            }
        }
    }
}
