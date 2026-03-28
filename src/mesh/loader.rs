use baby_shark::io::{Builder, IndexedBuilder};
use baby_shark::mesh::corner_table::CornerTableF;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::animation::{self, AnimationClip};
use super::skeleton::{Joint, JointTransform, Skeleton};

/// Embedded texture image data
pub struct TextureData {
    pub pixels: Vec<u8>,  // RGBA8
    pub width: u32,
    pub height: u32,
}

/// Mesh geometry with optional per-face material colors and textures
pub struct MeshWithColors {
    /// Vertex positions
    pub positions: Vec<[f32; 3]>,
    /// Triangle face indices (0-based, 3 per face)
    pub face_indices: Vec<[u32; 3]>,
    /// Per-face RGBA colors (empty if no materials found)
    pub face_colors: Vec<[f32; 4]>,
    /// Per-vertex UV coordinates (empty if no UVs)
    pub texcoords: Vec<[f32; 2]>,
    /// Embedded texture (first baseColorTexture found, if any)
    pub texture: Option<TextureData>,
    /// Skeleton extracted from glTF skin data (None if no skin)
    pub skeleton: Option<Skeleton>,
    /// Per-vertex joint indices for skeletal animation (empty if no skin data).
    /// Indices refer to positions in `skeleton.joints`, NOT glTF node indices.
    pub joint_indices: Vec<[u16; 4]>,
    /// Per-vertex joint weights for skeletal animation (empty if no skin data)
    pub joint_weights: Vec<[f32; 4]>,
    /// Animation clips extracted from glTF (empty if no animations)
    pub animations: Vec<AnimationClip>,
}

impl MeshWithColors {
    /// Convert Z-up coordinates to Y-up by swapping Y and Z
    pub fn convert_z_up_to_y_up(&mut self) {
        for pos in &mut self.positions {
            let y = pos[1];
            pos[1] = pos[2];
            pos[2] = -y;
        }
    }

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
        "3mf" => load_3mf_with_colors(input),
        _ => Err(format!("Unsupported file format: {}", extension).into()),
    }
}

/// Load mesh from file (supports .obj, .glb, .3mf) — returns CornerTableF for processing
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
        "3mf" => {
            let mesh_data = load_3mf_with_colors(input)?;
            mesh_data.to_corner_table()
        }
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
                    face_indices.push([verts[0], verts[i], verts[i + 1]]);
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
        texcoords: Vec::new(),
        texture: None,
        skeleton: None,
        joint_indices: Vec::new(),
        joint_weights: Vec::new(),
        animations: Vec::new(),
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
        } else if let Some(d_val) = line.strip_prefix("d ")
            && let Ok(alpha) = d_val.trim().parse::<f32>() {
                current_color[3] = alpha;
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
    let (document, buffers, images) = gltf::import(path)?;

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
    let mut texcoords: Vec<[f32; 2]> = Vec::new();
    let mut face_indices: Vec<[u32; 3]> = Vec::new();
    let mut face_colors: Vec<[f32; 4]> = Vec::new();
    let mut vertex_offset = 0u32;
    let mut first_color: Option<[f32; 4]> = None;
    let mut has_materials = false;
    let mut texture_image_index: Option<usize> = None;

    for primitive in selected_mesh.primitives() {
        let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

        let prim_positions = reader
            .read_positions()
            .ok_or("Primitive has no position data")?;
        let pos_vec: Vec<[f32; 3]> = prim_positions.collect();
        let vert_count = pos_vec.len();
        positions.extend_from_slice(&pos_vec);

        // Read UV coordinates
        if let Some(tex_coords) = reader.read_tex_coords(0) {
            let uv_vec: Vec<[f32; 2]> = tex_coords.into_f32().collect();
            texcoords.extend_from_slice(&uv_vec);
        } else {
            // No UVs for this primitive — fill with zeros
            texcoords.extend(std::iter::repeat_n([0.0f32; 2], vert_count));
        }

        // Get material color for this primitive
        let material = primitive.material();
        let pbr = material.pbr_metallic_roughness();
        let base_color_factor = pbr.base_color_factor(); // [f32; 4]

        // Detect meaningful material variation (distinct colors across primitives)
        match first_color {
            None => first_color = Some(base_color_factor),
            Some(first) if first != base_color_factor => has_materials = true,
            _ => {}
        }

        // Track first baseColorTexture
        if texture_image_index.is_none()
            && let Some(tex_info) = pbr.base_color_texture() {
                texture_image_index = Some(tex_info.texture().source().index());
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

    // If no UVs were found at all, clear the texcoords vec
    let has_uvs = texcoords.iter().any(|uv| uv[0] != 0.0 || uv[1] != 0.0);
    if !has_uvs {
        texcoords.clear();
    }

    // Load texture image data
    let texture = texture_image_index.and_then(|idx| {
        images.get(idx).map(|img| {
            let rgba_pixels = match img.format {
                gltf::image::Format::R8G8B8A8 => img.pixels.clone(),
                gltf::image::Format::R8G8B8 => {
                    // Convert RGB to RGBA
                    let mut rgba = Vec::with_capacity(img.pixels.len() / 3 * 4);
                    for chunk in img.pixels.chunks(3) {
                        rgba.extend_from_slice(chunk);
                        rgba.push(255);
                    }
                    rgba
                }
                _ => {
                    eprintln!("Warning: unsupported texture format {:?}, skipping", img.format);
                    return TextureData { pixels: Vec::new(), width: 0, height: 0 };
                }
            };
            TextureData {
                pixels: rgba_pixels,
                width: img.width,
                height: img.height,
            }
        })
    }).filter(|t| !t.pixels.is_empty());

    // --- Skin data extraction ---
    // Find the node that binds the selected mesh to a skin
    let skin_node = document.nodes().find(|node| {
        node.mesh().is_some_and(|m| m.index() == selected_mesh.index()) && node.skin().is_some()
    });

    let (skeleton, joint_indices, joint_weights) = if let Some(node) = skin_node {
        let skin = node.skin().unwrap();
        extract_skin_data(&skin, selected_mesh, &document, &buffers)?
    } else {
        (None, Vec::new(), Vec::new())
    };

    // Extract animation clips, mapping node indices to joint indices via the skeleton
    let joint_node_indices: Option<Vec<usize>> = skeleton.as_ref().map(|s| {
        s.joints.iter().map(|j| j.node_index).collect()
    });
    let animations = animation::extract_animations(
        &document,
        &buffers,
        joint_node_indices.as_deref(),
    );

    if !animations.is_empty() {
        println!(
            "Loaded {} animation clip(s): {}",
            animations.len(),
            animations
                .iter()
                .map(|a| a.name.as_deref().unwrap_or("<unnamed>"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    if let Some(ref skel) = skeleton {
        println!(
            "Loaded skeleton with {} joints, {} skinned vertices",
            skel.joints.len(),
            joint_indices.len(),
        );
    }

    Ok(MeshWithColors {
        positions,
        face_indices,
        face_colors,
        texcoords,
        texture,
        skeleton,
        joint_indices,
        joint_weights,
        animations,
    })
}

/// Extract skeleton, per-vertex joint indices, and per-vertex weights from a glTF skin.
fn extract_skin_data(
    skin: &gltf::Skin<'_>,
    mesh: &gltf::Mesh<'_>,
    document: &gltf::Document,
    buffers: &[gltf::buffer::Data],
) -> Result<
    (Option<Skeleton>, Vec<[u16; 4]>, Vec<[f32; 4]>),
    Box<dyn std::error::Error>,
> {
    let get_buf = |buffer: gltf::Buffer<'_>| Some(&*buffers[buffer.index()]);

    // Build parent map: child_node_index -> parent_node_index
    let mut parent_map: HashMap<usize, usize> = HashMap::new();
    for node in document.nodes() {
        for child in node.children() {
            parent_map.insert(child.index(), node.index());
        }
    }

    // Collect joint nodes in skin-order (index in this vec == joint index)
    let joint_nodes: Vec<gltf::Node<'_>> = skin.joints().collect();

    // Map node_index -> joint_index for parent lookup
    let node_to_joint: HashMap<usize, usize> = joint_nodes
        .iter()
        .enumerate()
        .map(|(ji, node)| (node.index(), ji))
        .collect();

    // Read inverse bind matrices (identity if not provided)
    let identity: [[f32; 4]; 4] = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let skin_reader = skin.reader(get_buf);
    let ibms: Vec<[[f32; 4]; 4]> = skin_reader
        .read_inverse_bind_matrices()
        .map(|iter| iter.collect())
        .unwrap_or_else(|| vec![identity; joint_nodes.len()]);

    // Build joints
    let joints: Vec<Joint> = joint_nodes
        .iter()
        .enumerate()
        .map(|(ji, node)| {
            let parent_joint = parent_map
                .get(&node.index())
                .and_then(|parent_node_idx| node_to_joint.get(parent_node_idx))
                .copied();

            let local_transform = match node.transform() {
                gltf::scene::Transform::Decomposed {
                    translation,
                    rotation,
                    scale,
                } => JointTransform::Decomposed {
                    translation,
                    rotation,
                    scale,
                },
                gltf::scene::Transform::Matrix { matrix } => JointTransform::Matrix(matrix),
            };

            Joint {
                index: ji,
                node_index: node.index(),
                name: node.name().map(|s| s.to_string()),
                parent: parent_joint,
                inverse_bind_matrix: ibms[ji],
                local_transform,
            }
        })
        .collect();

    let skeleton = Skeleton { joints };

    // Read per-vertex JOINTS_0 and WEIGHTS_0 from all primitives (concatenated in primitive order)
    let mut all_joint_indices: Vec<[u16; 4]> = Vec::new();
    let mut all_joint_weights: Vec<[f32; 4]> = Vec::new();

    for primitive in mesh.primitives() {
        let reader = primitive.reader(get_buf);
        let vert_count = reader.read_positions().map(|p| p.count()).unwrap_or(0);

        // Need a fresh reader for joints (the previous one consumed positions)
        let reader = primitive.reader(get_buf);
        if let Some(joints_reader) = reader.read_joints(0) {
            all_joint_indices.extend(joints_reader.into_u16());
        } else {
            // No skin data on this primitive — fill with zeros
            all_joint_indices.extend(std::iter::repeat_n([0u16; 4], vert_count));
        }

        let reader = primitive.reader(get_buf);
        if let Some(weights_reader) = reader.read_weights(0) {
            all_joint_weights.extend(weights_reader.into_f32());
        } else {
            all_joint_weights.extend(std::iter::repeat_n([0.0f32; 4], vert_count));
        }
    }

    Ok((Some(skeleton), all_joint_indices, all_joint_weights))
}

// --- 3MF loading ---

/// Parse a #RRGGBB or #RRGGBBAA hex color string to [f32; 4]
fn parse_hex_color(s: &str) -> Option<[f32; 4]> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 && s.len() != 8 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()? as f32 / 255.0;
    let g = u8::from_str_radix(&s[2..4], 16).ok()? as f32 / 255.0;
    let b = u8::from_str_radix(&s[4..6], 16).ok()? as f32 / 255.0;
    let a = if s.len() == 8 {
        u8::from_str_radix(&s[6..8], 16).ok()? as f32 / 255.0
    } else {
        1.0
    };
    Some([r, g, b, a])
}

/// Load a .3mf file with per-triangle colors
fn load_3mf_with_colors(path: &PathBuf) -> Result<MeshWithColors, Box<dyn std::error::Error>> {
    use quick_xml::events::Event;
    use quick_xml::Reader;
    use std::io::Read;

    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    // Find the model file (typically 3D/3dmodel.model)
    let model_name = (0..archive.len())
        .filter_map(|i| {
            let entry = archive.by_index(i).ok()?;
            let name = entry.name().to_string();
            if name.ends_with(".model") {
                Some(name)
            } else {
                None
            }
        })
        .next()
        .ok_or("No .model file found in 3MF archive")?;

    let mut model_file = archive.by_name(&model_name)?;
    let mut xml_content = String::new();
    model_file.read_to_string(&mut xml_content)?;

    // Parse XML
    let mut reader = Reader::from_str(&xml_content);

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut face_indices: Vec<[u32; 3]> = Vec::new();
    let mut face_colors: Vec<[f32; 4]> = Vec::new();
    let mut has_colors = false;

    // Color groups: group_id -> Vec<[f32; 4]> (indexed by position in group)
    let mut color_groups: HashMap<String, Vec<[f32; 4]>> = HashMap::new();
    let mut current_color_group_id: Option<String> = None;

    // Object-level default: pid (color group id) + pindex (index within group)
    let mut default_pid: Option<String> = None;
    let mut default_pindex: Option<usize> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                let ln = e.local_name();
                let local_name = std::str::from_utf8(ln.as_ref()).unwrap_or("");
                match local_name {
                    "vertex" => {
                        let mut x = 0.0f32;
                        let mut y = 0.0f32;
                        let mut z = 0.0f32;
                        for attr in e.attributes().flatten() {
                            let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                            let val = std::str::from_utf8(&attr.value).unwrap_or("");
                            match key {
                                "x" => x = val.parse().unwrap_or(0.0),
                                "y" => y = val.parse().unwrap_or(0.0),
                                "z" => z = val.parse().unwrap_or(0.0),
                                _ => {}
                            }
                        }
                        positions.push([x, y, z]);
                    }
                    "triangle" => {
                        let mut v1 = 0u32;
                        let mut v2 = 0u32;
                        let mut v3 = 0u32;
                        let mut pid: Option<String> = None;
                        let mut pindex: Option<usize> = None;
                        for attr in e.attributes().flatten() {
                            let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                            let val = std::str::from_utf8(&attr.value).unwrap_or("");
                            match key {
                                "v1" => v1 = val.parse().unwrap_or(0),
                                "v2" => v2 = val.parse().unwrap_or(0),
                                "v3" => v3 = val.parse().unwrap_or(0),
                                "pid" => pid = Some(val.to_string()),
                                "p1" => pindex = val.parse().ok(),
                                _ => {}
                            }
                        }
                        face_indices.push([v1, v2, v3]);

                        // Resolve color: triangle-level pid/p1, or object-level default
                        let use_pid = pid.as_ref().or(default_pid.as_ref());
                        let use_pindex = pindex.or(default_pindex);

                        let color = use_pid
                            .and_then(|pid| color_groups.get(pid))
                            .and_then(|group| {
                                use_pindex.and_then(|idx| group.get(idx).copied())
                            });

                        if let Some(c) = color {
                            has_colors = true;
                            face_colors.push(c);
                        } else {
                            face_colors.push([0.85, 0.85, 0.85, 1.0]);
                        }
                    }
                    "colorgroup" | "m:colorgroup" => {
                        for attr in e.attributes().flatten() {
                            let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                            if key == "id" {
                                let val = std::str::from_utf8(&attr.value).unwrap_or("");
                                current_color_group_id = Some(val.to_string());
                                color_groups.entry(val.to_string()).or_default();
                            }
                        }
                    }
                    "color" | "m:color" => {
                        if let Some(ref group_id) = current_color_group_id {
                            for attr in e.attributes().flatten() {
                                let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                                if key == "color" {
                                    let val = std::str::from_utf8(&attr.value).unwrap_or("");
                                    if let Some(rgba) = parse_hex_color(val) {
                                        color_groups
                                            .get_mut(group_id)
                                            .unwrap()
                                            .push(rgba);
                                    }
                                }
                            }
                        }
                    }
                    "object" => {
                        // Check for default pid/pindex on the object element
                        for attr in e.attributes().flatten() {
                            let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                            let val = std::str::from_utf8(&attr.value).unwrap_or("");
                            match key {
                                "pid" => default_pid = Some(val.to_string()),
                                "pindex" => default_pindex = val.parse().ok(),
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let ln = e.local_name();
                let local_name = std::str::from_utf8(ln.as_ref()).unwrap_or("");
                if local_name == "colorgroup" || local_name == "m:colorgroup" {
                    current_color_group_id = None;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {}", e).into()),
            _ => {}
        }
    }

    if !has_colors {
        face_colors.clear();
    }

    Ok(MeshWithColors {
        positions,
        face_indices,
        face_colors,
        texcoords: Vec::new(),
        texture: None,
        skeleton: None,
        joint_indices: Vec::new(),
        joint_weights: Vec::new(),
        animations: Vec::new(),
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
