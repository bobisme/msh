use nalgebra as na;
use std::path::PathBuf;
use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::*,
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use crate::mesh::loader::load_mesh_with_colors;

use super::{
    camera::ArcBallCamera,
    gpu::GpuState,
    mesh_renderer::{MeshRenderer, Vertex},
    state::{MeshStats, ViewerState},
    ui_renderer::UiRenderer,
};

/// Application state for the viewer
struct ViewerApp {
    window: Option<Arc<Window>>,
    gpu: Option<GpuState>,
    camera: Option<ArcBallCamera>,
    mesh_renderer: Option<MeshRenderer>,
    ui_renderer: Option<UiRenderer>,
    state: ViewerState,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    backface_indices: Vec<u32>,
    has_vertex_colors: bool,
    texture: Option<crate::mesh::loader::TextureData>,
    max_dimension: f32,
    mouse_pressed_left: bool,
    mouse_pressed_right: bool,
    last_mouse_pos: Option<winit::dpi::PhysicalPosition<f64>>,
    vsync: bool,
}

impl ViewerApp {
    #[allow(clippy::too_many_arguments)]
    fn new(
        state: ViewerState,
        vertices: Vec<Vertex>,
        indices: Vec<u32>,
        backface_indices: Vec<u32>,
        has_vertex_colors: bool,
        texture: Option<crate::mesh::loader::TextureData>,
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
            vertices,
            indices,
            backface_indices,
            has_vertex_colors,
            texture,
            max_dimension,
            mouse_pressed_left: false,
            mouse_pressed_right: false,
            last_mouse_pos: None,
            vsync,
        }
    }
}

impl ApplicationHandler for ViewerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Window creation and GPU initialization happens here in winit 0.30
        if self.window.is_none() {
            let window_attributes = Window::default_attributes()
                .with_title("Mesh Viewer - msh");
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
            mesh_renderer.load_mesh(&gpu.device, &gpu.queue, &self.vertices, &self.indices, &self.backface_indices, self.has_vertex_colors, self.texture.as_ref());

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
                if event.state == ElementState::Pressed
                    && let PhysicalKey::Code(keycode) = event.physical_key {
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
                    let view_proj = camera.view_projection_matrix_for(&self.state.projection);
                    let model = na::Matrix4::identity();
                    mesh_renderer.update_uniforms(
                        &gpu.queue,
                        &view_proj,
                        &model,
                        &camera.position(),
                        self.state.shading.as_u32(),
                        self.state.base_color,
                        self.state.light_direction,
                    );

                    // Queue UI text
                    if self.state.show_ui {
                        ui_renderer.queue_text(&gpu.device, &gpu.queue, &self.state, false);
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
                                self.state.show_wireframe,
                                self.state.show_backfaces,
                                self.state.clear_color,
                            );

                            // Render UI
                            if self.state.show_ui {
                                ui_renderer.render(&mut encoder, &view);
                            }

                            gpu.queue.submit(std::iter::once(encoder.finish()));
                            output.present();
                        }
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            // Reconfigure surface
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

/// Extract rendering data from MeshWithColors
pub fn extract_render_data(
    mesh_data: &crate::mesh::loader::MeshWithColors,
) -> (Vec<Vertex>, Vec<u32>, Vec<u32>, bool, f32) {
    let has_vertex_colors = !mesh_data.face_colors.is_empty();
    let has_uvs = !mesh_data.texcoords.is_empty();
    let default_color = [0.0f32; 4];

    // Calculate bounding box
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for pos in &mesh_data.positions {
        for i in 0..3 {
            min[i] = min[i].min(pos[i]);
            max[i] = max[i].max(pos[i]);
        }
    }
    let center = [
        (min[0] + max[0]) / 2.0,
        (min[1] + max[1]) / 2.0,
        (min[2] + max[2]) / 2.0,
    ];
    let size = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    let max_dimension = size[0].max(size[1]).max(size[2]);

    // Build triangle soup with per-vertex colors and UVs
    let mut vertices = Vec::with_capacity(mesh_data.face_indices.len() * 3);
    let mut indices = Vec::with_capacity(mesh_data.face_indices.len() * 3);
    let mut vertex_idx = 0u32;

    for (face_i, tri) in mesh_data.face_indices.iter().enumerate() {
        let color = if has_vertex_colors {
            mesh_data.face_colors[face_i]
        } else {
            default_color
        };

        for &vi in tri {
            let pos = mesh_data.positions[vi as usize];
            let uv = if has_uvs {
                mesh_data.texcoords[vi as usize]
            } else {
                [0.0, 0.0]
            };
            vertices.push(Vertex {
                position: [pos[0] - center[0], pos[1] - center[1], pos[2] - center[2]],
                color,
                texcoord: uv,
            });
            indices.push(vertex_idx);
            vertex_idx += 1;
        }
    }

    // Create backface indices (reversed winding)
    let mut backface_indices = Vec::with_capacity(indices.len());
    for i in (0..indices.len()).step_by(3) {
        backface_indices.push(indices[i]);
        backface_indices.push(indices[i + 2]);
        backface_indices.push(indices[i + 1]);
    }

    (vertices, indices, backface_indices, has_vertex_colors, max_dimension)
}

pub fn view_mesh(
    input: &PathBuf,
    mesh_name: Option<&str>,
    no_vsync: bool,
    z_up: bool,
    configure_state: impl FnOnce(&mut ViewerState),
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading mesh from {:?}...", input);

    // Load mesh with color data
    let mut mesh_data = load_mesh_with_colors(input, mesh_name)?;

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

    // Extract rendering data
    let (vertices, indices, backface_indices, has_vertex_colors, max_dimension) =
        extract_render_data(&mesh_data);

    println!(
        "Extracted {} vertices ({} triangles) as triangle soup{}",
        vertices.len(),
        indices.len() / 3,
        if has_vertex_colors { " with material colors" } else { "" },
    );

    // Create viewer state
    let mut state = ViewerState::for_mesh(max_dimension, stats);
    configure_state(&mut state);

    // Create application
    let vsync = !no_vsync;
    let texture = mesh_data.texture;
    let mut app = ViewerApp::new(state, vertices, indices, backface_indices, has_vertex_colors, texture, max_dimension, vsync);

    // Create and run event loop
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut app)?;

    Ok(())
}
