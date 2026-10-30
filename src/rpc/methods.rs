#[cfg(feature = "remote")]
use jsonrpsee::core::async_trait;
#[cfg(feature = "remote")]
use jsonrpsee::proc_macros::rpc;
#[cfg(feature = "remote")]
use jsonrpsee::types::ErrorObjectOwned;
#[cfg(feature = "remote")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "remote")]
use super::types::{MeshStatsResponse, ScreenshotResponse, parse_angle};
#[cfg(feature = "remote")]
use crate::viewer::{ViewerCommand, ViewerState};

#[cfg(feature = "remote")]
#[rpc(server)]
pub trait ViewerRpc {
    /// Load a mesh file
    #[method(name = "load_model")]
    async fn load_model(
        &self,
        path: String,
        mesh_name: Option<String>,
        keep_camera: Option<bool>,
        clips: Option<String>,
    ) -> Result<MeshStatsResponse, ErrorObjectOwned>;

    /// Set absolute model rotation (Euler angles)
    #[method(name = "set_rotation")]
    async fn set_rotation(&self, x: f32, y: f32, z: f32) -> Result<String, ErrorObjectOwned>;

    /// Rotate model around axis by angle
    #[method(name = "rotate_around_axis")]
    async fn rotate_around_axis(
        &self,
        axis: Vec<f32>,
        angle: String,
    ) -> Result<String, ErrorObjectOwned>;

    /// Set camera position
    #[method(name = "set_camera_position")]
    async fn set_camera_position(&self, x: f32, y: f32, z: f32)
    -> Result<String, ErrorObjectOwned>;

    /// Set camera target
    #[method(name = "set_camera_target")]
    async fn set_camera_target(&self, x: f32, y: f32, z: f32) -> Result<String, ErrorObjectOwned>;

    /// Update the camera as one ordered, validated command.
    #[method(name = "set_camera")]
    async fn set_camera(
        &self,
        eye: [f32; 3],
        target: [f32; 3],
        up: Option<[f32; 3]>,
        fov: Option<f32>,
    ) -> Result<String, ErrorObjectOwned>;

    #[method(name = "enable_ground")]
    async fn enable_ground(&self) -> Result<String, ErrorObjectOwned>;
    #[method(name = "disable_ground")]
    async fn disable_ground(&self) -> Result<String, ErrorObjectOwned>;
    #[method(name = "toggle_ground")]
    async fn toggle_ground(&self) -> Result<String, ErrorObjectOwned>;

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

    #[method(name = "set_backface_mode")]
    async fn set_backface_mode(&self, mode: String) -> Result<String, ErrorObjectOwned>;

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
    async fn screenshot(&self, path: String) -> Result<ScreenshotResponse, ErrorObjectOwned>;

    #[method(name = "shoot")]
    async fn shoot(
        &self,
        shots: Vec<super::types::Shot>,
    ) -> Result<Vec<super::types::ShotResult>, ErrorObjectOwned>;

    #[method(name = "backface_stats")]
    async fn backface_stats(&self) -> Result<super::types::PixelStats, ErrorObjectOwned>;

    #[method(name = "load_clips")]
    async fn load_clips(
        &self,
        path: String,
    ) -> Result<Vec<super::types::AnimationResponse>, ErrorObjectOwned>;

    #[method(name = "set_animation")]
    async fn set_animation(
        &self,
        name: String,
    ) -> Result<super::types::AnimationResponse, ErrorObjectOwned>;

    #[method(name = "set_time")]
    async fn set_time(
        &self,
        time: f32,
    ) -> Result<super::types::AnimationResponse, ErrorObjectOwned>;

    #[method(name = "set_frame")]
    async fn set_frame(
        &self,
        frame: u32,
        fps: Option<f32>,
    ) -> Result<super::types::AnimationResponse, ErrorObjectOwned>;

    #[method(name = "list_animations")]
    async fn list_animations(
        &self,
    ) -> Result<Vec<super::types::AnimationResponse>, ErrorObjectOwned>;

    /// Set projection mode
    #[method(name = "set_projection")]
    async fn set_projection(
        &self,
        mode: String,
        value: Option<f32>,
    ) -> Result<String, ErrorObjectOwned>;

    /// Set clear color (RGBA, 0.0-1.0)
    #[method(name = "set_clear_color")]
    async fn set_clear_color(
        &self,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
    ) -> Result<String, ErrorObjectOwned>;

    /// Set shading mode
    #[method(name = "set_shading")]
    async fn set_shading(&self, mode: String) -> Result<String, ErrorObjectOwned>;

    /// Set base color (RGBA, 0.0-1.0)
    #[method(name = "set_base_color")]
    async fn set_base_color(
        &self,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
    ) -> Result<String, ErrorObjectOwned>;

    /// Set light direction
    #[method(name = "set_light_direction")]
    async fn set_light_direction(&self, x: f32, y: f32, z: f32)
    -> Result<String, ErrorObjectOwned>;

    /// Apply a named render preset
    #[method(name = "apply_preset")]
    async fn apply_preset(&self, name: String) -> Result<String, ErrorObjectOwned>;

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
    async fn load_model(
        &self,
        path: String,
        mesh_name: Option<String>,
        keep_camera: Option<bool>,
        clips: Option<String>,
    ) -> Result<MeshStatsResponse, ErrorObjectOwned> {
        let (reply, receive) = tokio::sync::oneshot::channel();
        self.command_tx
            .try_send(ViewerCommand::LoadModel {
                path: path.into(),
                mesh_name,
                keep_camera: keep_camera.unwrap_or(false),
                clips: clips.map(Into::into),
                reply,
            })
            .map_err(command_error)?;
        completed(receive).await
    }

    async fn set_rotation(&self, x: f32, y: f32, z: f32) -> Result<String, ErrorObjectOwned> {
        let cmd = ViewerCommand::SetRotation { x, y, z };

        self.command_tx.try_send(cmd).map_err(|e| {
            ErrorObjectOwned::owned(
                -32000,
                "Failed to send command to viewer",
                Some(e.to_string()),
            )
        })?;

        Ok(format!("Set rotation to ({}, {}, {})", x, y, z))
    }

    async fn rotate_around_axis(
        &self,
        axis: Vec<f32>,
        angle: String,
    ) -> Result<String, ErrorObjectOwned> {
        if axis.len() != 3 {
            return Err(ErrorObjectOwned::owned(
                -32602,
                "Invalid axis",
                Some("Axis must be [x, y, z]"),
            ));
        }

        let angle_rad = parse_angle(&angle)
            .map_err(|e| ErrorObjectOwned::owned(-32602, "Invalid angle format", Some(e)))?;

        let axis_vec = nalgebra::Vector3::new(axis[0], axis[1], axis[2]);

        let cmd = ViewerCommand::RotateAroundAxis {
            axis: axis_vec,
            angle: angle_rad,
        };

        self.command_tx.try_send(cmd).map_err(|e| {
            ErrorObjectOwned::owned(
                -32000,
                "Failed to send command to viewer",
                Some(e.to_string()),
            )
        })?;

        Ok(format!("Rotated around axis {:?} by {}", axis, angle))
    }

    async fn set_camera_position(
        &self,
        x: f32,
        y: f32,
        z: f32,
    ) -> Result<String, ErrorObjectOwned> {
        self.camera_command(Some([x, y, z]), None, None, None).await
    }

    async fn set_camera_target(&self, x: f32, y: f32, z: f32) -> Result<String, ErrorObjectOwned> {
        self.camera_command(None, Some([x, y, z]), None, None).await
    }

    async fn set_camera(
        &self,
        eye: [f32; 3],
        target: [f32; 3],
        up: Option<[f32; 3]>,
        fov: Option<f32>,
    ) -> Result<String, ErrorObjectOwned> {
        self.camera_command(Some(eye), Some(target), up, fov).await
    }

    async fn enable_ground(&self) -> Result<String, ErrorObjectOwned> {
        self.ground_command(Some(true))
    }
    async fn disable_ground(&self) -> Result<String, ErrorObjectOwned> {
        self.ground_command(Some(false))
    }
    async fn toggle_ground(&self) -> Result<String, ErrorObjectOwned> {
        self.ground_command(None)
    }

    async fn enable_wireframe(&self) -> Result<String, ErrorObjectOwned> {
        self.command_tx
            .try_send(ViewerCommand::ToggleWireframe(true))
            .map_err(|e| {
                ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string()))
            })?;
        Ok("Wireframe enabled".to_string())
    }

    async fn disable_wireframe(&self) -> Result<String, ErrorObjectOwned> {
        self.command_tx
            .try_send(ViewerCommand::ToggleWireframe(false))
            .map_err(|e| {
                ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string()))
            })?;
        Ok("Wireframe disabled".to_string())
    }

    async fn toggle_wireframe(&self) -> Result<String, ErrorObjectOwned> {
        let state = self.state.lock().unwrap();
        let new_value = !state.show_wireframe;
        drop(state);

        self.command_tx
            .try_send(ViewerCommand::ToggleWireframe(new_value))
            .map_err(|e| {
                ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string()))
            })?;
        Ok(format!(
            "Wireframe {}",
            if new_value { "enabled" } else { "disabled" }
        ))
    }

    async fn enable_backfaces(&self) -> Result<String, ErrorObjectOwned> {
        self.command_tx
            .try_send(ViewerCommand::ToggleBackfaces(true))
            .map_err(|e| {
                ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string()))
            })?;
        Ok("Backfaces enabled".to_string())
    }

    async fn disable_backfaces(&self) -> Result<String, ErrorObjectOwned> {
        self.command_tx
            .try_send(ViewerCommand::ToggleBackfaces(false))
            .map_err(|e| {
                ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string()))
            })?;
        Ok("Backfaces disabled".to_string())
    }

    async fn toggle_backfaces(&self) -> Result<String, ErrorObjectOwned> {
        let state = self.state.lock().unwrap();
        let new_value = !state.show_backfaces;
        drop(state);

        self.command_tx
            .try_send(ViewerCommand::ToggleBackfaces(new_value))
            .map_err(|e| {
                ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string()))
            })?;
        Ok(format!(
            "Backfaces {}",
            if new_value { "enabled" } else { "disabled" }
        ))
    }

    async fn set_backface_mode(&self, mode: String) -> Result<String, ErrorObjectOwned> {
        let unlit = match mode.as_str() {
            "unlit" => true,
            "lit" => false,
            _ => {
                return Err(ErrorObjectOwned::owned(
                    -32602,
                    "Backface mode must be lit or unlit",
                    None::<()>,
                ));
            }
        };
        self.command_tx
            .try_send(ViewerCommand::SetBackfaceMode(unlit))
            .map_err(command_error)?;
        Ok(format!("Backface mode: {mode}"))
    }

    async fn enable_ui(&self) -> Result<String, ErrorObjectOwned> {
        self.command_tx
            .try_send(ViewerCommand::ToggleUI(true))
            .map_err(|e| {
                ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string()))
            })?;
        Ok("UI enabled".to_string())
    }

    async fn disable_ui(&self) -> Result<String, ErrorObjectOwned> {
        self.command_tx
            .try_send(ViewerCommand::ToggleUI(false))
            .map_err(|e| {
                ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string()))
            })?;
        Ok("UI disabled".to_string())
    }

    async fn toggle_ui(&self) -> Result<String, ErrorObjectOwned> {
        let state = self.state.lock().unwrap();
        let new_value = !state.show_ui;
        drop(state);

        self.command_tx
            .try_send(ViewerCommand::ToggleUI(new_value))
            .map_err(|e| {
                ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string()))
            })?;
        Ok(format!(
            "UI {}",
            if new_value { "enabled" } else { "disabled" }
        ))
    }

    async fn get_stats(&self) -> Result<MeshStatsResponse, ErrorObjectOwned> {
        let state = self.state.lock().unwrap();
        Ok(MeshStatsResponse {
            vertices: state.stats.vertex_count,
            edges: state.stats.edge_count,
            faces: state.stats.face_count,
            triangles: state.stats.face_count,
            is_manifold: state.stats.is_manifold,
            holes: state.stats.hole_count,
            topology: state.stats.topology.clone(),
        })
    }

    async fn capture_frame(&self, path: Option<String>) -> Result<String, ErrorObjectOwned> {
        #[cfg(feature = "renderdoc")]
        {
            let cmd = ViewerCommand::CaptureFrame { path };

            self.command_tx.try_send(cmd).map_err(|e| {
                ErrorObjectOwned::owned(
                    -32000,
                    "Failed to send command to viewer",
                    Some(e.to_string()),
                )
            })?;

            Ok("Frame capture triggered".to_string())
        }

        #[cfg(not(feature = "renderdoc"))]
        {
            let _ = path; // Suppress unused variable warning
            Err(ErrorObjectOwned::owned(
                -32601,
                "RenderDoc feature not enabled",
                Some("Rebuild with --features renderdoc to use frame capture"),
            ))
        }
    }

    async fn screenshot(&self, path: String) -> Result<ScreenshotResponse, ErrorObjectOwned> {
        let (reply, receive) = tokio::sync::oneshot::channel();
        self.command_tx
            .try_send(ViewerCommand::Screenshot { path, reply })
            .map_err(command_error)?;
        completed(receive).await
    }

    async fn shoot(
        &self,
        shots: Vec<super::types::Shot>,
    ) -> Result<Vec<super::types::ShotResult>, ErrorObjectOwned> {
        if shots.is_empty() || shots.len() > 64 || shots.iter().any(|s| s.path.len() > 4096) {
            return Err(ErrorObjectOwned::owned(
                -32602,
                "A shot batch must contain 1 to 64 shots with paths at most 4096 bytes",
                None::<()>,
            ));
        }
        let (reply, receive) = tokio::sync::oneshot::channel();
        self.command_tx
            .try_send(ViewerCommand::Shoot { shots, reply })
            .map_err(command_error)?;
        completed(receive).await
    }

    async fn backface_stats(&self) -> Result<super::types::PixelStats, ErrorObjectOwned> {
        let (reply, receive) = tokio::sync::oneshot::channel();
        self.command_tx
            .try_send(ViewerCommand::PixelStats { reply })
            .map_err(command_error)?;
        completed(receive).await
    }

    async fn load_clips(
        &self,
        path: String,
    ) -> Result<Vec<super::types::AnimationResponse>, ErrorObjectOwned> {
        let (reply, receive) = tokio::sync::oneshot::channel();
        self.command_tx
            .try_send(ViewerCommand::LoadClips {
                path: path.into(),
                reply,
            })
            .map_err(command_error)?;
        completed(receive).await
    }

    async fn set_animation(
        &self,
        name: String,
    ) -> Result<super::types::AnimationResponse, ErrorObjectOwned> {
        let (reply, receive) = tokio::sync::oneshot::channel();
        self.command_tx
            .try_send(ViewerCommand::SetAnimation { name, reply })
            .map_err(command_error)?;
        completed(receive).await
    }

    async fn set_time(
        &self,
        time: f32,
    ) -> Result<super::types::AnimationResponse, ErrorObjectOwned> {
        let (reply, receive) = tokio::sync::oneshot::channel();
        self.command_tx
            .try_send(ViewerCommand::SetTime { time, reply })
            .map_err(command_error)?;
        completed(receive).await
    }

    async fn set_frame(
        &self,
        frame: u32,
        fps: Option<f32>,
    ) -> Result<super::types::AnimationResponse, ErrorObjectOwned> {
        let (reply, receive) = tokio::sync::oneshot::channel();
        self.command_tx
            .try_send(ViewerCommand::SetFrame {
                frame,
                fps: fps.unwrap_or(30.0),
                reply,
            })
            .map_err(command_error)?;
        completed(receive).await
    }

    async fn list_animations(
        &self,
    ) -> Result<Vec<super::types::AnimationResponse>, ErrorObjectOwned> {
        let (reply, receive) = tokio::sync::oneshot::channel();
        self.command_tx
            .try_send(ViewerCommand::ListAnimations { reply })
            .map_err(command_error)?;
        completed(receive).await
    }

    async fn set_projection(
        &self,
        mode: String,
        value: Option<f32>,
    ) -> Result<String, ErrorObjectOwned> {
        use crate::viewer::state::ProjectionMode;
        let projection = match mode.as_str() {
            "perspective" => ProjectionMode::Perspective {
                fov_y_degrees: value.unwrap_or(45.0),
            },
            "ortho" | "orthographic" => ProjectionMode::Orthographic {
                world_height: value.unwrap_or(10.0),
            },
            _ => {
                return Err(ErrorObjectOwned::owned(
                    -32602,
                    "Invalid projection mode",
                    Some("Use 'perspective' or 'ortho'"),
                ));
            }
        };
        self.command_tx
            .try_send(ViewerCommand::SetProjection { mode: projection })
            .map_err(|e| {
                ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string()))
            })?;
        Ok(format!("Projection set to {}", mode))
    }

    async fn set_clear_color(
        &self,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
    ) -> Result<String, ErrorObjectOwned> {
        self.command_tx
            .try_send(ViewerCommand::SetClearColor {
                color: [r, g, b, a],
            })
            .map_err(|e| {
                ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string()))
            })?;
        Ok(format!("Clear color set to ({}, {}, {}, {})", r, g, b, a))
    }

    async fn set_shading(&self, mode: String) -> Result<String, ErrorObjectOwned> {
        use crate::viewer::state::ShadingMode;
        let shading = match mode.as_str() {
            "lit" => ShadingMode::Lit,
            "flat" => ShadingMode::Flat,
            "unlit" => ShadingMode::Unlit,
            _ => {
                return Err(ErrorObjectOwned::owned(
                    -32602,
                    "Invalid shading mode",
                    Some("Use 'lit', 'flat', or 'unlit'"),
                ));
            }
        };
        self.command_tx
            .try_send(ViewerCommand::SetShading { mode: shading })
            .map_err(|e| {
                ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string()))
            })?;
        Ok(format!("Shading set to {}", mode))
    }

    async fn set_base_color(
        &self,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
    ) -> Result<String, ErrorObjectOwned> {
        self.command_tx
            .try_send(ViewerCommand::SetBaseColor {
                color: [r, g, b, a],
            })
            .map_err(|e| {
                ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string()))
            })?;
        Ok(format!("Base color set to ({}, {}, {}, {})", r, g, b, a))
    }

    async fn set_light_direction(
        &self,
        x: f32,
        y: f32,
        z: f32,
    ) -> Result<String, ErrorObjectOwned> {
        self.command_tx
            .try_send(ViewerCommand::SetLightDirection {
                direction: [x, y, z],
            })
            .map_err(|e| {
                ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string()))
            })?;
        Ok(format!("Light direction set to ({}, {}, {})", x, y, z))
    }

    async fn apply_preset(&self, name: String) -> Result<String, ErrorObjectOwned> {
        self.command_tx
            .try_send(ViewerCommand::ApplyPreset { name: name.clone() })
            .map_err(|e| {
                ErrorObjectOwned::owned(-32000, "Failed to send command", Some(e.to_string()))
            })?;
        Ok(format!("Applying preset: {}", name))
    }

    async fn quit(&self) -> Result<String, ErrorObjectOwned> {
        let cmd = ViewerCommand::Quit;

        self.command_tx.try_send(cmd).map_err(|e| {
            ErrorObjectOwned::owned(
                -32000,
                "Failed to send command to viewer",
                Some(e.to_string()),
            )
        })?;

        Ok("Viewer will quit".to_string())
    }
}

#[cfg(feature = "remote")]
fn command_error(error: impl std::fmt::Display) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(
        -32000,
        "Viewer command queue unavailable",
        Some(error.to_string()),
    )
}

#[cfg(feature = "remote")]
async fn completed<T>(
    receive: tokio::sync::oneshot::Receiver<Result<T, String>>,
) -> Result<T, ErrorObjectOwned> {
    tokio::time::timeout(std::time::Duration::from_secs(60), receive)
        .await
        .map_err(|_| ErrorObjectOwned::owned(-32001, "Viewer command timed out", None::<()>))?
        .map_err(|_| {
            ErrorObjectOwned::owned(-32000, "Viewer closed before completion", None::<()>)
        })?
        .map_err(|error| ErrorObjectOwned::owned(-32000, "Viewer command failed", Some(error)))
}

#[cfg(feature = "remote")]
impl ViewerRpcImpl {
    async fn camera_command(
        &self,
        eye: Option<[f32; 3]>,
        target: Option<[f32; 3]>,
        up: Option<[f32; 3]>,
        fov: Option<f32>,
    ) -> Result<String, ErrorObjectOwned> {
        if fov.is_some_and(|v| !v.is_finite() || !(1.0..179.0).contains(&v)) {
            return Err(ErrorObjectOwned::owned(
                -32602,
                "FOV must be finite and between 1 and 179 degrees",
                None::<()>,
            ));
        }
        let (reply, receive) = tokio::sync::oneshot::channel();
        self.command_tx
            .try_send(ViewerCommand::SetCamera {
                eye,
                target,
                up,
                fov,
                reply,
            })
            .map_err(command_error)?;
        completed(receive).await?;
        Ok("Camera updated".into())
    }

    fn ground_command(&self, value: Option<bool>) -> Result<String, ErrorObjectOwned> {
        self.command_tx
            .try_send(ViewerCommand::ToggleGround(value))
            .map_err(command_error)?;
        Ok("Ground shadow update queued".into())
    }
}
