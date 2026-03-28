use nalgebra as na;
use std::path::PathBuf;

use crate::mesh::animation;
use crate::mesh::loader::load_mesh_with_colors;

use super::{
    camera::ArcBallCamera,
    mesh_renderer::MeshRenderer,
    render::extract_render_data,
    state::ViewerState,
};

/// Configuration for sprite sheet rendering.
pub struct SpriteSheetConfig {
    pub tile_width: u32,
    pub tile_height: u32,
    /// Yaw angles in radians.
    pub angles: Vec<f32>,
    /// Animation times for each frame.
    pub frames: Vec<f32>,
    pub animation_index: usize,
    pub transparent_bg: bool,
    /// Uniform scale multiplier (default 1.0).
    pub model_scale: f32,
}

/// Render a sprite sheet atlas: rows = angles, cols = frames.
/// Returns the composited RGBA image data and (width, height).
pub fn render_sprite_sheet(
    input: &PathBuf,
    config: &SpriteSheetConfig,
    mesh_name: Option<&str>,
    z_up: bool,
    configure_state: impl FnOnce(&mut ViewerState),
) -> Result<(Vec<u8>, u32, u32), Box<dyn std::error::Error>> {
    let tiles = render_all_tiles(input, config, mesh_name, z_up, configure_state)?;

    let num_cols = config.frames.len().max(1) as u32;
    let num_rows = config.angles.len().max(1) as u32;
    let atlas_width = config.tile_width * num_cols;
    let atlas_height = config.tile_height * num_rows;

    let mut atlas = vec![0u8; (atlas_width * atlas_height * 4) as usize];

    for (idx, tile_data) in tiles.iter().enumerate() {
        let row = (idx as u32) / num_cols;
        let col = (idx as u32) % num_cols;
        let x_offset = col * config.tile_width;
        let y_offset = row * config.tile_height;

        for ty in 0..config.tile_height {
            let src_start = (ty * config.tile_width * 4) as usize;
            let src_end = src_start + (config.tile_width * 4) as usize;
            let dst_y = y_offset + ty;
            let dst_start = ((dst_y * atlas_width + x_offset) * 4) as usize;
            let dst_end = dst_start + (config.tile_width * 4) as usize;
            atlas[dst_start..dst_end].copy_from_slice(&tile_data[src_start..src_end]);
        }
    }

    Ok((atlas, atlas_width, atlas_height))
}

/// Render individual frame files: output_dir/frame_AAAA_FFFF.png
pub fn render_frames(
    input: &PathBuf,
    output_dir: &str,
    config: &SpriteSheetConfig,
    mesh_name: Option<&str>,
    z_up: bool,
    configure_state: impl FnOnce(&mut ViewerState),
) -> Result<(), Box<dyn std::error::Error>> {
    let tiles = render_all_tiles(input, config, mesh_name, z_up, configure_state)?;

    std::fs::create_dir_all(output_dir)?;

    let num_cols = config.frames.len().max(1);
    for (idx, tile_data) in tiles.iter().enumerate() {
        let row = idx / num_cols;
        let col = idx % num_cols;
        let filename = format!("{}/frame_{:04}_{:04}.png", output_dir, row, col);
        image::save_buffer(
            &filename,
            tile_data,
            config.tile_width,
            config.tile_height,
            image::ColorType::Rgba8,
        )?;
    }

    Ok(())
}

/// Render all (angle x frame) tiles and return their RGBA pixel buffers.
/// Order: row-major, i.e. tiles[angle_idx * num_frames + frame_idx].
fn render_all_tiles(
    input: &PathBuf,
    config: &SpriteSheetConfig,
    mesh_name: Option<&str>,
    z_up: bool,
    configure_state: impl FnOnce(&mut ViewerState),
) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
    let width = config.tile_width;
    let height = config.tile_height;

    // Load mesh
    let mut mesh_data = load_mesh_with_colors(input, mesh_name)?;
    if z_up {
        mesh_data.convert_z_up_to_y_up();
    }

    // Build CornerTableF for stats
    let mesh = mesh_data.to_corner_table()?;
    let stats = super::state::MeshStats {
        vertex_count: mesh.count_vertices(),
        edge_count: mesh.unique_edges().count(),
        face_count: mesh.count_faces(),
        is_manifold: mesh.boundary_rings().is_empty(),
        hole_count: mesh.boundary_rings().len(),
    };

    // Extract render data
    let (vertices, indices, backface_indices, has_vertex_colors, max_dimension, skeleton_data) =
        extract_render_data(&mesh_data, false);

    // Configure state
    let mut state = ViewerState::for_mesh(max_dimension, stats);
    if config.transparent_bg {
        state.clear_color = [0.0, 0.0, 0.0, 0.0];
    }
    configure_state(&mut state);

    // Create headless GPU device
    let (device, queue, texture_format) =
        pollster::block_on(super::headless::create_headless_device())?;

    // Create surface config for MeshRenderer
    let surface_config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: texture_format,
        width,
        height,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };

    // Create mesh renderer and load mesh data
    let mut mesh_renderer = MeshRenderer::new(&device, &surface_config);
    mesh_renderer.load_mesh(
        &device,
        &queue,
        &vertices,
        &indices,
        &backface_indices,
        has_vertex_colors,
        mesh_data.texture.as_ref(),
    );

    // Set up initial joint palette if skeleton is present
    if let Some(ref skel_data) = skeleton_data {
        mesh_renderer.update_joint_palette(&queue, &skel_data.joint_matrices);
        mesh_renderer.set_joint_count(skel_data.joint_matrices.len() as u32);
    }

    // Set up camera (auto-position based on scaled mesh size)
    let d = max_dimension * config.model_scale * 2.5;
    let eye = na::Point3::new(d * 0.5, d * 0.3, d);
    let target = na::Point3::origin();
    let camera = ArcBallCamera::new(eye, target, width, height);
    let view_proj = camera.view_projection_matrix_for(&state.projection);

    // Create offscreen render target
    let render_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Sprite Sheet Render Target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: texture_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let render_view = render_texture.create_view(&wgpu::TextureViewDescriptor::default());

    // Prepare readback buffer
    let bytes_per_pixel = 4u32;
    let unpadded_bytes_per_row = width * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
    let buffer_size = (padded_bytes_per_row * height) as u64;

    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Sprite Sheet Readback Buffer"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    // Determine angles and frames
    let angles = if config.angles.is_empty() {
        vec![0.0f32]
    } else {
        config.angles.clone()
    };
    let frames = if config.frames.is_empty() {
        vec![0.0f32]
    } else {
        config.frames.clone()
    };

    let mut tiles = Vec::with_capacity(angles.len() * frames.len());

    for &angle in &angles {
        for &frame_time in &frames {
            // Compute model matrix: uniform scale * Y-axis rotation for this angle
            let model = na::Matrix4::from_axis_angle(&na::Vector3::y_axis(), angle)
                * na::Matrix4::new_scaling(config.model_scale);

            // Evaluate animation at this frame time
            if let Some(ref skeleton) = mesh_data.skeleton {
                if config.animation_index < mesh_data.animations.len() {
                    let clip = &mesh_data.animations[config.animation_index];
                    let local_transforms =
                        animation::evaluate_animation(clip, skeleton, frame_time);
                    let joint_matrices =
                        skeleton.compute_joint_matrices_with_pose(&local_transforms);
                    mesh_renderer.update_joint_palette(&queue, &joint_matrices);
                    mesh_renderer.set_joint_count(joint_matrices.len() as u32);
                }
            }

            // Update uniforms with this angle's model matrix
            mesh_renderer.update_uniforms(
                &queue,
                &view_proj,
                &model,
                &camera.position(),
                state.shading.as_u32(),
                state.base_color,
                state.light_direction,
            );

            // Render
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Sprite Sheet Render Encoder"),
                });

            mesh_renderer.render(
                &mut encoder,
                &render_view,
                state.show_wireframe,
                state.show_backfaces,
                state.clear_color,
            );

            // Copy render target to readback buffer
            encoder.copy_texture_to_buffer(
                render_texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &output_buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_bytes_per_row),
                        rows_per_image: Some(height),
                    },
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );

            queue.submit(Some(encoder.finish()));

            // Map and read back pixels
            let buffer_slice = output_buffer.slice(..);
            let (sender, receiver) = std::sync::mpsc::channel();
            buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap();
            });
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                })
                .unwrap();
            receiver.recv().unwrap()?;

            let data = buffer_slice.get_mapped_range();

            // Strip row padding
            let mut img_data = Vec::with_capacity((width * height * 4) as usize);
            for row in 0..height {
                let start = (row * padded_bytes_per_row) as usize;
                let end = start + (width * bytes_per_pixel) as usize;
                img_data.extend_from_slice(&data[start..end]);
            }
            drop(data);
            output_buffer.unmap();

            tiles.push(img_data);
        }
    }

    Ok(tiles)
}
