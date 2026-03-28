//! Standalone BVH skeleton viewer — renders BVH files as animated stick figures
//! without needing a mesh.

use nalgebra::{Matrix4, Quaternion, UnitQuaternion, Vector3};
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

use crate::mesh::bvh::BvhClip;
use crate::mesh::bvh_mapping::{euler_to_quat, extract_translation};

use super::{
    camera::ArcBallCamera,
    gpu::GpuState,
    mesh_renderer::{MeshRenderer, Vertex},
    state::{MeshStats, ViewerState},
    ui_renderer::UiRenderer,
};

// ---------------------------------------------------------------------------
// Forward kinematics
// ---------------------------------------------------------------------------

/// Compute world-space joint positions for a single BVH frame.
fn compute_bvh_world_positions(clip: &BvhClip, frame: usize) -> Vec<[f32; 3]> {
    let frame_data = &clip.frames[frame];
    let joint_count = clip.joints.len();
    let mut world_transforms: Vec<Matrix4<f32>> = vec![Matrix4::identity(); joint_count];
    let mut positions: Vec<[f32; 3]> = vec![[0.0; 3]; joint_count];

    for (i, joint) in clip.joints.iter().enumerate() {
        // Local offset as a translation matrix
        let offset = Matrix4::new_translation(&Vector3::new(
            joint.offset[0],
            joint.offset[1],
            joint.offset[2],
        ));

        // Local rotation + translation from channels
        let local_rotation;
        let local_translation;

        if joint.is_end_site || joint.channels.is_empty() {
            local_rotation = Matrix4::identity();
            local_translation = Matrix4::identity();
        } else {
            let ch_offset = clip.joint_channel_offset(i);
            let ch_count = joint.channels.len();
            let values = &frame_data[ch_offset..ch_offset + ch_count];

            // Rotation
            let quat = euler_to_quat(&joint.channels, values);
            let uq = UnitQuaternion::new_normalize(Quaternion::new(
                quat[3], quat[0], quat[1], quat[2],
            ));
            local_rotation = uq.to_homogeneous();

            // Translation (typically only root has position channels)
            if let Some(trans) = extract_translation(&joint.channels, values) {
                local_translation = Matrix4::new_translation(&Vector3::new(
                    trans[0], trans[1], trans[2],
                ));
            } else {
                local_translation = Matrix4::identity();
            }
        }

        // World transform: parent * offset * translation * rotation
        // For root (no parent): identity * offset * translation * rotation
        // BVH convention: the root's offset is its rest position, and the
        // position channels replace it. For child joints, only offset + rotation.
        let parent_transform = match joint.parent {
            Some(pi) => world_transforms[pi],
            None => Matrix4::identity(),
        };

        // For root joint with position channels, use position channels as translation
        // (they replace the offset conceptually, but BVH applies offset then channels)
        let world = parent_transform * local_translation * offset * local_rotation;
        world_transforms[i] = world;

        // Extract world position
        positions[i] = [world[(0, 3)], world[(1, 3)], world[(2, 3)]];
    }

    positions
}

/// Compute interpolated world-space joint positions at a given time.
fn compute_bvh_world_positions_interpolated(clip: &BvhClip, time: f32) -> Vec<[f32; 3]> {
    if clip.frame_count == 0 {
        return vec![[0.0; 3]; clip.joints.len()];
    }
    if clip.frame_count == 1 || clip.frame_time <= 0.0 {
        return compute_bvh_world_positions(clip, 0);
    }

    let duration = clip.duration();
    let t = if duration > 0.0 {
        time % duration
    } else {
        0.0
    };

    let frame_f = t / clip.frame_time;
    let frame0 = (frame_f as usize).min(clip.frame_count - 1);
    let frame1 = (frame0 + 1).min(clip.frame_count - 1);
    let frac = frame_f - frame0 as f32;

    if frame0 == frame1 || frac < 1e-6 {
        return compute_bvh_world_positions(clip, frame0);
    }

    let pos0 = compute_bvh_world_positions(clip, frame0);
    let pos1 = compute_bvh_world_positions(clip, frame1);

    // Lerp between the two frames
    pos0.iter()
        .zip(pos1.iter())
        .map(|(a, b)| {
            [
                a[0] + (b[0] - a[0]) * frac,
                a[1] + (b[1] - a[1]) * frac,
                a[2] + (b[2] - a[2]) * frac,
            ]
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Skeleton-to-mesh conversion (octahedron bones)
// ---------------------------------------------------------------------------

/// Generate vertex data for the skeleton as octahedron bones.
/// Each bone (parent->child edge) becomes an 8-triangle octahedron.
/// Joint positions at endpoints get small crosshair markers.
/// Bone width and marker size auto-scale to the skeleton's bounding box.
fn generate_bone_mesh(
    clip: &BvhClip,
    positions: &[[f32; 3]],
) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // Compute skeleton extent for auto-scaling bone width
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for p in positions {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    let extent = ((max[0] - min[0]).powi(2) + (max[1] - min[1]).powi(2) + (max[2] - min[2]).powi(2)).sqrt();
    let scale = (extent * 0.008).max(0.001); // bone width scale factor
    let marker_size = scale * 1.5;

    let bone_color = [0.0f32, 0.85, 0.9, 1.0]; // cyan
    let root_color = [1.0f32, 0.3, 0.3, 1.0]; // red for root
    let joint_color = [1.0f32, 1.0, 1.0, 1.0]; // white dots

    // For each joint that has a parent, draw a bone from parent to child
    for (i, joint) in clip.joints.iter().enumerate() {
        if let Some(pi) = joint.parent {
            let p = positions[pi]; // parent world pos
            let c = positions[i]; // child world pos

            let color = if pi == 0 { root_color } else { bone_color };
            add_octahedron_bone_scaled(&mut vertices, &mut indices, p, c, color, scale);
        }
    }

    // Add small crosshair markers at each joint position
    for (i, _joint) in clip.joints.iter().enumerate() {
        let pos = positions[i];
        let color = if i == 0 { root_color } else { joint_color };
        add_joint_marker(&mut vertices, &mut indices, pos, marker_size, color);
    }

    (vertices, indices)
}

/// Add an octahedron bone shape between two points.
/// `width_scale` controls the thickness of the bone relative to the skeleton size.
fn add_octahedron_bone_scaled(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    parent: [f32; 3],
    child: [f32; 3],
    color: [f32; 4],
    width_scale: f32,
) {
    let p = Vector3::new(parent[0], parent[1], parent[2]);
    let c = Vector3::new(child[0], child[1], child[2]);
    let bone_vec = c - p;
    let bone_len = bone_vec.norm();

    if bone_len < 1e-6 {
        return;
    }

    let dir = bone_vec / bone_len;

    // Find perpendicular vectors
    let up = if dir.y.abs() < 0.99 {
        Vector3::new(0.0, 1.0, 0.0)
    } else {
        Vector3::new(1.0, 0.0, 0.0)
    };
    let perp1 = dir.cross(&up).normalize();
    let perp2 = dir.cross(&perp1).normalize();

    // Midpoint (15% from parent), width from skeleton-scaled factor
    let width = width_scale;
    let mid = p + bone_vec * 0.15;

    // 4 vertices around the midpoint
    let m0 = mid + perp1 * width;
    let m1 = mid + perp2 * width;
    let m2 = mid - perp1 * width;
    let m3 = mid - perp2 * width;

    let base_idx = vertices.len() as u32;

    let mk_vertex = |pos: Vector3<f32>| -> Vertex {
        Vertex {
            position: [pos.x, pos.y, pos.z],
            color,
            texcoord: [0.0, 0.0],
            joint_indices: [0; 4],
            joint_weights: [0.0; 4],
        }
    };

    // 6 vertices: parent, 4 mid points, child
    vertices.push(mk_vertex(p));  // 0 = parent
    vertices.push(mk_vertex(m0)); // 1
    vertices.push(mk_vertex(m1)); // 2
    vertices.push(mk_vertex(m2)); // 3
    vertices.push(mk_vertex(m3)); // 4
    vertices.push(mk_vertex(c));  // 5 = child

    // 8 triangles (from parent to mid ring, from mid ring to child)
    // Parent -> mid ring
    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2]);
    indices.extend_from_slice(&[base_idx, base_idx + 2, base_idx + 3]);
    indices.extend_from_slice(&[base_idx, base_idx + 3, base_idx + 4]);
    indices.extend_from_slice(&[base_idx, base_idx + 4, base_idx + 1]);

    // Mid ring -> child
    indices.extend_from_slice(&[base_idx + 1, base_idx + 5, base_idx + 2]);
    indices.extend_from_slice(&[base_idx + 2, base_idx + 5, base_idx + 3]);
    indices.extend_from_slice(&[base_idx + 3, base_idx + 5, base_idx + 4]);
    indices.extend_from_slice(&[base_idx + 4, base_idx + 5, base_idx + 1]);
}

/// Add a small octahedron marker at a joint position.
fn add_joint_marker(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    pos: [f32; 3],
    size: f32,
    color: [f32; 4],
) {
    let base_idx = vertices.len() as u32;
    let p = Vector3::new(pos[0], pos[1], pos[2]);

    let mk_vertex = |v: Vector3<f32>| -> Vertex {
        Vertex {
            position: [v.x, v.y, v.z],
            color,
            texcoord: [0.0, 0.0],
            joint_indices: [0; 4],
            joint_weights: [0.0; 4],
        }
    };

    // 6 vertices of a small octahedron
    vertices.push(mk_vertex(p + Vector3::new(size, 0.0, 0.0)));  // 0 +X
    vertices.push(mk_vertex(p + Vector3::new(-size, 0.0, 0.0))); // 1 -X
    vertices.push(mk_vertex(p + Vector3::new(0.0, size, 0.0)));  // 2 +Y
    vertices.push(mk_vertex(p + Vector3::new(0.0, -size, 0.0))); // 3 -Y
    vertices.push(mk_vertex(p + Vector3::new(0.0, 0.0, size)));  // 4 +Z
    vertices.push(mk_vertex(p + Vector3::new(0.0, 0.0, -size))); // 5 -Z

    // 8 triangles
    indices.extend_from_slice(&[base_idx + 0, base_idx + 2, base_idx + 4]); // +X +Y +Z
    indices.extend_from_slice(&[base_idx + 0, base_idx + 4, base_idx + 3]); // +X +Z -Y
    indices.extend_from_slice(&[base_idx + 0, base_idx + 3, base_idx + 5]); // +X -Y -Z
    indices.extend_from_slice(&[base_idx + 0, base_idx + 5, base_idx + 2]); // +X -Z +Y
    indices.extend_from_slice(&[base_idx + 1, base_idx + 4, base_idx + 2]); // -X +Z +Y
    indices.extend_from_slice(&[base_idx + 1, base_idx + 3, base_idx + 4]); // -X -Y +Z
    indices.extend_from_slice(&[base_idx + 1, base_idx + 5, base_idx + 3]); // -X -Z -Y
    indices.extend_from_slice(&[base_idx + 1, base_idx + 2, base_idx + 5]); // -X +Y -Z
}

// ---------------------------------------------------------------------------
// BVH Viewer App
// ---------------------------------------------------------------------------

struct BvhViewerApp {
    window: Option<Arc<Window>>,
    gpu: Option<GpuState>,
    camera: Option<ArcBallCamera>,
    mesh_renderer: Option<MeshRenderer>,
    ui_renderer: Option<UiRenderer>,
    state: ViewerState,
    clip: BvhClip,
    // Animation state
    time: f32,
    playing: bool,
    last_frame: Instant,
    // Camera fitting data
    max_dimension: f32,
    center: [f32; 3],
    // Input
    mouse_pressed_left: bool,
    mouse_pressed_right: bool,
    last_mouse_pos: Option<winit::dpi::PhysicalPosition<f64>>,
    vsync: bool,
}

impl BvhViewerApp {
    fn new(
        state: ViewerState,
        clip: BvhClip,
        max_dimension: f32,
        center: [f32; 3],
        vsync: bool,
    ) -> Self {
        Self {
            window: None,
            gpu: None,
            camera: None,
            mesh_renderer: None,
            ui_renderer: None,
            state,
            clip,
            time: 0.0,
            playing: true,
            last_frame: Instant::now(),
            max_dimension,
            center,
            mouse_pressed_left: false,
            mouse_pressed_right: false,
            last_mouse_pos: None,
            vsync,
        }
    }

}

impl ApplicationHandler for BvhViewerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window_attributes = Window::default_attributes()
                .with_title("BVH Skeleton Viewer - msh");
            let window = Arc::new(event_loop.create_window(window_attributes).unwrap());

            let gpu = pollster::block_on(async {
                GpuState::new(window.clone(), self.vsync).await.unwrap()
            });

            // Camera setup
            let camera_distance = self.max_dimension * 2.5;
            let target = nalgebra::Point3::new(self.center[0], self.center[1], self.center[2]);
            let eye = nalgebra::Point3::new(
                target.x + camera_distance * 0.5,
                target.y + camera_distance * 0.3,
                target.z + camera_distance,
            );
            let size = window.inner_size();
            let camera = ArcBallCamera::new(eye, target, size.width, size.height);

            // Generate initial bone mesh
            let positions = compute_bvh_world_positions_interpolated(&self.clip, 0.0);
            let (vertices, idx) = generate_bone_mesh(&self.clip, &positions);

            // Create backface indices (reversed winding)
            let mut backface_indices = Vec::with_capacity(idx.len());
            for i in (0..idx.len()).step_by(3) {
                if i + 2 < idx.len() {
                    backface_indices.push(idx[i]);
                    backface_indices.push(idx[i + 2]);
                    backface_indices.push(idx[i + 1]);
                }
            }

            let mut mesh_renderer = MeshRenderer::new(&gpu.device, &gpu.config);
            mesh_renderer.load_mesh_dynamic(
                &gpu.device,
                &gpu.queue,
                &vertices,
                &idx,
                &backface_indices,
                true, // has_vertex_colors
                None, // no texture
            );

            let ui_renderer = UiRenderer::new(&gpu.device, &gpu.queue, &gpu.config);

            self.gpu = Some(gpu);
            self.camera = Some(camera);
            self.mesh_renderer = Some(mesh_renderer);
            self.ui_renderer = Some(ui_renderer);
            self.window = Some(window);
            self.last_frame = Instant::now();

            let real_joints = self.clip.joints.iter().filter(|j| !j.is_end_site).count();
            println!("Viewing BVH skeleton...");
            println!(
                "  {} joints, {} frames, {:.1}s duration",
                real_joints,
                self.clip.frame_count,
                self.clip.duration()
            );
            println!("  Mouse: Rotate (drag), Zoom (scroll), Pan (right-drag)");
            println!("  Space: Play/Pause animation");
            println!("  W: Toggle wireframe overlay");
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
                    && let PhysicalKey::Code(keycode) = event.physical_key
                {
                    match keycode {
                        KeyCode::KeyW => {
                            self.state.show_wireframe = !self.state.show_wireframe;
                        }
                        KeyCode::KeyU => {
                            self.state.show_ui = !self.state.show_ui;
                        }
                        KeyCode::Space => {
                            self.playing = !self.playing;
                            if self.playing {
                                self.last_frame = Instant::now();
                            }
                            println!(
                                "Animation: {}",
                                if self.playing { "PLAYING" } else { "PAUSED" }
                            );
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
                // Advance animation
                let now = Instant::now();
                if self.playing {
                    let dt = now.duration_since(self.last_frame).as_secs_f32();
                    self.time += dt;
                    let duration = self.clip.duration();
                    if duration > 0.0 {
                        self.time %= duration;
                    }
                }
                self.last_frame = now;

                // Recompute bone positions and regenerate mesh
                let positions =
                    compute_bvh_world_positions_interpolated(&self.clip, self.time);
                let (new_vertices, new_indices) =
                    generate_bone_mesh(&self.clip, &positions);

                // Also regenerate backface indices
                let mut new_backface_indices = Vec::with_capacity(new_indices.len());
                for i in (0..new_indices.len()).step_by(3) {
                    if i + 2 < new_indices.len() {
                        new_backface_indices.push(new_indices[i]);
                        new_backface_indices.push(new_indices[i + 2]);
                        new_backface_indices.push(new_indices[i + 1]);
                    }
                }

                if let (Some(gpu), Some(mesh_renderer)) =
                    (self.gpu.as_ref(), self.mesh_renderer.as_mut())
                {
                    mesh_renderer.update_dynamic_mesh(
                        &gpu.device,
                        &gpu.queue,
                        &new_vertices,
                        &new_indices,
                        &new_backface_indices,
                    );
                }

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
                    let view_proj = camera.view_projection_matrix_for(&self.state.projection);
                    let model = nalgebra::Matrix4::identity();
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
                        let anim_info = super::ui_renderer::AnimationInfo {
                            clip_name: "BVH Motion".to_string(),
                            clip_index: 0,
                            clip_count: 1,
                            time: self.time,
                            duration: self.clip.duration(),
                            playing: self.playing,
                        };
                        ui_renderer.queue_text(
                            &gpu.device,
                            &gpu.queue,
                            &self.state,
                            false,
                            Some(&anim_info),
                        );
                    }

                    let surface_texture = gpu.surface.get_current_texture();
                    match surface_texture {
                        Ok(output) => {
                            let view = output
                                .texture
                                .create_view(&wgpu::TextureViewDescriptor::default());

                            let mut encoder = gpu
                                .device
                                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                    label: Some("BVH Render Encoder"),
                                });

                            mesh_renderer.render(
                                &mut encoder,
                                &view,
                                self.state.show_wireframe,
                                false, // no backface viz for skeleton
                                self.state.clear_color,
                            );

                            if self.state.show_ui {
                                ui_renderer.render(&mut encoder, &view);
                            }

                            gpu.queue.submit(std::iter::once(encoder.finish()));
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

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Compute bounding box center and max dimension for a BVH clip (from first frame).
fn compute_bvh_bounds(clip: &BvhClip) -> ([f32; 3], f32) {
    let positions = if clip.frame_count > 0 {
        compute_bvh_world_positions(clip, 0)
    } else {
        vec![[0.0; 3]; clip.joints.len()]
    };

    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for pos in &positions {
        for i in 0..3 {
            min[i] = min[i].min(pos[i]);
            max[i] = max[i].max(pos[i]);
        }
    }

    // Also sample a few more frames to get better bounds
    let sample_frames = [
        clip.frame_count / 4,
        clip.frame_count / 2,
        clip.frame_count * 3 / 4,
    ];
    for &f in &sample_frames {
        if f > 0 && f < clip.frame_count {
            let frame_pos = compute_bvh_world_positions(clip, f);
            for pos in &frame_pos {
                for i in 0..3 {
                    min[i] = min[i].min(pos[i]);
                    max[i] = max[i].max(pos[i]);
                }
            }
        }
    }

    let center = [
        (min[0] + max[0]) / 2.0,
        (min[1] + max[1]) / 2.0,
        (min[2] + max[2]) / 2.0,
    ];
    let size = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    let max_dimension = size[0].max(size[1]).max(size[2]).max(1.0);

    (center, max_dimension)
}

/// Open an interactive viewer showing the BVH skeleton as animated stick figures.
pub fn view_bvh(
    path: &PathBuf,
    no_vsync: bool,
    configure_state: impl FnOnce(&mut ViewerState),
) -> Result<(), Box<dyn std::error::Error>> {
    let contents = std::fs::read_to_string(path)?;
    let clip = crate::mesh::bvh::parse_bvh(&contents)
        .map_err(|e| format!("BVH parse error: {}", e))?;

    let real_joints = clip.joints.iter().filter(|j| !j.is_end_site).count();
    println!(
        "BVH clip: {} joints, {} end sites, {} frames, {:.1}s duration",
        real_joints,
        clip.joints.len() - real_joints,
        clip.frame_count,
        clip.duration()
    );

    let (center, max_dimension) = compute_bvh_bounds(&clip);

    // Create viewer state with skeleton stats
    let stats = MeshStats {
        vertex_count: clip.joints.len(),
        edge_count: clip.joints.iter().filter(|j| j.parent.is_some()).count(),
        face_count: 0,
        is_manifold: false,
        hole_count: 0,
    };
    let mut state = ViewerState::for_mesh(max_dimension, stats);
    // Use unlit shading for the skeleton — we have vertex colors and don't need lighting artifacts
    state.shading = super::state::ShadingMode::Flat;
    state.clear_color = [0.08, 0.08, 0.12, 1.0]; // dark blue-gray background
    configure_state(&mut state);

    let vsync = !no_vsync;
    let mut app = BvhViewerApp::new(state, clip, max_dimension, center, vsync);

    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut app)?;

    Ok(())
}
