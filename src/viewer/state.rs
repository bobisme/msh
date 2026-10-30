use nalgebra as na;
use std::path::PathBuf;

/// Projection mode for the camera
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectionMode {
    Perspective { fov_y_degrees: f32 },
    Orthographic { world_height: f32 },
}

impl Default for ProjectionMode {
    fn default() -> Self {
        ProjectionMode::Perspective {
            fov_y_degrees: 45.0,
        }
    }
}

/// Shading mode for mesh rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShadingMode {
    /// Current two-light diffuse + specular
    #[default]
    Lit,
    /// Single directional light, no specular
    Flat,
    /// Constant color, no lighting
    Unlit,
}

impl ShadingMode {
    pub fn as_u32(&self) -> u32 {
        match self {
            ShadingMode::Lit => 0,
            ShadingMode::Flat => 1,
            ShadingMode::Unlit => 2,
        }
    }
}

/// What fills the frame behind the mesh. Colors are linear RGBA, like wgpu clear colors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Background {
    Solid([f32; 4]),
    /// Smooth radial gradient from `center` (middle of the frame) to `edge` (corners)
    RadialGradient {
        center: [f32; 4],
        edge: [f32; 4],
    },
}

impl Default for Background {
    /// Medium-dark cool gray in the middle fading to dark gray at the edges
    fn default() -> Self {
        Background::RadialGradient {
            // Linear equivalents of sRGB #2e3135 and #191919.
            center: [0.0273, 0.0307, 0.0356, 1.0],
            edge: [0.0097, 0.0097, 0.0097, 1.0],
        }
    }
}

/// Thread-safe viewer state that can be shared between RPC and render threads
#[derive(Debug, Clone)]
pub struct ViewerState {
    /// Model rotation (Euler angles in radians)
    pub model_rotation: na::Vector3<f32>,
    /// UI visibility toggles
    pub show_wireframe: bool,
    pub show_backfaces: bool,
    pub backface_unlit: bool,
    pub show_ground: bool,
    pub show_ui: bool,
    /// Statistics for display and RPC queries
    pub stats: MeshStats,
    pub window_title: Option<String>,
    pub window_size: Option<[u32; 2]>,
    pub rpc_address: Option<String>,
    pub clips_path: Option<PathBuf>,
    /// Projection mode
    pub projection: ProjectionMode,
    /// Background behind the mesh
    pub background: Background,
    /// Shading mode
    pub shading: ShadingMode,
    /// Base color override (RGBA)
    pub base_color: [f32; 4],
    /// Light direction (normalized)
    pub light_direction: [f32; 3],
}

#[derive(Debug, Clone, Default)]
pub struct MeshStats {
    pub vertex_count: usize,
    pub edge_count: usize,
    pub face_count: usize,
    pub is_manifold: bool,
    pub hole_count: usize,
    pub topology: Option<crate::mesh::topology::TopologyReport>,
}

impl Default for ViewerState {
    fn default() -> Self {
        Self {
            model_rotation: na::Vector3::zeros(),
            show_wireframe: false,
            show_backfaces: false,
            backface_unlit: false,
            show_ground: true,
            show_ui: true,
            stats: MeshStats::default(),
            window_title: None,
            window_size: None,
            rpc_address: None,
            clips_path: None,
            projection: ProjectionMode::default(),
            background: Background::default(),
            shading: ShadingMode::default(),
            base_color: [0.85, 0.85, 0.85, 1.0],
            light_direction: [0.5, 1.0, 0.5],
        }
    }
}

#[cfg(feature = "remote")]
pub type Reply<T> = tokio::sync::oneshot::Sender<Result<T, String>>;

/// Commands are processed in order; a screenshot is a rendering barrier.
#[cfg(feature = "remote")]
#[derive(Debug)]
pub enum ViewerCommand {
    /// Load a new mesh file
    LoadModel {
        path: PathBuf,
        mesh_name: Option<String>,
        keep_camera: bool,
        clips: Option<PathBuf>,
        reply: Reply<crate::rpc::types::MeshStatsResponse>,
    },
    /// Set absolute model rotation (Euler angles in radians)
    SetRotation {
        x: f32,
        y: f32,
        z: f32,
    },
    /// Rotate around axis by angle (radians)
    RotateAroundAxis {
        axis: na::Vector3<f32>,
        angle: f32,
    },
    /// Atomically update camera fields after validating the whole request.
    SetCamera {
        eye: Option<[f32; 3]>,
        target: Option<[f32; 3]>,
        up: Option<[f32; 3]>,
        fov: Option<f32>,
        reply: Reply<()>,
    },
    LoadClips {
        path: PathBuf,
        reply: Reply<Vec<crate::rpc::types::AnimationResponse>>,
    },
    SetAnimation {
        name: String,
        reply: Reply<crate::rpc::types::AnimationResponse>,
    },
    SetTime {
        time: f32,
        reply: Reply<crate::rpc::types::AnimationResponse>,
    },
    SetFrame {
        frame: u32,
        fps: f32,
        reply: Reply<crate::rpc::types::AnimationResponse>,
    },
    ListAnimations {
        reply: Reply<Vec<crate::rpc::types::AnimationResponse>>,
    },
    ToggleGround(Option<bool>),
    Shoot {
        shots: Vec<crate::rpc::types::Shot>,
        reply: Reply<Vec<crate::rpc::types::ShotResult>>,
    },
    PixelStats {
        reply: Reply<crate::rpc::types::PixelStats>,
    },
    /// Toggle wireframe display
    ToggleWireframe(bool),
    /// Toggle backface visualization
    ToggleBackfaces(bool),
    SetBackfaceMode(bool),
    /// Toggle UI overlay
    ToggleUI(bool),
    /// Capture frame (RenderDoc)
    #[cfg(feature = "renderdoc")]
    CaptureFrame {
        path: Option<String>,
    },
    /// Take a screenshot (save to PNG)
    Screenshot {
        path: String,
        reply: Reply<crate::rpc::types::ScreenshotResponse>,
    },
    /// Set projection mode
    SetProjection {
        mode: ProjectionMode,
    },
    /// Set clear color
    SetClearColor {
        color: [f32; 4],
    },
    /// Set shading mode
    SetShading {
        mode: ShadingMode,
    },
    /// Set base color
    SetBaseColor {
        color: [f32; 4],
    },
    /// Set light direction
    SetLightDirection {
        direction: [f32; 3],
    },
    /// Apply a named render preset
    ApplyPreset {
        name: String,
    },
    /// Quit the viewer
    Quit,
}

/// A bundle of rendering settings that can be applied at once
#[derive(Debug, Clone)]
pub struct RenderPreset {
    pub projection: ProjectionMode,
    pub background: Background,
    pub shading: ShadingMode,
    pub base_color: [f32; 4],
    pub light_direction: [f32; 3],
    pub show_wireframe: bool,
    pub show_backfaces: bool,
    pub backface_unlit: bool,
    pub show_ground: bool,
    pub show_ui: bool,
}

impl RenderPreset {
    /// Default interactive viewer preset
    pub fn viewer() -> Self {
        Self {
            projection: ProjectionMode::Perspective {
                fov_y_degrees: 45.0,
            },
            background: Background::default(),
            shading: ShadingMode::Lit,
            base_color: [0.85, 0.85, 0.85, 1.0],
            light_direction: [0.5, 1.0, 0.5],
            show_wireframe: false,
            show_backfaces: false,
            backface_unlit: false,
            show_ground: true,
            show_ui: true,
        }
    }

    /// Sprite baking preset: ortho, transparent, flat shading, no UI
    pub fn sprite_bake() -> Self {
        Self {
            projection: ProjectionMode::Orthographic { world_height: 10.0 },
            background: Background::Solid([0.0, 0.0, 0.0, 0.0]),
            shading: ShadingMode::Flat,
            base_color: [0.85, 0.85, 0.85, 1.0],
            light_direction: [0.5, 1.0, 0.8],
            show_wireframe: false,
            show_backfaces: false,
            backface_unlit: false,
            show_ground: false,
            show_ui: false,
        }
    }

    pub fn by_name(name: &str) -> Option<Self> {
        match name {
            "viewer" => Some(Self::viewer()),
            "sprite-bake" | "sprite_bake" => Some(Self::sprite_bake()),
            _ => None,
        }
    }
}

impl ViewerState {
    /// Create a new viewer state with calculated camera position for a mesh
    pub fn for_mesh(_max_dimension: f32, stats: MeshStats) -> Self {
        Self {
            model_rotation: na::Vector3::zeros(),
            show_wireframe: false,
            show_backfaces: false,
            backface_unlit: false,
            show_ground: true,
            show_ui: true,
            stats,
            window_title: None,
            window_size: None,
            rpc_address: None,
            clips_path: None,
            projection: ProjectionMode::default(),
            background: Background::default(),
            shading: ShadingMode::default(),
            base_color: [0.85, 0.85, 0.85, 1.0],
            light_direction: [0.5, 1.0, 0.5],
        }
    }

    /// Apply a render preset to this state
    pub fn apply_preset(&mut self, preset: &RenderPreset) {
        self.projection = preset.projection.clone();
        self.background = preset.background;
        self.shading = preset.shading;
        self.base_color = preset.base_color;
        self.light_direction = preset.light_direction;
        self.show_wireframe = preset.show_wireframe;
        self.show_backfaces = preset.show_backfaces;
        self.backface_unlit = preset.backface_unlit;
        self.show_ground = preset.show_ground;
        self.show_ui = preset.show_ui;
    }

    /// Apply a rotation transformation
    pub fn apply_rotation(&mut self, axis: na::Vector3<f32>, angle: f32) {
        // Convert current euler angles to rotation matrix
        let current = na::Rotation3::from_euler_angles(
            self.model_rotation.x,
            self.model_rotation.y,
            self.model_rotation.z,
        );

        // Create rotation around specified axis
        let rotation = na::Rotation3::from_axis_angle(&na::Unit::new_normalize(axis), angle);

        // Compose rotations
        let new_rotation = rotation * current;

        // Convert back to Euler angles
        let (x, y, z) = new_rotation.euler_angles();
        self.model_rotation = na::Vector3::new(x, y, z);
    }
}

impl MeshStats {
    pub fn from_mesh(
        data: &crate::mesh::loader::MeshWithColors,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let bytes = data
            .face_indices
            .len()
            .checked_mul(3 * std::mem::size_of::<super::mesh_renderer::Vertex>())
            .ok_or("Viewer vertex buffer size overflow")?;
        let limit = wgpu::Limits::default().max_buffer_size;
        if bytes as u64 > limit {
            return Err(format!("Mesh needs {bytes} vertex-buffer bytes; viewer device limit is {limit}. Simplify the mesh before viewing.").into());
        }
        let topology =
            crate::mesh::topology::report(&data.bind_positions(), &data.face_indices, None)?;
        Ok(Self {
            vertex_count: data.positions.len(),
            face_count: data.face_indices.len(),
            edge_count: topology.raw.edges,
            is_manifold: topology.raw.is_manifold,
            hole_count: topology.raw.boundary_loops,
            topology: Some(topology),
        })
    }
}
