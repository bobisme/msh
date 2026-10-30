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

    /// Simplify mesh using QEM (Quadric Error Metric) edge collapse
    Simplify {
        /// Input mesh file (.obj or .glb)
        input: PathBuf,

        /// Output mesh file (.obj)
        #[arg(short, long)]
        out: PathBuf,

        /// Mesh name (required if GLB contains multiple meshes)
        #[arg(short, long)]
        mesh: Option<String>,

        /// Target vertex count
        #[arg(long)]
        target_verts: Option<usize>,

        /// Target ratio (0.0-1.0, fraction of original face count)
        #[arg(long)]
        ratio: Option<f32>,

        /// Maximum quadric error threshold
        #[arg(long)]
        max_error: Option<f32>,

        /// Preserve boundary edges
        #[arg(long)]
        keep_boundary: bool,

        /// Transfer bone weights from input to simplified output via nearest-vertex lookup
        #[arg(long)]
        preserve_weights: bool,
    },

    /// Project simplified mesh vertices back toward a reference surface
    Project {
        /// Input mesh file (simplified mesh)
        input: PathBuf,

        /// Output mesh file (.obj)
        #[arg(short, long)]
        out: PathBuf,

        /// Reference mesh file (original pre-simplification mesh)
        #[arg(long)]
        reference: PathBuf,

        /// Mesh name for input (if GLB with multiple meshes)
        #[arg(short, long)]
        mesh: Option<String>,

        /// Number of projection iterations (default: 10)
        #[arg(long, default_value_t = 10)]
        iterations: usize,

        /// Projection blend weight, 0.0-1.0 (default: 0.5)
        #[arg(long, default_value_t = 0.5)]
        alpha: f32,

        /// Laplacian smoothing weight (default: 0.1)
        #[arg(long, default_value_t = 0.1)]
        beta: f32,

        /// Dihedral angle threshold for crease detection in degrees (default: 30)
        #[arg(long, default_value_t = 30.0)]
        crease_angle: f32,

        /// Crease snap distance (default: auto from average edge length)
        #[arg(long)]
        snap_threshold: Option<f32>,
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

        /// Transfer bone weights from input to remeshed output via nearest-vertex lookup
        #[arg(long)]
        preserve_weights: bool,
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
        input: Option<PathBuf>,

        /// Output mesh file (.obj)
        #[arg(short, long)]
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
        /// Machine-readable raw and welded topology
        #[arg(long)]
        json: bool,
        /// Absolute position-weld distance for analysis only
        #[arg(long)]
        weld_tolerance: Option<f64>,
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

        /// Loopback RPC port; 0 chooses a free port and prints RPC_READY
        #[cfg(feature = "remote")]
        #[arg(long, default_value_t = 9001)]
        port: u16,

        /// Initial window dimensions, e.g. 800x600 (compositor may override)
        #[arg(long, value_parser = parse_size)]
        size: Option<[u32; 2]>,

        /// Label the viewer window
        #[arg(long)]
        title: Option<String>,

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

        /// Hide the ground-plane shadow
        #[arg(long)]
        no_ground: bool,

        /// Show reversed-facing surfaces in red
        #[arg(long)]
        backfaces: bool,

        /// Backface diagnostic shading
        #[arg(long, default_value = "lit", value_parser = ["lit", "unlit"])]
        backface_mode: String,

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

        /// Import compatible animation clips from a separate GLB/glTF file
        #[arg(long)]
        clips: Option<PathBuf>,

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

        /// Hide the ground-plane shadow
        #[arg(long)]
        no_ground: bool,

        /// Show reversed-facing surfaces in red
        #[arg(long)]
        backfaces: bool,

        /// Backface diagnostic shading
        #[arg(long, default_value = "lit", value_parser = ["lit", "unlit"])]
        backface_mode: String,

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

        /// Import compatible animation clips from a separate GLB/glTF file
        #[arg(long)]
        clips: Option<PathBuf>,

        /// Render a single animation frame (0-based frame number)
        #[arg(long)]
        frame: Option<usize>,

        /// Render a range of animation frames (e.g. "0-15")
        #[arg(long)]
        frames: Option<String>,

        /// Number of rotation angles for multi-directional rendering (e.g. 8 for 8-directional)
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=64))]
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

    /// Simplify a mesh for game LODs (voxel remesh + QEM decimation)
    Clean {
        /// Input mesh file (.obj or .glb)
        input: PathBuf,

        /// Output mesh file (.obj)
        #[arg(short, long)]
        out: PathBuf,

        /// Mesh name (required if GLB contains multiple meshes)
        #[arg(short, long)]
        mesh: Option<String>,

        /// Target vertex count
        #[arg(long)]
        target_verts: Option<usize>,

        /// Target ratio (0.0-1.0, fraction of original face count to keep after simplification)
        #[arg(long)]
        ratio: Option<f32>,

        /// Override voxel size (default: auto from bounding box)
        #[arg(long)]
        voxel_size: Option<f32>,

        /// Voxel grid resolution per axis (default: 250, used for auto voxel size)
        #[arg(long, default_value_t = 250)]
        resolution: u32,

        /// Transfer bone weights from input to output via nearest-vertex lookup
        #[arg(long)]
        preserve_weights: bool,
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

    /// Serve persistent offscreen rendering without a desktop window
    #[cfg(feature = "remote")]
    Serve {
        #[arg(long, default_value_t = 9001)]
        port: u16,
        #[arg(long, default_value = "800x600", value_parser = parse_size)]
        size: [u32; 2],
        #[arg(long)]
        no_ground: bool,
        #[arg(long)]
        clips: Option<PathBuf>,
        #[arg(long, value_parser = parse_color)]
        clear_color: Option<(f32, f32, f32, f32)>,
        #[arg(long, default_value = "lit", value_parser = ["lit","flat","unlit"])]
        shading: String,
        #[arg(long)]
        backfaces: bool,
        #[arg(long, default_value = "lit", value_parser = ["lit","unlit"])]
        backface_mode: String,
    },

    /// Remote control commands for running viewer
    #[cfg(feature = "remote")]
    Remote {
        /// Port of the viewer or offscreen service
        #[arg(long, global = true, default_value_t = 9001)]
        port: u16,
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
        /// Preserve the existing camera pose
        #[arg(long)]
        keep_camera: bool,
        #[arg(long)]
        clips: Option<PathBuf>,
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

    /// Set eye and target together; optional up and vertical FOV
    Camera {
        #[arg(long, value_parser = parse_axis, allow_hyphen_values = true)]
        eye: (f32, f32, f32),
        #[arg(long, value_parser = parse_axis, allow_hyphen_values = true)]
        target: (f32, f32, f32),
        #[arg(long, value_parser = parse_axis, allow_hyphen_values = true)]
        up: Option<(f32, f32, f32)>,
        #[arg(long)]
        fov: Option<f32>,
    },
    EnableGround,
    DisableGround,
    ToggleGround,

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

    /// Attach a compatible external animation file to the loaded skin
    LoadClips {
        path: PathBuf,
    },
    /// List clip names, durations and selected time
    ListAnimations,
    /// Select a clip by name or index, paused at time zero
    SetAnimation {
        name: String,
    },
    /// Pause at an exact clip time in seconds
    SetTime {
        time: f32,
    },
    /// Pause at frame / fps seconds (default 30 FPS)
    SetFrame {
        frame: u32,
        #[arg(long, default_value_t = 30.0)]
        fps: f32,
    },

    /// Exact visible material and backface pixel counts from an integer ID pass
    BackfaceStats,
    /// Capture an ordered batch described by a JSON array file
    Shoot {
        file: PathBuf,
    },

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

    /// Choose lit red or unlit pure red backface diagnostics
    SetBackfaceMode {
        #[arg(value_parser = ["lit", "unlit"])]
        mode: String,
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
    let r = parts[0]
        .trim()
        .parse::<f32>()
        .map_err(|_| format!("Invalid r value: {}", parts[0]))?;
    let g = parts[1]
        .trim()
        .parse::<f32>()
        .map_err(|_| format!("Invalid g value: {}", parts[1]))?;
    let b = parts[2]
        .trim()
        .parse::<f32>()
        .map_err(|_| format!("Invalid b value: {}", parts[2]))?;
    let a = parts[3]
        .trim()
        .parse::<f32>()
        .map_err(|_| format!("Invalid a value: {}", parts[3]))?;
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
            return Err(format!(
                "End frame {} is less than start frame {}",
                end, start
            ));
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
                Some(RemeshCommands::Simplify {
                    input,
                    out,
                    mesh,
                    target_verts,
                    ratio,
                    max_error,
                    keep_boundary,
                    preserve_weights,
                }) => {
                    if preserve_weights {
                        use mesh::loader::load_mesh_with_colors;
                        use mesh::writer::SkinData;

                        // Load original mesh with skin data
                        let original = match load_mesh_with_colors(&input, mesh.as_deref()) {
                            Ok(m) => m,
                            Err(e) => {
                                eprintln!("Error loading mesh: {}", e);
                                std::process::exit(1);
                            }
                        };
                        if original.skeleton.is_none() || original.joint_indices.is_empty() {
                            eprintln!(
                                "Warning: input has no skin data, --preserve-weights has no effect"
                            );
                        }

                        // Run QEM simplification to a temp file
                        let tmp_simplified = std::env::temp_dir().join("msh_simplify_weights.obj");
                        if let Err(e) = mesh::remesh_simplify(
                            &input,
                            &tmp_simplified,
                            mesh.as_deref(),
                            target_verts,
                            ratio,
                            max_error,
                            keep_boundary,
                        ) {
                            eprintln!("Error during simplification: {}", e);
                            std::process::exit(1);
                        }

                        // Load simplified mesh geometry
                        let simplified = match load_mesh_with_colors(&tmp_simplified, None) {
                            Ok(m) => m,
                            Err(e) => {
                                eprintln!("Error loading simplified mesh: {}", e);
                                std::process::exit(1);
                            }
                        };
                        let _ = std::fs::remove_file(&tmp_simplified);

                        // Transfer weights from original to simplified
                        let (new_joints, new_weights) = mesh::transfer_weights_nearest_vertex(
                            &original.positions,
                            &original.joint_indices,
                            &original.joint_weights,
                            &simplified.positions,
                        );

                        let skin_data = original.skeleton.as_ref().map(|skel| SkinData {
                            skeleton: skel,
                            joint_indices: &new_joints,
                            joint_weights: &new_weights,
                            animations: &original.animations,
                        });

                        if let Err(e) = mesh::writer::write_mesh(
                            &out,
                            &simplified.positions,
                            &simplified.face_indices,
                            None,
                            None,
                            skin_data.as_ref(),
                        ) {
                            eprintln!("Error writing output: {}", e);
                            std::process::exit(1);
                        }
                        println!("Written to {:?} with skin data", out);
                    } else if let Err(e) = mesh::remesh_simplify(
                        &input,
                        &out,
                        mesh.as_deref(),
                        target_verts,
                        ratio,
                        max_error,
                        keep_boundary,
                    ) {
                        eprintln!("Error during simplification: {}", e);
                        std::process::exit(1);
                    }
                }
                Some(RemeshCommands::Project {
                    input,
                    out,
                    reference,
                    mesh,
                    iterations,
                    alpha,
                    beta,
                    crease_angle,
                    snap_threshold,
                }) => {
                    use mesh::loader::load_mesh_with_colors;
                    use mesh::projection::{
                        ProjectionConfig, SpatialHashGrid, average_edge_length, project_with_solver,
                    };

                    // Load simplified mesh
                    let simplified = match load_mesh_with_colors(&input, mesh.as_deref()) {
                        Ok(m) => m,
                        Err(e) => {
                            eprintln!("Error loading simplified mesh: {}", e);
                            std::process::exit(1);
                        }
                    };
                    let mut positions: Vec<nalgebra::Point3<f32>> = simplified
                        .positions
                        .iter()
                        .map(|p| nalgebra::Point3::new(p[0], p[1], p[2]))
                        .collect();
                    let triangles: Vec<[u32; 3]> = simplified
                        .face_indices
                        .iter()
                        .map(|f| [f[0], f[1], f[2]])
                        .collect();

                    // Load reference mesh
                    let reference_mesh = match load_mesh_with_colors(&reference, None) {
                        Ok(m) => m,
                        Err(e) => {
                            eprintln!("Error loading reference mesh: {}", e);
                            std::process::exit(1);
                        }
                    };
                    let ref_positions: Vec<nalgebra::Point3<f32>> = reference_mesh
                        .positions
                        .iter()
                        .map(|p| nalgebra::Point3::new(p[0], p[1], p[2]))
                        .collect();
                    let ref_triangles: Vec<[u32; 3]> = reference_mesh
                        .face_indices
                        .iter()
                        .map(|f| [f[0], f[1], f[2]])
                        .collect();

                    // Build spatial hash from reference mesh
                    let avg_edge = average_edge_length(&ref_positions, &ref_triangles);
                    let cell_size = avg_edge * 2.0;
                    println!(
                        "Reference mesh: {} verts, {} faces, avg edge length {:.4}",
                        ref_positions.len(),
                        ref_triangles.len(),
                        avg_edge
                    );
                    let grid = SpatialHashGrid::new(&ref_positions, &ref_triangles, cell_size);

                    let config = ProjectionConfig {
                        iterations,
                        alpha,
                        beta,
                        snap_threshold: snap_threshold.unwrap_or(0.0),
                    };

                    println!(
                        "Projecting {} vertices ({} iterations, alpha={})...",
                        positions.len(),
                        iterations,
                        alpha,
                    );
                    let stats = project_with_solver(&mut positions, &triangles, &grid, &config);

                    println!(
                        "Done: {} iterations, avg displacement {:.6}, max displacement {:.6}",
                        stats.iterations_run, stats.avg_displacement, stats.max_displacement
                    );

                    // Write output OBJ
                    let out_str = out.to_string_lossy();
                    let mut obj_content = String::new();
                    for pos in &positions {
                        obj_content.push_str(&format!("v {} {} {}\n", pos.x, pos.y, pos.z));
                    }
                    for tri in &triangles {
                        obj_content.push_str(&format!(
                            "f {} {} {}\n",
                            tri[0] + 1,
                            tri[1] + 1,
                            tri[2] + 1
                        ));
                    }
                    std::fs::write(&out, &obj_content)
                        .map_err(|e| {
                            eprintln!("Error writing output: {}", e);
                            std::process::exit(1);
                        })
                        .ok();
                    println!("Written to {}", out_str);
                }
                Some(RemeshCommands::Voxel {
                    input,
                    out,
                    mesh,
                    size,
                    method,
                    preserve_weights,
                }) => {
                    if preserve_weights {
                        use mesh::loader::load_mesh_with_colors;
                        use mesh::writer::SkinData;

                        // Load original mesh with skin data
                        let original = match load_mesh_with_colors(&input, mesh.as_deref()) {
                            Ok(m) => m,
                            Err(e) => {
                                eprintln!("Error loading mesh: {}", e);
                                std::process::exit(1);
                            }
                        };
                        if original.skeleton.is_none() || original.joint_indices.is_empty() {
                            eprintln!(
                                "Warning: input has no skin data, --preserve-weights has no effect"
                            );
                        }

                        // Voxel remesh to temp file
                        let tmp_voxel = std::env::temp_dir().join("msh_voxel_weights.obj");
                        if let Err(e) = mesh::remesh_voxel(
                            &input,
                            &tmp_voxel,
                            mesh.as_deref(),
                            size,
                            method.into(),
                        ) {
                            eprintln!("Error during voxel remeshing: {}", e);
                            std::process::exit(1);
                        }

                        // Load remeshed geometry
                        let remeshed = match load_mesh_with_colors(&tmp_voxel, None) {
                            Ok(m) => m,
                            Err(e) => {
                                eprintln!("Error loading remeshed mesh: {}", e);
                                std::process::exit(1);
                            }
                        };
                        let _ = std::fs::remove_file(&tmp_voxel);

                        // Transfer weights
                        let (new_joints, new_weights) = mesh::transfer_weights_nearest_vertex(
                            &original.positions,
                            &original.joint_indices,
                            &original.joint_weights,
                            &remeshed.positions,
                        );

                        let skin_data = original.skeleton.as_ref().map(|skel| SkinData {
                            skeleton: skel,
                            joint_indices: &new_joints,
                            joint_weights: &new_weights,
                            animations: &original.animations,
                        });

                        if let Err(e) = mesh::writer::write_mesh(
                            &out,
                            &remeshed.positions,
                            &remeshed.face_indices,
                            None,
                            None,
                            skin_data.as_ref(),
                        ) {
                            eprintln!("Error writing output: {}", e);
                            std::process::exit(1);
                        }
                        println!("Written to {:?} with skin data", out);
                    } else if let Err(e) =
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
        Commands::Stats {
            input,
            mesh,
            json,
            weld_tolerance,
        } => {
            if let Err(e) = mesh::show_stats(&input, mesh.as_deref(), json, weld_tolerance) {
                eprintln!("Error reading mesh stats: {}", e);
                std::process::exit(1);
            }
        }
        Commands::View {
            input,
            mesh,
            #[cfg(feature = "remote")]
            remote,
            #[cfg(feature = "remote")]
            port,
            size,
            title,
            no_vsync,
            projection,
            ortho_height,
            fov_deg,
            clear_color,
            transparent_bg,
            no_ground,
            backfaces,
            backface_mode,
            shading,
            base_color,
            light_dir,
            preset,
            z_up,
            bvh,
            animation,
            clips,
            scale,
            no_center,
        } => {
            use viewer::state::{
                Background, ProjectionMode, RenderPreset, ShadingMode, ViewerState,
            };

            // Build initial render state from CLI flags
            let build_state = |state: &mut ViewerState| {
                state.clips_path = clips.clone();
                state.window_size = size;
                state.window_title = title.clone();
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
                        "perspective" => ProjectionMode::Perspective {
                            fov_y_degrees: fov_deg.unwrap_or(45.0),
                        },
                        "ortho" | "orthographic" => ProjectionMode::Orthographic {
                            world_height: ortho_height.unwrap_or(10.0),
                        },
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
                    state.background = Background::Solid([0.0, 0.0, 0.0, 0.0]);
                }
                if no_ground {
                    state.show_ground = false;
                }
                if backfaces {
                    state.show_backfaces = true;
                }
                state.backface_unlit = backface_mode == "unlit";
                if let Some((r, g, b, a)) = clear_color {
                    state.background = Background::Solid([r, g, b, a]);
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
                    if let Err(e) = viewer::view_mesh_with_rpc(
                        input.as_ref(),
                        mesh.as_deref(),
                        no_vsync,
                        z_up,
                        port,
                        build_state,
                    ) {
                        eprintln!("Error viewing mesh: {}", e);
                        std::process::exit(1);
                    }
                } else {
                    let input_ref = input
                        .as_ref()
                        .expect("input required when not using --remote");
                    if let Err(e) = viewer::view_mesh_with_bvh(
                        input_ref,
                        mesh.as_deref(),
                        no_vsync,
                        z_up,
                        bvh.as_ref(),
                        animation.as_deref(),
                        scale,
                        no_center,
                        build_state,
                    ) {
                        eprintln!("Error viewing mesh: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            #[cfg(not(feature = "remote"))]
            {
                if let Err(e) = viewer::view_mesh_with_bvh(
                    &input,
                    mesh.as_deref(),
                    no_vsync,
                    z_up,
                    bvh.as_ref(),
                    animation.as_deref(),
                    scale,
                    no_center,
                    build_state,
                ) {
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
            no_ground,
            backfaces,
            backface_mode,
            shading,
            base_color,
            light_dir,
            preset,
            z_up,
            camera_pos,
            camera_target,
            animation,
            clips,
            frame,
            frames,
            angles,
            angle_offset,
            sprite_sheet,
            scale,
            no_center,
        } => {
            use mesh::animation::frame_to_time;
            use mesh::loader::load_mesh_with_colors;
            use viewer::state::{
                Background, ProjectionMode, RenderPreset, ShadingMode, ViewerState,
            };

            let build_state = |state: &mut ViewerState| {
                state.clips_path = clips.clone();
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
                        "perspective" => ProjectionMode::Perspective {
                            fov_y_degrees: fov_deg.unwrap_or(45.0),
                        },
                        "ortho" | "orthographic" => ProjectionMode::Orthographic {
                            world_height: ortho_height.unwrap_or(10.0),
                        },
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
                    state.background = Background::Solid([0.0, 0.0, 0.0, 0.0]);
                }
                if no_ground {
                    state.show_ground = false;
                }
                if backfaces {
                    state.show_backfaces = true;
                }
                state.backface_unlit = backface_mode == "unlit";
                if let Some((r, g, b, a)) = clear_color {
                    state.background = Background::Solid([r, g, b, a]);
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
            let has_animation_args = frame.is_some() || frames.is_some() || animation.is_some();

            if !has_animation_args && !sprite_sheet && angles == 1 {
                // --- Original single-image render path ---
                if let Err(e) = viewer::headless::render_to_file(
                    &input,
                    &out_str,
                    mesh.as_deref(),
                    width,
                    height,
                    z_up,
                    camera_pos,
                    camera_target,
                    None,
                    None,
                    scale,
                    no_center,
                    build_state,
                ) {
                    eprintln!("Error rendering: {}", e);
                    std::process::exit(1);
                }
                println!("Rendered to {}", out_str);
            } else {
                // --- Animation / sprite-sheet render path ---

                // Load mesh to inspect animations
                let mut mesh_data = match load_mesh_with_colors(&input, mesh.as_deref()) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Error loading mesh: {}", e);
                        std::process::exit(1);
                    }
                };

                if let Some(path) = &clips
                    && let Err(error) = mesh::clips::attach(&mut mesh_data, path)
                {
                    eprintln!("Error loading external clips: {error}");
                    std::process::exit(1);
                }

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
                        match mesh_data
                            .animations
                            .iter()
                            .position(|a| a.name.as_deref() == Some(anim_arg.as_str()))
                        {
                            Some(idx) => idx,
                            None => {
                                eprintln!(
                                    "Animation '{}' not found. Available animations:",
                                    anim_arg
                                );
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

                if mesh_data.animations.is_empty() && (has_animation_args || animation.is_some()) {
                    eprintln!("Error: model has no animations");
                    std::process::exit(1);
                }

                let clip = mesh_data.animations.get(anim_index);

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

                let total_frames = frame_end.checked_add(1).unwrap_or_else(|| {
                    eprintln!("Frame range overflows");
                    std::process::exit(1)
                });
                let requested_frames = (frame_end - frame_start) / frame_step + 1;
                if requested_frames > 4096 {
                    eprintln!("At most 4096 frames per render request");
                    std::process::exit(1);
                }
                // Validate output work before collecting frame vectors or allocating tiles.
                if u64::from(width)
                    .checked_mul(u64::from(height))
                    .and_then(|n| n.checked_mul(requested_frames as u64))
                    .and_then(|n| n.checked_mul(u64::from(angles)))
                    .is_none_or(|n| n > 67_108_864)
                {
                    eprintln!("Render request exceeds 64 megapixels total");
                    std::process::exit(1);
                }

                // Compute animation times for each frame (with step)
                let frame_times: Vec<f32> = (frame_start..=frame_end)
                    .step_by(frame_step)
                    .map(|f| clip.map_or(0.0, |clip| frame_to_time(clip, f, total_frames)))
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
                    println!(
                        "Rendered frame {} (t={:.3}s) to {}",
                        frame_start, anim_time, out_str
                    );
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
        Commands::Clean {
            input,
            out,
            mesh,
            target_verts,
            ratio,
            voxel_size,
            resolution,
            preserve_weights,
        } => {
            use mesh::loader::load_mesh_with_colors;

            // Load mesh to compute bounding box for auto voxel size
            let mesh_data = match load_mesh_with_colors(&input, mesh.as_deref()) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Error loading mesh: {}", e);
                    std::process::exit(1);
                }
            };

            let original_verts = mesh_data.positions.len();
            let original_faces = mesh_data.face_indices.len();
            println!(
                "Input: {} vertices, {} faces",
                original_verts, original_faces
            );

            if preserve_weights
                && (mesh_data.skeleton.is_none() || mesh_data.joint_indices.is_empty())
            {
                eprintln!("Warning: input has no skin data, --preserve-weights has no effect");
            }

            // Compute bounding box for auto voxel size
            let voxel_size = voxel_size.unwrap_or_else(|| {
                let mut min = [f32::INFINITY; 3];
                let mut max = [f32::NEG_INFINITY; 3];
                for pos in &mesh_data.positions {
                    for i in 0..3 {
                        min[i] = min[i].min(pos[i]);
                        max[i] = max[i].max(pos[i]);
                    }
                }
                let max_dim = (max[0] - min[0]).max(max[1] - min[1]).max(max[2] - min[2]);
                let auto_size = max_dim / resolution as f32;
                println!(
                    "Auto voxel size: {:.6} (bbox {:.3}, resolution {})",
                    auto_size, max_dim, resolution
                );
                auto_size
            });

            // Stage 1: Voxel remesh to manifold
            let tmp_voxel = std::env::temp_dir().join("msh_clean_voxel.obj");
            println!("Stage 1: Voxel remesh (size {:.6})...", voxel_size);
            if let Err(e) = mesh::remesh_voxel(
                &input,
                &tmp_voxel,
                mesh.as_deref(),
                voxel_size,
                mesh::VoxelMethod::Manifold,
            ) {
                eprintln!("Error during voxel remesh: {}", e);
                std::process::exit(1);
            }

            // Check manifold status of voxel output
            let voxel_mesh = match mesh::loader::load_mesh(&tmp_voxel, None) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Error loading voxel mesh: {}", e);
                    std::process::exit(1);
                }
            };
            let voxel_holes = voxel_mesh.boundary_rings().len();
            drop(voxel_mesh);

            if voxel_holes == 0 {
                println!("Voxel output is manifold, simplifying...");
            } else {
                eprintln!(
                    "Warning: voxel output has {} hole(s), simplifying anyway",
                    voxel_holes
                );
            }

            // Stage 2: QEM simplify the voxel output (even with holes — it's still
            // more regular than the original and QEM handles it much better)
            let tmp_simplified = std::env::temp_dir().join("msh_clean_simplified.obj");
            println!("Stage 2: QEM simplify...");
            if let Err(e) = mesh::remesh_simplify(
                &tmp_voxel,
                &tmp_simplified,
                None,
                target_verts,
                ratio,
                None,
                false,
            ) {
                eprintln!("Error during simplification: {}", e);
                std::process::exit(1);
            }

            // Write output in the requested format
            let simplified = match load_mesh_with_colors(&tmp_simplified, None) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Error loading simplified mesh: {}", e);
                    std::process::exit(1);
                }
            };
            let uvs = if simplified.texcoords.is_empty() {
                None
            } else {
                Some(simplified.texcoords.as_slice())
            };

            // Transfer weights if requested
            let skin_data_storage;
            let skin_ref = if preserve_weights && mesh_data.skeleton.is_some() {
                let (new_joints, new_weights) = mesh::transfer_weights_nearest_vertex(
                    &mesh_data.positions,
                    &mesh_data.joint_indices,
                    &mesh_data.joint_weights,
                    &simplified.positions,
                );
                skin_data_storage = Some((new_joints, new_weights));
                let (joints, weights) = skin_data_storage.as_ref().unwrap();
                Some(mesh::writer::SkinData {
                    skeleton: mesh_data.skeleton.as_ref().unwrap(),
                    joint_indices: joints,
                    joint_weights: weights,
                    animations: &mesh_data.animations,
                })
            } else {
                None
            };

            if let Err(e) = mesh::writer::write_mesh(
                &out,
                &simplified.positions,
                &simplified.face_indices,
                None,
                uvs,
                skin_ref.as_ref(),
            ) {
                eprintln!("Error writing output: {}", e);
                std::process::exit(1);
            }
            if preserve_weights && mesh_data.skeleton.is_some() {
                println!("Written to {:?} with skin data", out);
            } else {
                println!("Written to {:?}", out);
            }

            // Clean up temp files
            let _ = std::fs::remove_file(&tmp_voxel);
            let _ = std::fs::remove_file(&tmp_simplified);
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
        Commands::Serve {
            port,
            size,
            no_ground,
            clips,
            clear_color,
            shading,
            backfaces,
            backface_mode,
        } => {
            let result = viewer::serve::serve(port, size, |state| {
                state.clips_path = clips;
                state.show_ground = !no_ground;
                state.show_backfaces = backfaces;
                state.backface_unlit = backface_mode == "unlit";
                if let Some((r, g, b, a)) = clear_color {
                    state.background = viewer::state::Background::Solid([r, g, b, a]);
                }
                state.shading = match shading.as_str() {
                    "flat" => viewer::state::ShadingMode::Flat,
                    "unlit" => viewer::state::ShadingMode::Unlit,
                    _ => viewer::state::ShadingMode::Lit,
                };
            });
            if let Err(error) = result {
                eprintln!("Offscreen service failed: {error}");
                std::process::exit(1);
            }
        }
        #[cfg(feature = "remote")]
        Commands::Remote { command, port } => {
            handle_remote_command(command, port);
        }
    }
}

#[cfg(feature = "remote")]
fn handle_remote_command(command: RemoteCommands, port: u16) {
    use remote::client;

    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

    let result = rt.block_on(async {
        let url = format!("http://127.0.0.1:{port}");

        let client = match client::create_client(&url).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to connect to viewer at {}.", url);
                eprintln!("Make sure the viewer is running with --remote flag.");
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        };

        let result: Result<(), Box<dyn std::error::Error>> = match command {
            RemoteCommands::Load {
                path,
                mesh,
                keep_camera,
                clips,
            } => {
                let response = client::load_model(
                    &client,
                    path.to_string_lossy().to_string(),
                    mesh,
                    keep_camera,
                    clips.map(|p| p.to_string_lossy().into_owned()),
                )
                .await?;
                println!("{}", serde_json::to_string(&response)?);
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
            RemoteCommands::Camera {
                eye,
                target,
                up,
                fov,
            } => {
                let response =
                    client::set_camera(&client, eye.into(), target.into(), up.map(Into::into), fov)
                        .await?;
                println!("{response}");
                Ok(())
            }
            RemoteCommands::EnableGround
            | RemoteCommands::DisableGround
            | RemoteCommands::ToggleGround => {
                let method = match command {
                    RemoteCommands::EnableGround => "enable_ground",
                    RemoteCommands::DisableGround => "disable_ground",
                    _ => "toggle_ground",
                };
                use jsonrpsee::core::client::ClientT;
                let response: String = client
                    .request(method, jsonrpsee::core::params::ArrayParams::new())
                    .await?;
                println!("{response}");
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
            RemoteCommands::LoadClips { path } => {
                use jsonrpsee::core::client::ClientT;
                let result: serde_json::Value = client
                    .request("load_clips", (path.to_string_lossy().into_owned(),))
                    .await?;
                println!("{}", serde_json::to_string(&result)?);
                Ok(())
            }
            RemoteCommands::ListAnimations => {
                use jsonrpsee::core::client::ClientT;
                let result: serde_json::Value = client
                    .request(
                        "list_animations",
                        jsonrpsee::core::params::ArrayParams::new(),
                    )
                    .await?;
                println!("{}", serde_json::to_string(&result)?);
                Ok(())
            }
            RemoteCommands::SetAnimation { name } => {
                use jsonrpsee::core::client::ClientT;
                let result: serde_json::Value = client.request("set_animation", (name,)).await?;
                println!("{}", serde_json::to_string(&result)?);
                Ok(())
            }
            RemoteCommands::SetTime { time } => {
                use jsonrpsee::core::client::ClientT;
                let result: serde_json::Value = client.request("set_time", (time,)).await?;
                println!("{}", serde_json::to_string(&result)?);
                Ok(())
            }
            RemoteCommands::SetFrame { frame, fps } => {
                use jsonrpsee::core::client::ClientT;
                let result: serde_json::Value = client.request("set_frame", (frame, fps)).await?;
                println!("{}", serde_json::to_string(&result)?);
                Ok(())
            }
            RemoteCommands::BackfaceStats => {
                use jsonrpsee::core::client::ClientT;
                let result: crate::rpc::types::PixelStats = client
                    .request(
                        "backface_stats",
                        jsonrpsee::core::params::ArrayParams::new(),
                    )
                    .await?;
                println!("{}", serde_json::to_string(&result)?);
                Ok(())
            }
            RemoteCommands::Shoot { file } => {
                use jsonrpsee::core::client::ClientT;
                use std::io::Read;
                let mut bytes = Vec::new();
                std::fs::File::open(file)?
                    .take(1024 * 1024 + 1)
                    .read_to_end(&mut bytes)?;
                if bytes.len() > 1024 * 1024 {
                    return Err("Shot file exceeds 1 MiB".into());
                }
                let mut shots: Vec<crate::rpc::types::Shot> = serde_json::from_slice(&bytes)?;
                let cwd = std::env::current_dir()?;
                for shot in &mut shots {
                    if !std::path::Path::new(&shot.path).is_absolute() {
                        shot.path = cwd.join(&shot.path).to_string_lossy().into();
                    }
                }
                let result: Vec<crate::rpc::types::ShotResult> =
                    client.request("shoot", (shots,)).await?;
                println!("{}", serde_json::to_string(&result)?);
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
                        eprintln!("Using absolute path: {}", p);
                        Some(p.clone())
                    } else {
                        // Make relative paths absolute based on caller's working directory
                        let cwd = std::env::current_dir()
                            .map_err(|e| format!("Failed to get current directory: {}", e))?;
                        let abs = cwd.join(path_buf);
                        let abs_str = abs.to_string_lossy().to_string();
                        eprintln!("Resolved relative path '{}' to: {}", p, abs_str);
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
                    eprintln!("Using absolute path: {}", path);
                    path.clone()
                } else {
                    let cwd = std::env::current_dir()
                        .map_err(|e| format!("Failed to get current directory: {}", e))?;
                    let abs = cwd.join(path_buf);
                    let abs_str = abs.to_string_lossy().to_string();
                    eprintln!("Resolved relative path '{}' to: {}", path, abs_str);
                    abs_str
                };

                let response = client::screenshot(&client, absolute_path).await?;
                println!("{}", serde_json::to_string(&response)?);
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
            RemoteCommands::SetBackfaceMode { mode } => {
                use jsonrpsee::core::client::ClientT;
                let response: String = client.request("set_backface_mode", (mode,)).await?;
                println!("{response}");
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

fn parse_size(value: &str) -> Result<[u32; 2], String> {
    let (w, h) = value
        .split_once(['x', 'X'])
        .ok_or("Size must be WIDTHxHEIGHT")?;
    let size = [
        w.parse::<u32>().map_err(|e| e.to_string())?,
        h.parse::<u32>().map_err(|e| e.to_string())?,
    ];
    if size.iter().any(|&v| v == 0 || v > 8192)
        || u64::from(size[0]) * u64::from(size[1]) > 16_777_216
    {
        return Err("Size must be positive, at most 8192 per axis and 16 megapixels total".into());
    }
    Ok(size)
}
