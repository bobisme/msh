use wgpu;
use wgpu_text::{
    BrushBuilder, TextBrush,
    glyph_brush::{ab_glyph::FontArc, Section, Text},
};

use super::state::ViewerState;

/// UI renderer for text overlays
pub struct UiRenderer {
    brush: TextBrush<FontArc>,
}

impl UiRenderer {
    pub fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        config: &wgpu::SurfaceConfiguration,
    ) -> Self {
        // Load a system font - try multiple common locations
        let font_data = std::fs::read("/usr/share/fonts/TTF/DejaVuSans.ttf")
            .or_else(|_| std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"))
            .or_else(|_| std::fs::read("/usr/share/fonts/noto/NotoSans-Regular.ttf"))
            .expect("Failed to load system font - please install DejaVu or Noto fonts");

        let font = FontArc::try_from_vec(font_data)
            .expect("Failed to parse font");

        let brush = BrushBuilder::using_fonts(vec![font])
            .build(device, config.width, config.height, config.format);

        Self { brush }
    }

    /// Resize the UI renderer
    pub fn resize(&mut self, _device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32) {
        self.brush.resize_view(width as f32, height as f32, queue);
    }

    /// Queue text for rendering
    pub fn queue_text(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, state: &ViewerState, rpc_active: bool) {
        let x_offset = 11.0;
        let y_offset = 15.0;
        let line_height = 18.0;
        let header_size = 26.0;
        let text_size = 18.0;
        let header_padding = 8.0;

        let mut sections = Vec::new();
        let mut current_y = y_offset;

        // Controls section
        sections.push(Section {
            screen_position: (x_offset, current_y),
            text: vec![Text::new("Controls")
                .with_scale(header_size)
                .with_color([0.8, 0.8, 0.8, 1.0])],
            ..Default::default()
        });
        current_y += line_height + header_padding;

        let controls = vec![
            "Left Click+Drag: Rotate",
            "Right Click+Drag: Pan",
            "Scroll: Zoom",
            "W: Toggle Wireframe",
            "B: Toggle Backfaces",
            "U: Toggle UI",
            "Q/ESC: Exit",
        ];

        for control in controls {
            sections.push(Section {
                screen_position: (x_offset, current_y),
                text: vec![Text::new(control)
                    .with_scale(text_size)
                    .with_color([0.9, 0.9, 0.9, 1.0])],
                ..Default::default()
            });
            current_y += line_height;
        }

        // Mesh Info section
        current_y += line_height;
        sections.push(Section {
            screen_position: (x_offset, current_y),
            text: vec![Text::new("Mesh Info")
                .with_scale(header_size)
                .with_color([0.8, 0.8, 0.8, 1.0])],
            ..Default::default()
        });
        current_y += line_height + header_padding;

        // Create owned strings for dynamic content
        let vertices_text = format!("Vertices: {}", state.stats.vertex_count);
        let edges_text = format!("Edges: {}", state.stats.edge_count);
        let faces_text = format!("Faces: {}", state.stats.face_count);
        let manifold_text = if state.stats.is_manifold {
            "Manifold: Yes".to_string()
        } else {
            format!("Manifold: No ({} holes)", state.stats.hole_count)
        };

        sections.push(Section {
            screen_position: (x_offset, current_y),
            text: vec![Text::new(&vertices_text)
                .with_scale(text_size)
                .with_color([0.9, 0.9, 0.9, 1.0])],
            ..Default::default()
        });
        current_y += line_height;

        sections.push(Section {
            screen_position: (x_offset, current_y),
            text: vec![Text::new(&edges_text)
                .with_scale(text_size)
                .with_color([0.9, 0.9, 0.9, 1.0])],
            ..Default::default()
        });
        current_y += line_height;

        sections.push(Section {
            screen_position: (x_offset, current_y),
            text: vec![Text::new(&faces_text)
                .with_scale(text_size)
                .with_color([0.9, 0.9, 0.9, 1.0])],
            ..Default::default()
        });
        current_y += line_height;

        // Manifold status with color
        let manifold_color = if state.stats.is_manifold {
            [0.4, 1.0, 0.4, 1.0] // Green
        } else {
            [1.0, 0.4, 0.4, 1.0] // Red
        };

        sections.push(Section {
            screen_position: (x_offset, current_y),
            text: vec![Text::new(&manifold_text)
                .with_scale(text_size)
                .with_color(manifold_color)],
            ..Default::default()
        });

        // RPC status if active
        if rpc_active {
            current_y += line_height * 2.0;
            sections.push(Section {
                screen_position: (x_offset, current_y),
                text: vec![Text::new("RPC Server")
                    .with_scale(header_size)
                    .with_color([0.8, 0.8, 0.8, 1.0])],
                ..Default::default()
            });
            current_y += line_height + header_padding;

            sections.push(Section {
                screen_position: (x_offset, current_y),
                text: vec![Text::new("Active: 127.0.0.1:9001")
                    .with_scale(text_size)
                    .with_color([0.4, 1.0, 0.4, 1.0])],
                ..Default::default()
            });
        }

        // Queue all sections
        for section in sections {
            self.brush.queue(device, queue, vec![section]).unwrap();
        }
    }

    /// Render the UI
    pub fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("UI Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load, // Load existing content
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        self.brush.draw(&mut render_pass);
    }
}
