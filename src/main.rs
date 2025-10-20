use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod glb;
mod mesh;
mod remote;
mod rpc;
mod viewer;

#[derive(Parser)]
#[command(name = "msh")]
#[command(about = "A CLI tool for 3D mesh processing", long_about = None)]
#[command(version)]
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
        method: VoxelMethodArg,
    },
}

#[derive(Clone, Debug, clap::ValueEnum)]
enum VoxelMethodArg {
    /// Feature-preserving (may produce non-manifold meshes)
    FeaturePreserving,
    /// Guarantees manifold output (watertight)
    Manifold,
}

impl From<VoxelMethodArg> for mesh::VoxelMethod {
    fn from(arg: VoxelMethodArg) -> Self {
        match arg {
            VoxelMethodArg::FeaturePreserving => mesh::VoxelMethod::FeaturePreserving,
            VoxelMethodArg::Manifold => mesh::VoxelMethod::Manifold,
        }
    }
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
        /// Input mesh file (.obj or .glb) - optional when using --remote
        #[cfg(feature = "remote")]
        #[arg(required_unless_present = "remote")]
        input: Option<PathBuf>,

        /// Input mesh file (.obj or .glb)
        #[cfg(not(feature = "remote"))]
        input: PathBuf,

        /// Mesh name (required if GLB contains multiple meshes)
        #[arg(short, long)]
        mesh: Option<String>,

        /// Enable remote control via JSON-RPC (requires 'remote' feature)
        #[cfg(feature = "remote")]
        #[arg(long)]
        remote: bool,
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

    /// Remote control commands for running viewer
    #[cfg(feature = "remote")]
    Remote {
        #[command(subcommand)]
        command: RemoteCommands,
    },
}

#[cfg(feature = "remote")]
#[derive(Subcommand)]
enum RemoteCommands {
    /// Load a mesh file in the running viewer
    Load {
        /// Path to mesh file (.obj or .glb)
        path: PathBuf,

        /// Mesh name (for GLB with multiple meshes)
        #[arg(short, long)]
        mesh: Option<String>,
    },

    /// Set absolute model rotation (Euler angles)
    Rotate {
        /// X rotation (radians)
        x: f32,
        /// Y rotation (radians)
        y: f32,
        /// Z rotation (radians)
        z: f32,
    },

    /// Rotate model around an axis
    RotateAxis {
        /// Axis as x,y,z
        #[arg(value_parser = parse_axis)]
        axis: (f32, f32, f32),

        /// Angle (e.g., "90d" for degrees or "1.57r" for radians)
        angle: String,
    },

    /// Set camera position
    CameraPos {
        /// X position
        x: f32,
        /// Y position
        y: f32,
        /// Z position
        z: f32,
    },

    /// Set camera target (look-at point)
    CameraTarget {
        /// X position
        x: f32,
        /// Y position
        y: f32,
        /// Z position
        z: f32,
    },

    /// Enable wireframe display
    EnableWireframe,

    /// Disable wireframe display
    DisableWireframe,

    /// Toggle wireframe display
    ToggleWireframe,

    /// Enable backface visualization
    EnableBackfaces,

    /// Disable backface visualization
    DisableBackfaces,

    /// Toggle backface visualization
    ToggleBackfaces,

    /// Enable UI overlay
    EnableUi,

    /// Disable UI overlay
    DisableUi,

    /// Toggle UI overlay
    ToggleUi,

    /// Get mesh statistics from running viewer
    Stats,

    /// Capture frame (requires renderdoc feature)
    Capture {
        /// Output path for capture
        path: Option<String>,
    },

    /// Take a screenshot (save to PNG)
    Screenshot {
        /// Output path for screenshot (e.g., "screenshot.png")
        path: String,
    },

    /// Quit the running viewer
    Quit,
}

#[cfg(feature = "remote")]
fn parse_axis(s: &str) -> Result<(f32, f32, f32), String> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 3 {
        return Err("Axis must be in format: x,y,z".to_string());
    }

    let x = parts[0]
        .trim()
        .parse::<f32>()
        .map_err(|_| format!("Invalid x value: {}", parts[0]))?;
    let y = parts[1]
        .trim()
        .parse::<f32>()
        .map_err(|_| format!("Invalid y value: {}", parts[1]))?;
    let z = parts[2]
        .trim()
        .parse::<f32>()
        .map_err(|_| format!("Invalid z value: {}", parts[2]))?;

    Ok((x, y, z))
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
                    if let Err(e) = mesh::remesh_incremental(
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
                    if let Err(e) =
                        mesh::remesh_voxel(&input, &out, mesh.as_deref(), size, method.into())
                    {
                        eprintln!("Error during voxel remeshing: {}", e);
                        std::process::exit(1);
                    }
                }
                None => {
                    // Direct remesh: fix + incremental
                    let input = input.expect("input required");
                    let out = out.expect("output required");
                    if let Err(e) = mesh::remesh_pipeline(
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
            if let Err(e) = mesh::show_stats(&input, mesh.as_deref()) {
                eprintln!("Error reading mesh stats: {}", e);
                std::process::exit(1);
            }
        }
        Commands::View {
            input,
            mesh,
            #[cfg(feature = "remote")]
            remote,
        } => {
            #[cfg(feature = "remote")]
            {
                if remote {
                    if let Err(e) = viewer::view_mesh_with_rpc(input.as_ref(), mesh.as_deref()) {
                        eprintln!("Error viewing mesh: {}", e);
                        std::process::exit(1);
                    }
                } else {
                    let input_ref = input.as_ref().expect("input required when not using --remote");
                    if let Err(e) = viewer::view_mesh(input_ref, mesh.as_deref()) {
                        eprintln!("Error viewing mesh: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            #[cfg(not(feature = "remote"))]
            {
                if let Err(e) = viewer::view_mesh(&input, mesh.as_deref()) {
                    eprintln!("Error viewing mesh: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Check { input, mesh } => {
            if let Err(e) = mesh::check_manifold(&input, mesh.as_deref()) {
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
            if let Err(e) = mesh::fix_holes(
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
            if let Err(e) = glb::inspect_glb(&input, json) {
                eprintln!("Error inspecting GLB: {}", e);
                std::process::exit(1);
            }
        }

        #[cfg(feature = "remote")]
        Commands::Remote { command } => {
            handle_remote_command(command);
        }
    }
}

#[cfg(feature = "remote")]
fn handle_remote_command(command: RemoteCommands) {
    use remote::client;

    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

    let result = rt.block_on(async {
        let url = "http://127.0.0.1:9001";

        let client = match client::create_client(url).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to connect to viewer at {}.", url);
                eprintln!("Make sure the viewer is running with --remote flag.");
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        };

        let result: Result<(), Box<dyn std::error::Error>> = match command {
            RemoteCommands::Load { path, mesh } => {
                let response =
                    client::load_model(&client, path.to_string_lossy().to_string(), mesh).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::Rotate { x, y, z } => {
                let response = client::set_rotation(&client, x, y, z).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::RotateAxis { axis, angle } => {
                let (x, y, z) = axis;
                let response = client::rotate_around_axis(&client, vec![x, y, z], angle).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::CameraPos { x, y, z } => {
                let response = client::set_camera_position(&client, x, y, z).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::CameraTarget { x, y, z } => {
                let response = client::set_camera_target(&client, x, y, z).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::EnableWireframe => {
                let response = client::enable_wireframe(&client).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::DisableWireframe => {
                let response = client::disable_wireframe(&client).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::ToggleWireframe => {
                let response = client::toggle_wireframe(&client).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::EnableBackfaces => {
                let response = client::enable_backfaces(&client).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::DisableBackfaces => {
                let response = client::disable_backfaces(&client).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::ToggleBackfaces => {
                let response = client::toggle_backfaces(&client).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::EnableUi => {
                let response = client::enable_ui(&client).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::DisableUi => {
                let response = client::disable_ui(&client).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::ToggleUi => {
                let response = client::toggle_ui(&client).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::Stats => {
                let stats = client::get_stats(&client).await?;
                println!("\n=== Mesh Statistics ===");
                println!("Vertices:  {}", stats.vertices);
                println!("Edges:     {}", stats.edges);
                println!("Faces:     {}", stats.faces);
                if stats.is_manifold {
                    println!("Manifold:  Yes");
                } else {
                    println!("Manifold:  No ({} holes)", stats.holes);
                }
                Ok(())
            }
            RemoteCommands::Capture { path } => {
                // Convert relative paths to absolute (relative to caller's cwd)
                let absolute_path = if let Some(p) = &path {
                    let path_buf = std::path::PathBuf::from(p);
                    if path_buf.is_absolute() {
                        println!("Using absolute path: {}", p);
                        Some(p.clone())
                    } else {
                        // Make relative paths absolute based on caller's working directory
                        let cwd = std::env::current_dir()
                            .map_err(|e| format!("Failed to get current directory: {}", e))?;
                        let abs = cwd.join(path_buf);
                        let abs_str = abs.to_string_lossy().to_string();
                        println!("Resolved relative path '{}' to: {}", p, abs_str);
                        Some(abs_str)
                    }
                } else {
                    None
                };

                let response = client::capture_frame(&client, absolute_path).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::Screenshot { path } => {
                // Convert relative paths to absolute (relative to caller's cwd)
                let path_buf = std::path::PathBuf::from(&path);
                let absolute_path = if path_buf.is_absolute() {
                    println!("Using absolute path: {}", path);
                    path.clone()
                } else {
                    let cwd = std::env::current_dir()
                        .map_err(|e| format!("Failed to get current directory: {}", e))?;
                    let abs = cwd.join(path_buf);
                    let abs_str = abs.to_string_lossy().to_string();
                    println!("Resolved relative path '{}' to: {}", path, abs_str);
                    abs_str
                };

                let response = client::screenshot(&client, absolute_path).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::Quit => {
                let response = client::quit(&client).await?;
                println!("{}", response);
                Ok(())
            }
        };

        result
    });

    if let Err(e) = result {
        eprintln!("Remote command failed: {}", e);
        std::process::exit(1);
    }
}
