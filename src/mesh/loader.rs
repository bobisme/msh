use baby_shark::io::{Builder, IndexedBuilder};
use baby_shark::mesh::corner_table::CornerTableF;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::animation::{self, AnimationClip};
use super::skeleton::{Joint, JointTransform, Skeleton};

/// Embedded texture image data
pub struct TextureData {
    pub pixels: Vec<u8>, // RGBA8
    pub width: u32,
    pub height: u32,
}

/// glTF alpha mode
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AlphaMode {
    #[default]
    Opaque,
    Mask,
    Blend,
}

/// A glTF metallic-roughness material. Texture fields index into `MeshMaterials::textures`.
#[derive(Clone, Debug)]
pub struct MaterialData {
    pub name: Option<String>,
    pub base_color_factor: [f32; 4],
    pub base_color_texture: Option<usize>,
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    /// Roughness in G, metalness in B
    pub metallic_roughness_texture: Option<usize>,
    pub normal_texture: Option<usize>,
    pub normal_scale: f32,
    /// Occlusion in R (often the same image as metallic_roughness_texture, i.e. ORM)
    pub occlusion_texture: Option<usize>,
    pub occlusion_strength: f32,
    /// Two affine rows per texture: base color, normal, metallic/roughness, occlusion.
    pub uv_transforms: [[f32; 4]; 8],
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub double_sided: bool,
}

impl Default for MaterialData {
    /// glTF spec defaults
    fn default() -> Self {
        Self {
            name: None,
            base_color_factor: [1.0; 4],
            base_color_texture: None,
            metallic_factor: 1.0,
            roughness_factor: 1.0,
            metallic_roughness_texture: None,
            normal_texture: None,
            normal_scale: 1.0,
            occlusion_texture: None,
            occlusion_strength: 1.0,
            uv_transforms: identity_uv_transforms(),
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            double_sided: false,
        }
    }
}

/// Materials and the textures they reference
#[derive(Default)]
pub struct MeshMaterials {
    pub materials: Vec<MaterialData>,
    pub textures: Vec<TextureData>,
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
    /// Per-vertex normals (empty if none; zero for vertices of primitives without normals)
    pub normals: Vec<[f32; 3]>,
    /// Per-vertex tangents, w = bitangent sign (empty if none; zero where missing)
    pub tangents: Vec<[f32; 4]>,
    /// glTF materials used by this mesh
    pub materials: MeshMaterials,
    /// Per-face index into `materials.materials` (empty if the mesh has no glTF materials;
    /// `NO_MATERIAL` for faces of primitives without a material)
    pub face_materials: Vec<u32>,
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

/// `face_materials` value for faces that use the viewer's default material
pub const NO_MATERIAL: u32 = u32::MAX;

impl MeshWithColors {
    /// Convert Z-up coordinates to Y-up by swapping Y and Z
    pub fn convert_z_up_to_y_up(&mut self) {
        for pos in &mut self.positions {
            let y = pos[1];
            pos[1] = pos[2];
            pos[2] = -y;
        }
        for n in &mut self.normals {
            let y = n[1];
            n[1] = n[2];
            n[2] = -y;
        }
        for t in &mut self.tangents {
            let y = t[1];
            t[1] = t[2];
            t[2] = -y;
        }
    }

    /// Actual bind-pose positions in scene space. Quantized skins can encode their
    /// dequantization in inverse binds rather than in mesh-node transforms.
    pub fn bind_positions(&self) -> Vec<[f32; 3]> {
        let matrices = self.skeleton.as_ref().map(|s| s.compute_joint_matrices());
        self.positions
            .iter()
            .enumerate()
            .map(|(index, &position)| {
                if let Some(matrices) = &matrices
                    && self
                        .joint_weights
                        .get(index)
                        .is_some_and(|weights| weights.iter().any(|&w| w > 0.0))
                {
                    let mut p = nalgebra::Vector4::zeros();
                    for (&joint, &weight) in self.joint_indices[index]
                        .iter()
                        .zip(&self.joint_weights[index])
                    {
                        if weight > 0.0 {
                            p += nalgebra::Matrix4::from(matrices[joint as usize])
                                * nalgebra::Vector4::new(
                                    position[0],
                                    position[1],
                                    position[2],
                                    1.0,
                                )
                                * weight;
                        }
                    }
                    [p.x, p.y, p.z]
                } else {
                    position
                }
            })
            .collect()
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
        normals: Vec::new(),
        tangents: Vec::new(),
        materials: MeshMaterials::default(),
        face_materials: Vec::new(),
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
            && let Ok(alpha) = d_val.trim().parse::<f32>()
        {
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
    let (document, buffers, images) = super::gltf_import::import(path)?;

    let meshes: Vec<_> = document.meshes().collect();
    if meshes.is_empty() {
        return Err("GLB file contains no meshes".into());
    }

    let all = mesh_name == Some("all");
    let selected_index = if all {
        None
    } else if meshes.len() == 1 {
        Some(meshes[0].index())
    } else {
        let name = mesh_name.ok_or_else(|| {
            format!(
                "GLB contains {} meshes; select --mesh <name> or --mesh all",
                meshes.len()
            )
        })?;
        Some(
            meshes
                .iter()
                .find(|m| m.name() == Some(name) || name == format!("#{}", m.index()))
                .ok_or_else(|| format!("Mesh '{name}' not found"))?
                .index(),
        )
    };
    let (world, _) = super::scene_graph::world_transforms(&document)?;
    let nodes: Vec<_> = document.nodes().collect();
    let mut active = vec![false; nodes.len()];
    let mut pending: Vec<_> = document
        .default_scene()
        .or_else(|| document.scenes().next())
        .map(|s| s.nodes().map(|n| n.index()).collect())
        .unwrap_or_default();
    while let Some(index) = pending.pop() {
        if active[index] {
            continue;
        }
        active[index] = true;
        pending.extend(nodes[index].children().map(|n| n.index()));
    }
    let mut selected = Vec::new();
    for node in document.nodes() {
        if let Some(mesh) = node.mesh()
            && (active[node.index()] || !all)
            && selected_index.is_none_or(|index| mesh.index() == index)
        {
            selected.push((mesh, node.skin(), world[node.index()]));
            if !all {
                break;
            }
        }
    }
    if selected.is_empty() {
        for mesh in &meshes {
            if selected_index.is_none_or(|index| mesh.index() == index) {
                selected.push((mesh.clone(), None, nalgebra::Matrix4::identity()));
            }
        }
    }
    // Bound expansion before instancing or skin arrays allocate: tiny source buffers
    // can otherwise be referenced by arbitrarily many scene nodes.
    let mut expanded = 0usize;
    for (mesh, _, _) in &selected {
        for primitive in mesh.primitives() {
            let vertices = primitive
                .get(&gltf::Semantic::Positions)
                .map_or(0, |a| a.count());
            let indices = primitive.indices().map_or(vertices, |a| a.count());
            let bytes = vertices
                .checked_mul(72)
                .and_then(|v| indices.checked_mul(8).and_then(|i| v.checked_add(i)))
                .ok_or("Instanced geometry size overflow")?;
            expanded = expanded
                .checked_add(bytes)
                .ok_or("Instanced geometry size overflow")?;
            if expanded > 512 * 1024 * 1024 {
                return Err("Instanced mesh geometry exceeds 512 MiB budget".into());
            }
            if primitive.attributes().any(|(_, a)| a.count() != vertices) {
                return Err("Primitive attribute counts differ from positions".into());
            }
        }
    }
    let mut skin_cache = HashMap::new();
    let mut skeleton = None;
    let mut joint_indices = Vec::new();
    let mut joint_weights = Vec::new();
    let mut skin_identity: Option<HashMap<usize, (usize, [[f32; 4]; 4])>> = None;
    let mut skin_remaps: HashMap<usize, Vec<u16>> = HashMap::new();
    // Shared joint nodes and inverse binds can use one palette. Skin-order
    // differences are remapped; genuinely different rigs are rejected.
    for (mesh, skin, _) in &selected {
        if let Some(skin) = skin {
            let key = (mesh.index(), skin.index());
            if !skin_cache.contains_key(&key) {
                let (skeleton, indices, weights) =
                    extract_skin_data(skin, mesh, &document, &buffers)?;
                skin_cache.insert(key, (skeleton.unwrap(), indices, weights));
            }
            let (candidate, indices, weights) = &skin_cache[&key];
            if skeleton.is_none() {
                skin_identity = Some(
                    candidate
                        .joints
                        .iter()
                        .map(|j| (j.node_index, (j.index, j.inverse_bind_matrix)))
                        .collect(),
                );
                skeleton = Some(candidate.clone());
            }
            if !skin_remaps.contains_key(&skin.index()) {
                let reference = skin_identity.as_ref().unwrap();
                if candidate.joints.len() != reference.len() {
                    return Err(
                        "--mesh all requires skins with shared joint nodes and inverse binds"
                            .into(),
                    );
                }
                let remap: Result<Vec<u16>, _> = candidate
                    .joints
                    .iter()
                    .map(|joint| {
                        reference
                            .get(&joint.node_index)
                            .filter(|(_, matrix)| matrix == &joint.inverse_bind_matrix)
                            .map(|(index, _)| *index as u16)
                            .ok_or("--mesh all cannot combine different rigs or inverse binds")
                    })
                    .collect();
                skin_remaps.insert(skin.index(), remap?);
            }
            let remap = &skin_remaps[&skin.index()];
            joint_indices.extend(
                indices
                    .iter()
                    .map(|j| j.map(|index| remap.get(index as usize).copied().unwrap_or(0))),
            );
            joint_weights.extend_from_slice(weights);
        } else {
            let count: usize = mesh
                .primitives()
                .map(|p| p.get(&gltf::Semantic::Positions).map_or(0, |a| a.count()))
                .sum();
            joint_indices.extend(std::iter::repeat_n([0u16; 4], count));
            joint_weights.extend(std::iter::repeat_n([0.0f32; 4], count));
        }
    }
    if skeleton.is_none() {
        joint_indices.clear();
        joint_weights.clear();
    }

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut texcoords: Vec<[f32; 2]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut tangents: Vec<[f32; 4]> = Vec::new();
    let mut face_indices: Vec<[u32; 3]> = Vec::new();
    let mut face_materials: Vec<u32> = Vec::new();
    let mut vertex_offset = 0u32;
    let mut has_normals = false;
    let mut has_tangents = false;

    // Only the materials (and their images) used by the selected mesh are loaded
    let mut materials = MeshMaterials::default();
    let mut material_map: HashMap<usize, u32> = HashMap::new();
    let mut texture_map: HashMap<usize, usize> = HashMap::new();

    for (selected_mesh, skin, transform) in &selected {
        let linear = transform.fixed_view::<3, 3>(0, 0).into_owned();
        let normal_matrix = linear
            .try_inverse()
            .map(|m| m.transpose())
            .unwrap_or_else(nalgebra::Matrix3::zeros);
        let reflected = skin.is_none() && linear.determinant() < 0.0;
        for primitive in selected_mesh.primitives() {
            if primitive.mode() != gltf::mesh::Mode::Triangles {
                return Err("Only triangle primitives are supported".into());
            }
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

            let prim_positions = reader
                .read_positions()
                .ok_or("Primitive has no position data")?;
            let pos_vec: Vec<[f32; 3]> = prim_positions.collect();
            let vert_count = pos_vec.len();
            if skin.is_some() {
                positions.extend_from_slice(&pos_vec);
            } else {
                positions.extend(pos_vec.iter().map(|p| -> [f32; 3] {
                    transform
                        .transform_point(&nalgebra::Point3::from(*p))
                        .coords
                        .into()
                }));
            }

            // Read UV coordinates
            if let Some(tex_coords) = reader.read_tex_coords(0) {
                let uv_vec: Vec<[f32; 2]> = tex_coords.into_f32().collect();
                texcoords.extend_from_slice(&uv_vec);
            } else {
                // No UVs for this primitive — fill with zeros
                texcoords.extend(std::iter::repeat_n([0.0f32; 2], vert_count));
            }

            if let Some(prim_normals) = reader.read_normals() {
                has_normals = true;
                normals.extend(prim_normals.map(|normal| -> [f32; 3] {
                    if skin.is_some() {
                        normal
                    } else {
                        (normal_matrix * nalgebra::Vector3::from(normal)).into()
                    }
                }));
            } else {
                normals.extend(std::iter::repeat_n([0.0f32; 3], vert_count));
            }

            if let Some(prim_tangents) = reader.read_tangents() {
                has_tangents = true;
                tangents.extend(prim_tangents.map(|tangent| {
                    if skin.is_some() {
                        tangent
                    } else {
                        let t = linear * nalgebra::Vector3::new(tangent[0], tangent[1], tangent[2]);
                        [
                            t.x,
                            t.y,
                            t.z,
                            tangent[3] * if reflected { -1.0 } else { 1.0 },
                        ]
                    }
                }));
            } else {
                tangents.extend(std::iter::repeat_n([0.0f32; 4], vert_count));
            }

            let material = primitive.material();
            let material_idx = match material.index() {
                None => NO_MATERIAL,
                Some(gltf_idx) => {
                    if let Some(&index) = material_map.get(&gltf_idx) {
                        index
                    } else {
                        let mut data = convert_material(
                            &material,
                            &images,
                            &mut materials.textures,
                            &mut texture_map,
                        );
                        let json = serde_json::to_value(&document.as_json().materials[gltf_idx])?;
                        data.uv_transforms = material_uv_transforms(&json)?;
                        let index = materials.materials.len() as u32;
                        materials.materials.push(data);
                        material_map.insert(gltf_idx, index);
                        index
                    }
                }
            };

            // Get indices and build face list
            let idx_list: Vec<u32> = if let Some(indices) = reader.read_indices() {
                indices.into_u32().collect()
            } else {
                (0..pos_vec.len() as u32).collect()
            };

            if idx_list.len() % 3 != 0 || idx_list.iter().any(|&index| index as usize >= vert_count)
            {
                return Err("Invalid triangle indices".into());
            }
            // Build triangles
            for chunk in idx_list.chunks(3) {
                if chunk.len() == 3 {
                    face_indices.push([
                        chunk[0] + vertex_offset,
                        chunk[if reflected { 2 } else { 1 }] + vertex_offset,
                        chunk[if reflected { 1 } else { 2 }] + vertex_offset,
                    ]);
                    face_materials.push(material_idx);
                }
            }

            vertex_offset = vertex_offset
                .checked_add(u32::try_from(pos_vec.len())?)
                .ok_or("Combined vertex count exceeds u32")?;
        }
    }

    // If no UVs were found at all, clear the texcoords vec
    let has_uvs = texcoords.iter().any(|uv| uv[0] != 0.0 || uv[1] != 0.0);
    if !has_uvs {
        texcoords.clear();
    }
    if !has_normals {
        normals.clear();
    }
    if !has_tangents {
        tangents.clear();
    }
    if materials.materials.is_empty() {
        face_materials.clear();
    }

    // Extract animation clips, mapping node indices to joint indices via the skeleton
    let joint_node_indices: Option<Vec<usize>> = skeleton
        .as_ref()
        .map(|s| s.joints.iter().map(|j| j.node_index).collect());
    let animations =
        animation::extract_animations(&document, &buffers, joint_node_indices.as_deref());

    if !animations.is_empty() {
        eprintln!(
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
        eprintln!(
            "Loaded skeleton with {} joints, {} skinned vertices",
            skel.joints.len(),
            joint_indices.len(),
        );
    }

    Ok(MeshWithColors {
        positions,
        face_indices,
        face_colors: Vec::new(),
        texcoords,
        normals,
        tangents,
        materials,
        face_materials,
        skeleton,
        joint_indices,
        joint_weights,
        animations,
    })
}

pub(crate) fn identity_uv_transforms() -> [[f32; 4]; 8] {
    [
        [1., 0., 0., 0.],
        [0., 1., 0., 0.],
        [1., 0., 0., 0.],
        [0., 1., 0., 0.],
        [1., 0., 0., 0.],
        [0., 1., 0., 0.],
        [1., 0., 0., 0.],
        [0., 1., 0., 0.],
    ]
}
fn material_uv_transforms(
    material: &serde_json::Value,
) -> Result<[[f32; 4]; 8], Box<dyn std::error::Error>> {
    let infos = [
        &material["pbrMetallicRoughness"]["baseColorTexture"],
        &material["normalTexture"],
        &material["pbrMetallicRoughness"]["metallicRoughnessTexture"],
        &material["occlusionTexture"],
    ];
    let mut rows = identity_uv_transforms();
    for (slot, info) in infos.into_iter().enumerate() {
        if info.is_null() {
            continue;
        }
        let transform = &info["extensions"]["KHR_texture_transform"];
        if transform
            .get("texCoord")
            .or_else(|| info.get("texCoord"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
            != 0
        {
            return Err("This viewer currently supports TEXCOORD_0 textures only".into());
        }
        let pair = |key: &str, default: [f32; 2]| -> Result<[f32; 2], Box<dyn std::error::Error>> {
            if transform.get(key).is_none() {
                return Ok(default);
            }
            let a = transform[key]
                .as_array()
                .filter(|a| a.len() == 2)
                .ok_or("Invalid texture transform vector")?;
            Ok([
                a[0].as_f64().ok_or("Invalid texture transform component")? as f32,
                a[1].as_f64().ok_or("Invalid texture transform component")? as f32,
            ])
        };
        let offset = pair("offset", [0., 0.])?;
        let scale = pair("scale", [1., 1.])?;
        let rotation = transform
            .get("rotation")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0) as f32;
        let (sin, cos) = rotation.sin_cos();
        rows[slot * 2] = [cos * scale[0], -sin * scale[1], offset[0], 0.];
        rows[slot * 2 + 1] = [sin * scale[0], cos * scale[1], offset[1], 0.];
    }
    if rows.iter().flatten().any(|v| !v.is_finite()) {
        return Err("Nonfinite texture transform".into());
    }
    Ok(rows)
}

/// Convert a glTF material, decoding any images it references into `textures`
/// (deduplicated by glTF image index via `texture_map`).
fn convert_material(
    material: &gltf::Material<'_>,
    images: &[gltf::image::Data],
    textures: &mut Vec<TextureData>,
    texture_map: &mut HashMap<usize, usize>,
) -> MaterialData {
    let mut load = |texture: gltf::Texture<'_>| -> Option<usize> {
        let image_idx = texture.source().index();
        if let Some(&idx) = texture_map.get(&image_idx) {
            return Some(idx);
        }
        let data = convert_image(images.get(image_idx)?)?;
        textures.push(data);
        texture_map.insert(image_idx, textures.len() - 1);
        Some(textures.len() - 1)
    };

    let pbr = material.pbr_metallic_roughness();
    let normal = material.normal_texture();
    let occlusion = material.occlusion_texture();
    MaterialData {
        name: material.name().map(String::from),
        uv_transforms: identity_uv_transforms(),
        base_color_factor: pbr.base_color_factor(),
        base_color_texture: pbr.base_color_texture().and_then(|t| load(t.texture())),
        metallic_factor: pbr.metallic_factor(),
        roughness_factor: pbr.roughness_factor(),
        metallic_roughness_texture: pbr
            .metallic_roughness_texture()
            .and_then(|t| load(t.texture())),
        normal_scale: normal.as_ref().map_or(1.0, |n| n.scale()),
        normal_texture: normal.and_then(|n| load(n.texture())),
        occlusion_strength: occlusion.as_ref().map_or(1.0, |o| o.strength()),
        occlusion_texture: occlusion.and_then(|o| load(o.texture())),
        alpha_mode: match material.alpha_mode() {
            gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
            gltf::material::AlphaMode::Mask => AlphaMode::Mask,
            gltf::material::AlphaMode::Blend => AlphaMode::Blend,
        },
        alpha_cutoff: material.alpha_cutoff().unwrap_or(0.5),
        double_sided: material.double_sided(),
    }
}

/// Convert decoded glTF image data to RGBA8
fn convert_image(img: &gltf::image::Data) -> Option<TextureData> {
    use gltf::image::Format;
    let pixels = match img.format {
        Format::R8G8B8A8 => img.pixels.clone(),
        Format::R8G8B8 => img
            .pixels
            .chunks(3)
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        Format::R8G8 => img
            .pixels
            .chunks(2)
            .flat_map(|c| [c[0], c[1], 0, 255])
            .collect(),
        Format::R8 => img.pixels.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        _ => {
            eprintln!(
                "Warning: unsupported texture format {:?}, skipping",
                img.format
            );
            return None;
        }
    };
    Some(TextureData {
        pixels,
        width: img.width,
        height: img.height,
    })
}

pub(crate) fn extract_skeleton(
    skin: &gltf::Skin<'_>,
    document: &gltf::Document,
    buffers: &[gltf::buffer::Data],
) -> Result<Skeleton, Box<dyn std::error::Error>> {
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

    if joint_nodes.is_empty() || joint_nodes.len() > 256 || ibms.len() < joint_nodes.len() {
        return Err("Skin requires 1..256 joints and enough inverse bind matrices".into());
    }
    let nodes: Vec<_> = document.nodes().collect();
    super::scene_graph::world_transforms(document)?;
    // Build joints
    let joints: Vec<Joint> = joint_nodes
        .iter()
        .enumerate()
        .map(|(ji, node)| {
            let mut parent_joint = None;
            let mut parent_transform = nalgebra::Matrix4::identity();
            let mut cursor = parent_map.get(&node.index()).copied();
            while let Some(parent) = cursor {
                if let Some(&joint) = node_to_joint.get(&parent) {
                    parent_joint = Some(joint);
                    break;
                }
                parent_transform =
                    nalgebra::Matrix4::from(nodes[parent].transform().matrix()) * parent_transform;
                cursor = parent_map.get(&parent).copied();
            }

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
                parent_transform: parent_transform.into(),
                inverse_bind_matrix: ibms[ji],
                local_transform,
            }
        })
        .collect();

    Ok(Skeleton { joints })
}

/// Extract skeleton, per-vertex joint indices, and per-vertex weights from a glTF skin.
fn extract_skin_data(
    skin: &gltf::Skin<'_>,
    mesh: &gltf::Mesh<'_>,
    document: &gltf::Document,
    buffers: &[gltf::buffer::Data],
) -> Result<(Option<Skeleton>, Vec<[u16; 4]>, Vec<[f32; 4]>), Box<dyn std::error::Error>> {
    let skeleton = extract_skeleton(skin, document, buffers)?;
    let get_buf = |buffer: gltf::Buffer<'_>| Some(&*buffers[buffer.index()]);

    // Read per-vertex JOINTS_0 and WEIGHTS_0 from all primitives (concatenated in primitive order)
    let mut all_joint_indices: Vec<[u16; 4]> = Vec::new();
    let mut all_joint_weights: Vec<[f32; 4]> = Vec::new();

    for primitive in mesh.primitives() {
        let reader = primitive.reader(get_buf);
        let vert_count = reader.read_positions().map(|p| p.count()).unwrap_or(0);
        let start_indices = all_joint_indices.len();
        let start_weights = all_joint_weights.len();

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
        if all_joint_indices.len() - start_indices != vert_count
            || all_joint_weights.len() - start_weights != vert_count
        {
            return Err("Skin attribute counts differ from positions".into());
        }
    }

    if all_joint_indices.len() != all_joint_weights.len() {
        return Err("Skin indices/weights have different lengths".into());
    }
    for (indices, weights) in all_joint_indices.iter().zip(&mut all_joint_weights) {
        if weights.iter().any(|w| !w.is_finite() || *w < 0.0) {
            return Err("Skin weights must be nonnegative and finite".into());
        }
        if indices
            .iter()
            .zip(weights.iter())
            .any(|(&i, &w)| w > 0.0 && i as usize >= skeleton.joints.len())
        {
            return Err("Skin joint index exceeds palette".into());
        }
        let sum: f32 = weights.iter().sum();
        if !sum.is_finite() {
            return Err("Skin weight sum must be finite".into());
        }
        if sum > 0.0 {
            for w in weights {
                *w /= sum;
            }
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
    use quick_xml::Reader;
    use quick_xml::events::Event;
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
                            .and_then(|group| use_pindex.and_then(|idx| group.get(idx).copied()));

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
                                        color_groups.get_mut(group_id).unwrap().push(rgba);
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
        normals: Vec::new(),
        tangents: Vec::new(),
        materials: MeshMaterials::default(),
        face_materials: Vec::new(),
        skeleton: None,
        joint_indices: Vec::new(),
        joint_weights: Vec::new(),
        animations: Vec::new(),
    })
}

/// List all mesh names in a GLB/glTF file (for mesh selection UI).
/// Returns names in document order; unnamed meshes get "<unnamed>".
pub fn list_glb_meshes(path: &PathBuf) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let (document, _, _) = super::gltf_import::import(path)?;
    Ok(document
        .meshes()
        .map(|m| {
            m.name()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("#{}", m.index()))
        })
        .collect())
}

/// Load mesh from GLB/glTF file (CornerTableF only, for processing)
pub fn load_mesh_from_glb(
    path: &PathBuf,
    mesh_name: Option<&str>,
) -> Result<CornerTableF, Box<dyn std::error::Error>> {
    let mesh_data = load_glb_with_colors(path, mesh_name)?;
    mesh_data.to_corner_table()
}

#[cfg(test)]
pub(crate) mod test_fixtures {
    use std::path::PathBuf;

    fn png_1x1(rgba: [u8; 4]) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(1, 1, image::Rgba(rgba));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    /// Write a GLB with one mesh of two primitives (one triangle each):
    /// 0 = "hull": OPAQUE, single-sided, red 1x1 baseColorTexture
    /// 1 = "sail": BLEND, double-sided, blue 1x1 baseColorTexture, baseColorFactor alpha 0.5
    pub(crate) fn two_material_glb(name: &str) -> PathBuf {
        let tri = |x: f32| -> Vec<u8> {
            [[x, 0.0, 0.0], [x + 1.0, 0.0, 0.0], [x, 1.0, 0.0]]
                .iter()
                .flatten()
                .flat_map(|v: &f32| v.to_le_bytes())
                .collect()
        };
        let uvs: Vec<u8> = [[0.0f32, 0.0], [1.0, 0.0], [0.0, 1.0]]
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let red = png_1x1([255, 0, 0, 255]);
        let blue = png_1x1([0, 0, 255, 255]);

        let mut bin = Vec::new();
        let mut views = Vec::new();
        for chunk in [tri(0.0), tri(2.0), uvs.clone(), uvs, red, blue] {
            views.push(serde_json::json!({
                "buffer": 0, "byteOffset": bin.len(), "byteLength": chunk.len()
            }));
            bin.extend_from_slice(&chunk);
            while bin.len() % 4 != 0 {
                bin.push(0);
            }
        }

        let pos_accessor = |view: usize, x: f32| {
            serde_json::json!({
                "bufferView": view, "componentType": 5126, "count": 3, "type": "VEC3",
                "min": [x, 0.0, 0.0], "max": [x + 1.0, 1.0, 0.0]
            })
        };
        let json = serde_json::json!({
            "asset": { "version": "2.0" },
            "buffers": [{ "byteLength": bin.len() }],
            "bufferViews": views,
            "accessors": [
                pos_accessor(0, 0.0),
                pos_accessor(1, 2.0),
                { "bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC2" },
                { "bufferView": 3, "componentType": 5126, "count": 3, "type": "VEC2" }
            ],
            "images": [
                { "bufferView": 4, "mimeType": "image/png" },
                { "bufferView": 5, "mimeType": "image/png" }
            ],
            "textures": [{ "source": 0 }, { "source": 1 }],
            "materials": [
                {
                    "name": "hull",
                    "pbrMetallicRoughness": { "baseColorTexture": { "index": 0 } }
                },
                {
                    "name": "sail",
                    "alphaMode": "BLEND",
                    "doubleSided": true,
                    "pbrMetallicRoughness": {
                        "baseColorFactor": [1.0, 1.0, 1.0, 0.5],
                        "baseColorTexture": { "index": 1 }
                    }
                }
            ],
            "meshes": [{
                "name": "ship",
                "primitives": [
                    { "attributes": { "POSITION": 0, "TEXCOORD_0": 2 }, "material": 0 },
                    { "attributes": { "POSITION": 1, "TEXCOORD_0": 3 }, "material": 1 }
                ]
            }],
            "nodes": [{ "mesh": 0 }],
            "scenes": [{ "nodes": [0] }],
            "scene": 0
        });

        let mut json_bytes = serde_json::to_vec(&json).unwrap();
        while json_bytes.len() % 4 != 0 {
            json_bytes.push(b' ');
        }
        let total = 12 + 8 + json_bytes.len() + 8 + bin.len();
        let mut glb = Vec::with_capacity(total);
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total as u32).to_le_bytes());
        glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"JSON");
        glb.extend_from_slice(&json_bytes);
        glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"BIN\0");
        glb.extend_from_slice(&bin);

        let path = std::env::temp_dir().join(format!("{}_{}.glb", name, std::process::id()));
        std::fs::write(&path, glb).unwrap();
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glb_primitives_keep_their_own_materials() {
        let path = test_fixtures::two_material_glb("msh_test_loader_materials");
        let mesh = load_mesh_with_colors(&path, None).unwrap();
        std::fs::remove_file(&path).ok();

        let mats = &mesh.materials.materials;
        assert_eq!(mats.len(), 2);
        assert_eq!(mesh.materials.textures.len(), 2);
        assert_eq!(mesh.face_materials, vec![0, 1]);

        let hull = &mats[0];
        assert_eq!(hull.name.as_deref(), Some("hull"));
        assert_eq!(hull.alpha_mode, AlphaMode::Opaque);
        assert!(!hull.double_sided);

        let sail = &mats[1];
        assert_eq!(sail.name.as_deref(), Some("sail"));
        assert_eq!(sail.alpha_mode, AlphaMode::Blend);
        assert!(sail.double_sided);
        assert_eq!(sail.base_color_factor, [1.0, 1.0, 1.0, 0.5]);

        // Each primitive samples its own texture, not the first one in the file
        let pixel = |m: &MaterialData| {
            let tex = &mesh.materials.textures[m.base_color_texture.unwrap()];
            [tex.pixels[0], tex.pixels[1], tex.pixels[2], tex.pixels[3]]
        };
        assert_eq!(pixel(hull), [255, 0, 0, 255]);
        assert_eq!(pixel(sail), [0, 0, 255, 255]);

        // Material colors live in the materials now, not in per-face colors
        assert!(mesh.face_colors.is_empty());
    }

    #[test]
    fn test_z_up_conversion_rotates_normals_and_tangents() {
        let mut mesh = MeshWithColors {
            positions: vec![[0.0, 1.0, 2.0]],
            face_indices: Vec::new(),
            face_colors: Vec::new(),
            texcoords: Vec::new(),
            normals: vec![[0.0, 0.0, 1.0]],
            tangents: vec![[0.0, 1.0, 0.0, -1.0]],
            materials: MeshMaterials::default(),
            face_materials: Vec::new(),
            skeleton: None,
            joint_indices: Vec::new(),
            joint_weights: Vec::new(),
            animations: Vec::new(),
        };
        mesh.convert_z_up_to_y_up();
        assert_eq!(mesh.positions[0], [0.0, 2.0, -1.0]);
        assert_eq!(mesh.normals[0], [0.0, 1.0, 0.0]);
        assert_eq!(mesh.tangents[0], [0.0, 0.0, -1.0, -1.0]);
    }
}
