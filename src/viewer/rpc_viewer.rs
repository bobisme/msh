#[cfg(feature = "remote")]
use crossbeam::channel::{self, Receiver, Sender};
#[cfg(feature = "remote")]
use nalgebra as na;
#[cfg(feature = "remote")]
use std::path::PathBuf;
#[cfg(feature = "remote")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "remote")]
use winit::{
    application::ApplicationHandler,
    event::*,
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

#[cfg(feature = "remote")]
use super::render::{RenderMesh, extract_render_data};
#[cfg(feature = "remote")]
use super::state::{Background, MeshStats, ViewerCommand, ViewerState};
#[cfg(feature = "remote")]
use super::{
    camera::ArcBallCamera,
    gpu::GpuState,
    mesh_renderer::{MeshRenderer, Vertex},
    ui_renderer::UiRenderer,
};
#[cfg(feature = "remote")]
use crate::mesh::loader::{MeshMaterials, load_mesh_with_colors};
#[cfg(feature = "remote")]
use crate::rpc::spawn_rpc_server;

#[cfg(all(feature = "remote", feature = "renderdoc"))]
use super::renderdoc_helper::RenderDocCapture;

enum CaptureReply {
    Single(super::state::Reply<crate::rpc::types::ScreenshotResponse>),
    Batch,
}
struct ShotBatch {
    shots: std::collections::VecDeque<crate::rpc::types::Shot>,
    results: Vec<crate::rpc::types::ShotResult>,
    reply: super::state::Reply<Vec<crate::rpc::types::ShotResult>>,
}

#[cfg(feature = "remote")]
/// Application state for the RPC-enabled viewer
struct RpcViewerApp {
    window: Option<Arc<Window>>,
    gpu: Option<GpuState>,
    camera: Option<ArcBallCamera>,
    mesh_renderer: Option<MeshRenderer>,
    ui_renderer: Option<UiRenderer>,
    state: Arc<Mutex<ViewerState>>,
    command_rx: Receiver<ViewerCommand>,
    render_mesh: RenderMesh,
    materials: MeshMaterials,
    mouse_pressed_left: bool,
    mouse_pressed_right: bool,
    last_mouse_pos: Option<winit::dpi::PhysicalPosition<f64>>,
    screenshot_path: Option<(String, CaptureReply)>,
    z_up: bool,
    pose: Option<super::pose::PoseState>,
    shot_batch: Option<ShotBatch>,
    pixel_reply: Option<super::state::Reply<crate::rpc::types::PixelStats>>,
    vsync: bool,
    #[cfg(feature = "renderdoc")]
    renderdoc: RenderDocCapture,
}

#[cfg(feature = "remote")]
impl RpcViewerApp {
    #[allow(clippy::too_many_arguments)]
    fn new(
        state: Arc<Mutex<ViewerState>>,
        command_rx: Receiver<ViewerCommand>,
        render_mesh: RenderMesh,
        materials: MeshMaterials,
        vsync: bool,
        z_up: bool,
        pose: Option<super::pose::PoseState>,
    ) -> Self {
        Self {
            window: None,
            gpu: None,
            camera: None,
            mesh_renderer: None,
            ui_renderer: None,
            state,
            command_rx,
            render_mesh,
            materials,
            mouse_pressed_left: false,
            mouse_pressed_right: false,
            last_mouse_pos: None,
            screenshot_path: None,
            shot_batch: None,
            pixel_reply: None,
            vsync,
            z_up,
            pose,
            #[cfg(feature = "renderdoc")]
            renderdoc: RenderDocCapture::new(),
        }
    }

    fn process_commands(&mut self) {
        // A capture freezes command processing until that exact frame is saved.
        if self.screenshot_path.is_some() || self.pixel_reply.is_some() {
            return;
        }
        if let Some(mut batch) = self.shot_batch.take() {
            if !batch.reply.is_closed() {
                while let Some(shot) = batch.shots.pop_front() {
                    let result = if shot
                        .fov
                        .is_some_and(|f| !f.is_finite() || !(1.0..179.0).contains(&f))
                    {
                        Err("FOV must be between 1 and 179 degrees".into())
                    } else {
                        self.camera
                            .as_mut()
                            .ok_or("Camera not ready".into())
                            .and_then(|c| c.set_pose(Some(shot.eye), Some(shot.target), shot.up))
                    };
                    if let Err(error) = result {
                        batch.results.push(crate::rpc::types::ShotResult {
                            path: shot.path,
                            width: 0,
                            height: 0,
                            error: Some(error),
                        });
                        continue;
                    }
                    if let Some(fov_y_degrees) = shot.fov
                        && let Ok(mut state) = self.state.lock()
                    {
                        state.projection =
                            super::state::ProjectionMode::Perspective { fov_y_degrees };
                    }
                    self.screenshot_path = Some((shot.path, CaptureReply::Batch));
                    self.shot_batch = Some(batch);
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                    return;
                }
                let _ = batch.reply.send(Ok(batch.results));
            }
        }
        while let Ok(cmd) = self.command_rx.try_recv() {
            if let Some(camera) = self.camera.as_mut() {
                match cmd {
                    ViewerCommand::ToggleWireframe(enabled) => {
                        if let Ok(mut state) = self.state.lock() {
                            state.show_wireframe = enabled;
                            println!(
                                "Wireframe: {}",
                                if state.show_wireframe { "ON" } else { "OFF" }
                            );
                        }
                    }
                    ViewerCommand::ToggleBackfaces(enabled) => {
                        if let Ok(mut state) = self.state.lock() {
                            state.show_backfaces = enabled;
                            println!(
                                "Backfaces: {}",
                                if state.show_backfaces { "ON" } else { "OFF" }
                            );
                        }
                    }
                    ViewerCommand::SetBackfaceMode(unlit) => {
                        if let Ok(mut state) = self.state.lock() {
                            state.backface_unlit = unlit;
                        }
                    }
                    ViewerCommand::ToggleUI(enabled) => {
                        if let Ok(mut state) = self.state.lock() {
                            state.show_ui = enabled;
                            println!("UI: {}", if state.show_ui { "ON" } else { "OFF" });
                        }
                    }
                    ViewerCommand::SetCamera {
                        eye,
                        target,
                        up,
                        fov,
                        reply,
                    } => {
                        if reply.is_closed() {
                            continue;
                        }
                        let result = camera.set_pose(eye, target, up);
                        if result.is_ok()
                            && let Some(fov_y_degrees) = fov
                        {
                            if let Ok(mut state) = self.state.lock() {
                                state.projection =
                                    super::state::ProjectionMode::Perspective { fov_y_degrees };
                            }
                        }
                        let _ = reply.send(result);
                    }
                    ViewerCommand::LoadClips { path, reply } => {
                        let result: Result<Vec<crate::rpc::types::AnimationResponse>, String> =
                            self.pose
                                .as_mut()
                                .ok_or_else(|| "No skinned model loaded".to_string())
                                .and_then(|pose| pose.load_clips(&path));
                        let _ = reply.send(result);
                    }
                    ViewerCommand::SetAnimation { name, reply } => {
                        let result: Result<crate::rpc::types::AnimationResponse, String> = self
                            .pose
                            .as_mut()
                            .ok_or_else(|| "No skinned model loaded".to_string())
                            .and_then(|pose| pose.select(&name));
                        let _ = reply.send(result);
                    }
                    ViewerCommand::SetTime { time, reply } => {
                        let result: Result<crate::rpc::types::AnimationResponse, String> = self
                            .pose
                            .as_mut()
                            .ok_or_else(|| "No skinned model loaded".to_string())
                            .and_then(|pose| pose.set_time(time));
                        let _ = reply.send(result);
                    }
                    ViewerCommand::SetFrame { frame, fps, reply } => {
                        let result: Result<crate::rpc::types::AnimationResponse, String> = self
                            .pose
                            .as_mut()
                            .ok_or_else(|| "No skinned model loaded".to_string())
                            .and_then(|pose| pose.set_frame(frame, fps));
                        let _ = reply.send(result);
                    }
                    ViewerCommand::ListAnimations { reply } => {
                        let result: Result<Vec<crate::rpc::types::AnimationResponse>, String> =
                            self.pose
                                .as_mut()
                                .ok_or_else(|| "No skinned model loaded".to_string())
                                .and_then(|pose| Ok(pose.list()));
                        let _ = reply.send(result);
                    }
                    ViewerCommand::ToggleGround(value) => {
                        if let Ok(mut state) = self.state.lock() {
                            state.show_ground = value.unwrap_or(!state.show_ground);
                        }
                    }
                    ViewerCommand::SetRotation { x, y, z } => {
                        if let Ok(mut state) = self.state.lock() {
                            state.model_rotation = na::Vector3::new(x, y, z);
                            println!("Set rotation: x={}, y={}, z={}", x, y, z);
                        }
                    }
                    ViewerCommand::RotateAroundAxis { axis, angle } => {
                        if let Ok(mut state) = self.state.lock() {
                            state.apply_rotation(axis, angle);
                            println!("Applied rotation: axis={:?}, angle={}", axis, angle);
                        }
                    }
                    ViewerCommand::LoadModel {
                        path,
                        mesh_name,
                        keep_camera,
                        clips,
                        reply,
                    } => {
                        if reply.is_closed() {
                            continue;
                        }
                        let result =
                            (|| -> Result<crate::rpc::types::MeshStatsResponse, String> {
                                let mut mesh_data =
                                    load_mesh_with_colors(&path, mesh_name.as_deref())
                                        .map_err(|e| e.to_string())?;
                                let clips =
                                    clips.or_else(|| self.state.lock().unwrap().clips_path.clone());
                                if let Some(path) = clips {
                                    crate::mesh::clips::attach(&mut mesh_data, &path)
                                        .map_err(|e| e.to_string())?;
                                }
                                if self.z_up {
                                    mesh_data.convert_z_up_to_y_up();
                                }
                                let stats =
                                    MeshStats::from_mesh(&mesh_data).map_err(|e| e.to_string())?;
                                let render_mesh = extract_render_data(&mesh_data, false);
                                let gpu = self.gpu.as_ref().ok_or("GPU is not ready")?;
                                // Prepare the replacement before replacing the currently valid scene.
                                gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
                                gpu.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
                                let mut renderer =
                                    MeshRenderer::new(&gpu.device, &gpu.queue, &gpu.config);
                                renderer.load_mesh(
                                    &gpu.device,
                                    &gpu.queue,
                                    &render_mesh,
                                    &mesh_data.materials,
                                );
                                if let Some(ref skeleton) = render_mesh.skeleton_data {
                                    renderer
                                        .update_joint_palette(&gpu.queue, &skeleton.joint_matrices);
                                    renderer.set_joint_count(skeleton.joint_matrices.len() as u32);
                                }
                                let allocation = pollster::block_on(gpu.device.pop_error_scope());
                                let validation = pollster::block_on(gpu.device.pop_error_scope());
                                if let Some(error) = allocation.or(validation) {
                                    return Err(error.to_string());
                                }
                                if !keep_camera {
                                    let d = render_mesh.max_dimension.max(0.001) * 2.5;
                                    camera.set_pose(
                                        Some([d * 0.5, d * 0.3, d]),
                                        Some([0.0; 3]),
                                        None,
                                    )?;
                                }
                                self.mesh_renderer = Some(renderer);
                                self.render_mesh = render_mesh;
                                self.pose = mesh_data.skeleton.take().map(|s| {
                                    super::pose::PoseState::new(
                                        s,
                                        std::mem::take(&mut mesh_data.animations),
                                        false,
                                    )
                                });
                                self.materials = mesh_data.materials;
                                let response = crate::rpc::types::MeshStatsResponse {
                                    vertices: stats.vertex_count,
                                    edges: stats.edge_count,
                                    faces: stats.face_count,
                                    triangles: stats.face_count,
                                    is_manifold: stats.is_manifold,
                                    holes: stats.hole_count,
                                    topology: stats.topology.clone(),
                                };
                                if let Ok(mut state) = self.state.lock() {
                                    state.stats = stats;
                                }
                                println!("Mesh loaded: {} triangles", response.faces);
                                Ok(response)
                            })();
                        let _ = reply.send(result);
                    }
                    ViewerCommand::Screenshot { path, reply } => {
                        if reply.is_closed() {
                            continue;
                        }
                        self.screenshot_path = Some((path, CaptureReply::Single(reply)));
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                        break;
                    }
                    ViewerCommand::Shoot { shots, reply } => {
                        if self.gpu.as_ref().is_some_and(|g| {
                            u64::from(g.config.width)
                                * u64::from(g.config.height)
                                * shots.len() as u64
                                > 67_108_864
                        }) {
                            let _ =
                                reply.send(Err("Shot batch exceeds 64 megapixels total".into()));
                            continue;
                        }
                        self.shot_batch = Some(ShotBatch {
                            shots: shots.into(),
                            results: Vec::new(),
                            reply,
                        });
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                        break;
                    }
                    ViewerCommand::PixelStats { reply } => {
                        // Reuse the next render's exact camera, uniforms and depth dimensions.
                        // A synthetic capture barrier below performs this without a PNG.
                        self.pixel_reply = Some(reply);
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                        break;
                    }
                    ViewerCommand::SetProjection { mode } => {
                        if let Ok(mut state) = self.state.lock() {
                            println!("Projection: {:?}", mode);
                            state.projection = mode;
                        }
                    }
                    ViewerCommand::SetClearColor { color } => {
                        if let Ok(mut state) = self.state.lock() {
                            println!("Clear color: {:?}", color);
                            state.background = Background::Solid(color);
                        }
                    }
                    ViewerCommand::SetShading { mode } => {
                        if let Ok(mut state) = self.state.lock() {
                            println!("Shading: {:?}", mode);
                            state.shading = mode;
                        }
                    }
                    ViewerCommand::SetBaseColor { color } => {
                        if let Ok(mut state) = self.state.lock() {
                            println!("Base color: {:?}", color);
                            state.base_color = color;
                        }
                    }
                    ViewerCommand::SetLightDirection { direction } => {
                        if let Ok(mut state) = self.state.lock() {
                            println!("Light direction: {:?}", direction);
                            state.light_direction = direction;
                        }
                    }
                    ViewerCommand::ApplyPreset { name } => {
                        use super::state::RenderPreset;
                        if let Some(preset) = RenderPreset::by_name(&name) {
                            if let Ok(mut state) = self.state.lock() {
                                state.apply_preset(&preset);
                                println!("Applied preset: {}", name);
                            }
                        } else {
                            eprintln!("Unknown preset: {}", name);
                        }
                    }
                    #[cfg(feature = "renderdoc")]
                    ViewerCommand::CaptureFrame { path } => {
                        self.renderdoc.trigger_capture(path.as_deref());
                    }
                    ViewerCommand::Quit => {
                        println!("Quit command received via RPC");
                        std::process::exit(0);
                    }
                }
            }
        }
    }
}

#[cfg(feature = "remote")]
impl ApplicationHandler for RpcViewerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let state = self.state.lock().unwrap();
            let mut window_attributes = Window::default_attributes().with_title(
                state
                    .window_title
                    .as_deref()
                    .unwrap_or("Mesh Viewer - msh (RPC Enabled)"),
            );
            if let Some([width, height]) = state.window_size {
                window_attributes =
                    window_attributes.with_inner_size(winit::dpi::PhysicalSize::new(width, height));
            }
            drop(state);
            let window = Arc::new(event_loop.create_window(window_attributes).unwrap());

            // Initialize GPU
            let size = window.inner_size();
            let vsync = self.vsync;
            let gpu =
                pollster::block_on(async { GpuState::new(window.clone(), vsync).await.unwrap() });

            // Create camera
            let camera_distance = self.render_mesh.max_dimension * 2.5;
            let eye = na::Point3::new(
                camera_distance * 0.5,
                camera_distance * 0.3,
                camera_distance,
            );
            let target = na::Point3::origin();
            let camera = ArcBallCamera::new(eye, target, size.width, size.height);

            // Create mesh renderer
            let mut mesh_renderer = MeshRenderer::new(&gpu.device, &gpu.queue, &gpu.config);
            mesh_renderer.load_mesh(&gpu.device, &gpu.queue, &self.render_mesh, &self.materials);
            if let Some(skeleton) = &self.render_mesh.skeleton_data {
                mesh_renderer.update_joint_palette(&gpu.queue, &skeleton.joint_matrices);
                mesh_renderer.set_joint_count(skeleton.joint_matrices.len() as u32);
            }

            // Create UI renderer
            let ui_renderer = UiRenderer::new(&gpu.device, &gpu.queue, &gpu.config);

            self.gpu = Some(gpu);
            self.camera = Some(camera);
            self.mesh_renderer = Some(mesh_renderer);
            self.ui_renderer = Some(ui_renderer);
            self.window = Some(window);

            println!(
                "Viewing mesh with RPC server on {}",
                self.state
                    .lock()
                    .unwrap()
                    .rpc_address
                    .as_deref()
                    .unwrap_or("unknown")
            );
            println!("  Mouse: Rotate (drag), Zoom (scroll), Pan (right-drag)");
            println!("  W: Toggle wireframe overlay");
            println!("  B: Toggle backface visualization (red)");
            println!("  U: Toggle UI overlay");
            println!("  Q/ESC: Exit");
            println!("  RPC Commands: wireframe, backfaces, ui, screenshot, load, quit");
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // Process RPC commands
        self.process_commands();

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if let (Some(gpu), Some(mesh_renderer), Some(ui_renderer), Some(camera)) = (
                    self.gpu.as_mut(),
                    self.mesh_renderer.as_mut(),
                    self.ui_renderer.as_mut(),
                    self.camera.as_mut(),
                ) {
                    gpu.resize(new_size);
                    mesh_renderer.resize(&gpu.device, &gpu.config);
                    ui_renderer.resize(&gpu.device, &gpu.queue, new_size.width, new_size.height);
                    camera.resize(new_size.width, new_size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed
                    && let PhysicalKey::Code(keycode) = event.physical_key
                {
                    match keycode {
                        KeyCode::KeyW => {
                            if let Ok(mut state) = self.state.lock() {
                                state.show_wireframe = !state.show_wireframe;
                                println!(
                                    "Wireframe: {}",
                                    if state.show_wireframe { "ON" } else { "OFF" }
                                );
                            }
                        }
                        KeyCode::KeyB => {
                            if let Ok(mut state) = self.state.lock() {
                                state.show_backfaces = !state.show_backfaces;
                                println!(
                                    "Backfaces: {}",
                                    if state.show_backfaces { "ON" } else { "OFF" }
                                );
                            }
                        }
                        KeyCode::KeyU => {
                            if let Ok(mut state) = self.state.lock() {
                                state.show_ui = !state.show_ui;
                                println!("UI: {}", if state.show_ui { "ON" } else { "OFF" });
                            }
                        }
                        KeyCode::Space => {
                            if let Some(pose) = &mut self.pose {
                                pose.playing = !pose.playing;
                                pose.last_frame = std::time::Instant::now();
                            }
                        }
                        KeyCode::KeyG => {
                            if let Ok(mut state) = self.state.lock() {
                                state.show_ground = !state.show_ground;
                                println!(
                                    "Ground: {}",
                                    if state.show_ground { "ON" } else { "OFF" }
                                );
                            }
                        }
                        KeyCode::KeyQ | KeyCode::Escape => {
                            event_loop.exit();
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::MouseInput {
                state: btn_state,
                button,
                ..
            } => match button {
                MouseButton::Left => {
                    self.mouse_pressed_left = btn_state == ElementState::Pressed;
                    if !self.mouse_pressed_left {
                        self.last_mouse_pos = None;
                    }
                }
                MouseButton::Right => {
                    self.mouse_pressed_right = btn_state == ElementState::Pressed;
                    if !self.mouse_pressed_right {
                        self.last_mouse_pos = None;
                    }
                }
                _ => {}
            },
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(camera) = self.camera.as_mut() {
                    if let Some(last_pos) = self.last_mouse_pos {
                        let delta_x = position.x - last_pos.x;
                        let delta_y = position.y - last_pos.y;

                        if self.mouse_pressed_left {
                            camera.rotate(delta_x as f32, delta_y as f32);
                        } else if self.mouse_pressed_right {
                            camera.pan(delta_x as f32, delta_y as f32);
                        }
                    }
                    if self.mouse_pressed_left || self.mouse_pressed_right {
                        self.last_mouse_pos = Some(position);
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(camera) = self.camera.as_mut() {
                    let scroll_delta = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(pos) => (pos.y / 100.0) as f32,
                    };
                    camera.zoom(scroll_delta);
                }
            }
            WindowEvent::RedrawRequested => {
                if let (
                    Some(window),
                    Some(gpu),
                    Some(camera),
                    Some(mesh_renderer),
                    Some(ui_renderer),
                ) = (
                    self.window.as_ref(),
                    self.gpu.as_mut(),
                    self.camera.as_ref(),
                    self.mesh_renderer.as_mut(),
                    self.ui_renderer.as_mut(),
                ) {
                    // Get current state
                    let (show_wireframe, show_backfaces, show_ui, model_rotation) =
                        if let Ok(state) = self.state.lock() {
                            (
                                state.show_wireframe,
                                state.show_backfaces,
                                state.show_ui,
                                state.model_rotation,
                            )
                        } else {
                            (false, false, true, na::Vector3::zeros())
                        };

                    // Read render state
                    let (
                        projection,
                        background,
                        shading_mode,
                        base_color,
                        light_direction,
                        show_ground,
                    ) = if let Ok(state) = self.state.lock() {
                        (
                            state.projection.clone(),
                            state.background,
                            state.shading.as_u32(),
                            state.base_color,
                            state.light_direction,
                            state.show_ground,
                        )
                    } else {
                        (
                            Default::default(),
                            Background::default(),
                            0,
                            [0.85, 0.85, 0.85, 1.0],
                            [0.5, 1.0, 0.5],
                            true,
                        )
                    };

                    if let Some(pose) = &mut self.pose {
                        pose.update(mesh_renderer, &gpu.queue);
                    }

                    // Update uniforms
                    let view_proj = camera.view_projection_matrix_for(&projection);
                    let rotation = na::Rotation3::from_euler_angles(
                        model_rotation.x,
                        model_rotation.y,
                        model_rotation.z,
                    );
                    let model = rotation.to_homogeneous();
                    mesh_renderer.update_uniforms(
                        &gpu.queue,
                        &view_proj,
                        &model,
                        &camera.position(),
                        shading_mode,
                        base_color,
                        light_direction,
                        background,
                        self.state.lock().map(|s| s.backface_unlit).unwrap_or(false),
                    );

                    if let Some(reply) = self.pixel_reply.take() {
                        let result = mesh_renderer
                            .pixel_stats(
                                &gpu.device,
                                &gpu.queue,
                                gpu.config.width,
                                gpu.config.height,
                            )
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }

                    // Queue UI text
                    if show_ui && let Ok(state) = self.state.lock() {
                        ui_renderer.queue_text(&gpu.device, &gpu.queue, &state, true, None, None);
                    }

                    // Render
                    let surface_texture = gpu.surface.get_current_texture();
                    match surface_texture {
                        Ok(output) => {
                            let view = output
                                .texture
                                .create_view(&wgpu::TextureViewDescriptor::default());

                            let mut encoder = gpu.device.create_command_encoder(
                                &wgpu::CommandEncoderDescriptor {
                                    label: Some("Render Encoder"),
                                },
                            );

                            // Render mesh
                            mesh_renderer.render(
                                &mut encoder,
                                &view,
                                show_wireframe,
                                show_backfaces,
                                show_ground,
                            );

                            // Render UI
                            if show_ui {
                                ui_renderer.render(&mut encoder, &view);
                            }

                            gpu.queue.submit(std::iter::once(encoder.finish()));

                            // Capture screenshot if requested (before present)
                            if let Some((path, reply)) = self.screenshot_path.take() {
                                let result = gpu
                                    .screenshot_from_texture(&output.texture, &path)
                                    .map(|()| crate::rpc::types::ScreenshotResponse {
                                        path: path.clone(),
                                        width: gpu.config.width,
                                        height: gpu.config.height,
                                    })
                                    .map_err(|e| e.to_string());
                                if result.is_ok() {
                                    println!("Screenshot saved to {}", path);
                                }
                                match reply {
                                    CaptureReply::Single(reply) => {
                                        let _ = reply.send(result);
                                    }
                                    CaptureReply::Batch => {
                                        if let Some(batch) = &mut self.shot_batch {
                                            batch.results.push(crate::rpc::types::ShotResult {
                                                path,
                                                width: gpu.config.width,
                                                height: gpu.config.height,
                                                error: result.err(),
                                            });
                                        }
                                    }
                                }
                            }

                            output.present();
                        }
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            gpu.surface.configure(&gpu.device, &gpu.config);
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => {
                            eprintln!("Out of GPU memory");
                            event_loop.exit();
                        }
                        Err(e) => {
                            eprintln!("Surface error: {:?}", e);
                        }
                    }

                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

#[cfg(feature = "remote")]
pub fn view_mesh_with_rpc(
    input: Option<&PathBuf>,
    mesh_name: Option<&str>,
    no_vsync: bool,
    z_up: bool,
    port: u16,
    configure_state: impl FnOnce(&mut ViewerState),
) -> Result<(), Box<dyn std::error::Error>> {
    let mut initial_state = ViewerState::default();
    configure_state(&mut initial_state);
    let (render_mesh, materials, stats, pose) = if let Some(input_path) = input {
        println!("Loading mesh from {:?}...", input_path);

        let mut mesh_data = load_mesh_with_colors(input_path, mesh_name)?;

        if let Some(path) = &initial_state.clips_path {
            crate::mesh::clips::attach(&mut mesh_data, path)?;
        }

        if z_up {
            mesh_data.convert_z_up_to_y_up();
            println!("Converted Z-up to Y-up");
        }

        let stats = MeshStats::from_mesh(&mesh_data)?;

        let render_mesh = extract_render_data(&mesh_data, false);
        let materials = std::mem::take(&mut mesh_data.materials);
        let pose = mesh_data.skeleton.take().map(|s| {
            super::pose::PoseState::new(s, std::mem::take(&mut mesh_data.animations), false)
        });
        (render_mesh, materials, stats, pose)
    } else {
        println!("Starting viewer without initial mesh (use 'msh remote load' to load a mesh)...");
        let vertices = vec![
            Vertex {
                position: [0.0, 0.0, 0.0],
                color: [0.0; 4],
                texcoord: [0.0; 2],
                joint_indices: [0; 4],
                joint_weights: [0.0; 4],
                normal: [0.0; 3],
                tangent: [0.0; 4],
            },
            Vertex {
                position: [0.0, 0.0, 0.0],
                color: [0.0; 4],
                texcoord: [0.0; 2],
                joint_indices: [0; 4],
                joint_weights: [0.0; 4],
                normal: [0.0; 3],
                tangent: [0.0; 4],
            },
            Vertex {
                position: [0.0, 0.0, 0.0],
                color: [0.0; 4],
                texcoord: [0.0; 2],
                joint_indices: [0; 4],
                joint_weights: [0.0; 4],
                normal: [0.0; 3],
                tangent: [0.0; 4],
            },
        ];
        let render_mesh = RenderMesh {
            vertices,
            indices: vec![0, 1, 2],
            backface_indices: vec![0, 2, 1],
            batches: Vec::new(),
            has_vertex_colors: false,
            max_dimension: 1.0,
            skeleton_data: None,
            ground_bounds: None,
        };
        (
            render_mesh,
            MeshMaterials::default(),
            MeshStats::default(),
            None,
        )
    };

    // Create shared state
    initial_state.stats = stats;
    let state = Arc::new(Mutex::new(initial_state));

    // Create command channel
    let (command_tx, command_rx): (Sender<ViewerCommand>, Receiver<ViewerCommand>) =
        channel::bounded(128);

    // Spawn RPC server in background thread
    let state_clone = Arc::clone(&state);
    let (address, rpc_handle) = spawn_rpc_server(state_clone, command_tx, port)?;
    state.lock().unwrap().rpc_address = Some(address.to_string());

    // Create application
    let vsync = !no_vsync;
    let mut app = RpcViewerApp::new(state, command_rx, render_mesh, materials, vsync, z_up, pose);

    // Create and run event loop
    let event_loop = EventLoop::new()?;
    let result = event_loop.run_app(&mut app);
    let _ = rpc_handle.stop();
    result?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_are_barriers_between_camera_commands() {
        let (tx, rx) = channel::bounded(8);
        let state = Arc::new(Mutex::new(ViewerState::default()));
        let render_mesh = RenderMesh {
            vertices: vec![],
            indices: vec![],
            backface_indices: vec![],
            batches: vec![],
            has_vertex_colors: false,
            max_dimension: 1.0,
            skeleton_data: None,
            ground_bounds: None,
        };
        let mut app = RpcViewerApp::new(
            state,
            rx,
            render_mesh,
            MeshMaterials::default(),
            true,
            false,
            None,
        );
        app.camera = Some(ArcBallCamera::new(
            na::Point3::new(1.0, 2.0, 3.0),
            na::Point3::origin(),
            800,
            600,
        ));
        let mut captures = Vec::new();
        let mut screenshot_receivers = Vec::new();
        for x in [4.0, 8.0] {
            let (reply, _receive) = tokio::sync::oneshot::channel();
            // Keep the camera completion receiver alive until the command is processed.
            captures.push(_receive);
            tx.send(ViewerCommand::SetCamera {
                eye: Some([x, 2.0, 3.0]),
                target: None,
                up: None,
                fov: None,
                reply,
            })
            .unwrap();
            let (reply, receive) = tokio::sync::oneshot::channel();
            // Deliberately do not await the first screenshot before submitting the next.
            screenshot_receivers.push(receive);
            tx.send(ViewerCommand::Screenshot {
                path: format!("{x}.png"),
                reply,
            })
            .unwrap();
        }
        app.process_commands();
        assert_eq!(app.camera.as_ref().unwrap().position().x, 4.0);
        assert_eq!(app.screenshot_path.as_ref().unwrap().0, "4.png");
        app.process_commands();
        assert_eq!(app.camera.as_ref().unwrap().position().x, 4.0);
        app.screenshot_path.take(); // Simulate saving this frame.
        app.process_commands();
        assert_eq!(app.camera.as_ref().unwrap().position().x, 8.0);
        assert_eq!(app.screenshot_path.as_ref().unwrap().0, "8.png");
    }
}
