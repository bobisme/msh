use kiss3d::nalgebra as na;
use std::path::PathBuf;

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
            show_wireframe: true,
            show_backfaces: false,
            show_ui: true,
            stats: MeshStats::default(),
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
            show_wireframe: true,
            show_backfaces: false,
            show_ui: true,
            stats,
        }
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
