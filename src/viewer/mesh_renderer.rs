use bytemuck::{Pod, Zeroable};
use nalgebra as na;
use wgpu;

/// Vertex for mesh rendering (position + per-vertex color + UV + skeletal animation data)
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
    pub texcoord: [f32; 2],
    pub joint_indices: [u32; 4],
    pub joint_weights: [f32; 4],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x4,
        2 => Float32x2,
        3 => Uint32x4,
        4 => Float32x4,
    ];

    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Uniforms for mesh rendering
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Uniforms {
    pub view_proj: [[f32; 4]; 4],
    pub model: [[f32; 4]; 4],
    pub camera_pos: [f32; 3],
    pub _padding: f32,
    // Shading parameters
    pub shading_mode: u32,       // 0=Lit, 1=Flat, 2=Unlit
    pub has_vertex_colors: u32,  // 1=use per-vertex color, 0=use uniform base_color
    pub has_texture: u32,        // 1=sample baseColorTexture, 0=no texture
    pub joint_count: u32,        // number of active joints (0 = no skinning)
    pub base_color: [f32; 4],
    pub light_direction: [f32; 3],
    pub _pad4: f32,
}

/// Mesh renderer handles rendering of 3D meshes
pub struct MeshRenderer {
    // Render pipelines
    solid_pipeline: wgpu::RenderPipeline,
    wireframe_pipeline: wgpu::RenderPipeline,
    backface_pipeline: wgpu::RenderPipeline,

    // Buffers
    vertex_buffer: Option<wgpu::Buffer>,
    index_buffer: Option<wgpu::Buffer>,
    backface_index_buffer: Option<wgpu::Buffer>,
    uniform_buffer: wgpu::Buffer,

    // Bind group
    bind_group: wgpu::BindGroup,

    // Mesh data
    num_indices: u32,
    num_backface_indices: u32,

    // Whether the loaded mesh has per-vertex colors
    has_vertex_colors: bool,
    // Whether the loaded mesh has a texture
    has_texture: bool,

    // Texture bind group (group 1)
    texture_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,

    // Joint palette (group 2)
    joint_palette_buffer: wgpu::Buffer,
    joint_palette_bind_group: wgpu::BindGroup,
    joint_count: u32,

    // Depth texture
    depth_texture: wgpu::TextureView,
}

impl MeshRenderer {
    pub fn new(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> Self {
        // Load shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Mesh Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/mesh.wgsl").into()),
        });

        // Create uniform buffer
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Mesh Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Mesh Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Create texture bind group layout (group 1)
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Texture Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        // Create default 1x1 white texture (placeholder when no texture loaded)
        let (default_texture_bind_group, _) =
            Self::create_texture_bind_group(device, &texture_bind_group_layout, 1, 1);

        // Create joint palette bind group layout (group 2)
        let joint_palette_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Joint Palette Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        // Create joint palette buffer (256 mat4x4<f32> = 16384 bytes, default identity)
        let joint_palette_buffer = Self::create_joint_palette_buffer(device, &[]);
        let joint_palette_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Joint Palette Bind Group"),
            layout: &joint_palette_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: joint_palette_buffer.as_entire_binding(),
            }],
        });

        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Mesh Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout, &texture_bind_group_layout, &joint_palette_bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create depth texture
        let depth_texture = Self::create_depth_texture(device, config);

        // Create solid pipeline
        let solid_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Solid Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Create wireframe pipeline
        let wireframe_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Wireframe Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_wireframe"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Line, // Wireframe mode
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Create backface pipeline
        let backface_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Backface Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_backface"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            solid_pipeline,
            wireframe_pipeline,
            backface_pipeline,
            vertex_buffer: None,
            index_buffer: None,
            backface_index_buffer: None,
            uniform_buffer,
            bind_group,
            num_indices: 0,
            num_backface_indices: 0,
            has_vertex_colors: false,
            has_texture: false,
            texture_bind_group: default_texture_bind_group,
            texture_bind_group_layout,
            joint_palette_buffer,
            joint_palette_bind_group,
            joint_count: 0,
            depth_texture,
        }
    }

    /// Create depth texture
    fn create_depth_texture(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> wgpu::TextureView {
        let size = wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        };
        let desc = wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        };
        let texture = device.create_texture(&desc);
        texture.create_view(&wgpu::TextureViewDescriptor::default())
    }

    /// Resize depth texture
    pub fn resize(&mut self, device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) {
        self.depth_texture = Self::create_depth_texture(device, config);
    }

    /// Create a texture + sampler bind group
    fn create_texture_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        width: u32,
        height: u32,
    ) -> (wgpu::BindGroup, wgpu::Texture) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Mesh Texture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Texture Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        (bind_group, texture)
    }

    /// Maximum number of joints supported in the palette
    const MAX_JOINTS: usize = 256;
    /// Create a joint palette buffer filled with identity matrices,
    /// optionally overwriting the first `matrices.len()` entries.
    fn create_joint_palette_buffer(
        device: &wgpu::Device,
        matrices: &[[[f32; 4]; 4]],
    ) -> wgpu::Buffer {
        let identity: [[f32; 4]; 4] = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let mut data = vec![identity; Self::MAX_JOINTS];
        let count = matrices.len().min(Self::MAX_JOINTS);
        data[..count].copy_from_slice(&matrices[..count]);

        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Joint Palette Buffer"),
            contents: bytemuck::cast_slice(&data),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    }

    /// Update the joint palette buffer with new matrices.
    /// Pads to 256 matrices with identity if fewer are provided.
    pub fn update_joint_palette(&self, queue: &wgpu::Queue, matrices: &[[[f32; 4]; 4]]) {
        let identity: [[f32; 4]; 4] = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let mut data = vec![identity; Self::MAX_JOINTS];
        let count = matrices.len().min(Self::MAX_JOINTS);
        data[..count].copy_from_slice(&matrices[..count]);
        queue.write_buffer(&self.joint_palette_buffer, 0, bytemuck::cast_slice(&data));
    }

    /// Set the joint count (number of active joints for skinning)
    pub fn set_joint_count(&mut self, count: u32) {
        self.joint_count = count;
    }

    /// Load mesh data with per-vertex colors and optional texture
    #[allow(clippy::too_many_arguments)]
    pub fn load_mesh(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[Vertex],
        indices: &[u32],
        backface_indices: &[u32],
        has_vertex_colors: bool,
        texture_data: Option<&crate::mesh::loader::TextureData>,
    ) {
        self.has_vertex_colors = has_vertex_colors;

        // Load texture if provided
        if let Some(tex) = texture_data {
            let (bind_group, texture) = Self::create_texture_bind_group(
                device, &self.texture_bind_group_layout, tex.width, tex.height,
            );
            // Write pixel data to texture
            queue.write_texture(
                texture.as_image_copy(),
                &tex.pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * tex.width),
                    rows_per_image: Some(tex.height),
                },
                wgpu::Extent3d { width: tex.width, height: tex.height, depth_or_array_layers: 1 },
            );
            self.texture_bind_group = bind_group;
            self.has_texture = true;
        } else {
            self.has_texture = false;
        }

        // Create vertex buffer
        self.vertex_buffer = Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }));

        // Create index buffer
        self.index_buffer = Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        }));

        // Create backface index buffer
        self.backface_index_buffer = Some(device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Backface Index Buffer"),
                contents: bytemuck::cast_slice(backface_indices),
                usage: wgpu::BufferUsages::INDEX,
            },
        ));

        self.num_indices = indices.len() as u32;
        self.num_backface_indices = backface_indices.len() as u32;
    }

    /// Load mesh data for dynamic per-frame updates (buffers created with COPY_DST).
    /// Used by the BVH skeleton viewer where vertex positions change every frame.
    #[allow(clippy::too_many_arguments)]
    pub fn load_mesh_dynamic(
        &mut self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        vertices: &[Vertex],
        indices: &[u32],
        backface_indices: &[u32],
        has_vertex_colors: bool,
        texture_data: Option<&crate::mesh::loader::TextureData>,
    ) {
        self.has_vertex_colors = has_vertex_colors;
        self.has_texture = texture_data.is_some();

        // Allocate with headroom so small frame-to-frame size changes don't require realloc
        let vert_capacity = vertices.len().max(256);
        let idx_capacity = indices.len().max(256);
        let back_capacity = backface_indices.len().max(256);

        // Create vertex buffer with COPY_DST for dynamic updates
        let vb = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Dynamic Vertex Buffer"),
            size: (vert_capacity * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: true,
        });
        vb.slice(..(vertices.len() * std::mem::size_of::<Vertex>()) as u64)
            .get_mapped_range_mut()[..bytemuck::cast_slice::<Vertex, u8>(vertices).len()]
            .copy_from_slice(bytemuck::cast_slice(vertices));
        vb.unmap();
        self.vertex_buffer = Some(vb);

        // Create index buffer with COPY_DST
        let ib = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Dynamic Index Buffer"),
            size: (idx_capacity * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: true,
        });
        ib.slice(..(indices.len() * std::mem::size_of::<u32>()) as u64)
            .get_mapped_range_mut()[..bytemuck::cast_slice::<u32, u8>(indices).len()]
            .copy_from_slice(bytemuck::cast_slice(indices));
        ib.unmap();
        self.index_buffer = Some(ib);

        // Create backface index buffer with COPY_DST
        let bb = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Dynamic Backface Index Buffer"),
            size: (back_capacity * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: true,
        });
        bb.slice(..(backface_indices.len() * std::mem::size_of::<u32>()) as u64)
            .get_mapped_range_mut()[..bytemuck::cast_slice::<u32, u8>(backface_indices).len()]
            .copy_from_slice(bytemuck::cast_slice(backface_indices));
        bb.unmap();
        self.backface_index_buffer = Some(bb);

        self.num_indices = indices.len() as u32;
        self.num_backface_indices = backface_indices.len() as u32;
    }

    /// Update vertex and index data for a dynamic mesh. Recreates buffers if needed.
    pub fn update_dynamic_mesh(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[Vertex],
        indices: &[u32],
        backface_indices: &[u32],
    ) {
        let vert_bytes = bytemuck::cast_slice::<Vertex, u8>(vertices);
        let idx_bytes = bytemuck::cast_slice::<u32, u8>(indices);
        let back_bytes = bytemuck::cast_slice::<u32, u8>(backface_indices);

        // Realloc vertex buffer if too small
        let need_vb_realloc = match &self.vertex_buffer {
            Some(b) => b.size() < vert_bytes.len() as u64,
            None => true,
        };
        if need_vb_realloc {
            self.vertex_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Dynamic Vertex Buffer"),
                size: (vert_bytes.len() * 2) as u64, // 2x headroom
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        queue.write_buffer(self.vertex_buffer.as_ref().unwrap(), 0, vert_bytes);

        // Realloc index buffer if too small
        let need_ib_realloc = match &self.index_buffer {
            Some(b) => b.size() < idx_bytes.len() as u64,
            None => true,
        };
        if need_ib_realloc {
            self.index_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Dynamic Index Buffer"),
                size: (idx_bytes.len() * 2) as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        queue.write_buffer(self.index_buffer.as_ref().unwrap(), 0, idx_bytes);

        // Realloc backface index buffer if too small
        let need_bb_realloc = match &self.backface_index_buffer {
            Some(b) => b.size() < back_bytes.len() as u64,
            None => true,
        };
        if need_bb_realloc {
            self.backface_index_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Dynamic Backface Index Buffer"),
                size: (back_bytes.len() * 2) as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        queue.write_buffer(self.backface_index_buffer.as_ref().unwrap(), 0, back_bytes);

        self.num_indices = indices.len() as u32;
        self.num_backface_indices = backface_indices.len() as u32;
    }

    /// Get a reference to the vertex buffer (for external writes).
    #[allow(dead_code)]
    pub fn vertex_buffer_ref(&self) -> Option<&wgpu::Buffer> {
        self.vertex_buffer.as_ref()
    }

    /// Get a reference to the index buffer (for external writes).
    #[allow(dead_code)]
    pub fn index_buffer_ref(&self) -> Option<&wgpu::Buffer> {
        self.index_buffer.as_ref()
    }

    /// Update uniforms
    #[allow(clippy::too_many_arguments)]
    pub fn update_uniforms(
        &self,
        queue: &wgpu::Queue,
        view_proj: &na::Matrix4<f32>,
        model: &na::Matrix4<f32>,
        camera_pos: &na::Point3<f32>,
        shading_mode: u32,
        base_color: [f32; 4],
        light_direction: [f32; 3],
    ) {
        let uniforms = Uniforms {
            view_proj: (*view_proj).into(),
            model: (*model).into(),
            camera_pos: [camera_pos.x, camera_pos.y, camera_pos.z],
            _padding: 0.0,
            shading_mode,
            has_vertex_colors: if self.has_vertex_colors { 1 } else { 0 },
            has_texture: if self.has_texture { 1 } else { 0 },
            joint_count: self.joint_count,
            base_color,
            light_direction,
            _pad4: 0.0,
        };

        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }

    /// Render the mesh
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        show_wireframe: bool,
        show_backfaces: bool,
        clear_color: [f32; 4],
    ) {
        if self.vertex_buffer.is_none() {
            return; // No mesh loaded
        }

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Mesh Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: clear_color[0] as f64,
                        g: clear_color[1] as f64,
                        b: clear_color[2] as f64,
                        a: clear_color[3] as f64,
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_texture,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_bind_group(1, &self.texture_bind_group, &[]);
        render_pass.set_bind_group(2, &self.joint_palette_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.as_ref().unwrap().slice(..));
        render_pass.set_index_buffer(
            self.index_buffer.as_ref().unwrap().slice(..),
            wgpu::IndexFormat::Uint32,
        );

        // Draw solid mesh
        render_pass.set_pipeline(&self.solid_pipeline);
        render_pass.draw_indexed(0..self.num_indices, 0, 0..1);

        // Draw wireframe if enabled
        if show_wireframe {
            render_pass.set_pipeline(&self.wireframe_pipeline);
            render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
        }

        // Draw backfaces if enabled
        if show_backfaces {
            render_pass.set_index_buffer(
                self.backface_index_buffer.as_ref().unwrap().slice(..),
                wgpu::IndexFormat::Uint32,
            );
            render_pass.set_pipeline(&self.backface_pipeline);
            render_pass.draw_indexed(0..self.num_backface_indices, 0, 0..1);
        }
    }
}

// Add wgpu::util for buffer_init
use wgpu::util::DeviceExt;
