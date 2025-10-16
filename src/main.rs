use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "msh")]
#[command(about = "A CLI tool for 3D mesh processing", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum RemeshCommands {
    /// Incremental remeshing (edge-based operations)
    Incremental {
        /// Input mesh file (.obj or .glb)
        input: PathBuf,

        /// Output mesh file (.obj)
        #[arg(short, long)]
        out: PathBuf,

        /// Mesh name (required if GLB contains multiple meshes)
        #[arg(short, long)]
        mesh: Option<String>,

        /// Number of remeshing iterations (default: 10)
        #[arg(short, long, default_value_t = 10)]
        iterations: u32,

        /// Target edge length for remeshing (default: 0.01)
        #[arg(short, long, default_value_t = 0.01)]
        target_edge_length: f32,
    },

    /// Voxel-based remeshing (converts to SDF then remeshes)
    Voxel {
        /// Input mesh file (.obj or .glb)
        input: PathBuf,

        /// Output mesh file (.obj)
        #[arg(short, long)]
        out: PathBuf,

        /// Mesh name (required if GLB contains multiple meshes)
        #[arg(short, long)]
        mesh: Option<String>,

        /// Voxel size (controls output resolution, default: 0.01)
        #[arg(short, long, default_value_t = 0.01)]
        size: f32,

        /// Meshing method
        #[arg(short = 'M', long, default_value = "manifold")]
        method: VoxelMethod,
    },
}

#[derive(Clone, Debug, clap::ValueEnum)]
enum VoxelMethod {
    /// Feature-preserving (may produce non-manifold meshes)
    FeaturePreserving,
    /// Guarantees manifold output (watertight)
    Manifold,
}

#[derive(Subcommand)]
enum Commands {
    /// Remesh a mesh file (fixes then incrementally remeshes, or use subcommands for specific methods)
    Remesh {
        /// Input mesh file (.obj or .glb)
        #[arg(required_unless_present = "command")]
        input: Option<PathBuf>,

        /// Output mesh file (.obj)
        #[arg(short, long, required_unless_present = "command")]
        out: Option<PathBuf>,

        /// Mesh name (required if GLB contains multiple meshes)
        #[arg(short, long)]
        mesh: Option<String>,

        /// Number of incremental remeshing iterations (default: 10)
        #[arg(short, long, default_value_t = 10)]
        iterations: u32,

        /// Target edge length for incremental remeshing (default: 0.01)
        #[arg(short, long, default_value_t = 0.01)]
        target_edge_length: f32,

        /// Voxel size for fix step (default: 0.01)
        #[arg(short, long, default_value_t = 0.01)]
        voxel_size: f32,

        /// Vertex merge tolerance for fix step (default: 0.0001)
        #[arg(long, default_value_t = 0.0001)]
        tolerance: f32,

        /// Skip the fix step (just do incremental remesh)
        #[arg(long, default_value_t = false)]
        no_fix: bool,

        #[command(subcommand)]
        command: Option<RemeshCommands>,
    },

    /// Display mesh statistics
    Stats {
        /// Input mesh file (.obj or .glb)
        input: PathBuf,

        /// Mesh name (required if GLB contains multiple meshes)
        #[arg(short, long)]
        mesh: Option<String>,
    },

    /// View mesh in a 3D viewer
    View {
        /// Input mesh file (.obj or .glb)
        input: PathBuf,

        /// Mesh name (required if GLB contains multiple meshes)
        #[arg(short, long)]
        mesh: Option<String>,
    },

    /// Check if mesh is manifold (watertight)
    Check {
        /// Input mesh file (.obj or .glb)
        input: PathBuf,

        /// Mesh name (required if GLB contains multiple meshes)
        #[arg(short, long)]
        mesh: Option<String>,
    },

    /// Fix holes in mesh automatically
    Fix {
        /// Input mesh file (.obj or .glb)
        input: PathBuf,

        /// Output mesh file (.obj)
        #[arg(short, long)]
        out: PathBuf,

        /// Mesh name (required if GLB contains multiple meshes)
        #[arg(short, long)]
        mesh: Option<String>,

        /// Voxel size for remeshing (default: 0.01)
        #[arg(short, long, default_value_t = 0.01)]
        voxel_size: f32,

        /// Merge vertices closer than this distance before fixing (default: 0.0001)
        #[arg(short, long, default_value_t = 0.0001)]
        tolerance: f32,

        /// Skip vertex merging step
        #[arg(long, default_value_t = false)]
        no_merge: bool,
    },

    /// Inspect GLB/glTF file structure and contents
    InspectGlb {
        /// Input GLB/glTF file
        input: PathBuf,

        /// Output as JSON instead of tree format
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Remesh {
            input,
            out,
            mesh,
            iterations,
            target_edge_length,
            voxel_size,
            tolerance,
            no_fix,
            command,
        } => {
            match command {
                Some(RemeshCommands::Incremental {
                    input,
                    out,
                    mesh,
                    iterations,
                    target_edge_length,
                }) => {
                    if let Err(e) = remesh_incremental(
                        &input,
                        &out,
                        mesh.as_deref(),
                        iterations,
                        target_edge_length,
                    ) {
                        eprintln!("Error during incremental remeshing: {}", e);
                        std::process::exit(1);
                    }
                }
                Some(RemeshCommands::Voxel {
                    input,
                    out,
                    mesh,
                    size,
                    method,
                }) => {
                    if let Err(e) = remesh_voxel(&input, &out, mesh.as_deref(), size, method) {
                        eprintln!("Error during voxel remeshing: {}", e);
                        std::process::exit(1);
                    }
                }
                None => {
                    // Direct remesh: fix + incremental
                    let input = input.expect("input required");
                    let out = out.expect("output required");
                    if let Err(e) = remesh_pipeline(
                        &input,
                        &out,
                        mesh.as_deref(),
                        voxel_size,
                        tolerance,
                        no_fix,
                        iterations,
                        target_edge_length,
                    ) {
                        eprintln!("Error during remeshing pipeline: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        Commands::Stats { input, mesh } => {
            if let Err(e) = show_stats(&input, mesh.as_deref()) {
                eprintln!("Error reading mesh stats: {}", e);
                std::process::exit(1);
            }
        }
        Commands::View { input, mesh } => {
            if let Err(e) = view_mesh(&input, mesh.as_deref()) {
                eprintln!("Error viewing mesh: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Check { input, mesh } => {
            if let Err(e) = check_manifold(&input, mesh.as_deref()) {
                eprintln!("Error checking mesh: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Fix {
            input,
            out,
            mesh,
            voxel_size,
            tolerance,
            no_merge,
        } => {
            if let Err(e) = fix_holes(
                &input,
                &out,
                mesh.as_deref(),
                voxel_size,
                tolerance,
                no_merge,
            ) {
                eprintln!("Error fixing mesh: {}", e);
                std::process::exit(1);
            }
        }
        Commands::InspectGlb { input, json } => {
            if let Err(e) = inspect_glb(&input, json) {
                eprintln!("Error inspecting GLB: {}", e);
                std::process::exit(1);
            }
        }
    }
}

/// Load mesh from file (supports .obj and .glb)
fn load_mesh(
    input: &PathBuf,
    mesh_name: Option<&str>,
) -> Result<baby_shark::mesh::corner_table::CornerTableF, Box<dyn std::error::Error>> {
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
fn load_mesh_from_glb(
    path: &PathBuf,
    mesh_name: Option<&str>,
) -> Result<baby_shark::mesh::corner_table::CornerTableF, Box<dyn std::error::Error>> {
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
    use baby_shark::io::{Builder, IndexedBuilder};
    use baby_shark::mesh::corner_table::CornerTableF;

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

fn remesh_incremental(
    input: &PathBuf,
    output: &PathBuf,
    mesh_name: Option<&str>,
    iterations: u32,
    target_edge_length: f32,
) -> Result<(), Box<dyn std::error::Error>> {
    use baby_shark::io::write_to_file;
    use baby_shark::remeshing::incremental::IncrementalRemesher;

    println!("Loading mesh from {:?}...", input);
    let mut mesh = load_mesh(input, mesh_name)?;

    let vertex_count_before = mesh.count_vertices();
    let face_count_before = mesh.count_faces();

    println!(
        "Before remeshing: {} vertices, {} faces",
        vertex_count_before, face_count_before
    );
    println!(
        "Remeshing with {} iterations, target edge length: {}...",
        iterations, target_edge_length
    );

    // Convert u32 to u16 for iterations
    let iterations_u16 = iterations.min(u16::MAX as u32) as u16;

    let remesher = IncrementalRemesher::new()
        .with_iterations_count(iterations_u16)
        .with_split_edges(true)
        .with_collapse_edges(true)
        .with_flip_edges(true)
        .with_shift_vertices(true)
        .with_project_vertices(true);

    remesher.remesh(&mut mesh, target_edge_length);

    let vertex_count_after = mesh.count_vertices();
    let face_count_after = mesh.count_faces();

    println!(
        "After remeshing: {} vertices, {} faces",
        vertex_count_after, face_count_after
    );
    println!("Writing output to {:?}...", output);

    write_to_file(&mesh, output).map_err(|e| format!("Failed to write mesh: {:?}", e))?;

    println!("Done!");
    Ok(())
}

fn remesh_pipeline(
    input: &PathBuf,
    output: &PathBuf,
    mesh_name: Option<&str>,
    voxel_size: f32,
    tolerance: f32,
    no_fix: bool,
    iterations: u32,
    target_edge_length: f32,
) -> Result<(), Box<dyn std::error::Error>> {
    use baby_shark::io::write_to_file;
    use baby_shark::remeshing::incremental::IncrementalRemesher;
    use baby_shark::remeshing::voxel::{MeshingMethod, VoxelRemesher};

    println!("Loading mesh from {:?}...", input);
    let mut mesh = load_mesh(input, mesh_name)?;

    let vertex_count_initial = mesh.count_vertices();
    let face_count_initial = mesh.count_faces();

    println!(
        "Initial: {} vertices, {} faces",
        vertex_count_initial, face_count_initial
    );

    // Step 1: Fix the mesh (unless disabled)
    if !no_fix {
        println!("\n=== Step 1: Fixing Mesh ===");

        // Merge close vertices
        mesh = merge_close_vertices(&mesh, tolerance)?;
        println!(
            "After merging: {} vertices, {} faces",
            mesh.count_vertices(),
            mesh.count_faces()
        );

        // Check if mesh needs hole fixing
        let boundary_rings = mesh.boundary_rings();
        if !boundary_rings.is_empty() {
            println!("Found {} hole(s) in mesh", boundary_rings.len());
            println!(
                "Fixing holes using voxel remeshing (voxel size: {})...",
                voxel_size
            );

            let mut remesher = VoxelRemesher::default()
                .with_voxel_size(voxel_size)
                .with_meshing_method(MeshingMethod::Manifold);

            mesh = remesher.remesh(&mesh).ok_or("Voxel remeshing failed")?;

            println!(
                "After fixing: {} vertices, {} faces",
                mesh.count_vertices(),
                mesh.count_faces()
            );

            let boundary_rings_after = mesh.boundary_rings();
            if boundary_rings_after.is_empty() {
                println!("✓ Mesh is now manifold!");
            } else {
                println!("⚠ Warning: {} hole(s) remain", boundary_rings_after.len());
            }
        } else {
            println!("✓ Mesh is already manifold (no holes to fix)");
        }
    }

    // Step 2: Incremental remeshing
    println!("\n=== Step 2: Incremental Remeshing ===");
    println!(
        "Remeshing with {} iterations, target edge length: {}...",
        iterations, target_edge_length
    );

    let vertex_count_before_incremental = mesh.count_vertices();
    let face_count_before_incremental = mesh.count_faces();

    let iterations_u16 = iterations.min(u16::MAX as u32) as u16;

    let remesher = IncrementalRemesher::new()
        .with_iterations_count(iterations_u16)
        .with_split_edges(true)
        .with_collapse_edges(true)
        .with_flip_edges(true)
        .with_shift_vertices(true)
        .with_project_vertices(true);

    remesher.remesh(&mut mesh, target_edge_length);

    let vertex_count_final = mesh.count_vertices();
    let face_count_final = mesh.count_faces();

    println!(
        "After incremental remeshing: {} vertices, {} faces",
        vertex_count_final, face_count_final
    );

    // Final summary
    println!("\n=== Summary ===");
    println!(
        "Initial:  {} vertices, {} faces",
        vertex_count_initial, face_count_initial
    );
    if !no_fix {
        println!(
            "After fix: {} vertices, {} faces",
            vertex_count_before_incremental, face_count_before_incremental
        );
    }
    println!(
        "Final:    {} vertices, {} faces",
        vertex_count_final, face_count_final
    );

    println!("\nWriting output to {:?}...", output);
    write_to_file(&mesh, output).map_err(|e| format!("Failed to write mesh: {:?}", e))?;

    println!("Done!");
    Ok(())
}

fn remesh_voxel(
    input: &PathBuf,
    output: &PathBuf,
    mesh_name: Option<&str>,
    voxel_size: f32,
    method: VoxelMethod,
) -> Result<(), Box<dyn std::error::Error>> {
    use baby_shark::remeshing::voxel::{MeshingMethod, VoxelRemesher};

    println!("Loading mesh from {:?}...", input);
    let mesh = load_mesh(input, mesh_name)?;

    let vertex_count_before = mesh.count_vertices();
    let face_count_before = mesh.count_faces();

    println!(
        "Before remeshing: {} vertices, {} faces",
        vertex_count_before, face_count_before
    );
    println!(
        "Voxel remeshing with method: {:?}, voxel size: {}",
        method, voxel_size
    );

    let meshing_method = match method {
        VoxelMethod::FeaturePreserving => MeshingMethod::FeaturePreserving,
        VoxelMethod::Manifold => MeshingMethod::Manifold,
    };

    let mut remesher = VoxelRemesher::default()
        .with_voxel_size(voxel_size)
        .with_meshing_method(meshing_method);

    let remeshed_mesh = remesher.remesh(&mesh).ok_or("Voxel remeshing failed")?;

    let vertex_count_after = remeshed_mesh.count_vertices();
    let face_count_after = remeshed_mesh.count_faces();

    println!(
        "After remeshing: {} vertices, {} faces",
        vertex_count_after, face_count_after
    );

    // Check manifold status if using Manifold method
    if matches!(method, VoxelMethod::Manifold) {
        let boundary_rings = remeshed_mesh.boundary_rings();
        if boundary_rings.is_empty() {
            println!("✓ Output mesh is manifold (watertight)");
        } else {
            println!(
                "⚠ Warning: {} boundary ring(s) detected",
                boundary_rings.len()
            );
        }
    }

    println!("Writing output to {:?}...", output);
    use baby_shark::io::write_to_file;
    write_to_file(&remeshed_mesh, output).map_err(|e| format!("Failed to write mesh: {:?}", e))?;

    println!("Done!");
    Ok(())
}

fn show_stats(input: &PathBuf, mesh_name: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading mesh from {:?}...", input);
    let mesh = load_mesh(input, mesh_name)?;

    println!("\n=== Mesh Statistics ===");
    println!("Vertices:  {}", mesh.count_vertices());
    println!("Faces:     {}", mesh.count_faces());
    println!("Triangles: {}", mesh.count_faces()); // For triangle meshes, faces = triangles
    println!("Edges:     {}", mesh.unique_edges().count());

    // Calculate bounding box
    let mut first = true;
    let mut min = [0.0f32, 0.0, 0.0];
    let mut max = [0.0f32, 0.0, 0.0];

    for vertex_id in mesh.vertices() {
        let pos = mesh.vertex_position(vertex_id);

        if first {
            min = [pos.x, pos.y, pos.z];
            max = [pos.x, pos.y, pos.z];
            first = false;
        } else {
            min[0] = min[0].min(pos.x);
            min[1] = min[1].min(pos.y);
            min[2] = min[2].min(pos.z);
            max[0] = max[0].max(pos.x);
            max[1] = max[1].max(pos.y);
            max[2] = max[2].max(pos.z);
        }
    }

    if !first {
        let size = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
        println!("\n=== Bounding Box ===");
        println!("Min: ({:.3}, {:.3}, {:.3})", min[0], min[1], min[2]);
        println!("Max: ({:.3}, {:.3}, {:.3})", max[0], max[1], max[2]);
        println!("Size: ({:.3}, {:.3}, {:.3})", size[0], size[1], size[2]);
    }

    Ok(())
}

fn view_mesh(input: &PathBuf, mesh_name: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    use kiss3d::light::Light;
    use kiss3d::nalgebra as na;
    use kiss3d::window::Window;
    use std::cell::RefCell;
    use std::rc::Rc;

    println!("Loading mesh from {:?}...", input);

    // Load mesh through baby_shark, export to temp OBJ, then load with kiss3d's OBJ loader
    let mesh = load_mesh(input, mesh_name)?;

    // Write to temporary OBJ file
    let temp_obj = std::env::temp_dir().join("msh_temp_view.obj");
    println!("Converting to OBJ format...");

    use baby_shark::io::write_to_file;
    write_to_file(&mesh, &temp_obj).map_err(|e| format!("Failed to write temp mesh: {:?}", e))?;

    // Calculate bounding box to center and scale the mesh
    let mut min = [f32::INFINITY, f32::INFINITY, f32::INFINITY];
    let mut max = [f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];

    for vertex_id in mesh.vertices() {
        let pos = mesh.vertex_position(vertex_id);
        min[0] = min[0].min(pos.x);
        min[1] = min[1].min(pos.y);
        min[2] = min[2].min(pos.z);
        max[0] = max[0].max(pos.x);
        max[1] = max[1].max(pos.y);
        max[2] = max[2].max(pos.z);
    }

    let center = [
        (min[0] + max[0]) / 2.0,
        (min[1] + max[1]) / 2.0,
        (min[2] + max[2]) / 2.0,
    ];

    let size = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];

    let max_dimension = size[0].max(size[1]).max(size[2]);

    println!(
        "Mesh bounds: ({:.3}, {:.3}, {:.3}) to ({:.3}, {:.3}, {:.3})",
        min[0], min[1], min[2], max[0], max[1], max[2]
    );
    println!(
        "Mesh center: ({:.3}, {:.3}, {:.3})",
        center[0], center[1], center[2]
    );
    println!("Mesh size: {:.3}", max_dimension);

    // Extract as triangle soup (no vertex sharing) to avoid any indexing issues
    let mut vertices: Vec<na::Point3<f32>> = Vec::new();
    let mut indices: Vec<na::Point3<u32>> = Vec::new();

    let mut vertex_idx = 0u32;

    for face_id in mesh.faces() {
        let triangle = mesh.face_positions(face_id);

        // Get the three vertices of the triangle
        let v0 = triangle.p1();
        let v1 = triangle.p2();
        let v3 = triangle.p3();

        // Add vertices directly (centered)
        vertices.push(na::Point3::new(
            v0.x - center[0],
            v0.y - center[1],
            v0.z - center[2],
        ));
        vertices.push(na::Point3::new(
            v1.x - center[0],
            v1.y - center[1],
            v1.z - center[2],
        ));
        vertices.push(na::Point3::new(
            v3.x - center[0],
            v3.y - center[1],
            v3.z - center[2],
        ));

        // Create triangle with sequential indices
        indices.push(na::Point3::new(vertex_idx, vertex_idx + 1, vertex_idx + 2));
        vertex_idx += 3;
    }

    println!(
        "Extracted {} vertices ({} triangles) as triangle soup",
        vertices.len(),
        indices.len()
    );

    // Calculate mesh statistics for overlay
    let vertex_count = mesh.count_vertices();
    let face_count = mesh.count_faces();
    let edge_count = mesh.unique_edges().count();
    let boundary_rings = mesh.boundary_rings();
    let is_manifold = boundary_rings.is_empty();

    // Create reversed mesh for backface visualization (flip winding)
    let mut reversed_indices: Vec<na::Point3<u32>> = Vec::new();
    for tri in &indices {
        // Reverse winding order: (v0, v1, v2) -> (v0, v2, v1)
        reversed_indices.push(na::Point3::new(tri.x, tri.z, tri.y));
    }

    println!("Creating viewer window...");
    let mut window = Window::new("Mesh Viewer - msh");
    window.set_light(Light::StickToCamera);

    // Main mesh (front faces)
    let mesh_rc = Rc::new(RefCell::new(kiss3d::resource::Mesh::new(
        vertices.clone(),
        indices,
        None,
        None,
        false,
    )));

    let mut mesh_obj = window.add_mesh(mesh_rc, na::Vector3::new(1.0, 1.0, 1.0));

    mesh_obj.set_color(0.8, 0.8, 0.8);
    mesh_obj.enable_backface_culling(true); // Always cull backfaces on main mesh

    // Enable wireframe overlay by default (surfaces + black edges)
    mesh_obj.set_lines_width(1.0);
    mesh_obj.set_lines_color(Some(na::Point3::new(0.0, 0.0, 0.0)));
    mesh_obj.set_surface_rendering_activation(true);

    // Backface mesh (reversed, red) - hidden by default
    let backface_mesh_rc = Rc::new(RefCell::new(kiss3d::resource::Mesh::new(
        vertices,
        reversed_indices,
        None,
        None,
        false,
    )));

    let mut backface_obj = window.add_mesh(backface_mesh_rc, na::Vector3::new(1.0, 1.0, 1.0));

    backface_obj.set_color(1.0, 0.0, 0.0); // Red
    backface_obj.enable_backface_culling(true); // Cull backfaces on reversed mesh too
    backface_obj.set_visible(false); // Hidden by default

    // Set camera to look at the centered mesh from a good distance
    let camera_distance = max_dimension * 2.5;
    let eye = na::Point3::new(
        camera_distance * 0.5,
        camera_distance * 0.3,
        camera_distance,
    );
    let at = na::Point3::new(0.0, 0.0, 0.0);
    let mut arc_ball = kiss3d::camera::ArcBall::new(eye, at);

    // State for interactive controls
    let mut show_wireframe = true; // On by default
    let mut show_backfaces = false;

    println!("Viewing mesh...");
    println!("  Mouse: Rotate (drag), Zoom (scroll), Pan (right-drag)");
    println!("  W: Toggle wireframe overlay");
    println!("  B: Toggle backface visualization (red)");
    println!("  Q/ESC: Exit");
    println!("Wireframe: ON (default)");

    use kiss3d::event::{Action, Key};
    use kiss3d::text::Font;

    // Load font for text rendering (use built-in font)
    let font = Font::default();

    while window.render_with_camera(&mut arc_ball) {
        // Draw controls overlay
        let x_offset = 11.0;
        let y_offset = 15.0;
        let line_height = 18.0;
        let header_size = 26.0;
        let text_size = 18.0;
        let header_padding = 8.0; // Extra padding after headers

        // Headers use lighter gray for a "thinner" appearance
        let header_color = na::Point3::new(0.8, 0.8, 0.8);
        let text_color = na::Point3::new(0.9, 0.9, 0.9);

        window.draw_text(
            "Controls",
            &na::Point2::new(x_offset - 1.0, y_offset),
            header_size,
            &font,
            &header_color,
        );
        let mut current_y = y_offset + line_height + header_padding;

        window.draw_text(
            "Left Click+Drag: Rotate",
            &na::Point2::new(x_offset, current_y),
            text_size,
            &font,
            &text_color,
        );
        current_y += line_height;

        window.draw_text(
            "Right Click+Drag: Pan",
            &na::Point2::new(x_offset, current_y),
            text_size,
            &font,
            &text_color,
        );
        current_y += line_height;

        window.draw_text(
            "Scroll: Zoom",
            &na::Point2::new(x_offset, current_y),
            text_size,
            &font,
            &text_color,
        );
        current_y += line_height;

        window.draw_text(
            "W: Toggle Wireframe",
            &na::Point2::new(x_offset, current_y),
            text_size,
            &font,
            &text_color,
        );
        current_y += line_height;

        window.draw_text(
            "B: Toggle Backfaces",
            &na::Point2::new(x_offset, current_y),
            text_size,
            &font,
            &text_color,
        );
        current_y += line_height;

        window.draw_text(
            "Q/ESC: Exit",
            &na::Point2::new(x_offset, current_y),
            text_size,
            &font,
            &text_color,
        );

        // Draw mesh statistics
        current_y += line_height * 2.0;
        window.draw_text(
            "Mesh Info",
            &na::Point2::new(x_offset - 1.0, current_y),
            header_size,
            &font,
            &header_color,
        );
        current_y += line_height + header_padding;

        window.draw_text(
            &format!("Vertices: {}", vertex_count),
            &na::Point2::new(x_offset, current_y),
            text_size,
            &font,
            &text_color,
        );
        current_y += line_height;

        window.draw_text(
            &format!("Edges: {}", edge_count),
            &na::Point2::new(x_offset, current_y),
            text_size,
            &font,
            &text_color,
        );
        current_y += line_height;

        window.draw_text(
            &format!("Faces: {}", face_count),
            &na::Point2::new(x_offset, current_y),
            text_size,
            &font,
            &text_color,
        );
        current_y += line_height;

        // Manifold status with color
        let manifold_text = if is_manifold {
            "Manifold: Yes".to_string()
        } else {
            format!("Manifold: No ({} holes)", boundary_rings.len())
        };
        let manifold_color = if is_manifold {
            na::Point3::new(0.4, 1.0, 0.4) // Bright green
        } else {
            na::Point3::new(1.0, 0.4, 0.4) // Bright red
        };
        window.draw_text(
            &manifold_text,
            &na::Point2::new(10.0, current_y),
            text_size,
            &font,
            &manifold_color,
        );

        // Handle keyboard input
        for event in window.events().iter() {
            match event.value {
                kiss3d::event::WindowEvent::Key(Key::W, Action::Press, _) => {
                    show_wireframe = !show_wireframe;
                    if show_wireframe {
                        mesh_obj.set_lines_width(1.0);
                        mesh_obj.set_lines_color(Some(na::Point3::new(0.0, 0.0, 0.0)));
                    } else {
                        mesh_obj.set_lines_width(0.0);
                    }
                    println!("Wireframe: {}", if show_wireframe { "ON" } else { "OFF" });
                }
                kiss3d::event::WindowEvent::Key(Key::B, Action::Press, _) => {
                    show_backfaces = !show_backfaces;
                    backface_obj.set_visible(show_backfaces);
                    println!(
                        "Backface visualization: {}",
                        if show_backfaces { "ON (red)" } else { "OFF" }
                    );
                }
                kiss3d::event::WindowEvent::Key(Key::Q, Action::Press, _)
                | kiss3d::event::WindowEvent::Key(Key::Escape, Action::Press, _) => {
                    return Ok(());
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn check_manifold(
    input: &PathBuf,
    mesh_name: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading mesh from {:?}...", input);
    let mesh = load_mesh(input, mesh_name)?;

    println!("\n=== Manifold Check ===");
    println!("Analyzing mesh topology...\n");

    // Check boundary rings (holes)
    let boundary_rings = mesh.boundary_rings();

    if boundary_rings.is_empty() {
        println!("✓ Mesh is MANIFOLD (watertight)");
        println!("  No holes or boundaries detected.");
    } else {
        println!("✗ Mesh is NOT MANIFOLD");
        println!(
            "  Found {} boundary ring(s) (holes):\n",
            boundary_rings.len()
        );

        for (i, ring) in boundary_rings.iter().enumerate() {
            let mut edge_count = 0;
            mesh.boundary_edges(*ring, |_edge| {
                edge_count += 1;
                std::ops::ControlFlow::Continue(())
            });

            println!("  Hole {}: {} boundary edges", i + 1, edge_count);
        }

        println!("\nTo fix these holes, run:");
        println!("  msh fix {:?} --out <output.obj>", input);
    }

    Ok(())
}

/// Merge vertices that are closer than tolerance
fn merge_close_vertices(
    mesh: &baby_shark::mesh::corner_table::CornerTableF,
    tolerance: f32,
) -> Result<baby_shark::mesh::corner_table::CornerTableF, Box<dyn std::error::Error>> {
    use baby_shark::algo::merge_points::merge_points;
    use baby_shark::exports::nalgebra::Vector3;
    use baby_shark::io::{Builder, IndexedBuilder};

    println!("Merging vertices with tolerance: {}", tolerance);

    let vertex_count_before = mesh.count_vertices();

    // Extract all vertex positions and build a VertexId -> index mapping
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut vertex_id_to_idx: std::collections::HashMap<_, usize> =
        std::collections::HashMap::new();

    for (idx, vertex_id) in mesh.vertices().enumerate() {
        let pos = mesh.vertex_position(vertex_id);
        positions.push([pos.x, pos.y, pos.z]);
        vertex_id_to_idx.insert(vertex_id, idx);
    }

    // Quantize positions to tolerance grid
    let inv_tolerance = 1.0 / tolerance;
    let quantized_positions: Vec<Vector3<f32>> = positions
        .iter()
        .map(|pos| {
            Vector3::new(
                (pos[0] * inv_tolerance).round() * tolerance,
                (pos[1] * inv_tolerance).round() * tolerance,
                (pos[2] * inv_tolerance).round() * tolerance,
            )
        })
        .collect();

    // Merge quantized vertices
    let merged = merge_points(quantized_positions.into_iter());

    println!(
        "Merged {} vertices into {} unique vertices",
        vertex_count_before,
        merged.points.len()
    );

    // Build vertex mapping: old vertex array index -> new vertex index
    let vertex_map: Vec<usize> = merged.indices;

    // Rebuild mesh with merged vertices
    let mut builder = baby_shark::mesh::corner_table::CornerTableF::builder_indexed();
    builder.set_num_vertices(merged.points.len());

    for point in &merged.points {
        builder
            .add_vertex([point.x, point.y, point.z])
            .map_err(|e| format!("Failed to add vertex: {:?}", e))?;
    }

    // Add faces with remapped vertex indices
    let face_count = mesh.count_faces();
    builder.set_num_faces(face_count);

    for face_id in mesh.faces() {
        let (v0_id, v1_id, v2_id) = mesh.face_vertices(face_id);
        let v0_idx = vertex_id_to_idx[&v0_id];
        let v1_idx = vertex_id_to_idx[&v1_id];
        let v2_idx = vertex_id_to_idx[&v2_id];
        let v0 = vertex_map[v0_idx];
        let v1 = vertex_map[v1_idx];
        let v2 = vertex_map[v2_idx];

        // Skip degenerate faces (where vertices got merged into same point)
        if v0 != v1 && v1 != v2 && v0 != v2 {
            if let Err(e) = builder.add_face(v0, v1, v2) {
                // Skip faces that fail to add (likely degenerate)
                eprintln!("Warning: Skipping degenerate face: {:?}", e);
            }
        }
    }

    builder
        .finish()
        .map_err(|e| format!("Failed to build merged mesh: {:?}", e).into())
}

fn fix_holes(
    input: &PathBuf,
    output: &PathBuf,
    mesh_name: Option<&str>,
    voxel_size: f32,
    tolerance: f32,
    no_merge: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use baby_shark::remeshing::voxel::{MeshingMethod, VoxelRemesher};

    println!("Loading mesh from {:?}...", input);
    let mut mesh = load_mesh(input, mesh_name)?;

    let vertex_count_initial = mesh.count_vertices();
    let face_count_initial = mesh.count_faces();

    println!(
        "Initial: {} vertices, {} faces",
        vertex_count_initial, face_count_initial
    );

    // Merge close vertices first (unless disabled)
    if !no_merge {
        mesh = merge_close_vertices(&mesh, tolerance)?;
        println!(
            "After merging: {} vertices, {} faces",
            mesh.count_vertices(),
            mesh.count_faces()
        );
    }

    // Check if mesh needs fixing
    let boundary_rings = mesh.boundary_rings();
    if boundary_rings.is_empty() {
        println!("Mesh is already manifold (watertight). No fixing needed.");

        // Still write the output if we merged vertices
        if !no_merge && mesh.count_vertices() < vertex_count_initial {
            println!("Writing merged mesh to {:?}...", output);
            use baby_shark::io::write_to_file;
            write_to_file(&mesh, output).map_err(|e| format!("Failed to write mesh: {:?}", e))?;
            println!("Done!");
        }

        return Ok(());
    }

    println!("Found {} hole(s) in mesh", boundary_rings.len());
    println!("Fixing holes using voxel remeshing...");
    println!("Voxel size: {}", voxel_size);

    // Use voxel remeshing with Manifold method to close holes
    let mut remesher = VoxelRemesher::default()
        .with_voxel_size(voxel_size)
        .with_meshing_method(MeshingMethod::Manifold);

    let fixed_mesh = remesher.remesh(&mesh).ok_or("Voxel remeshing failed")?;

    let vertex_count_after = fixed_mesh.count_vertices();
    let face_count_after = fixed_mesh.count_faces();

    println!(
        "After: {} vertices, {} faces",
        vertex_count_after, face_count_after
    );

    // Verify the result
    let boundary_rings_after = fixed_mesh.boundary_rings();
    if boundary_rings_after.is_empty() {
        println!("✓ Mesh is now manifold!");
    } else {
        println!(
            "⚠ Warning: {} hole(s) remain (may need smaller voxel size)",
            boundary_rings_after.len()
        );
    }

    println!("Writing output to {:?}...", output);
    use baby_shark::io::write_to_file;
    write_to_file(&fixed_mesh, output).map_err(|e| format!("Failed to write mesh: {:?}", e))?;

    println!("Done!");
    Ok(())
}

/// Inspect GLB/glTF file structure
fn inspect_glb(path: &PathBuf, as_json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let (document, _buffers, _images) = gltf::import(path)?;

    if as_json {
        // JSON output
        let json_data = build_json_structure(&document)?;
        println!("{}", serde_json::to_string_pretty(&json_data)?);
    } else {
        for (scene_idx, scene) in document.scenes().enumerate() {
            println!(
                "Scene {}: {}",
                scene_idx,
                scene.name().unwrap_or("<unnamed>")
            );

            let nodes: Vec<_> = scene.nodes().collect();
            for (idx, node) in nodes.iter().enumerate() {
                let is_last = idx == nodes.len() - 1;
                print_node_tree(node, "", is_last);
            }
        }
    }

    Ok(())
}

fn print_node_tree(node: &gltf::Node, prefix: &str, is_last: bool) {
    let name = node.name().unwrap_or("<unnamed>");

    // Determine entity types
    let mut entity_types = Vec::new();

    if node.mesh().is_some() {
        entity_types.push("Mesh");
    }
    if node.camera().is_some() {
        entity_types.push("Camera");
    }
    entity_types.push("Transform");

    let type_str = entity_types.join(", ");

    // Print node info with proper connector
    let connector = if is_last { "└─" } else { "├─" };
    println!("{}{} {} [{}]", prefix, connector, name, type_str);

    // Determine the prefix for child content and check if node has children
    let children: Vec<_> = node.children().collect();
    let has_children = !children.is_empty();

    let child_prefix = if is_last {
        format!("{}   ", prefix)
    } else {
        format!("{}│  ", prefix)
    };

    // Use vertical line for properties when node has children
    let prop_prefix = if has_children {
        format!("{}│  ", child_prefix)
    } else {
        format!("{}   ", child_prefix)
    };

    // Print transform
    let (translation, rotation, scale) = node.transform().decomposed();
    println!(
        "{}Position: ({:.2}, {:.2}, {:.2})",
        prop_prefix, translation[0], translation[1], translation[2]
    );

    if scale != [1.0, 1.0, 1.0] {
        println!(
            "{}Scale: ({:.2}, {:.2}, {:.2})",
            prop_prefix, scale[0], scale[1], scale[2]
        );
    }

    if rotation != [0.0, 0.0, 0.0, 1.0] {
        println!(
            "{}Rotation: ({:.2}, {:.2}, {:.2}, {:.2})",
            prop_prefix, rotation[0], rotation[1], rotation[2], rotation[3]
        );
    }

    // Print custom properties if present
    let extras = node.extras();
    if let Some(extras_raw) = extras.as_ref() {
        if let Ok(extras_value) = serde_json::from_str::<serde_json::Value>(extras_raw.get()) {
            if let Some(obj) = extras_value.as_object() {
                if !obj.is_empty() {
                    println!("{}Custom Properties:", prop_prefix);
                    for (key, val) in obj {
                        println!("{}• {}: {}", prop_prefix, key, val);
                    }
                }
            }
        }
    }

    // Print mesh info if present
    if let Some(mesh) = node.mesh() {
        println!(
            "{}Mesh: {} ({} primitives)",
            prop_prefix,
            mesh.name().unwrap_or("<unnamed>"),
            mesh.primitives().count()
        );
    }

    // Recursively print children
    for (idx, child) in children.iter().enumerate() {
        let is_last_child = idx == children.len() - 1;
        print_node_tree(child, &child_prefix, is_last_child);
    }
}

fn build_json_structure(
    document: &gltf::Document,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut scenes = Vec::new();

    for scene in document.scenes() {
        let mut scene_data = serde_json::Map::new();
        scene_data.insert(
            "name".to_string(),
            serde_json::Value::String(scene.name().unwrap_or("<unnamed>").to_string()),
        );

        let mut nodes = Vec::new();
        for node in scene.nodes() {
            nodes.push(build_node_json(&node)?);
        }
        scene_data.insert("nodes".to_string(), serde_json::Value::Array(nodes));

        scenes.push(serde_json::Value::Object(scene_data));
    }

    Ok(serde_json::json!({
        "scenes": scenes
    }))
}

fn build_node_json(node: &gltf::Node) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut node_data = serde_json::Map::new();

    node_data.insert(
        "name".to_string(),
        serde_json::Value::String(node.name().unwrap_or("<unnamed>").to_string()),
    );

    // Add transform
    let (translation, rotation, scale) = node.transform().decomposed();
    node_data.insert(
        "transform".to_string(),
        serde_json::json!({
            "translation": translation,
            "rotation": rotation,
            "scale": scale,
        }),
    );

    // Add component types
    let mut types = Vec::new();
    if node.mesh().is_some() {
        types.push("Mesh");
    }
    if node.camera().is_some() {
        types.push("Camera");
    }
    types.push("Transform");
    node_data.insert(
        "types".to_string(),
        serde_json::Value::Array(
            types
                .into_iter()
                .map(|s| serde_json::Value::String(s.to_string()))
                .collect(),
        ),
    );

    // Add mesh info if present
    if let Some(mesh) = node.mesh() {
        node_data.insert(
            "mesh".to_string(),
            serde_json::json!({
                "name": mesh.name().unwrap_or("<unnamed>"),
                "primitive_count": mesh.primitives().count(),
            }),
        );
    }

    // Add custom properties if present
    let extras = node.extras();
    if let Some(extras_raw) = extras.as_ref() {
        if let Ok(extras_value) = serde_json::from_str::<serde_json::Value>(extras_raw.get()) {
            node_data.insert("extras".to_string(), extras_value);
        }
    }

    // Add children recursively
    let children: Vec<_> = node
        .children()
        .map(|child| build_node_json(&child))
        .collect::<Result<Vec<_>, _>>()?;

    if !children.is_empty() {
        node_data.insert("children".to_string(), serde_json::Value::Array(children));
    }

    Ok(serde_json::Value::Object(node_data))
}
