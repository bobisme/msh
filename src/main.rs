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

        /// BVH motion capture file to overlay on the mesh skeleton
        #[arg(long)]
        bvh: Option<PathBuf>,

        /// Animation clip name or index to play on load (default: first)
        #[arg(long)]
        animation: Option<String>,

        /// Uniform scale multiplier (e.g. 50 to scale up a 1-unit model to 50 units)
        #[arg(long)]
        scale: Option<f32>,

        /// Skip auto-centering the mesh at the origin
        #[arg(long)]
        no_center: bool,
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

        /// Animation clip name or index (default: first animation)
        #[arg(long)]
        animation: Option<String>,

        /// Render a single animation frame (0-based frame number)
        #[arg(long)]
        frame: Option<usize>,

        /// Render a range of animation frames (e.g. "0-15")
        #[arg(long)]
        frames: Option<String>,

        /// Number of rotation angles for multi-directional rendering (e.g. 8 for 8-directional)
        #[arg(long, default_value_t = 1)]
        angles: u32,

        /// Rotation offset in degrees added to all angle steps (e.g. 5 shifts 0°,45°,90°... to 5°,50°,95°...)
        #[arg(long, default_value_t = 0.0)]
        angle_offset: f32,

        /// Compose output into a single sprite sheet atlas PNG
        #[arg(long)]
        sprite_sheet: bool,

        /// Uniform scale multiplier (e.g. 50 to scale up a 1-unit model to 50 units)
        #[arg(long)]
        scale: Option<f32>,

        /// Skip auto-centering the mesh at the origin
        #[arg(long)]
        no_center: bool,
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

/// Parse a frame range like "0-15" into (start, end), or a single number "5" into (5, 5).
/// Parse a frame range with optional step: "0-39", "0-39:4", or "5"
fn parse_frame_range(s: &str) -> Result<(usize, usize, usize), String> {
    // Split off optional :step suffix
    let (range_part, step) = if let Some((range, step_str)) = s.split_once(':') {
        let step = step_str
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("Invalid step value: '{}'", step_str.trim()))?;
        if step == 0 {
            return Err("Step value must be > 0".to_string());
        }
        (range, step)
    } else {
        (s, 1)
    };

    if let Some((left, right)) = range_part.split_once('-') {
        let start = left
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("Invalid start frame: '{}'", left.trim()))?;
        let end = right
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("Invalid end frame: '{}'", right.trim()))?;
        if end < start {
            return Err(format!("End frame {} is less than start frame {}", end, start));
        }
        Ok((start, end, step))
    } else {
        let n = range_part
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("Invalid frame number: '{}'", range_part.trim()))?;
        Ok((n, n, step))
    }
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
            bvh,
            animation,
            scale,
            no_center,
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

            // Handle BVH motion capture files (parse-only for now)
            #[cfg(feature = "remote")]
            let input_path: Option<&PathBuf> = input.as_ref();
            #[cfg(not(feature = "remote"))]
            let input_path: Option<&PathBuf> = Some(&input);

            if let Some(path) = input_path {
                if path.extension().and_then(|s| s.to_str()) == Some("bvh") {
                    if let Err(e) = viewer::view_bvh(path, no_vsync, build_state) {
                        eprintln!("Error viewing BVH: {}", e);
                        std::process::exit(1);
                    }
                    return;
                }
            }

            #[cfg(feature = "remote")]
            {
                if remote {
                    if let Err(e) = viewer::view_mesh_with_rpc(input.as_ref(), mesh.as_deref(), no_vsync, z_up, build_state) {
                        eprintln!("Error viewing mesh: {}", e);
                        std::process::exit(1);
                    }
                } else {
                    let input_ref = input.as_ref().expect("input required when not using --remote");
                    if let Err(e) = viewer::view_mesh_with_bvh(input_ref, mesh.as_deref(), no_vsync, z_up, bvh.as_ref(), animation.as_deref(), scale, no_center, build_state) {
                        eprintln!("Error viewing mesh: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            #[cfg(not(feature = "remote"))]
            {
                if let Err(e) = viewer::view_mesh_with_bvh(&input, mesh.as_deref(), no_vsync, z_up, bvh.as_ref(), animation.as_deref(), scale, no_center, build_state) {
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
            animation,
            frame,
            frames,
            angles,
            angle_offset,
            sprite_sheet,
            scale,
            no_center,
        } => {
            use viewer::state::{ProjectionMode, ShadingMode, RenderPreset, ViewerState};
            use mesh::animation::frame_to_time;
            use mesh::loader::load_mesh_with_colors;

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

            // Determine if we're doing animation/sprite-sheet rendering
            let has_animation_args = frame.is_some() || frames.is_some();

            if !has_animation_args && !sprite_sheet && angles == 1 {
                // --- Original single-image render path ---
                if let Err(e) = viewer::headless::render_to_file(
                    &input, &out_str, mesh.as_deref(), width, height, z_up, camera_pos, camera_target, None, None, scale, no_center, build_state,
                ) {
                    eprintln!("Error rendering: {}", e);
                    std::process::exit(1);
                }
                println!("Rendered to {}", out_str);
            } else {
                // --- Animation / sprite-sheet render path ---

                // Load mesh to inspect animations
                let mesh_data = match load_mesh_with_colors(&input, mesh.as_deref()) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Error loading mesh: {}", e);
                        std::process::exit(1);
                    }
                };

                // Resolve animation index from --animation (name or index)
                let anim_index: usize = if let Some(ref anim_arg) = animation {
                    if let Ok(idx) = anim_arg.parse::<usize>() {
                        if idx >= mesh_data.animations.len() {
                            eprintln!(
                                "Animation index {} out of range (model has {} animations)",
                                idx,
                                mesh_data.animations.len()
                            );
                            std::process::exit(1);
                        }
                        idx
                    } else {
                        // Search by name
                        match mesh_data.animations.iter().position(|a| {
                            a.name.as_deref() == Some(anim_arg.as_str())
                        }) {
                            Some(idx) => idx,
                            None => {
                                eprintln!("Animation '{}' not found. Available animations:", anim_arg);
                                for (i, a) in mesh_data.animations.iter().enumerate() {
                                    eprintln!(
                                        "  [{}] {}",
                                        i,
                                        a.name.as_deref().unwrap_or("<unnamed>")
                                    );
                                }
                                std::process::exit(1);
                            }
                        }
                    }
                } else {
                    0 // default to first animation
                };

                if mesh_data.animations.is_empty() {
                    eprintln!("Error: model has no animations");
                    std::process::exit(1);
                }

                let clip = &mesh_data.animations[anim_index];

                // Determine frame range and step
                let (frame_start, frame_end, frame_step) = if let Some(ref frames_str) = frames {
                    match parse_frame_range(frames_str) {
                        Ok(range) => range,
                        Err(e) => {
                            eprintln!("Invalid --frames value '{}': {}", frames_str, e);
                            std::process::exit(1);
                        }
                    }
                } else if let Some(f) = frame {
                    (f, f, 1)
                } else {
                    // No --frame or --frames but --angles or --sprite-sheet was given;
                    // default to frame 0
                    (0, 0, 1)
                };

                let total_frames = frame_end + 1; // total_frames parameter for frame_to_time

                // Compute animation times for each frame (with step)
                let frame_times: Vec<f32> = (frame_start..=frame_end)
                    .step_by(frame_step)
                    .map(|f| frame_to_time(clip, f, total_frames))
                    .collect();
                let frame_count = frame_times.len();

                // Compute rotation angles (with optional offset in degrees)
                let offset_rad = angle_offset.to_radians();
                let angle_values: Vec<f32> = (0..angles)
                    .map(|i| i as f32 * std::f32::consts::TAU / angles as f32 + offset_rad)
                    .collect();

                if frame_count == 1 && angles == 1 && !sprite_sheet {
                    // Single frame, single angle: use render_to_file directly
                    let anim_time = frame_times[0];
                    if let Err(e) = viewer::headless::render_to_file(
                        &input,
                        &out_str,
                        mesh.as_deref(),
                        width,
                        height,
                        z_up,
                        camera_pos,
                        camera_target,
                        Some(anim_index),
                        Some(anim_time),
                        scale,
                        no_center,
                        build_state,
                    ) {
                        eprintln!("Error rendering: {}", e);
                        std::process::exit(1);
                    }
                    println!("Rendered frame {} (t={:.3}s) to {}", frame_start, anim_time, out_str);
                } else if sprite_sheet {
                    // Sprite sheet atlas mode
                    let config = viewer::sprite_sheet::SpriteSheetConfig {
                        tile_width: width,
                        tile_height: height,
                        angles: angle_values.clone(),
                        frames: frame_times.clone(),
                        animation_index: anim_index,
                        transparent_bg,
                        model_scale: scale.unwrap_or(1.0),
                    };
                    match viewer::sprite_sheet::render_sprite_sheet(
                        &input,
                        &config,
                        mesh.as_deref(),
                        z_up,
                        build_state,
                    ) {
                        Ok((atlas_data, atlas_w, atlas_h)) => {
                            // Save the atlas PNG
                            if let Some(parent) = std::path::Path::new(&out_str).parent() {
                                if !parent.as_os_str().is_empty() {
                                    if let Err(e) = std::fs::create_dir_all(parent) {
                                        eprintln!("Error creating output directory: {}", e);
                                        std::process::exit(1);
                                    }
                                }
                            }
                            if let Err(e) = image::save_buffer(
                                &out_str,
                                &atlas_data,
                                atlas_w,
                                atlas_h,
                                image::ColorType::Rgba8,
                            ) {
                                eprintln!("Error saving sprite sheet: {}", e);
                                std::process::exit(1);
                            }
                            let tile_count = frame_count * angles as usize;
                            println!(
                                "Rendered {} tiles ({} angles x {} frames) to {}",
                                tile_count, angles, frame_count, out_str
                            );
                        }
                        Err(e) => {
                            eprintln!("Error rendering sprite sheet: {}", e);
                            std::process::exit(1);
                        }
                    }
                } else {
                    // Individual frames mode (--frames and/or --angles without --sprite-sheet)
                    let config = viewer::sprite_sheet::SpriteSheetConfig {
                        tile_width: width,
                        tile_height: height,
                        angles: angle_values.clone(),
                        frames: frame_times.clone(),
                        animation_index: anim_index,
                        transparent_bg,
                        model_scale: scale.unwrap_or(1.0),
                    };
                    if let Err(e) = viewer::sprite_sheet::render_frames(
                        &input,
                        &out_str,
                        &config,
                        mesh.as_deref(),
                        z_up,
                        build_state,
                    ) {
                        eprintln!("Error rendering frames: {}", e);
                        std::process::exit(1);
                    }
                    let tile_count = frame_count * angles as usize;
                    println!(
                        "Rendered {} frames ({} angles x {} frames) to {}",
                        tile_count, angles, frame_count, out_str
                    );
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
