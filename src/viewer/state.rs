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
        ProjectionMode::Perspective { fov_y_degrees: 45.0 }
    }
}

/// Shading mode for mesh rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadingMode {
    /// Current two-light diffuse + specular
    Lit,
    /// Single directional light, no specular
    Flat,
    /// Constant color, no lighting
    Unlit,
}

impl Default for ShadingMode {
    fn default() -> Self {
        ShadingMode::Lit
    }
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

/// Thread-safe viewer state that can be shared between RPC and render threads
#[derive(Debug, Clone)]
pub struct ViewerState {
    /// Camera position in world space
    pub camera_position: na::Point3<f32>,
    /// Camera target (look-at point)
    pub camera_target: na::Point3<f32>,
    /// Model rotation (Euler angles in radians)
    pub model_rotation: na::Vector3<f32>,
    /// UI visibility toggles
    pub show_wireframe: bool,
    pub show_backfaces: bool,
    pub show_ui: bool,
    /// Statistics for display and RPC queries
    pub stats: MeshStats,
    /// Projection mode
    pub projection: ProjectionMode,
    /// Clear color (RGBA)
    pub clear_color: [f32; 4],
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
}

impl Default for ViewerState {
    fn default() -> Self {
        Self {
            camera_position: na::Point3::new(5.0, 3.0, 5.0),
            camera_target: na::Point3::origin(),
            model_rotation: na::Vector3::zeros(),
            show_wireframe: false,
            show_backfaces: false,
            show_ui: true,
            stats: MeshStats::default(),
            projection: ProjectionMode::default(),
            clear_color: [0.0, 0.0, 0.0, 1.0],
            shading: ShadingMode::default(),
            base_color: [0.85, 0.85, 0.85, 1.0],
            light_direction: [0.5, 1.0, 0.5],
        }
    }
}

/// Commands sent from RPC thread to render thread
#[derive(Debug, Clone)]
pub enum ViewerCommand {
    /// Load a new mesh file
    LoadModel {
        path: PathBuf,
        mesh_name: Option<String>,
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
    /// Set camera position
    SetCameraPosition {
        position: na::Point3<f32>,
    },
    /// Set camera target
    SetCameraTarget {
        target: na::Point3<f32>,
    },
    /// Toggle wireframe display
    ToggleWireframe(bool),
    /// Toggle backface visualization
    ToggleBackfaces(bool),
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
    pub clear_color: [f32; 4],
    pub shading: ShadingMode,
    pub base_color: [f32; 4],
    pub light_direction: [f32; 3],
    pub show_wireframe: bool,
    pub show_backfaces: bool,
    pub show_ui: bool,
}

impl RenderPreset {
    /// Default interactive viewer preset
    pub fn viewer() -> Self {
        Self {
            projection: ProjectionMode::Perspective { fov_y_degrees: 45.0 },
            clear_color: [0.0, 0.0, 0.0, 1.0],
            shading: ShadingMode::Lit,
            base_color: [0.85, 0.85, 0.85, 1.0],
            light_direction: [0.5, 1.0, 0.5],
            show_wireframe: false,
            show_backfaces: false,
            show_ui: true,
        }
    }

    /// Sprite baking preset: ortho, transparent, flat shading, no UI
    pub fn sprite_bake() -> Self {
        Self {
            projection: ProjectionMode::Orthographic { world_height: 10.0 },
            clear_color: [0.0, 0.0, 0.0, 0.0],
            shading: ShadingMode::Flat,
            base_color: [0.85, 0.85, 0.85, 1.0],
            light_direction: [0.5, 1.0, 0.8],
            show_wireframe: false,
            show_backfaces: false,
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
    pub fn for_mesh(max_dimension: f32, stats: MeshStats) -> Self {
        let camera_distance = max_dimension * 2.5;
        Self {
            camera_position: na::Point3::new(
                camera_distance * 0.5,
                camera_distance * 0.3,
                camera_distance,
            ),
            camera_target: na::Point3::origin(),
            model_rotation: na::Vector3::zeros(),
            show_wireframe: false,
            show_backfaces: false,
            show_ui: true,
            stats,
            projection: ProjectionMode::default(),
            clear_color: [0.0, 0.0, 0.0, 1.0],
            shading: ShadingMode::default(),
            base_color: [0.85, 0.85, 0.85, 1.0],
            light_direction: [0.5, 1.0, 0.5],
        }
    }

    /// Apply a render preset to this state
    pub fn apply_preset(&mut self, preset: &RenderPreset) {
        self.projection = preset.projection.clone();
        self.clear_color = preset.clear_color;
        self.shading = preset.shading;
        self.base_color = preset.base_color;
        self.light_direction = preset.light_direction;
        self.show_wireframe = preset.show_wireframe;
        self.show_backfaces = preset.show_backfaces;
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
