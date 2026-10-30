//! Persistent offscreen rendering. The single worker owns all scene and camera state;
//! each command, capture, and shot batch completes before the next command starts.
use super::{
    camera::ArcBallCamera,
    mesh_renderer::MeshRenderer,
    render::extract_render_data,
    state::{Background, MeshStats, ProjectionMode, ViewerCommand, ViewerState},
};
use crate::mesh::loader::{MeshWithColors, load_mesh_with_colors};
use crate::rpc::types::{MeshStatsResponse, ScreenshotResponse, Shot, ShotResult};
use nalgebra as na;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

struct OffscreenScene {
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    target: wgpu::Texture,
    renderer: Option<MeshRenderer>,
    camera: ArcBallCamera,
    state: Arc<Mutex<ViewerState>>,
    data: Option<MeshWithColors>,
    pose: Option<super::pose::PoseState>,
}

impl OffscreenScene {
    fn new(size: [u32; 2], state: Arc<Mutex<ViewerState>>) -> Result<Self> {
        let (device, queue, format) =
            pollster::block_on(super::headless::create_headless_device())?;
        let [width, height] = size;
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Offscreen Service Color Target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        Ok(Self {
            device,
            queue,
            config,
            target,
            renderer: None,
            state,
            data: None,
            pose: None,
            camera: ArcBallCamera::new(
                na::Point3::new(1.0, 1.0, 3.0),
                na::Point3::origin(),
                width,
                height,
            ),
        })
    }

    fn load(
        &mut self,
        path: &PathBuf,
        mesh_name: Option<&str>,
        keep_camera: bool,
        clips: Option<PathBuf>,
    ) -> Result<MeshStatsResponse> {
        let mut data = load_mesh_with_colors(path, mesh_name)?;
        let clips = clips.or_else(|| self.state.lock().unwrap().clips_path.clone());
        if let Some(path) = clips {
            crate::mesh::clips::attach(&mut data, &path)?;
        }
        let stats = MeshStats::from_mesh(&data)?;
        let mesh = extract_render_data(&data, false);
        self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let mut renderer = MeshRenderer::new(&self.device, &self.queue, &self.config);
        renderer.load_mesh(&self.device, &self.queue, &mesh, &data.materials);
        if let Some(skeleton) = &mesh.skeleton_data {
            renderer.update_joint_palette(&self.queue, &skeleton.joint_matrices);
            renderer.set_joint_count(skeleton.joint_matrices.len() as u32);
        }
        let allocation = pollster::block_on(self.device.pop_error_scope());
        let validation = pollster::block_on(self.device.pop_error_scope());
        if let Some(error) = allocation.or(validation) {
            return Err(error.to_string().into());
        }
        if !keep_camera {
            let d = mesh.max_dimension.max(0.001) * 2.5;
            self.camera
                .set_pose(Some([d * 0.5, d * 0.3, d]), Some([0.0; 3]), None)?;
        }
        let result = MeshStatsResponse {
            vertices: stats.vertex_count,
            edges: stats.edge_count,
            faces: stats.face_count,
            triangles: stats.face_count,
            is_manifold: stats.is_manifold,
            holes: stats.hole_count,
            topology: stats.topology.clone(),
        };
        self.state.lock().unwrap().stats = stats;
        self.renderer = Some(renderer);
        self.pose = data
            .skeleton
            .take()
            .map(|s| super::pose::PoseState::new(s, std::mem::take(&mut data.animations), false));
        self.data = Some(data);
        Ok(result)
    }

    fn camera(
        &mut self,
        eye: Option<[f32; 3]>,
        target: Option<[f32; 3]>,
        up: Option<[f32; 3]>,
        fov: Option<f32>,
    ) -> Result<()> {
        if fov.is_some_and(|v| !v.is_finite() || !(1.0..179.0).contains(&v)) {
            return Err("FOV must be finite and between 1 and 179 degrees".into());
        }
        self.camera.set_pose(eye, target, up)?;
        if let Some(fov_y_degrees) = fov {
            self.state.lock().unwrap().projection = ProjectionMode::Perspective { fov_y_degrees };
        }
        Ok(())
    }

    fn update(&mut self) -> Result<()> {
        let renderer = self.renderer.as_ref().ok_or("No model loaded")?;
        if let Some(pose) = &mut self.pose {
            pose.update(renderer, &self.queue);
        }
        let state = self.state.lock().unwrap();
        let model = na::Rotation3::from_euler_angles(
            state.model_rotation.x,
            state.model_rotation.y,
            state.model_rotation.z,
        )
        .to_homogeneous();
        let view_proj = self.camera.view_projection_matrix_for(&state.projection);
        renderer.update_uniforms(
            &self.queue,
            &view_proj,
            &model,
            &self.camera.position(),
            state.shading.as_u32(),
            state.base_color,
            state.light_direction,
            state.background,
            state.backface_unlit,
        );
        Ok(())
    }

    fn capture(&mut self, path: String) -> Result<ScreenshotResponse> {
        self.update()?;
        let state = self.state.lock().unwrap();
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let view = self.target.create_view(&Default::default());
        self.renderer.as_ref().unwrap().render(
            &mut encoder,
            &view,
            state.show_wireframe,
            state.show_backfaces,
            state.show_ground,
        );
        drop(state);
        self.queue.submit(Some(encoder.finish()));
        super::readback::save_png(&self.device, &self.queue, &self.target, &path)?;
        Ok(ScreenshotResponse {
            path,
            width: self.config.width,
            height: self.config.height,
        })
    }

    fn shoot(&mut self, shots: Vec<Shot>) -> Vec<ShotResult> {
        let over_budget =
            u64::from(self.config.width) * u64::from(self.config.height) * shots.len() as u64
                > 67_108_864;
        shots
            .into_iter()
            .map(|shot| {
                if over_budget {
                    return ShotResult {
                        path: shot.path,
                        width: 0,
                        height: 0,
                        error: Some("Shot batch exceeds 64 megapixels total".into()),
                    };
                }
                let result = self
                    .camera(Some(shot.eye), Some(shot.target), shot.up, shot.fov)
                    .and_then(|()| self.capture(shot.path.clone()));
                ShotResult {
                    path: shot.path,
                    width: self.config.width,
                    height: self.config.height,
                    error: result.err().map(|e| e.to_string()),
                }
            })
            .collect()
    }
}

pub fn serve(port: u16, size: [u32; 2], configure: impl FnOnce(&mut ViewerState)) -> Result<()> {
    let mut state = ViewerState::default();
    configure(&mut state);
    // Offscreen services have no overlay; captures are invariant to desktop UI.
    state.show_ui = false;
    let state = Arc::new(Mutex::new(state));
    let mut scene = OffscreenScene::new(size, state.clone())?;
    let (send, receive) = crossbeam::channel::bounded(128);
    let (address, server) = crate::rpc::spawn_rpc_server(state.clone(), send, port)?;
    state.lock().unwrap().rpc_address = Some(address.to_string());
    println!("Offscreen renderer ready: {}x{}", size[0], size[1]);
    while let Ok(command) = receive.recv() {
        match command {
            ViewerCommand::LoadModel {
                path,
                mesh_name,
                keep_camera,
                clips,
                reply,
            } => {
                if !reply.is_closed() {
                    let _ = reply.send(
                        scene
                            .load(&path, mesh_name.as_deref(), keep_camera, clips)
                            .map_err(|e| e.to_string()),
                    );
                }
            }
            ViewerCommand::SetCamera {
                eye,
                target,
                up,
                fov,
                reply,
            } => {
                if !reply.is_closed() {
                    let _ = reply.send(
                        scene
                            .camera(eye, target, up, fov)
                            .map_err(|e| e.to_string()),
                    );
                }
            }
            ViewerCommand::Screenshot { path, reply } => {
                if !reply.is_closed() {
                    let _ = reply.send(scene.capture(path).map_err(|e| e.to_string()));
                }
            }
            ViewerCommand::Shoot { shots, reply } => {
                if !reply.is_closed() {
                    let _ = reply.send(Ok(scene.shoot(shots)));
                }
            }
            ViewerCommand::PixelStats { reply } => {
                let result = scene.update().and_then(|()| {
                    scene.renderer.as_ref().unwrap().pixel_stats(
                        &scene.device,
                        &scene.queue,
                        size[0],
                        size[1],
                    )
                });
                let _ = reply.send(result.map_err(|e| e.to_string()));
            }
            ViewerCommand::SetRotation { x, y, z } => {
                state.lock().unwrap().model_rotation = na::Vector3::new(x, y, z)
            }
            ViewerCommand::RotateAroundAxis { axis, angle } => {
                state.lock().unwrap().apply_rotation(axis, angle)
            }
            ViewerCommand::ToggleWireframe(value) => state.lock().unwrap().show_wireframe = value,
            ViewerCommand::ToggleBackfaces(value) => state.lock().unwrap().show_backfaces = value,
            ViewerCommand::SetBackfaceMode(value) => state.lock().unwrap().backface_unlit = value,
            ViewerCommand::LoadClips { path, reply } => {
                let result: std::result::Result<Vec<crate::rpc::types::AnimationResponse>, String> =
                    scene
                        .pose
                        .as_mut()
                        .ok_or_else(|| "No skinned model loaded".to_string())
                        .and_then(|pose| pose.load_clips(&path));
                let _ = reply.send(result);
            }
            ViewerCommand::SetAnimation { name, reply } => {
                let result: std::result::Result<crate::rpc::types::AnimationResponse, String> =
                    scene
                        .pose
                        .as_mut()
                        .ok_or_else(|| "No skinned model loaded".to_string())
                        .and_then(|pose| pose.select(&name));
                let _ = reply.send(result);
            }
            ViewerCommand::SetTime { time, reply } => {
                let result: std::result::Result<crate::rpc::types::AnimationResponse, String> =
                    scene
                        .pose
                        .as_mut()
                        .ok_or_else(|| "No skinned model loaded".to_string())
                        .and_then(|pose| pose.set_time(time));
                let _ = reply.send(result);
            }
            ViewerCommand::SetFrame { frame, fps, reply } => {
                let result: std::result::Result<crate::rpc::types::AnimationResponse, String> =
                    scene
                        .pose
                        .as_mut()
                        .ok_or_else(|| "No skinned model loaded".to_string())
                        .and_then(|pose| pose.set_frame(frame, fps));
                let _ = reply.send(result);
            }
            ViewerCommand::ListAnimations { reply } => {
                let result: std::result::Result<Vec<crate::rpc::types::AnimationResponse>, String> =
                    scene
                        .pose
                        .as_mut()
                        .ok_or_else(|| "No skinned model loaded".to_string())
                        .and_then(|pose| Ok(pose.list()));
                let _ = reply.send(result);
            }
            ViewerCommand::ToggleGround(value) => {
                let mut state = state.lock().unwrap();
                state.show_ground = value.unwrap_or(!state.show_ground);
            }
            ViewerCommand::ToggleUI(_) => {} // Deliberately absent from offscreen output.
            ViewerCommand::SetProjection { mode } => state.lock().unwrap().projection = mode,
            ViewerCommand::SetClearColor { color } => {
                state.lock().unwrap().background = Background::Solid(color)
            }
            ViewerCommand::SetShading { mode } => state.lock().unwrap().shading = mode,
            ViewerCommand::SetBaseColor { color } => state.lock().unwrap().base_color = color,
            ViewerCommand::SetLightDirection { direction } => {
                state.lock().unwrap().light_direction = direction
            }
            ViewerCommand::ApplyPreset { name } => {
                if let Some(preset) = super::state::RenderPreset::by_name(&name) {
                    state.lock().unwrap().apply_preset(&preset);
                }
            }
            #[cfg(feature = "renderdoc")]
            ViewerCommand::CaptureFrame { .. } => {
                eprintln!("RenderDoc capture is not supported by msh serve")
            }
            ViewerCommand::Quit => break,
        }
    }
    let _ = server.stop();
    Ok(())
}
