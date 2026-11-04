use std::sync::Arc;
use wgpu;
use winit::window::Window;

/// Bound large source previews while respecting the selected adapter's actual capability.
pub const VIEWER_BUFFER_LIMIT: u64 = 1 << 30;
pub(super) fn viewer_limits(available: wgpu::Limits) -> wgpu::Limits {
    wgpu::Limits {
        max_buffer_size: available.max_buffer_size.min(VIEWER_BUFFER_LIMIT),
        ..wgpu::Limits::default()
    }
}

/// GPU state for wgpu rendering
pub struct GpuState {
    instance: wgpu::Instance,
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
}

impl GpuState {
    /// Create a new GPU state for the given window
    pub async fn new(window: Arc<Window>, vsync: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: default_backends(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });

        // Create surface (Arc<Window> gives us Surface<'static>)
        let surface = instance.create_surface(window.clone())?;

        // Request adapter
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .expect("Failed to find an appropriate adapter");

        println!("Using GPU: {}", adapter.get_info().name);
        println!("Backend: {:?}", adapter.get_info().backend);

        // Request device and queue
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::POLYGON_MODE_LINE, // For wireframe
                required_limits: viewer_limits(adapter.limits()),
                memory_hints: Default::default(),
                trace: Default::default(),
                experimental_features: Default::default(),
            })
            .await?;

        // Configure surface
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let present_mode = if vsync {
            wgpu::PresentMode::Fifo // Vsync on
        } else {
            wgpu::PresentMode::AutoNoVsync // Select a supported unsynchronized mode, or fall back
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);

        Ok(Self {
            instance,
            surface,
            device,
            queue,
            config,
            size,
        })
    }

    pub fn recreate_surface(
        &mut self,
        window: Arc<Window>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.surface = self.instance.create_surface(window)?;
        self.surface.configure(&self.device, &self.config);
        Ok(())
    }

    /// Resize the surface
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// Capture a screenshot from a rendered texture
    pub fn screenshot_from_texture(
        &self,
        texture: &wgpu::Texture,
        path: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        super::readback::save_png(&self.device, &self.queue, texture, path)
    }
}

/// Graphics backends to request: Metal on macOS (wgpu does not compile in Vulkan/MoltenVK
/// there by default), Vulkan elsewhere for RenderDoc support. `WGPU_BACKEND` (e.g. `metal`,
/// `vulkan`, `dx12`, `gl`, comma-separated) overrides the default.
pub fn default_backends() -> wgpu::Backends {
    let platform = if cfg!(target_os = "macos") {
        wgpu::Backends::METAL
    } else {
        wgpu::Backends::VULKAN
    };
    wgpu::Backends::from_env().unwrap_or(platform)
}

#[cfg(test)]
mod preview_limit_tests {
    use super::*;
    #[test]
    fn preview_buffer_limit_is_bounded_by_the_adapter() {
        let mut available = wgpu::Limits::default();
        available.max_buffer_size = 2 << 30;
        assert_eq!(
            viewer_limits(available.clone()).max_buffer_size,
            VIEWER_BUFFER_LIMIT
        );
        available.max_buffer_size = 128 << 20;
        assert_eq!(viewer_limits(available).max_buffer_size, 128 << 20);
        let source_bytes =
            1_998_196u64 * 3 * std::mem::size_of::<super::super::mesh_renderer::Vertex>() as u64;
        assert!(source_bytes > wgpu::Limits::default().max_buffer_size);
        assert!(source_bytes < VIEWER_BUFFER_LIMIT);
    }
}
