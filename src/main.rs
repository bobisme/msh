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

        /// Disable vsync (unlocked framerate)
        #[arg(long)]
        no_vsync: bool,

        /// Projection mode: perspective or ortho
        #[arg(long)]
        projection: Option<String>,

        /// Orthographic world height (default: 10.0)
        #[arg(long)]
        ortho_height: Option<f32>,

        /// Field of view in degrees (default: 45.0)
        #[arg(long)]
        fov_deg: Option<f32>,

        /// Clear color as r,g,b,a (0.0-1.0)
        #[arg(long, value_parser = parse_color)]
        clear_color: Option<(f32, f32, f32, f32)>,

        /// Use transparent background
        #[arg(long)]
        transparent_bg: bool,

        /// Shading mode: lit, flat, or unlit
        #[arg(long)]
        shading: Option<String>,

        /// Base color as r,g,b,a (0.0-1.0)
        #[arg(long, value_parser = parse_color)]
        base_color: Option<(f32, f32, f32, f32)>,

        /// Light direction as x,y,z
        #[arg(long, value_parser = parse_axis)]
        light_dir: Option<(f32, f32, f32)>,

        /// Render preset: viewer or sprite-bake
        #[arg(long)]
        preset: Option<String>,

        /// Treat input as Z-up and convert to Y-up (for OpenSCAD, CAD tools)
        #[arg(long)]
        z_up: bool,
    },

    /// Render mesh to PNG without opening a window
    #[command(allow_negative_numbers = true, allow_hyphen_values = true)]
    Render {
        /// Input mesh file (.obj, .glb, or .3mf)
        input: PathBuf,

        /// Output PNG file
        #[arg(short, long)]
        out: PathBuf,

        /// Mesh name (required if GLB contains multiple meshes)
        #[arg(short, long)]
        mesh: Option<String>,

        /// Image width in pixels (default: 800)
        #[arg(long, default_value_t = 800)]
        width: u32,

        /// Image height in pixels (default: 600)
        #[arg(long, default_value_t = 600)]
        height: u32,

        /// Projection mode: perspective or ortho
        #[arg(long)]
        projection: Option<String>,

        /// Orthographic world height (default: 10.0)
        #[arg(long)]
        ortho_height: Option<f32>,

        /// Field of view in degrees (default: 45.0)
        #[arg(long)]
        fov_deg: Option<f32>,

        /// Clear color as r,g,b,a (0.0-1.0)
        #[arg(long, value_parser = parse_color)]
        clear_color: Option<(f32, f32, f32, f32)>,

        /// Use transparent background
        #[arg(long)]
        transparent_bg: bool,

        /// Shading mode: lit, flat, or unlit
        #[arg(long)]
        shading: Option<String>,

        /// Base color as r,g,b,a (0.0-1.0)
        #[arg(long, value_parser = parse_color)]
        base_color: Option<(f32, f32, f32, f32)>,

        /// Light direction as x,y,z
        #[arg(long, value_parser = parse_axis, allow_hyphen_values = true)]
        light_dir: Option<(f32, f32, f32)>,

        /// Render preset: viewer or sprite-bake
        #[arg(long)]
        preset: Option<String>,

        /// Treat input as Z-up and convert to Y-up (for OpenSCAD, CAD tools)
        #[arg(long)]
        z_up: bool,

        /// Camera position as x,y,z (use = for negatives: --camera-pos=-1,2,3)
        #[arg(long, value_parser = parse_axis, allow_hyphen_values = true)]
        camera_pos: Option<(f32, f32, f32)>,

        /// Camera look-at target as x,y,z (use = for negatives: --camera-target=-1,0,0)
        #[arg(long, value_parser = parse_axis, allow_hyphen_values = true)]
        camera_target: Option<(f32, f32, f32)>,
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
    #[command(allow_negative_numbers = true)]
    Rotate {
        /// X rotation (radians)
        x: f32,
        /// Y rotation (radians)
        y: f32,
        /// Z rotation (radians)
        z: f32,
    },

    /// Rotate model around an axis
    #[command(allow_negative_numbers = true)]
    RotateAxis {
        /// Axis as x,y,z
        #[arg(value_parser = parse_axis)]
        axis: (f32, f32, f32),

        /// Angle (e.g., "90d" for degrees or "1.57r" for radians)
        angle: String,
    },

    /// Set camera position
    #[command(allow_negative_numbers = true)]
    CameraPos {
        /// X position
        x: f32,
        /// Y position
        y: f32,
        /// Z position
        z: f32,
    },

    /// Set camera target (look-at point)
    #[command(allow_negative_numbers = true)]
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

    /// Set projection mode
    SetProjection {
        /// Mode: perspective or ortho
        mode: String,
        /// Optional value: FOV degrees for perspective, world height for ortho
        value: Option<f32>,
    },

    /// Set clear color (RGBA, 0.0-1.0)
    #[command(allow_negative_numbers = true)]
    SetClearColor {
        r: f32,
        g: f32,
        b: f32,
        a: f32,
    },

    /// Set transparent background
    TransparentBg,

    /// Set shading mode
    SetShading {
        /// Mode: lit, flat, or unlit
        mode: String,
    },

    /// Set base color (RGBA, 0.0-1.0)
    #[command(allow_negative_numbers = true)]
    SetBaseColor {
        r: f32,
        g: f32,
        b: f32,
        a: f32,
    },

    /// Set light direction
    #[command(allow_negative_numbers = true)]
    SetLightDir {
        x: f32,
        y: f32,
        z: f32,
    },

    /// Apply a render preset
    Preset {
        /// Preset name: viewer or sprite-bake
        name: String,
    },

    /// Quit the running viewer
    Quit,
}

fn parse_color(s: &str) -> Result<(f32, f32, f32, f32), String> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 4 {
        return Err("Color must be in format: r,g,b,a".to_string());
    }
    let r = parts[0].trim().parse::<f32>().map_err(|_| format!("Invalid r value: {}", parts[0]))?;
    let g = parts[1].trim().parse::<f32>().map_err(|_| format!("Invalid g value: {}", parts[1]))?;
    let b = parts[2].trim().parse::<f32>().map_err(|_| format!("Invalid b value: {}", parts[2]))?;
    let a = parts[3].trim().parse::<f32>().map_err(|_| format!("Invalid a value: {}", parts[3]))?;
    Ok((r, g, b, a))
}

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
            no_vsync,
            projection,
            ortho_height,
            fov_deg,
            clear_color,
            transparent_bg,
            shading,
            base_color,
            light_dir,
            preset,
            z_up,
        } => {
            use viewer::state::{ProjectionMode, ShadingMode, RenderPreset, ViewerState};

            // Build initial render state from CLI flags
            let build_state = |state: &mut ViewerState| {
                // Apply preset first (individual flags override)
                if let Some(ref preset_name) = preset {
                    if let Some(p) = RenderPreset::by_name(preset_name) {
                        state.apply_preset(&p);
                    } else {
                        eprintln!("Unknown preset: {}", preset_name);
                        std::process::exit(1);
                    }
                }
                // Individual overrides
                if let Some(ref proj) = projection {
                    state.projection = match proj.as_str() {
                        "perspective" => ProjectionMode::Perspective { fov_y_degrees: fov_deg.unwrap_or(45.0) },
                        "ortho" | "orthographic" => ProjectionMode::Orthographic { world_height: ortho_height.unwrap_or(10.0) },
                        _ => {
                            eprintln!("Invalid projection: {}. Use 'perspective' or 'ortho'", proj);
                            std::process::exit(1);
                        }
                    };
                } else {
                    if let Some(fov) = fov_deg {
                        state.projection = ProjectionMode::Perspective { fov_y_degrees: fov };
                    }
                    if let Some(h) = ortho_height {
                        state.projection = ProjectionMode::Orthographic { world_height: h };
                    }
                }
                if transparent_bg {
                    state.clear_color = [0.0, 0.0, 0.0, 0.0];
                }
                if let Some((r, g, b, a)) = clear_color {
                    state.clear_color = [r, g, b, a];
                }
                if let Some(ref mode) = shading {
                    state.shading = match mode.as_str() {
                        "lit" => ShadingMode::Lit,
                        "flat" => ShadingMode::Flat,
                        "unlit" => ShadingMode::Unlit,
                        _ => {
                            eprintln!("Invalid shading: {}. Use 'lit', 'flat', or 'unlit'", mode);
                            std::process::exit(1);
                        }
                    };
                }
                if let Some((r, g, b, a)) = base_color {
                    state.base_color = [r, g, b, a];
                }
                if let Some((x, y, z)) = light_dir {
                    state.light_direction = [x, y, z];
                }
            };

            #[cfg(feature = "remote")]
            {
                if remote {
                    if let Err(e) = viewer::view_mesh_with_rpc(input.as_ref(), mesh.as_deref(), no_vsync, z_up, build_state) {
                        eprintln!("Error viewing mesh: {}", e);
                        std::process::exit(1);
                    }
                } else {
                    let input_ref = input.as_ref().expect("input required when not using --remote");
                    if let Err(e) = viewer::view_mesh(input_ref, mesh.as_deref(), no_vsync, z_up, build_state) {
                        eprintln!("Error viewing mesh: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            #[cfg(not(feature = "remote"))]
            {
                if let Err(e) = viewer::view_mesh(&input, mesh.as_deref(), no_vsync, z_up, build_state) {
                    eprintln!("Error viewing mesh: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Render {
            input,
            out,
            mesh,
            width,
            height,
            projection,
            ortho_height,
            fov_deg,
            clear_color,
            transparent_bg,
            shading,
            base_color,
            light_dir,
            preset,
            z_up,
            camera_pos,
            camera_target,
        } => {
            use viewer::state::{ProjectionMode, ShadingMode, RenderPreset, ViewerState};

            let build_state = |state: &mut ViewerState| {
                if let Some(ref preset_name) = preset {
                    if let Some(p) = RenderPreset::by_name(preset_name) {
                        state.apply_preset(&p);
                    } else {
                        eprintln!("Unknown preset: {}", preset_name);
                        std::process::exit(1);
                    }
                }
                if let Some(ref proj) = projection {
                    state.projection = match proj.as_str() {
                        "perspective" => ProjectionMode::Perspective { fov_y_degrees: fov_deg.unwrap_or(45.0) },
                        "ortho" | "orthographic" => ProjectionMode::Orthographic { world_height: ortho_height.unwrap_or(10.0) },
                        _ => {
                            eprintln!("Invalid projection: {}. Use 'perspective' or 'ortho'", proj);
                            std::process::exit(1);
                        }
                    };
                } else {
                    if let Some(fov) = fov_deg {
                        state.projection = ProjectionMode::Perspective { fov_y_degrees: fov };
                    }
                    if let Some(h) = ortho_height {
                        state.projection = ProjectionMode::Orthographic { world_height: h };
                    }
                }
                if transparent_bg {
                    state.clear_color = [0.0, 0.0, 0.0, 0.0];
                }
                if let Some((r, g, b, a)) = clear_color {
                    state.clear_color = [r, g, b, a];
                }
                if let Some(ref mode) = shading {
                    state.shading = match mode.as_str() {
                        "lit" => ShadingMode::Lit,
                        "flat" => ShadingMode::Flat,
                        "unlit" => ShadingMode::Unlit,
                        _ => {
                            eprintln!("Invalid shading: {}. Use 'lit', 'flat', or 'unlit'", mode);
                            std::process::exit(1);
                        }
                    };
                }
                if let Some((r, g, b, a)) = base_color {
                    state.base_color = [r, g, b, a];
                }
                if let Some((x, y, z)) = light_dir {
                    state.light_direction = [x, y, z];
                }
            };

            let out_str = out.to_string_lossy().to_string();
            if let Err(e) = viewer::headless::render_to_file(
                &input, &out_str, mesh.as_deref(), width, height, z_up, camera_pos, camera_target, build_state,
            ) {
                eprintln!("Error rendering: {}", e);
                std::process::exit(1);
            }
            println!("Rendered to {}", out_str);
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
            RemoteCommands::SetProjection { mode, value } => {
                let response = client::set_projection(&client, mode, value).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::SetClearColor { r, g, b, a } => {
                let response = client::set_clear_color(&client, r, g, b, a).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::TransparentBg => {
                let response = client::set_clear_color(&client, 0.0, 0.0, 0.0, 0.0).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::SetShading { mode } => {
                let response = client::set_shading(&client, mode).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::SetBaseColor { r, g, b, a } => {
                let response = client::set_base_color(&client, r, g, b, a).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::SetLightDir { x, y, z } => {
                let response = client::set_light_direction(&client, x, y, z).await?;
                println!("{}", response);
                Ok(())
            }
            RemoteCommands::Preset { name } => {
                let response = client::apply_preset(&client, name).await?;
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
