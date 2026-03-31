use gltf::json::{self, validation::Checked::Valid};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Write a mesh to a file, choosing format based on file extension.
/// Supports .obj and .glb.
pub fn write_mesh(
    path: &PathBuf,
    positions: &[[f32; 3]],
    face_indices: &[[u32; 3]],
    normals: Option<&[[f32; 3]]>,
    texcoords: Option<&[[f32; 2]]>,
) -> Result<(), Box<dyn std::error::Error>> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "glb" => write_glb(path, positions, face_indices, normals, texcoords),
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
) -> Result<(), Box<dyn std::error::Error>> {
    // Pack binary buffer: positions | normals | texcoords | indices
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

    let node_idx = root.push(json::Node {
        mesh: Some(mesh_idx),
        name: None,
        camera: None,
        children: None,
        matrix: None,
        rotation: None,
        scale: None,
        translation: None,
        skin: None,
        weights: None,
        extensions: None,
        extras: Default::default(),
    });

    root.push(json::Scene {
        nodes: vec![node_idx],
        extensions: None,
        extras: Default::default(),
        name: None,
    });
    root.scene = Some(json::Index::new(0));

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
