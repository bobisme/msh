use baby_shark::io::{Builder, IndexedBuilder};
use baby_shark::mesh::corner_table::CornerTableF;
use std::path::PathBuf;

/// Load mesh from file (supports .obj and .glb)
pub fn load_mesh(
    input: &PathBuf,
    mesh_name: Option<&str>,
) -> Result<CornerTableF, Box<dyn std::error::Error>> {
    let extension = input
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .ok_or("File has no extension")?;

    match extension.as_str() {
        "obj" => {
            use baby_shark::io::read_from_file;
            read_from_file(input).map_err(|e| format!("Failed to read OBJ file: {:?}", e).into())
        }
        "glb" | "gltf" => load_mesh_from_glb(input, mesh_name),
        _ => Err(format!("Unsupported file format: {}", extension).into()),
    }
}

/// Load mesh from GLB/glTF file
pub fn load_mesh_from_glb(
    path: &PathBuf,
    mesh_name: Option<&str>,
) -> Result<CornerTableF, Box<dyn std::error::Error>> {
    let (document, buffers, _images) = gltf::import(path)?;

    let meshes: Vec<_> = document.meshes().collect();

    if meshes.is_empty() {
        return Err("GLB file contains no meshes".into());
    }

    // Select the appropriate mesh
    let selected_mesh = if meshes.len() == 1 {
        &meshes[0]
    } else {
        // Multiple meshes - need mesh name
        match mesh_name {
            None => {
                let mesh_list: Vec<String> = meshes
                    .iter()
                    .map(|m| m.name().unwrap_or("<unnamed>").to_string())
                    .collect();
                return Err(format!(
                    "GLB file contains {} meshes. Please specify one with --mesh <name>.\nAvailable meshes: {}",
                    meshes.len(),
                    mesh_list.join(", ")
                ).into());
            }
            Some(name) => meshes
                .iter()
                .find(|m| m.name() == Some(name))
                .ok_or_else(|| {
                    let mesh_list: Vec<String> = meshes
                        .iter()
                        .map(|m| m.name().unwrap_or("<unnamed>").to_string())
                        .collect();
                    format!(
                        "Mesh '{}' not found in GLB file.\nAvailable meshes: {}",
                        name,
                        mesh_list.join(", ")
                    )
                })?,
        }
    };

    println!(
        "Loading mesh: {}",
        selected_mesh.name().unwrap_or("<unnamed>")
    );

    // Extract vertex positions and indices from all primitives
    let mut all_positions = Vec::new();
    let mut all_indices = Vec::new();
    let mut vertex_offset = 0u32;

    for primitive in selected_mesh.primitives() {
        // Get positions
        let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

        let positions = reader
            .read_positions()
            .ok_or("Primitive has no position data")?;

        let pos_vec: Vec<[f32; 3]> = positions.collect();
        all_positions.extend_from_slice(&pos_vec);

        // Get indices
        if let Some(indices) = reader.read_indices() {
            let idx_vec: Vec<u32> = indices.into_u32().map(|i| i + vertex_offset).collect();
            all_indices.extend_from_slice(&idx_vec);
        } else {
            // Generate indices for non-indexed geometry
            for i in (0..pos_vec.len()).step_by(3) {
                all_indices.push(vertex_offset + i as u32);
                all_indices.push(vertex_offset + i as u32 + 1);
                all_indices.push(vertex_offset + i as u32 + 2);
            }
        }

        vertex_offset += pos_vec.len() as u32;
    }

    // Convert to baby_shark CornerTableF
    let mut builder = CornerTableF::builder_indexed();

    builder.set_num_vertices(all_positions.len());
    for pos in all_positions {
        builder
            .add_vertex(pos)
            .map_err(|e| format!("Failed to add vertex: {:?}", e))?;
    }

    // Add triangular faces
    if all_indices.len() % 3 != 0 {
        return Err("Index count is not a multiple of 3 (non-triangular faces)".into());
    }

    builder.set_num_faces(all_indices.len() / 3);
    for chunk in all_indices.chunks(3) {
        builder
            .add_face(chunk[0] as usize, chunk[1] as usize, chunk[2] as usize)
            .map_err(|e| format!("Failed to add face: {:?}", e))?;
    }

    builder
        .finish()
        .map_err(|e| format!("Failed to build mesh: {:?}", e).into())
}
