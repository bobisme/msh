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
    mesh_renderer::MeshRenderer,
    ui_renderer::UiRenderer,
};
#[cfg(feature = "remote")]
use crate::mesh::loader::load_mesh;
#[cfg(feature = "remote")]
use crate::rpc::spawn_rpc_server;

#[cfg(all(feature = "remote", feature = "renderdoc"))]
use super::renderdoc_helper::RenderDocCapture;

#[cfg(feature = "remote")]
/// Application state for the RPC-enabled viewer
struct RpcViewerApp {
    window: Option<Window>,
    gpu: Option<GpuState<'static>>,
    camera: Option<ArcBallCamera>,
    mesh_renderer: Option<MeshRenderer>,
    ui_renderer: Option<UiRenderer>,
    state: Arc<Mutex<ViewerState>>,
    command_rx: Receiver<ViewerCommand>,
    vertices: Vec<na::Point3<f32>>,
    indices: Vec<u32>,
    backface_indices: Vec<u32>,
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
    fn new(
        state: Arc<Mutex<ViewerState>>,
        command_rx: Receiver<ViewerCommand>,
        vertices: Vec<na::Point3<f32>>,
        indices: Vec<u32>,
        backface_indices: Vec<u32>,
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
                        match load_mesh(&path, mesh_name.as_deref()) {
                            Ok(mesh) => {
                                // Calculate bounding box
                                let mut min = [f32::INFINITY; 3];
                                let mut max = [f32::NEG_INFINITY; 3];
                                for vertex_id in mesh.vertices() {
                                    let pos = mesh.vertex_position(vertex_id);
                                    min[0] = min[0].min(pos.x);
                                    min[1] = min[1].min(pos.y);
                                    min[2] = min[2].min(pos.z);
                                    max[0] = max[0].max(pos.x);
                                    max[1] = max[1].max(pos.y);
                                    max[2] = max[2].max(pos.z);
                                }

                                let center = [
                                    (min[0] + max[0]) / 2.0,
                                    (min[1] + max[1]) / 2.0,
                                    (min[2] + max[2]) / 2.0,
                                ];

                                let size = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
                                self.max_dimension = size[0].max(size[1]).max(size[2]);

                                // Extract geometry
                                self.vertices.clear();
                                self.indices.clear();
                                self.backface_indices.clear();

                                let mut vertex_idx = 0u32;
                                for face_id in mesh.faces() {
                                    let triangle = mesh.face_positions(face_id);
                                    let v0 = triangle.p1();
                                    let v1 = triangle.p2();
                                    let v2 = triangle.p3();

                                    self.vertices.push(na::Point3::new(
                                        v0.x - center[0],
                                        v0.y - center[1],
                                        v0.z - center[2],
                                    ));
                                    self.vertices.push(na::Point3::new(
                                        v1.x - center[0],
                                        v1.y - center[1],
                                        v1.z - center[2],
                                    ));
                                    self.vertices.push(na::Point3::new(
                                        v2.x - center[0],
                                        v2.y - center[1],
                                        v2.z - center[2],
                                    ));

                                    self.indices.push(vertex_idx);
                                    self.indices.push(vertex_idx + 1);
                                    self.indices.push(vertex_idx + 2);
                                    vertex_idx += 3;
                                }

                                // Create backface indices
                                for i in (0..self.indices.len()).step_by(3) {
                                    self.backface_indices.push(self.indices[i]);
                                    self.backface_indices.push(self.indices[i + 2]);
                                    self.backface_indices.push(self.indices[i + 1]);
                                }

                                // Reload mesh in renderer
                                if let (Some(gpu), Some(mesh_renderer)) = (self.gpu.as_ref(), self.mesh_renderer.as_mut()) {
                                    mesh_renderer.load_mesh(&gpu.device, &self.vertices, &self.indices, &self.backface_indices);
                                }

                                // Update camera to frame new mesh
                                if let Some(camera) = self.camera.as_mut() {
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
                                }

                                // Update stats
                                if let Ok(mut state) = self.state.lock() {
                                    state.stats.vertex_count = mesh.count_vertices();
                                    state.stats.face_count = mesh.count_faces();
                                    state.stats.edge_count = mesh.unique_edges().count();
                                    let boundary_rings = mesh.boundary_rings();
                                    state.stats.is_manifold = boundary_rings.is_empty();
                                    state.stats.hole_count = boundary_rings.len();
                                }

                                println!("Mesh loaded: {} triangles", self.indices.len() / 3);
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
            let window = event_loop.create_window(window_attributes).unwrap();

            // Initialize GPU
            let size = window.inner_size();
            let vsync = self.vsync;
            let gpu = pollster::block_on(async {
                let window_ptr: &'static Window = unsafe {
                    std::mem::transmute(&window as &Window)
                };
                GpuState::new(window_ptr, vsync).await.unwrap()
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
            mesh_renderer.load_mesh(&gpu.device, &self.vertices, &self.indices, &self.backface_indices);

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
                if event.state == ElementState::Pressed {
                    if let PhysicalKey::Code(keycode) = event.physical_key {
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
                    let (show_wireframe, show_backfaces, show_ui) = if let Ok(state) = self.state.lock() {
                        (state.show_wireframe, state.show_backfaces, state.show_ui)
                    } else {
                        (false, false, true)
                    };

                    // Update uniforms
                    let view_proj = camera.view_projection_matrix();
                    let model = na::Matrix4::identity();
                    mesh_renderer.update_uniforms(&gpu.queue, &view_proj, &model, &camera.position());

                    // Queue UI text
                    if show_ui {
                        if let Ok(state) = self.state.lock() {
                            ui_renderer.queue_text(&gpu.device, &gpu.queue, &state, true);
                        }
                    }

                    // Render
                    match gpu.surface.get_current_texture() {
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
                        Err(e) => {
                            eprintln!("Failed to get surface texture: {:?}", e);
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
) -> Result<(), Box<dyn std::error::Error>> {
    let (vertices, indices, backface_indices, max_dimension, stats) = if let Some(input_path) = input {
        println!("Loading mesh from {:?}...", input_path);

        // Load initial mesh
        let mesh = load_mesh(input_path, mesh_name)?;

        // Calculate mesh statistics
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

        // Calculate bounding box
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];

        for vertex_id in mesh.vertices() {
            let pos = mesh.vertex_position(vertex_id);
            min[0] = min[0].min(pos.x);
            min[1] = min[1].min(pos.y);
            min[2] = min[2].min(pos.z);
            max[0] = max[0].max(pos.x);
            max[1] = max[1].max(pos.y);
            max[2] = max[2].max(pos.z);
        }

        let center = [
            (min[0] + max[0]) / 2.0,
            (min[1] + max[1]) / 2.0,
            (min[2] + max[2]) / 2.0,
        ];

        let size = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
        let max_dimension = size[0].max(size[1]).max(size[2]);

        // Extract geometry
        let mut vertices: Vec<na::Point3<f32>> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut vertex_idx = 0u32;

        for face_id in mesh.faces() {
            let triangle = mesh.face_positions(face_id);
            let v0 = triangle.p1();
            let v1 = triangle.p2();
            let v2 = triangle.p3();

            vertices.push(na::Point3::new(
                v0.x - center[0],
                v0.y - center[1],
                v0.z - center[2],
            ));
            vertices.push(na::Point3::new(
                v1.x - center[0],
                v1.y - center[1],
                v1.z - center[2],
            ));
            vertices.push(na::Point3::new(
                v2.x - center[0],
                v2.y - center[1],
                v2.z - center[2],
            ));

            indices.push(vertex_idx);
            indices.push(vertex_idx + 1);
            indices.push(vertex_idx + 2);
            vertex_idx += 3;
        }

        // Create backface indices
        let mut backface_indices: Vec<u32> = Vec::new();
        for i in (0..indices.len()).step_by(3) {
            backface_indices.push(indices[i]);
            backface_indices.push(indices[i + 2]);
            backface_indices.push(indices[i + 1]);
        }

        (vertices, indices, backface_indices, max_dimension, stats)
    } else {
        println!("Starting viewer without initial mesh (use 'msh remote load' to load a mesh)...");
        // No initial mesh - start with empty geometry
        let vertices = vec![
            na::Point3::new(0.0, 0.0, 0.0),
            na::Point3::new(0.0, 0.0, 0.0),
            na::Point3::new(0.0, 0.0, 0.0),
        ];
        let indices = vec![0, 1, 2];
        let backface_indices = vec![0, 2, 1];
        (vertices, indices, backface_indices, 1.0, MeshStats::default())
    };

    // Create shared state
    let state = Arc::new(Mutex::new(ViewerState::for_mesh(max_dimension, stats)));

    // Create command channel
    let (command_tx, command_rx): (Sender<ViewerCommand>, Receiver<ViewerCommand>) =
        channel::unbounded();

    // Spawn RPC server in background thread
    let state_clone = Arc::clone(&state);
    let _rpc_handle = spawn_rpc_server(state_clone, command_tx, 9001);

    // Create application
    let vsync = !no_vsync; // Convert flag: --no-vsync means vsync=false
    let mut app = RpcViewerApp::new(state, command_rx, vertices, indices, backface_indices, max_dimension, vsync);

    // Create and run event loop
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut app)?;

    Ok(())
}
