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
enum Commands {
    /// Remesh a messy mesh file (.obj or .glb)
    Remesh {
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
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Remesh { input, out, mesh, iterations, target_edge_length } => {
            if let Err(e) = remesh(&input, &out, mesh.as_deref(), iterations, target_edge_length) {
                eprintln!("Error during remeshing: {}", e);
                std::process::exit(1);
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
    }
}

/// Load mesh from file (supports .obj and .glb)
fn load_mesh(input: &PathBuf, mesh_name: Option<&str>) -> Result<baby_shark::mesh::corner_table::CornerTableF, Box<dyn std::error::Error>> {
    let extension = input.extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .ok_or("File has no extension")?;

    match extension.as_str() {
        "obj" => {
            use baby_shark::io::read_from_file;
            read_from_file(input)
                .map_err(|e| format!("Failed to read OBJ file: {:?}", e).into())
        }
        "glb" | "gltf" => {
            load_mesh_from_glb(input, mesh_name)
        }
        _ => Err(format!("Unsupported file format: {}", extension).into())
    }
}

/// Load mesh from GLB/glTF file
fn load_mesh_from_glb(path: &PathBuf, mesh_name: Option<&str>) -> Result<baby_shark::mesh::corner_table::CornerTableF, Box<dyn std::error::Error>> {
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
                let mesh_list: Vec<String> = meshes.iter()
                    .map(|m| m.name().unwrap_or("<unnamed>").to_string())
                    .collect();
                return Err(format!(
                    "GLB file contains {} meshes. Please specify one with --mesh <name>.\nAvailable meshes: {}",
                    meshes.len(),
                    mesh_list.join(", ")
                ).into());
            }
            Some(name) => {
                meshes.iter()
                    .find(|m| m.name() == Some(name))
                    .ok_or_else(|| {
                        let mesh_list: Vec<String> = meshes.iter()
                            .map(|m| m.name().unwrap_or("<unnamed>").to_string())
                            .collect();
                        format!(
                            "Mesh '{}' not found in GLB file.\nAvailable meshes: {}",
                            name,
                            mesh_list.join(", ")
                        )
                    })?
            }
        }
    };

    println!("Loading mesh: {}", selected_mesh.name().unwrap_or("<unnamed>"));

    // Extract vertex positions and indices from all primitives
    let mut all_positions = Vec::new();
    let mut all_indices = Vec::new();
    let mut vertex_offset = 0u32;

    for primitive in selected_mesh.primitives() {
        // Get positions
        let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

        let positions = reader.read_positions()
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
        builder.add_vertex(pos)
            .map_err(|e| format!("Failed to add vertex: {:?}", e))?;
    }

    // Add triangular faces
    if all_indices.len() % 3 != 0 {
        return Err("Index count is not a multiple of 3 (non-triangular faces)".into());
    }

    builder.set_num_faces(all_indices.len() / 3);
    for chunk in all_indices.chunks(3) {
        builder.add_face(chunk[0] as usize, chunk[1] as usize, chunk[2] as usize)
            .map_err(|e| format!("Failed to add face: {:?}", e))?;
    }

    builder.finish()
        .map_err(|e| format!("Failed to build mesh: {:?}", e).into())
}

fn remesh(input: &PathBuf, output: &PathBuf, mesh_name: Option<&str>, iterations: u32, target_edge_length: f32) -> Result<(), Box<dyn std::error::Error>> {
    use baby_shark::io::write_to_file;
    use baby_shark::remeshing::incremental::IncrementalRemesher;

    println!("Loading mesh from {:?}...", input);
    let mut mesh = load_mesh(input, mesh_name)?;

    let vertex_count_before = mesh.count_vertices();
    let face_count_before = mesh.count_faces();

    println!("Before remeshing: {} vertices, {} faces", vertex_count_before, face_count_before);
    println!("Remeshing with {} iterations, target edge length: {}...", iterations, target_edge_length);

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

    println!("After remeshing: {} vertices, {} faces", vertex_count_after, face_count_after);
    println!("Writing output to {:?}...", output);

    write_to_file(&mesh, output)
        .map_err(|e| format!("Failed to write mesh: {:?}", e))?;

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
    use kiss3d::window::Window;
    use kiss3d::nalgebra as na;
    use std::rc::Rc;
    use std::cell::RefCell;

    println!("Loading mesh from {:?}...", input);

    // Load mesh through baby_shark, export to temp OBJ, then load with kiss3d's OBJ loader
    let mesh = load_mesh(input, mesh_name)?;

    // Write to temporary OBJ file
    let temp_obj = std::env::temp_dir().join("msh_temp_view.obj");
    println!("Converting to OBJ format...");

    use baby_shark::io::write_to_file;
    write_to_file(&mesh, &temp_obj)
        .map_err(|e| format!("Failed to write temp mesh: {:?}", e))?;

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

    let size = [
        max[0] - min[0],
        max[1] - min[1],
        max[2] - min[2],
    ];

    let max_dimension = size[0].max(size[1]).max(size[2]);

    println!("Mesh bounds: ({:.3}, {:.3}, {:.3}) to ({:.3}, {:.3}, {:.3})",
             min[0], min[1], min[2], max[0], max[1], max[2]);
    println!("Mesh center: ({:.3}, {:.3}, {:.3})", center[0], center[1], center[2]);
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

    println!("Extracted {} vertices ({} triangles) as triangle soup", vertices.len(), indices.len());

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
    let mesh_rc = Rc::new(RefCell::new(
        kiss3d::resource::Mesh::new(vertices.clone(), indices, None, None, false)
    ));

    let mut mesh_obj = window.add_mesh(
        mesh_rc,
        na::Vector3::new(1.0, 1.0, 1.0),
    );

    mesh_obj.set_color(0.8, 0.8, 0.8);
    mesh_obj.enable_backface_culling(true);  // Always cull backfaces on main mesh

    // Enable wireframe overlay by default (surfaces + black edges)
    mesh_obj.set_lines_width(1.0);
    mesh_obj.set_lines_color(Some(na::Point3::new(0.0, 0.0, 0.0)));
    mesh_obj.set_surface_rendering_activation(true);

    // Backface mesh (reversed, red) - hidden by default
    let backface_mesh_rc = Rc::new(RefCell::new(
        kiss3d::resource::Mesh::new(vertices, reversed_indices, None, None, false)
    ));

    let mut backface_obj = window.add_mesh(
        backface_mesh_rc,
        na::Vector3::new(1.0, 1.0, 1.0),
    );

    backface_obj.set_color(1.0, 0.0, 0.0);  // Red
    backface_obj.enable_backface_culling(true);  // Cull backfaces on reversed mesh too
    backface_obj.set_visible(false);  // Hidden by default

    // Set camera to look at the centered mesh from a good distance
    let camera_distance = max_dimension * 2.5;
    let eye = na::Point3::new(camera_distance * 0.5, camera_distance * 0.3, camera_distance);
    let at = na::Point3::new(0.0, 0.0, 0.0);
    let mut arc_ball = kiss3d::camera::ArcBall::new(eye, at);

    // State for interactive controls
    let mut show_wireframe = true;  // On by default
    let mut show_backfaces = false;

    println!("Viewing mesh...");
    println!("  Mouse: Rotate (drag), Zoom (scroll), Pan (right-drag)");
    println!("  W: Toggle wireframe overlay");
    println!("  B: Toggle backface visualization (red)");
    println!("  Q/ESC: Exit");
    println!("Wireframe: ON (default)");

    use kiss3d::event::{Key, Action};

    while window.render_with_camera(&mut arc_ball) {
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
                    println!("Backface visualization: {}", if show_backfaces { "ON (red)" } else { "OFF" });
                }
                kiss3d::event::WindowEvent::Key(Key::Q, Action::Press, _) |
                kiss3d::event::WindowEvent::Key(Key::Escape, Action::Press, _) => {
                    return Ok(());
                }
                _ => {}
            }
        }
    }

    Ok(())
}
