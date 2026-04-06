use nalgebra as na;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winit::{
    application::ApplicationHandler,
    event::*,
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use crate::mesh::animation::{self, AnimationClip};
use crate::mesh::loader::load_mesh_with_colors;
use crate::mesh::skeleton::Skeleton;

use super::{
    camera::ArcBallCamera,
    gpu::GpuState,
    mesh_renderer::{MeshRenderer, Vertex},
    state::{MeshStats, ViewerState},
    ui_renderer::UiRenderer,
};

/// Animation playback state
struct AnimationState {
    /// All animation clips loaded from the mesh
    clips: Vec<AnimationClip>,
    /// Index of the currently active clip
    current_clip: usize,
    /// Current playback time in seconds
    time: f32,
    /// Whether the animation is playing
    playing: bool,
    /// Playback speed multiplier
    speed: f32,
    /// The skeleton used to compute joint matrices
    skeleton: Skeleton,
    /// Last frame instant for delta-time computation
    last_frame: Instant,
}

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
    skeleton_data: Option<SkeletonRenderData>,
    max_dimension: f32,
    mouse_pressed_left: bool,
    mouse_pressed_right: bool,
    last_mouse_pos: Option<winit::dpi::PhysicalPosition<f64>>,
    vsync: bool,
    /// Animation playback state (present when skeleton + animations exist)
    animation: Option<AnimationState>,
    /// Uniform model scale multiplier
    model_scale: f32,
    /// For N/P mesh switching: source path and all mesh names in the file
    input_path: PathBuf,
    mesh_names: Vec<String>,
    current_mesh_idx: usize,
    z_up: bool,
    no_center: bool,
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
        skeleton_data: Option<SkeletonRenderData>,
        max_dimension: f32,
        vsync: bool,
        animation: Option<AnimationState>,
        model_scale: f32,
        input_path: PathBuf,
        mesh_names: Vec<String>,
        current_mesh_idx: usize,
        z_up: bool,
        no_center: bool,
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
            skeleton_data,
            max_dimension,
            mouse_pressed_left: false,
            mouse_pressed_right: false,
            last_mouse_pos: None,
            vsync,
            animation,
            model_scale,
            input_path,
            mesh_names,
            current_mesh_idx,
            z_up,
            no_center,
        }
    }

    /// Load the mesh at `new_idx` and rebuild all render data.
    /// Used by N/P key handlers to switch meshes at runtime.
    fn switch_to_mesh(&mut self, new_idx: usize) {
        if new_idx >= self.mesh_names.len() || new_idx == self.current_mesh_idx {
            return;
        }

        let name = self.mesh_names[new_idx].clone();
        let mut mesh_data = match load_mesh_with_colors(&self.input_path, Some(&name)) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Failed to load mesh '{}': {}", name, e);
                return;
            }
        };

        if self.z_up {
            mesh_data.convert_z_up_to_y_up();
        }

        // Rebuild stats from CornerTable
        let (vertex_count, edge_count, face_count, is_manifold, hole_count) =
            match mesh_data.to_corner_table() {
                Ok(m) => {
                    let boundary_rings = m.boundary_rings();
                    (
                        m.count_vertices(),
                        m.unique_edges().count(),
                        m.count_faces(),
                        boundary_rings.is_empty(),
                        boundary_rings.len(),
                    )
                }
                Err(_) => (0, 0, 0, false, 0),
            };

        self.state.stats = MeshStats {
            vertex_count,
            edge_count,
            face_count,
            is_manifold,
            hole_count,
        };

        // Extract render data
        let (vertices, indices, backface_indices, has_vertex_colors, _max_dim, skeleton_data) =
            extract_render_data(&mesh_data, self.no_center);

        // Rebuild GPU buffers
        if let (Some(gpu), Some(mesh_renderer)) = (self.gpu.as_ref(), self.mesh_renderer.as_mut()) {
            mesh_renderer.load_mesh(
                &gpu.device,
                &gpu.queue,
                &vertices,
                &indices,
                &backface_indices,
                has_vertex_colors,
                mesh_data.texture.as_ref(),
            );

            // Update joint palette if skeleton is present
            if let Some(ref skel_data) = skeleton_data {
                mesh_renderer.update_joint_palette(&gpu.queue, &skel_data.joint_matrices);
                mesh_renderer.set_joint_count(skel_data.joint_matrices.len() as u32);
            } else {
                mesh_renderer.set_joint_count(0);
            }
        }

        self.vertices = vertices;
        self.indices = indices;
        self.backface_indices = backface_indices;
        self.has_vertex_colors = has_vertex_colors;
        self.texture = mesh_data.texture.take();
        self.skeleton_data = skeleton_data;
        self.current_mesh_idx = new_idx;

        // Rebuild animation state from this mesh's skeleton + clips
        if !mesh_data.animations.is_empty() && mesh_data.skeleton.is_some() {
            let skeleton = mesh_data.skeleton.take().unwrap();
            let clips = std::mem::take(&mut mesh_data.animations);
            let playing = !clips.is_empty();
            self.animation = Some(AnimationState {
                clips,
                current_clip: 0,
                time: 0.0,
                playing,
                speed: 1.0,
                skeleton,
                last_frame: Instant::now(),
            });
        } else {
            self.animation = None;
        }

        println!(
            "Mesh {}/{}: {}",
            self.current_mesh_idx + 1,
            self.mesh_names.len(),
            self.mesh_names[self.current_mesh_idx]
        );

        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
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
            let camera_distance = self.max_dimension * self.model_scale * 2.5;
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

            // Set up joint palette if skeleton is present
            if let Some(ref skel_data) = self.skeleton_data {
                mesh_renderer.update_joint_palette(&gpu.queue, &skel_data.joint_matrices);
                mesh_renderer.set_joint_count(skel_data.joint_matrices.len() as u32);
            }

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
            if self.animation.is_some() {
                println!("  Space: Play/Pause animation");
                println!("  [/]: Previous/Next animation clip");
            }
            if self.mesh_names.len() > 1 {
                println!("  N/P: Next/Previous mesh");
            }
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
                            KeyCode::Space => {
                                if let Some(ref mut anim) = self.animation {
                                    anim.playing = !anim.playing;
                                    if anim.playing {
                                        // Reset the last_frame so we don't get a huge dt
                                        anim.last_frame = Instant::now();
                                    }
                                    println!(
                                        "Animation: {}",
                                        if anim.playing { "PLAYING" } else { "PAUSED" }
                                    );
                                    if let Some(window) = self.window.as_ref() {
                                        window.request_redraw();
                                    }
                                }
                            }
                            KeyCode::BracketLeft => {
                                if let Some(ref mut anim) = self.animation {
                                    if !anim.clips.is_empty() {
                                        if anim.current_clip == 0 {
                                            anim.current_clip = anim.clips.len() - 1;
                                        } else {
                                            anim.current_clip -= 1;
                                        }
                                        anim.time = 0.0;
                                        let clip = &anim.clips[anim.current_clip];
                                        println!(
                                            "Animation clip {}/{}: {}",
                                            anim.current_clip + 1,
                                            anim.clips.len(),
                                            clip.name.as_deref().unwrap_or("<unnamed>")
                                        );
                                        if let Some(window) = self.window.as_ref() {
                                            window.request_redraw();
                                        }
                                    }
                                }
                            }
                            KeyCode::BracketRight => {
                                if let Some(ref mut anim) = self.animation {
                                    if !anim.clips.is_empty() {
                                        anim.current_clip =
                                            (anim.current_clip + 1) % anim.clips.len();
                                        anim.time = 0.0;
                                        let clip = &anim.clips[anim.current_clip];
                                        println!(
                                            "Animation clip {}/{}: {}",
                                            anim.current_clip + 1,
                                            anim.clips.len(),
                                            clip.name.as_deref().unwrap_or("<unnamed>")
                                        );
                                        if let Some(window) = self.window.as_ref() {
                                            window.request_redraw();
                                        }
                                    }
                                }
                            }
                            KeyCode::KeyN => {
                                if self.mesh_names.len() > 1 {
                                    let next = (self.current_mesh_idx + 1) % self.mesh_names.len();
                                    self.switch_to_mesh(next);
                                }
                            }
                            KeyCode::KeyP => {
                                if self.mesh_names.len() > 1 {
                                    let prev = if self.current_mesh_idx == 0 {
                                        self.mesh_names.len() - 1
                                    } else {
                                        self.current_mesh_idx - 1
                                    };
                                    self.switch_to_mesh(prev);
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
                // Advance animation and update joint palette
                if let Some(ref mut anim) = self.animation {
                    let now = Instant::now();
                    if anim.playing {
                        let dt = now.duration_since(anim.last_frame).as_secs_f32();
                        anim.time += dt * anim.speed;
                        let clip = &anim.clips[anim.current_clip];
                        if clip.duration > 0.0 {
                            anim.time %= clip.duration;
                        }
                    }
                    anim.last_frame = now;

                    // Evaluate animation at the current time
                    let clip = &anim.clips[anim.current_clip];
                    let local_transforms =
                        animation::evaluate_animation(clip, &anim.skeleton, anim.time);
                    let joint_matrices =
                        anim.skeleton.compute_joint_matrices_with_pose(&local_transforms);

                    if let (Some(gpu), Some(mesh_renderer)) =
                        (self.gpu.as_ref(), self.mesh_renderer.as_ref())
                    {
                        mesh_renderer.update_joint_palette(&gpu.queue, &joint_matrices);
                    }
                }

                if let (Some(window), Some(gpu), Some(camera), Some(mesh_renderer), Some(ui_renderer)) = (
                    self.window.as_ref(),
                    self.gpu.as_mut(),
                    self.camera.as_ref(),
                    self.mesh_renderer.as_mut(),
                    self.ui_renderer.as_mut(),
                ) {
                    // Update uniforms
                    let view_proj = camera.view_projection_matrix_for(&self.state.projection);
                    let model = na::Matrix4::new_scaling(self.model_scale);
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
                        let anim_info = self.animation.as_ref().map(|anim| {
                            let clip = &anim.clips[anim.current_clip];
                            super::ui_renderer::AnimationInfo {
                                clip_name: clip.name.clone().unwrap_or_else(|| "<unnamed>".into()),
                                clip_index: anim.current_clip,
                                clip_count: anim.clips.len(),
                                time: anim.time,
                                duration: clip.duration,
                                playing: anim.playing,
                            }
                        });
                        let mesh_info = if self.mesh_names.len() > 1 {
                            Some(super::ui_renderer::MeshSelectionInfo {
                                name: self.mesh_names[self.current_mesh_idx].clone(),
                                index: self.current_mesh_idx,
                                count: self.mesh_names.len(),
                            })
                        } else {
                            None
                        };
                        ui_renderer.queue_text(&gpu.device, &gpu.queue, &self.state, false, anim_info.as_ref(), mesh_info.as_ref());
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

/// Joint matrices computed from skeleton bind pose (if present)
pub struct SkeletonRenderData {
    /// Joint matrices for GPU skinning (world * inverse_bind per joint)
    pub joint_matrices: Vec<[[f32; 4]; 4]>,
}

/// Extract rendering data from MeshWithColors
pub fn extract_render_data(
    mesh_data: &crate::mesh::loader::MeshWithColors,
    no_center: bool,
) -> (Vec<Vertex>, Vec<u32>, Vec<u32>, bool, f32, Option<SkeletonRenderData>) {
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
    let center = if no_center {
        [0.0, 0.0, 0.0]
    } else {
        [
            (min[0] + max[0]) / 2.0,
            (min[1] + max[1]) / 2.0,
            (min[2] + max[2]) / 2.0,
        ]
    };
    let size = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    let max_dimension = size[0].max(size[1]).max(size[2]);

    // Build triangle soup with per-vertex colors, UVs, and joint data
    let has_joints = !mesh_data.joint_indices.is_empty() && !mesh_data.joint_weights.is_empty();
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
            let (ji, jw) = if has_joints {
                let src_ji = mesh_data.joint_indices[vi as usize];
                let src_jw = mesh_data.joint_weights[vi as usize];
                ([src_ji[0] as u32, src_ji[1] as u32, src_ji[2] as u32, src_ji[3] as u32], src_jw)
            } else {
                ([0u32; 4], [0.0f32; 4])
            };
            vertices.push(Vertex {
                position: [pos[0] - center[0], pos[1] - center[1], pos[2] - center[2]],
                color,
                texcoord: uv,
                joint_indices: ji,
                joint_weights: jw,
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

    // Compute bind-pose joint matrices if skeleton is present
    let skeleton_data = mesh_data.skeleton.as_ref().map(|skeleton| {
        SkeletonRenderData {
            joint_matrices: skeleton.compute_joint_matrices(),
        }
    });

    (vertices, indices, backface_indices, has_vertex_colors, max_dimension, skeleton_data)
}

pub fn view_mesh_with_bvh(
    input: &PathBuf,
    mesh_name: Option<&str>,
    no_vsync: bool,
    z_up: bool,
    bvh_path: Option<&PathBuf>,
    initial_animation: Option<&str>,
    model_scale: Option<f32>,
    no_center: bool,
    configure_state: impl FnOnce(&mut ViewerState),
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading mesh from {:?}...", input);

    // If the input is a GLB, enumerate its meshes so we can switch between them.
    let is_glb = input
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case("glb") || s.eq_ignore_ascii_case("gltf"))
        .unwrap_or(false);

    let mesh_names: Vec<String> = if is_glb {
        crate::mesh::loader::list_glb_meshes(input).unwrap_or_default()
    } else {
        Vec::new()
    };

    // Resolve which mesh to load initially.
    let (resolved_mesh_name, current_mesh_idx) = if mesh_names.is_empty() {
        (mesh_name.map(String::from), 0)
    } else if let Some(name) = mesh_name {
        let idx = mesh_names.iter().position(|n| n == name).unwrap_or(0);
        (Some(name.to_string()), idx)
    } else {
        // No --mesh given: default to first, and if there are multiple, print the list.
        if mesh_names.len() > 1 {
            println!("GLB contains {} meshes (use N/P to switch):", mesh_names.len());
            for (i, name) in mesh_names.iter().enumerate() {
                println!("  [{}] {}", i, name);
            }
        }
        (Some(mesh_names[0].clone()), 0)
    };

    // Load mesh with color data
    let mut mesh_data = load_mesh_with_colors(input, resolved_mesh_name.as_deref())?;

    // If a BVH file was provided, parse it and map onto the skeleton
    if let Some(bvh_file) = bvh_path {
        if let Some(ref skeleton) = mesh_data.skeleton {
            let bvh_contents = std::fs::read_to_string(bvh_file)?;
            let bvh_clip = crate::mesh::bvh::parse_bvh(&bvh_contents)
                .map_err(|e| format!("BVH parse error: {}", e))?;

            let real_joints = bvh_clip.joints.iter().filter(|j| !j.is_end_site).count();
            println!(
                "BVH clip: {} joints, {} frames, {:.1}s duration",
                real_joints,
                bvh_clip.frame_count,
                bvh_clip.duration()
            );

            let joint_mapping = crate::mesh::bvh_mapping::match_bvh_to_skeleton(&bvh_clip, skeleton)
                .map_err(|e| format!("BVH skeleton matching failed: {}", e))?;

            let anim_clip = crate::mesh::bvh_mapping::bvh_to_animation_clip(&bvh_clip, &joint_mapping);
            println!(
                "Converted BVH to animation clip: {} channels, {:.2}s duration",
                anim_clip.channels.len(),
                anim_clip.duration
            );
            mesh_data.animations.push(anim_clip);
        } else {
            return Err("Cannot apply BVH: the loaded mesh has no skeleton".into());
        }
    }

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
    let (vertices, indices, backface_indices, has_vertex_colors, max_dimension, skeleton_data) =
        extract_render_data(&mesh_data, no_center);

    println!(
        "Extracted {} vertices ({} triangles) as triangle soup{}",
        vertices.len(),
        indices.len() / 3,
        if has_vertex_colors { " with material colors" } else { "" },
    );

    // Create viewer state
    let mut state = ViewerState::for_mesh(max_dimension, stats);
    configure_state(&mut state);

    // Create animation state if skeleton and animations are present
    let animation = if !mesh_data.animations.is_empty() && mesh_data.skeleton.is_some() {
        let skeleton = mesh_data.skeleton.take().unwrap();
        let clips = std::mem::take(&mut mesh_data.animations);
        let playing = !clips.is_empty();

        // List available clips
        if clips.len() > 1 {
            println!("Available animation clips:");
            for (i, clip) in clips.iter().enumerate() {
                let name = clip.name.as_deref().unwrap_or("<unnamed>");
                println!("  [{}] {} ({:.2}s)", i, name, clip.duration);
            }
        } else if let Some(clip) = clips.first() {
            let name = clip.name.as_deref().unwrap_or("<unnamed>");
            println!("Animation: {} ({:.2}s)", name, clip.duration);
        }

        // Resolve initial clip selection
        let initial_clip = if let Some(sel) = initial_animation {
            if let Ok(idx) = sel.parse::<usize>() {
                if idx < clips.len() { idx } else {
                    eprintln!("Animation index {} out of range (0-{})", idx, clips.len() - 1);
                    0
                }
            } else {
                clips.iter().position(|c| c.name.as_deref() == Some(sel))
                    .unwrap_or_else(|| {
                        eprintln!("Animation '{}' not found, using first clip", sel);
                        0
                    })
            }
        } else {
            0
        };

        if initial_clip != 0 {
            let name = clips[initial_clip].name.as_deref().unwrap_or("<unnamed>");
            println!("Playing clip [{}]: {}", initial_clip, name);
        }

        Some(AnimationState {
            clips,
            current_clip: initial_clip,
            time: 0.0,
            playing,
            speed: 1.0,
            skeleton,
            last_frame: Instant::now(),
        })
    } else {
        None
    };

    // Create application
    let vsync = !no_vsync;
    let texture = mesh_data.texture;
    let mut app = ViewerApp::new(
        state,
        vertices,
        indices,
        backface_indices,
        has_vertex_colors,
        texture,
        skeleton_data,
        max_dimension,
        vsync,
        animation,
        model_scale.unwrap_or(1.0),
        input.clone(),
        mesh_names,
        current_mesh_idx,
        z_up,
        no_center,
    );

    // Create and run event loop
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut app)?;

    Ok(())
}
