use nalgebra as na;
use std::path::PathBuf;
use winit::{
    application::ApplicationHandler,
    event::*,
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use crate::mesh::loader::load_mesh;

use super::{
    camera::ArcBallCamera,
    gpu::GpuState,
    mesh_renderer::MeshRenderer,
    state::{MeshStats, ViewerState},
    ui_renderer::UiRenderer,
};

/// Application state for the viewer
struct ViewerApp {
    window: Option<Window>,
    gpu: Option<GpuState<'static>>,
    camera: Option<ArcBallCamera>,
    mesh_renderer: Option<MeshRenderer>,
    ui_renderer: Option<UiRenderer>,
    state: ViewerState,
    vertices: Vec<na::Point3<f32>>,
    indices: Vec<u32>,
    backface_indices: Vec<u32>,
    max_dimension: f32,
    mouse_pressed_left: bool,
    mouse_pressed_right: bool,
    last_mouse_pos: Option<winit::dpi::PhysicalPosition<f64>>,
}

impl ViewerApp {
    fn new(
        state: ViewerState,
        vertices: Vec<na::Point3<f32>>,
        indices: Vec<u32>,
        backface_indices: Vec<u32>,
        max_dimension: f32,
    ) -> Self {
        Self {
            window: None,
            gpu: None,
            camera: None,
            mesh_renderer: None,
            ui_renderer: None,
            state,
            vertices,
            indices,
            backface_indices,
            max_dimension,
            mouse_pressed_left: false,
            mouse_pressed_right: false,
            last_mouse_pos: None,
        }
    }
}

impl ApplicationHandler for ViewerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Window creation and GPU initialization happens here in winit 0.30
        if self.window.is_none() {
            let window_attributes = Window::default_attributes()
                .with_title("Mesh Viewer - msh");
            let window = event_loop.create_window(window_attributes).unwrap();

            // Initialize GPU - use pollster to block on async init
            let size = window.inner_size();
            let gpu = pollster::block_on(async {
                // SAFETY: The window lives as long as ViewerApp, and we ensure
                // the surface (which borrows the window) is dropped before the window
                let window_ptr: &'static Window = unsafe {
                    std::mem::transmute(&window as &Window)
                };
                GpuState::new(window_ptr).await.unwrap()
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

            println!("Viewing mesh...");
            println!("  Mouse: Rotate (drag), Zoom (scroll), Pan (right-drag)");
            println!("  W: Toggle wireframe overlay");
            println!("  B: Toggle backface visualization (red)");
            println!("  U: Toggle UI overlay");
            println!("  Q/ESC: Exit");
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
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
                                self.state.show_wireframe = !self.state.show_wireframe;
                                println!(
                                    "Wireframe: {}",
                                    if self.state.show_wireframe { "ON" } else { "OFF" }
                                );
                            }
                            KeyCode::KeyB => {
                                self.state.show_backfaces = !self.state.show_backfaces;
                                println!(
                                    "Backface visualization: {}",
                                    if self.state.show_backfaces {
                                        "ON (red)"
                                    } else {
                                        "OFF"
                                    }
                                );
                            }
                            KeyCode::KeyU => {
                                self.state.show_ui = !self.state.show_ui;
                                println!("UI overlay: {}", if self.state.show_ui { "ON" } else { "OFF" });
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
                    // Update uniforms
                    let view_proj = camera.view_projection_matrix();
                    let model = na::Matrix4::identity();
                    mesh_renderer.update_uniforms(&gpu.queue, &view_proj, &model, &camera.position());

                    // Queue UI text
                    if self.state.show_ui {
                        ui_renderer.queue_text(&gpu.device, &gpu.queue, &self.state, false);
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
                                self.state.show_wireframe,
                                self.state.show_backfaces,
                            );

                            // Render UI
                            if self.state.show_ui {
                                ui_renderer.render(&mut encoder, &view);
                            }

                            gpu.queue.submit(std::iter::once(encoder.finish()));
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

pub fn view_mesh(
    input: &PathBuf,
    mesh_name: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading mesh from {:?}...", input);

    // Load mesh through baby_shark
    let mesh = load_mesh(input, mesh_name)?;

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

    // Calculate bounding box to center and scale the mesh
    let mut min = [f32::INFINITY, f32::INFINITY, f32::INFINITY];
    let mut max = [f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];

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

    println!(
        "Mesh bounds: ({:.3}, {:.3}, {:.3}) to ({:.3}, {:.3}, {:.3})",
        min[0], min[1], min[2], max[0], max[1], max[2]
    );
    println!(
        "Mesh center: ({:.3}, {:.3}, {:.3})",
        center[0], center[1], center[2]
    );
    println!("Mesh size: {:.3}", max_dimension);

    // Extract as triangle soup (no vertex sharing)
    let mut vertices: Vec<na::Point3<f32>> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let mut vertex_idx = 0u32;

    for face_id in mesh.faces() {
        let triangle = mesh.face_positions(face_id);

        // Get the three vertices of the triangle
        let v0 = triangle.p1();
        let v1 = triangle.p2();
        let v2 = triangle.p3();

        // Add vertices directly (centered)
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

        // Create triangle with sequential indices
        indices.push(vertex_idx);
        indices.push(vertex_idx + 1);
        indices.push(vertex_idx + 2);
        vertex_idx += 3;
    }

    println!(
        "Extracted {} vertices ({} triangles) as triangle soup",
        vertices.len(),
        indices.len() / 3
    );

    // Create reversed indices for backface visualization
    let mut backface_indices: Vec<u32> = Vec::new();
    for i in (0..indices.len()).step_by(3) {
        // Reverse winding order: (v0, v1, v2) -> (v0, v2, v1)
        backface_indices.push(indices[i]);
        backface_indices.push(indices[i + 2]);
        backface_indices.push(indices[i + 1]);
    }

    // Create viewer state
    let state = ViewerState::for_mesh(max_dimension, stats);

    // Create application
    let mut app = ViewerApp::new(state, vertices, indices, backface_indices, max_dimension);

    // Create and run event loop
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut app)?;

    Ok(())
}
