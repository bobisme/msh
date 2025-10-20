#[cfg(feature = "remote")]
use jsonrpsee::core::async_trait;
#[cfg(feature = "remote")]
use jsonrpsee::proc_macros::rpc;
#[cfg(feature = "remote")]
use jsonrpsee::types::ErrorObjectOwned;
#[cfg(feature = "remote")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "remote")]
use crate::viewer::{MeshStats, ViewerCommand, ViewerState};
#[cfg(feature = "remote")]
use super::types::{parse_angle, MeshStatsResponse};

#[cfg(feature = "remote")]
#[rpc(server)]
pub trait ViewerRpc {
    /// Load a mesh file
    #[method(name = "load_model")]
    async fn load_model(&self, path: String, mesh_name: Option<String>) -> Result<String, ErrorObjectOwned>;

    /// Set absolute model rotation (Euler angles)
    #[method(name = "set_rotation")]
    async fn set_rotation(&self, x: f32, y: f32, z: f32) -> Result<String, ErrorObjectOwned>;

    /// Rotate model around axis by angle
    #[method(name = "rotate_around_axis")]
    async fn rotate_around_axis(&self, axis: Vec<f32>, angle: String) -> Result<String, ErrorObjectOwned>;

    /// Set camera position
    #[method(name = "set_camera_position")]
    async fn set_camera_position(&self, x: f32, y: f32, z: f32) -> Result<String, ErrorObjectOwned>;

    /// Set camera target
    #[method(name = "set_camera_target")]
    async fn set_camera_target(&self, x: f32, y: f32, z: f32) -> Result<String, ErrorObjectOwned>;

    /// Enable wireframe
    #[method(name = "enable_wireframe")]
    async fn enable_wireframe(&self) -> Result<String, ErrorObjectOwned>;

    /// Disable wireframe
    #[method(name = "disable_wireframe")]
    async fn disable_wireframe(&self) -> Result<String, ErrorObjectOwned>;

    /// Toggle wireframe
    #[method(name = "toggle_wireframe")]
    async fn toggle_wireframe(&self) -> Result<String, ErrorObjectOwned>;

    /// Enable backfaces
    #[method(name = "enable_backfaces")]
    async fn enable_backfaces(&self) -> Result<String, ErrorObjectOwned>;

    /// Disable backfaces
    #[method(name = "disable_backfaces")]
    async fn disable_backfaces(&self) -> Result<String, ErrorObjectOwned>;

    /// Toggle backfaces
    #[method(name = "toggle_backfaces")]
    async fn toggle_backfaces(&self) -> Result<String, ErrorObjectOwned>;

    /// Enable UI
    #[method(name = "enable_ui")]
    async fn enable_ui(&self) -> Result<String, ErrorObjectOwned>;

    /// Disable UI
    #[method(name = "disable_ui")]
    async fn disable_ui(&self) -> Result<String, ErrorObjectOwned>;

    /// Toggle UI
    #[method(name = "toggle_ui")]
    async fn toggle_ui(&self) -> Result<String, ErrorObjectOwned>;

    /// Get mesh statistics
    #[method(name = "get_stats")]
    async fn get_stats(&self) -> Result<MeshStatsResponse, ErrorObjectOwned>;

    /// Capture frame (RenderDoc)
    #[method(name = "capture_frame")]
    async fn capture_frame(&self, path: Option<String>) -> Result<String, ErrorObjectOwned>;

    /// Take a screenshot (save to PNG)
    #[method(name = "screenshot")]
    async fn screenshot(&self, path: String) -> Result<String, ErrorObjectOwned>;

    /// Quit the viewer
    #[method(name = "quit")]
    async fn quit(&self) -> Result<String, ErrorObjectOwned>;
}

#[cfg(feature = "remote")]
pub struct ViewerRpcImpl {
    pub state: Arc<Mutex<ViewerState>>,
    pub command_tx: crossbeam::channel::Sender<ViewerCommand>,
}

#[cfg(feature = "remote")]
#[async_trait]
impl ViewerRpcServer for ViewerRpcImpl {
    async fn load_model(&self, path: String, mesh_name: Option<String>) -> Result<String, ErrorObjectOwned> {
        let cmd = ViewerCommand::LoadModel {
            path: path.clone().into(),
            mesh_name,
        };

        self.command_tx.send(cmd)
            .map_err(|e| ErrorObjectOwned::owned(
                -32000,
                "Failed to send command to viewer",
                Some(e.to_string())
            ))?;

        Ok(format!("Loading model: {}", path))
    }

    async fn set_rotation(&self, x: f32, y: f32, z: f32) -> Result<String, ErrorObjectOwned> {
        let cmd = ViewerCommand::SetRotation { x, y, z };

        self.command_tx.send(cmd)
            .map_err(|e| ErrorObjectOwned::owned(
                -32000,
                "Failed to send command to viewer",
                Some(e.to_string())
            ))?;

        Ok(format!("Set rotation to ({}, {}, {})", x, y, z))
    }

    async fn rotate_around_axis(&self, axis: Vec<f32>, angle: String) -> Result<String, ErrorObjectOwned> {
        if axis.len() != 3 {
            return Err(ErrorObjectOwned::owned(
                -32602,
                "Invalid axis",
                Some("Axis must be [x, y, z]")
            ));
        }

        let angle_rad = parse_angle(&angle)
            .map_err(|e| ErrorObjectOwned::owned(-32602, "Invalid angle format", Some(e)))?;

        let axis_vec = nalgebra::Vector3::new(axis[0], axis[1], axis[2]);

        let cmd = ViewerCommand::RotateAroundAxis {
            axis: axis_vec,
            angle: angle_rad,
        };

        self.command_tx.send(cmd)
            .map_err(|e| ErrorObjectOwned::owned(
                -32000,
                "Failed to send command to viewer",
                Some(e.to_string())
            ))?;

        Ok(format!("Rotated around axis {:?} by {}", axis, angle))
    }

    async fn set_camera_position(&self, x: f32, y: f32, z: f32) -> Result<String, ErrorObjectOwned> {
        let cmd = ViewerCommand::SetCameraPosition {
            position: nalgebra::Point3::new(x, y, z),
        };

        self.command_tx.send(cmd)
            .map_err(|e| ErrorObjectOwned::owned(
                -32000,
                "Failed to send command to viewer",
                Some(e.to_string())
            ))?;

        Ok(format!("Set camera position to ({}, {}, {})", x, y, z))
    }

    async fn set_camera_target(&self, x: f32, y: f32, z: f32) -> Result<String, ErrorObjectOwned> {
        let cmd = ViewerCommand::SetCameraTarget {
            target: nalgebra::Point3::new(x, y, z),
        };

        self.command_tx.send(cmd)
            .map_err(|e| ErrorObjectOwned::owned(
                -32000,
                "Failed to send command to viewer",
                Some(e.to_string())
            ))?;

        Ok(format!("Set camera target to ({}, {}, {})", x, y, z))
    }

    async fn enable_wireframe(&self) -> Result<String, ErrorObjectOwned> {
        self.command_tx.send(ViewerCommand::ToggleWireframe(true))
            .map_err(|e| ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string())))?;
        Ok("Wireframe enabled".to_string())
    }

    async fn disable_wireframe(&self) -> Result<String, ErrorObjectOwned> {
        self.command_tx.send(ViewerCommand::ToggleWireframe(false))
            .map_err(|e| ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string())))?;
        Ok("Wireframe disabled".to_string())
    }

    async fn toggle_wireframe(&self) -> Result<String, ErrorObjectOwned> {
        let state = self.state.lock().unwrap();
        let new_value = !state.show_wireframe;
        drop(state);

        self.command_tx.send(ViewerCommand::ToggleWireframe(new_value))
            .map_err(|e| ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string())))?;
        Ok(format!("Wireframe {}", if new_value { "enabled" } else { "disabled" }))
    }

    async fn enable_backfaces(&self) -> Result<String, ErrorObjectOwned> {
        self.command_tx.send(ViewerCommand::ToggleBackfaces(true))
            .map_err(|e| ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string())))?;
        Ok("Backfaces enabled".to_string())
    }

    async fn disable_backfaces(&self) -> Result<String, ErrorObjectOwned> {
        self.command_tx.send(ViewerCommand::ToggleBackfaces(false))
            .map_err(|e| ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string())))?;
        Ok("Backfaces disabled".to_string())
    }

    async fn toggle_backfaces(&self) -> Result<String, ErrorObjectOwned> {
        let state = self.state.lock().unwrap();
        let new_value = !state.show_backfaces;
        drop(state);

        self.command_tx.send(ViewerCommand::ToggleBackfaces(new_value))
            .map_err(|e| ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string())))?;
        Ok(format!("Backfaces {}", if new_value { "enabled" } else { "disabled" }))
    }

    async fn enable_ui(&self) -> Result<String, ErrorObjectOwned> {
        self.command_tx.send(ViewerCommand::ToggleUI(true))
            .map_err(|e| ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string())))?;
        Ok("UI enabled".to_string())
    }

    async fn disable_ui(&self) -> Result<String, ErrorObjectOwned> {
        self.command_tx.send(ViewerCommand::ToggleUI(false))
            .map_err(|e| ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string())))?;
        Ok("UI disabled".to_string())
    }

    async fn toggle_ui(&self) -> Result<String, ErrorObjectOwned> {
        let state = self.state.lock().unwrap();
        let new_value = !state.show_ui;
        drop(state);

        self.command_tx.send(ViewerCommand::ToggleUI(new_value))
            .map_err(|e| ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string())))?;
        Ok(format!("UI {}", if new_value { "enabled" } else { "disabled" }))
    }

    async fn get_stats(&self) -> Result<MeshStatsResponse, ErrorObjectOwned> {
        let state = self.state.lock().unwrap();
        Ok(MeshStatsResponse {
            vertices: state.stats.vertex_count,
            edges: state.stats.edge_count,
            faces: state.stats.face_count,
            is_manifold: state.stats.is_manifold,
            holes: state.stats.hole_count,
        })
    }

    async fn capture_frame(&self, path: Option<String>) -> Result<String, ErrorObjectOwned> {
        #[cfg(feature = "renderdoc")]
        {
            let cmd = ViewerCommand::CaptureFrame { path };

            self.command_tx.send(cmd)
                .map_err(|e| ErrorObjectOwned::owned(
                    -32000,
                    "Failed to send command to viewer",
                    Some(e.to_string())
                ))?;

            Ok("Frame capture triggered".to_string())
        }

        #[cfg(not(feature = "renderdoc"))]
        {
            let _ = path;  // Suppress unused variable warning
            Err(ErrorObjectOwned::owned(
                -32601,
                "RenderDoc feature not enabled",
                Some("Rebuild with --features renderdoc to use frame capture")
            ))
        }
    }

    async fn screenshot(&self, path: String) -> Result<String, ErrorObjectOwned> {
        let cmd = ViewerCommand::Screenshot { path: path.clone() };

        self.command_tx.send(cmd)
            .map_err(|e| ErrorObjectOwned::owned(
                -32000,
                "Failed to send command to viewer",
                Some(e.to_string())
            ))?;

        Ok(format!("Screenshot will be saved to: {}", path))
    }

    async fn quit(&self) -> Result<String, ErrorObjectOwned> {
        let cmd = ViewerCommand::Quit;

        self.command_tx.send(cmd)
            .map_err(|e| ErrorObjectOwned::owned(
                -32000,
                "Failed to send command to viewer",
                Some(e.to_string())
            ))?;

        Ok("Viewer will quit".to_string())
    }
}
