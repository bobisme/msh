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
use super::state::{MeshStats, ViewerCommand, ViewerState};
#[cfg(feature = "remote")]
use super::{
    camera::ArcBallCamera,
    gpu::GpuState,
    mesh_renderer::{MeshRenderer, Vertex},
    ui_renderer::UiRenderer,
};
#[cfg(feature = "remote")]
use crate::mesh::loader::load_mesh_with_colors;
#[cfg(feature = "remote")]
use crate::rpc::spawn_rpc_server;
#[cfg(feature = "remote")]
use super::render::extract_render_data;

#[cfg(all(feature = "remote", feature = "renderdoc"))]
use super::renderdoc_helper::RenderDocCapture;

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
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    backface_indices: Vec<u32>,
    has_vertex_colors: bool,
    max_dimension: f32,
    mouse_pressed_left: bool,
    mouse_pressed_right: bool,
    last_mouse_pos: Option<winit::dpi::PhysicalPosition<f64>>,
    screenshot_path: Option<String>,
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
        vertices: Vec<Vertex>,
        indices: Vec<u32>,
        backface_indices: Vec<u32>,
        has_vertex_colors: bool,
        max_dimension: f32,
        vsync: bool,
    ) -> Self {
        Self {
            window: None,
            gpu: None,
            camera: None,
            mesh_renderer: None,
            ui_renderer: None,
            state,
            command_rx,
            vertices,
            indices,
            backface_indices,
            has_vertex_colors,
            max_dimension,
            mouse_pressed_left: false,
            mouse_pressed_right: false,
            last_mouse_pos: None,
            screenshot_path: None,
            vsync,
            #[cfg(feature = "renderdoc")]
            renderdoc: RenderDocCapture::new(),
        }
    }

    fn process_commands(&mut self) {
        // Process all pending commands
        while let Ok(cmd) = self.command_rx.try_recv() {
            if let Some(camera) = self.camera.as_mut() {
                match cmd {
                    ViewerCommand::ToggleWireframe(enabled) => {
                        if let Ok(mut state) = self.state.lock() {
                            state.show_wireframe = enabled;
                            println!("Wireframe: {}", if state.show_wireframe { "ON" } else { "OFF" });
                        }
                    }
                    ViewerCommand::ToggleBackfaces(enabled) => {
                        if let Ok(mut state) = self.state.lock() {
                            state.show_backfaces = enabled;
                            println!("Backfaces: {}", if state.show_backfaces { "ON" } else { "OFF" });
                        }
                    }
                    ViewerCommand::ToggleUI(enabled) => {
                        if let Ok(mut state) = self.state.lock() {
                            state.show_ui = enabled;
                            println!("UI: {}", if state.show_ui { "ON" } else { "OFF" });
                        }
                    }
                    ViewerCommand::SetCameraPosition { position } => {
                        camera.set_position(position);
                        println!("Camera position set to {:?}", position);
                    }
                    ViewerCommand::SetCameraTarget { target } => {
                        camera.set_target(target);
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
                    ViewerCommand::LoadModel { path, mesh_name } => {
                        println!("Loading mesh from {:?}...", path);
                        match load_mesh_with_colors(&path, mesh_name.as_deref()) {
                            Ok(mesh_data) => {
                                // Extract render data
                                let (vertices, indices, backface_indices, has_vertex_colors, max_dimension) =
                                    extract_render_data(&mesh_data);

                                self.vertices = vertices;
                                self.indices = indices;
                                self.backface_indices = backface_indices;
                                self.has_vertex_colors = has_vertex_colors;
                                self.max_dimension = max_dimension;

                                // Reload mesh in renderer
                                if let (Some(gpu), Some(mesh_renderer)) = (self.gpu.as_ref(), self.mesh_renderer.as_mut()) {
                                    mesh_renderer.load_mesh(&gpu.device, &self.vertices, &self.indices, &self.backface_indices, self.has_vertex_colors);
                                }

                                // Update camera to frame new mesh
                                let camera_distance = self.max_dimension * 2.5;
                                let eye = na::Point3::new(
                                    camera_distance * 0.5,
                                    camera_distance * 0.3,
                                    camera_distance,
                                );
                                let target = na::Point3::origin();
                                camera.set_position(eye);
                                camera.set_target(target);
                                println!("Camera repositioned to fit model (dimension: {:.3})", self.max_dimension);

                                // Update stats from CornerTableF
                                match mesh_data.to_corner_table() {
                                    Ok(mesh) => {
                                        if let Ok(mut state) = self.state.lock() {
                                            state.stats.vertex_count = mesh.count_vertices();
                                            state.stats.face_count = mesh.count_faces();
                                            state.stats.edge_count = mesh.unique_edges().count();
                                            let boundary_rings = mesh.boundary_rings();
                                            state.stats.is_manifold = boundary_rings.is_empty();
                                            state.stats.hole_count = boundary_rings.len();
                                        }
                                    }
                                    Err(e) => eprintln!("Warning: could not compute mesh stats: {}", e),
                                }

                                println!("Mesh loaded: {} triangles{}", self.indices.len() / 3,
                                    if has_vertex_colors { " with material colors" } else { "" });
                            }
                            Err(e) => {
                                eprintln!("Failed to load mesh: {}", e);
                            }
                        }
                    }
                    ViewerCommand::Screenshot { path } => {
                        // Schedule screenshot for next frame render
                        self.screenshot_path = Some(path);
                        println!("Screenshot will be captured on next frame");
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
                            state.clear_color = color;
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
            let window_attributes = Window::default_attributes()
                .with_title("Mesh Viewer - msh (RPC Enabled)");
            let window = Arc::new(event_loop.create_window(window_attributes).unwrap());

            // Initialize GPU
            let size = window.inner_size();
            let vsync = self.vsync;
            let gpu = pollster::block_on(async {
                GpuState::new(window.clone(), vsync).await.unwrap()
            });

            // Create camera
            let camera_distance = self.max_dimension * 2.5;
            let eye = na::Point3::new(
                camera_distance * 0.5,
                camera_distance * 0.3,
                camera_distance,
            );
            let target = na::Point3::origin();
            let camera = ArcBallCamera::new(eye, target, size.width, size.height);

            // Create mesh renderer
            let mut mesh_renderer = MeshRenderer::new(&gpu.device, &gpu.config);
            mesh_renderer.load_mesh(&gpu.device, &self.vertices, &self.indices, &self.backface_indices, self.has_vertex_colors);

            // Create UI renderer
            let ui_renderer = UiRenderer::new(&gpu.device, &gpu.queue, &gpu.config);

            self.gpu = Some(gpu);
            self.camera = Some(camera);
            self.mesh_renderer = Some(mesh_renderer);
            self.ui_renderer = Some(ui_renderer);
            self.window = Some(window);

            println!("Viewing mesh with RPC server on 127.0.0.1:9001...");
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
                    && let PhysicalKey::Code(keycode) = event.physical_key {
                        match keycode {
                            KeyCode::KeyW => {
                                if let Ok(mut state) = self.state.lock() {
                                    state.show_wireframe = !state.show_wireframe;
                                    println!("Wireframe: {}", if state.show_wireframe { "ON" } else { "OFF" });
                                }
                            }
                            KeyCode::KeyB => {
                                if let Ok(mut state) = self.state.lock() {
                                    state.show_backfaces = !state.show_backfaces;
                                    println!("Backfaces: {}", if state.show_backfaces { "ON" } else { "OFF" });
                                }
                            }
                            KeyCode::KeyU => {
                                if let Ok(mut state) = self.state.lock() {
                                    state.show_ui = !state.show_ui;
                                    println!("UI: {}", if state.show_ui { "ON" } else { "OFF" });
                                }
                            }
                            KeyCode::KeyQ | KeyCode::Escape => {
                                event_loop.exit();
                            }
                            _ => {}
                        }
                    }
            }
            WindowEvent::MouseInput { state: btn_state, button, .. } => {
                match button {
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
                }
            }
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
                if let (Some(window), Some(gpu), Some(camera), Some(mesh_renderer), Some(ui_renderer)) = (
                    self.window.as_ref(),
                    self.gpu.as_mut(),
                    self.camera.as_ref(),
                    self.mesh_renderer.as_mut(),
                    self.ui_renderer.as_mut(),
                ) {
                    // Get current state
                    let (show_wireframe, show_backfaces, show_ui, model_rotation) = if let Ok(state) = self.state.lock() {
                        (state.show_wireframe, state.show_backfaces, state.show_ui, state.model_rotation)
                    } else {
                        (false, false, true, na::Vector3::zeros())
                    };

                    // Read render state
                    let (projection, clear_color, shading_mode, base_color, light_direction) = if let Ok(state) = self.state.lock() {
                        (state.projection.clone(), state.clear_color, state.shading.as_u32(), state.base_color, state.light_direction)
                    } else {
                        (Default::default(), [0.0, 0.0, 0.0, 1.0], 0, [0.85, 0.85, 0.85, 1.0], [0.5, 1.0, 0.5])
                    };

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
                    );

                    // Queue UI text
                    if show_ui
                        && let Ok(state) = self.state.lock() {
                            ui_renderer.queue_text(&gpu.device, &gpu.queue, &state, true);
                        }

                    // Render
                    let surface_texture = gpu.surface.get_current_texture();
                    match surface_texture {
                        Ok(output) => {
                            let view = output
                                .texture
                                .create_view(&wgpu::TextureViewDescriptor::default());

                            let mut encoder = gpu
                                .device
                                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                    label: Some("Render Encoder"),
                                });

                            // Render mesh
                            mesh_renderer.render(
                                &mut encoder,
                                &view,
                                show_wireframe,
                                show_backfaces,
                                clear_color,
                            );

                            // Render UI
                            if show_ui {
                                ui_renderer.render(&mut encoder, &view);
                            }

                            gpu.queue.submit(std::iter::once(encoder.finish()));

                            // Capture screenshot if requested (before present)
                            if let Some(path) = self.screenshot_path.take() {
                                match gpu.screenshot_from_texture(&output.texture, &path) {
                                    Ok(_) => println!("Screenshot saved to {}", path),
                                    Err(e) => eprintln!("Failed to save screenshot: {}", e),
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
    configure_state: impl FnOnce(&mut ViewerState),
) -> Result<(), Box<dyn std::error::Error>> {
    let (vertices, indices, backface_indices, has_vertex_colors, max_dimension, stats) = if let Some(input_path) = input {
        println!("Loading mesh from {:?}...", input_path);

        let mut mesh_data = load_mesh_with_colors(input_path, mesh_name)?;

        if z_up {
            mesh_data.convert_z_up_to_y_up();
            println!("Converted Z-up to Y-up");
        }

        // Build CornerTableF for stats
        let mesh = mesh_data.to_corner_table()?;
        let vertex_count = mesh.count_vertices();
        let face_count = mesh.count_faces();
        let edge_count = mesh.unique_edges().count();
        let boundary_rings = mesh.boundary_rings();
        let is_manifold = boundary_rings.is_empty();

        let stats = MeshStats {
            vertex_count,
            edge_count,
            face_count,
            is_manifold,
            hole_count: boundary_rings.len(),
        };

        let (vertices, indices, backface_indices, has_vertex_colors, max_dimension) =
            extract_render_data(&mesh_data);

        (vertices, indices, backface_indices, has_vertex_colors, max_dimension, stats)
    } else {
        println!("Starting viewer without initial mesh (use 'msh remote load' to load a mesh)...");
        let vertices = vec![
            Vertex { position: [0.0, 0.0, 0.0], color: [0.0; 4] },
            Vertex { position: [0.0, 0.0, 0.0], color: [0.0; 4] },
            Vertex { position: [0.0, 0.0, 0.0], color: [0.0; 4] },
        ];
        let indices = vec![0, 1, 2];
        let backface_indices = vec![0, 2, 1];
        (vertices, indices, backface_indices, false, 1.0, MeshStats::default())
    };

    // Create shared state
    let mut initial_state = ViewerState::for_mesh(max_dimension, stats);
    configure_state(&mut initial_state);
    let state = Arc::new(Mutex::new(initial_state));

    // Create command channel
    let (command_tx, command_rx): (Sender<ViewerCommand>, Receiver<ViewerCommand>) =
        channel::unbounded();

    // Spawn RPC server in background thread
    let state_clone = Arc::clone(&state);
    let _rpc_handle = spawn_rpc_server(state_clone, command_tx, 9001);

    // Create application
    let vsync = !no_vsync;
    let mut app = RpcViewerApp::new(state, command_rx, vertices, indices, backface_indices, has_vertex_colors, max_dimension, vsync);

    // Create and run event loop
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut app)?;

    Ok(())
}
