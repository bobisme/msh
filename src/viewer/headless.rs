use nalgebra as na;
use std::path::PathBuf;

use crate::mesh::loader::load_mesh_with_colors;

use super::{
    camera::ArcBallCamera,
    mesh_renderer::MeshRenderer,
    render::extract_render_data,
    state::ViewerState,
};

/// Render a mesh to a PNG file without opening a window
#[allow(clippy::too_many_arguments)]
pub fn render_to_file(
    input: &PathBuf,
    output: &str,
    mesh_name: Option<&str>,
    width: u32,
    height: u32,
    z_up: bool,
    camera_pos_override: Option<(f32, f32, f32)>,
    camera_target_override: Option<(f32, f32, f32)>,
    configure_state: impl FnOnce(&mut ViewerState),
) -> Result<(), Box<dyn std::error::Error>> {
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
    let (vertices, indices, backface_indices, has_vertex_colors, max_dimension) =
        extract_render_data(&mesh_data);

    // Configure state
    let mut state = ViewerState::for_mesh(max_dimension, stats);
    configure_state(&mut state);

    // Create headless GPU device
    let (device, queue, texture_format) = pollster::block_on(create_headless_device())?;

    // Create a SurfaceConfiguration-like struct for MeshRenderer
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: texture_format,
        width,
        height,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };

    // Create mesh renderer and load mesh
    let mut mesh_renderer = MeshRenderer::new(&device, &config);
    mesh_renderer.load_mesh(&device, &queue, &vertices, &indices, &backface_indices, has_vertex_colors, mesh_data.texture.as_ref());

    // Set up camera
    let eye = if let Some((x, y, z)) = camera_pos_override {
        na::Point3::new(x, y, z)
    } else {
        let d = max_dimension * 2.5;
        na::Point3::new(d * 0.5, d * 0.3, d)
    };
    let target = if let Some((x, y, z)) = camera_target_override {
        na::Point3::new(x, y, z)
    } else {
        na::Point3::origin()
    };
    let camera = ArcBallCamera::new(eye, target, width, height);

    // Update uniforms
    let view_proj = camera.view_projection_matrix_for(&state.projection);
    let model = na::Matrix4::identity();
    mesh_renderer.update_uniforms(
        &queue,
        &view_proj,
        &model,
        &camera.position(),
        state.shading.as_u32(),
        state.base_color,
        state.light_direction,
    );

    // Create offscreen render target
    let render_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Offscreen Render Target"),
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

    // Render
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Headless Render Encoder"),
    });

    mesh_renderer.render(
        &mut encoder,
        &render_view,
        state.show_wireframe,
        state.show_backfaces,
        state.clear_color,
    );

    // Read back texture to buffer
    let bytes_per_pixel = 4u32;
    let unpadded_bytes_per_row = width * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
    let buffer_size = (padded_bytes_per_row * height) as u64;

    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Readback Buffer"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

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

    // Map buffer and read pixels
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

    // Create parent directories if needed
    if let Some(parent) = std::path::Path::new(output).parent()
        && !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }

    // Save PNG
    image::save_buffer(
        output,
        &img_data,
        width,
        height,
        image::ColorType::Rgba8,
    )?;

    Ok(())
}

/// Create a headless wgpu device (no window surface)
async fn create_headless_device() -> Result<(wgpu::Device, wgpu::Queue, wgpu::TextureFormat), Box<dyn std::error::Error>> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..Default::default()
    });

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await?;

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("Headless Device"),
            required_features: wgpu::Features::POLYGON_MODE_LINE,
            required_limits: wgpu::Limits::default(),
            memory_hints: Default::default(),
            trace: Default::default(),
            experimental_features: Default::default(),
        })
        .await?;

    // Use Rgba8UnormSrgb for headless — standard sRGB output
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;

    Ok((device, queue, format))
}
