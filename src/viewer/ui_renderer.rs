use wgpu;
use wgpu_text::{
    BrushBuilder, TextBrush,
    glyph_brush::{ab_glyph::FontArc, Section, Text},
};
use font_kit::family_name::FamilyName;
use font_kit::properties::Properties;
use font_kit::source::SystemSource;

use super::state::ViewerState;

/// Animation info to display in the UI overlay
pub struct AnimationInfo {
    pub clip_name: String,
    pub clip_index: usize,
    pub clip_count: usize,
    pub time: f32,
    pub duration: f32,
    pub playing: bool,
}

/// UI renderer for text overlays
pub struct UiRenderer {
    brush: TextBrush<FontArc>,
}

impl UiRenderer {
    /// Load a system font using font-kit (cross-platform)
    fn load_system_font() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let source = SystemSource::new();

        // Try to find a sans-serif font
        let handle = source
            .select_best_match(&[FamilyName::SansSerif], &Properties::new())
            .or_else(|_| {
                // Fallback: try monospace
                source.select_best_match(&[FamilyName::Monospace], &Properties::new())
            })?;

        // Load the font data
        let font_data = handle.load()?.copy_font_data().ok_or("Failed to copy font data")?;
        Ok(font_data.to_vec())
    }

    pub fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        config: &wgpu::SurfaceConfiguration,
    ) -> Self {
        // Load a system font using font-kit (cross-platform)
        let font_data = Self::load_system_font()
            .expect("Failed to load system font");

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
    pub fn queue_text(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, state: &ViewerState, rpc_active: bool, animation_info: Option<&AnimationInfo>) {
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

        // Animation section
        let anim_clip_text;
        let anim_time_text;
        let anim_status_text;
        if let Some(info) = animation_info {
            current_y += line_height;
            sections.push(Section {
                screen_position: (x_offset, current_y),
                text: vec![Text::new("Animation")
                    .with_scale(header_size)
                    .with_color([0.8, 0.8, 0.8, 1.0])],
                ..Default::default()
            });
            current_y += line_height + header_padding;

            anim_clip_text = format!(
                "Clip {}/{}: {}",
                info.clip_index + 1,
                info.clip_count,
                info.clip_name
            );
            sections.push(Section {
                screen_position: (x_offset, current_y),
                text: vec![Text::new(&anim_clip_text)
                    .with_scale(text_size)
                    .with_color([0.9, 0.9, 0.9, 1.0])],
                ..Default::default()
            });
            current_y += line_height;

            anim_time_text = format!("Time: {:.2} / {:.2}s", info.time, info.duration);
            sections.push(Section {
                screen_position: (x_offset, current_y),
                text: vec![Text::new(&anim_time_text)
                    .with_scale(text_size)
                    .with_color([0.9, 0.9, 0.9, 1.0])],
                ..Default::default()
            });
            current_y += line_height;

            anim_status_text = if info.playing {
                "Status: Playing".to_string()
            } else {
                "Status: Paused".to_string()
            };
            let status_color = if info.playing {
                [0.4, 1.0, 0.4, 1.0]
            } else {
                [1.0, 1.0, 0.4, 1.0]
            };
            sections.push(Section {
                screen_position: (x_offset, current_y),
                text: vec![Text::new(&anim_status_text)
                    .with_scale(text_size)
                    .with_color(status_color)],
                ..Default::default()
            });
        }

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

        // Queue all sections at once
        self.brush.queue(device, queue, sections).unwrap();
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
