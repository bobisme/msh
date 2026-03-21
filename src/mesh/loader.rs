use baby_shark::io::{Builder, IndexedBuilder};
use baby_shark::mesh::corner_table::CornerTableF;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Mesh geometry with optional per-face material colors
pub struct MeshWithColors {
    /// Vertex positions
    pub positions: Vec<[f32; 3]>,
    /// Triangle face indices (0-based, 3 per face)
    pub face_indices: Vec<[u32; 3]>,
    /// Per-face RGBA colors (empty if no materials found)
    pub face_colors: Vec<[f32; 4]>,
}

impl MeshWithColors {
    /// Build a CornerTableF from the parsed geometry (for mesh stats)
    pub fn to_corner_table(&self) -> Result<CornerTableF, Box<dyn std::error::Error>> {
        let mut builder = CornerTableF::builder_indexed();
        builder.set_num_vertices(self.positions.len());
        for pos in &self.positions {
            builder
                .add_vertex(*pos)
                .map_err(|e| format!("Failed to add vertex: {:?}", e))?;
        }
        builder.set_num_faces(self.face_indices.len());
        for tri in &self.face_indices {
            builder
                .add_face(tri[0] as usize, tri[1] as usize, tri[2] as usize)
                .map_err(|e| format!("Failed to add face: {:?}", e))?;
        }
        builder
            .finish()
            .map_err(|e| format!("Failed to build mesh: {:?}", e).into())
    }
}

/// Load mesh with per-face material colors (supports .obj+.mtl and .glb/.gltf)
pub fn load_mesh_with_colors(
    input: &PathBuf,
    mesh_name: Option<&str>,
) -> Result<MeshWithColors, Box<dyn std::error::Error>> {
    let extension = input
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .ok_or("File has no extension")?;

    match extension.as_str() {
        "obj" => parse_obj_with_colors(input),
        "glb" | "gltf" => load_glb_with_colors(input, mesh_name),
        _ => Err(format!("Unsupported file format: {}", extension).into()),
    }
}

/// Load mesh from file (supports .obj and .glb) — returns CornerTableF for processing
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

// --- OBJ + MTL parsing ---

/// Parse an OBJ file with optional MTL material colors
fn parse_obj_with_colors(path: &PathBuf) -> Result<MeshWithColors, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let parent_dir = path.parent().unwrap_or(Path::new("."));

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut face_indices: Vec<[u32; 3]> = Vec::new();
    let mut face_colors: Vec<[f32; 4]> = Vec::new();
    let mut materials: HashMap<String, [f32; 4]> = HashMap::new();
    let mut current_color: [f32; 4] = [0.85, 0.85, 0.85, 1.0];
    let mut has_materials = false;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(mtl_name) = line.strip_prefix("mtllib ") {
            let mtl_name = mtl_name.trim();
            let mtl_path = parent_dir.join(mtl_name);
            if mtl_path.exists() {
                match parse_mtl(&mtl_path) {
                    Ok(mtls) => {
                        if !mtls.is_empty() {
                            has_materials = true;
                            materials = mtls;
                        }
                    }
                    Err(e) => eprintln!("Warning: failed to parse MTL file {:?}: {}", mtl_path, e),
                }
            }
        } else if let Some(coords) = line.strip_prefix("v ") {
            let parts: Vec<f32> = coords
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();
            if parts.len() >= 3 {
                positions.push([parts[0], parts[1], parts[2]]);
            }
        } else if let Some(mat_name) = line.strip_prefix("usemtl ") {
            let mat_name = mat_name.trim();
            if let Some(color) = materials.get(mat_name) {
                current_color = *color;
            }
        } else if let Some(face_str) = line.strip_prefix("f ") {
            let verts: Vec<u32> = face_str
                .split_whitespace()
                .filter_map(|s| {
                    // Parse v, v/vt, v//vn, or v/vt/vn — extract just the vertex index
                    s.split('/').next()?.parse::<u32>().ok().map(|i| i - 1) // 1-based to 0-based
                })
                .collect();

            // Fan triangulation for n-gons
            if verts.len() >= 3 {
                for i in 1..verts.len() - 1 {
                    face_indices.push([verts[0], verts[i] as u32, verts[i + 1] as u32]);
                    face_colors.push(current_color);
                }
            }
        }
    }

    if !has_materials {
        face_colors.clear();
    }

    Ok(MeshWithColors {
        positions,
        face_indices,
        face_colors,
    })
}

/// Parse an MTL file, returning material name → RGBA color
fn parse_mtl(path: &Path) -> Result<HashMap<String, [f32; 4]>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let mut materials = HashMap::new();
    let mut current_name: Option<String> = None;
    let mut current_color: [f32; 4] = [0.85, 0.85, 0.85, 1.0];

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(name) = line.strip_prefix("newmtl ") {
            // Save previous material
            if let Some(prev_name) = current_name.take() {
                materials.insert(prev_name, current_color);
            }
            current_name = Some(name.trim().to_string());
            current_color = [0.85, 0.85, 0.85, 1.0]; // reset
        } else if let Some(kd) = line.strip_prefix("Kd ") {
            let parts: Vec<f32> = kd
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();
            if parts.len() >= 3 {
                current_color = [parts[0], parts[1], parts[2], 1.0];
            }
        } else if let Some(d_val) = line.strip_prefix("d ") {
            if let Ok(alpha) = d_val.trim().parse::<f32>() {
                current_color[3] = alpha;
            }
        }
    }

    // Save last material
    if let Some(name) = current_name {
        materials.insert(name, current_color);
    }

    Ok(materials)
}

// --- GLB/glTF loading ---

/// Load GLB/glTF mesh with material colors
fn load_glb_with_colors(
    path: &PathBuf,
    mesh_name: Option<&str>,
) -> Result<MeshWithColors, Box<dyn std::error::Error>> {
    let (document, buffers, _images) = gltf::import(path)?;

    let meshes: Vec<_> = document.meshes().collect();
    if meshes.is_empty() {
        return Err("GLB file contains no meshes".into());
    }

    let selected_mesh = if meshes.len() == 1 {
        &meshes[0]
    } else {
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

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut face_indices: Vec<[u32; 3]> = Vec::new();
    let mut face_colors: Vec<[f32; 4]> = Vec::new();
    let mut vertex_offset = 0u32;
    let mut has_materials = false;

    for primitive in selected_mesh.primitives() {
        let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

        let prim_positions = reader
            .read_positions()
            .ok_or("Primitive has no position data")?;
        let pos_vec: Vec<[f32; 3]> = prim_positions.collect();
        positions.extend_from_slice(&pos_vec);

        // Get material color for this primitive
        let material = primitive.material();
        let pbr = material.pbr_metallic_roughness();
        let base_color_factor = pbr.base_color_factor(); // [f32; 4]

        // Check if this is a non-default material with a distinct color
        if base_color_factor != [1.0, 1.0, 1.0, 1.0] || material.name().is_some() {
            has_materials = true;
        }

        // Get indices and build face list
        let idx_list: Vec<u32> = if let Some(indices) = reader.read_indices() {
            indices.into_u32().collect()
        } else {
            (0..pos_vec.len() as u32).collect()
        };

        // Build triangles
        for chunk in idx_list.chunks(3) {
            if chunk.len() == 3 {
                face_indices.push([
                    chunk[0] + vertex_offset,
                    chunk[1] + vertex_offset,
                    chunk[2] + vertex_offset,
                ]);
                face_colors.push(base_color_factor);
            }
        }

        vertex_offset += pos_vec.len() as u32;
    }

    if !has_materials {
        face_colors.clear();
    }

    Ok(MeshWithColors {
        positions,
        face_indices,
        face_colors,
    })
}

/// Load mesh from GLB/glTF file (CornerTableF only, for processing)
pub fn load_mesh_from_glb(
    path: &PathBuf,
    mesh_name: Option<&str>,
) -> Result<CornerTableF, Box<dyn std::error::Error>> {
    let mesh_data = load_glb_with_colors(path, mesh_name)?;
    mesh_data.to_corner_table()
}
