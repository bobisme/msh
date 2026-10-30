use bytemuck::{Pod, Zeroable};
use nalgebra as na;
use std::collections::HashMap;
use wgpu;

use crate::mesh::loader::{AlphaMode, MaterialData, MeshMaterials};

use super::ground::GroundBounds;
use super::render::RenderMesh;
use super::shadow::GroundShadow;
use super::state::Background;

/// Vertex for mesh rendering (position + per-vertex color + UV + skeletal animation data + normal/tangent)
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
    pub texcoord: [f32; 2],
    pub joint_indices: [u32; 4],
    pub joint_weights: [f32; 4],
    /// Zero when the source has no normals (shader falls back to face normals)
    pub normal: [f32; 3],
    /// xyz = tangent, w = bitangent sign; zero when the source has no tangents
    pub tangent: [f32; 4],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x4,
        2 => Float32x2,
        3 => Uint32x4,
        4 => Float32x4,
        5 => Float32x3,
        6 => Float32x4,
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
    pub shading_mode: u32,      // 0=Lit, 1=Flat, 2=Unlit
    pub has_vertex_colors: u32, // 1=use per-vertex color, 0=use uniform base_color
    pub viewport_aspect: f32,   // width / height, for the round background gradient
    pub joint_count: u32,       // number of active joints (0 = no skinning)
    pub base_color: [f32; 4],
    pub light_direction: [f32; 3],
    pub backface_unlit: u32,
    pub background_center: [f32; 4],
    pub background_edge: [f32; 4],
    pub ground_plane: [f32; 4],
    pub shadow_view_proj: [[f32; 4]; 4],
}

/// Per-material uniforms (group 1, binding 0)
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct MaterialUniforms {
    pub base_color_factor: [f32; 4],
    pub has_base_color_texture: u32,
    pub has_normal_texture: u32,
    pub has_metallic_roughness_texture: u32,
    pub has_occlusion_texture: u32,
    pub alpha_mode: u32,       // 0=Opaque, 1=Mask, 2=Blend
    pub use_viewer_color: u32, // 1=ignore base_color_factor, use vertex colors / viewer base color
    pub alpha_cutoff: f32,
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub normal_scale: f32,
    pub occlusion_strength: f32,
    pub material_id: u32,
    pub uv_transforms: [[f32; 4]; 8],
}

impl MaterialUniforms {
    /// Material for meshes without glTF materials (OBJ, 3MF, primitives with no material)
    fn viewer_default() -> Self {
        Self {
            base_color_factor: [1.0; 4],
            has_base_color_texture: 0,
            has_normal_texture: 0,
            has_metallic_roughness_texture: 0,
            has_occlusion_texture: 0,
            alpha_mode: 0,
            use_viewer_color: 1,
            alpha_cutoff: 0.5,
            metallic_factor: 0.0,
            roughness_factor: 0.5,
            normal_scale: 1.0,
            occlusion_strength: 1.0,
            material_id: 1,
            uv_transforms: crate::mesh::loader::identity_uv_transforms(),
        }
    }

    fn from_material(m: &MaterialData) -> Self {
        Self {
            base_color_factor: m.base_color_factor,
            has_base_color_texture: m.base_color_texture.is_some() as u32,
            has_normal_texture: m.normal_texture.is_some() as u32,
            has_metallic_roughness_texture: m.metallic_roughness_texture.is_some() as u32,
            has_occlusion_texture: m.occlusion_texture.is_some() as u32,
            alpha_mode: match m.alpha_mode {
                AlphaMode::Opaque => 0,
                AlphaMode::Mask => 1,
                AlphaMode::Blend => 2,
            },
            use_viewer_color: 0,
            alpha_cutoff: m.alpha_cutoff,
            metallic_factor: m.metallic_factor,
            roughness_factor: m.roughness_factor,
            normal_scale: m.normal_scale,
            occlusion_strength: m.occlusion_strength,
            material_id: 1,
            uv_transforms: m.uv_transforms,
        }
    }
}

/// Whether glTF materials should defer their color to the viewer (vertex colors / `--base-color`).
///
/// True when no material has a base color texture and they all share one `baseColorFactor`:
/// the materials carry no color information, so the viewer's base color applies (as it did
/// before per-primitive materials). Alpha mode and culling still come from each material.
pub fn materials_use_viewer_color(materials: &[MaterialData]) -> bool {
    materials.iter().all(|m| m.base_color_texture.is_none())
        && materials
            .windows(2)
            .all(|w| w[0].base_color_factor == w[1].base_color_factor)
}

/// A contiguous range of the index buffer drawn with one material
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawBatch {
    pub first_index: u32,
    pub index_count: u32,
    /// Index into the mesh's materials; None = viewer default material
    pub material: Option<usize>,
}

/// Fixed-function state for one draw of a material
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PassState {
    pub cull: Option<wgpu::Face>,
    pub blend: bool,
}

/// Draws needed to render a material, in order.
///
/// Double-sided blended materials draw their back faces before their front faces so the
/// near side composites over the far side without per-triangle sorting.
pub fn material_passes(alpha_mode: AlphaMode, double_sided: bool) -> &'static [PassState] {
    const BACK: PassState = PassState {
        cull: Some(wgpu::Face::Back),
        blend: false,
    };
    const NONE: PassState = PassState {
        cull: None,
        blend: false,
    };
    const BLEND_BACK: PassState = PassState {
        cull: Some(wgpu::Face::Back),
        blend: true,
    };
    const BLEND_FRONT: PassState = PassState {
        cull: Some(wgpu::Face::Front),
        blend: true,
    };
    match (alpha_mode, double_sided) {
        (AlphaMode::Blend, false) => &[BLEND_BACK],
        (AlphaMode::Blend, true) => &[BLEND_FRONT, BLEND_BACK],
        (_, false) => &[BACK],
        (_, true) => &[NONE],
    }
}

/// GPU resources for one material
struct GpuMaterial {
    bind_group: wgpu::BindGroup,
    alpha_mode: AlphaMode,
    double_sided: bool,
}

/// Mesh renderer handles rendering of 3D meshes
pub struct MeshRenderer {
    // Render pipelines: solid pipelines indexed by `solid_pipeline_index`
    solid_pipelines: Vec<wgpu::RenderPipeline>,
    wireframe_pipeline: wgpu::RenderPipeline,
    backface_pipeline: wgpu::RenderPipeline,
    background_pipeline: wgpu::RenderPipeline,
    ground_pipeline: wgpu::RenderPipeline,
    id_pipeline: wgpu::RenderPipeline,
    material_labels: Vec<Option<String>>,
    ground_shadow: GroundShadow,
    ground_bounds: Option<GroundBounds>,
    joint_shadow_bounds: [Option<GroundBounds>; 256],
    shadow_bounds: std::cell::Cell<Option<GroundBounds>>,
    ground_visible: std::cell::Cell<bool>,

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
    /// Draw batches; empty = draw all indices with the default material
    batches: Vec<DrawBatch>,

    // Whether the loaded mesh has per-vertex colors
    has_vertex_colors: bool,

    // Materials (group 1)
    material_bind_group_layout: wgpu::BindGroupLayout,
    default_material: GpuMaterial,
    materials: Vec<GpuMaterial>,
    white_texture: wgpu::TextureView,
    sampler: wgpu::Sampler,

    // Joint palette (group 2)
    joint_palette_buffer: wgpu::Buffer,
    joint_palette_bind_group: wgpu::BindGroup,
    joint_count: u32,
    skin_center: [f32; 3],

    // Depth texture
    depth_texture: wgpu::TextureView,
    viewport_aspect: f32,

    /// Background set by the last `update_uniforms`
    background: std::cell::Cell<Background>,
}

impl MeshRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
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

        // Create material bind group layout (group 1): uniforms, base color, normal,
        // metallic-roughness, occlusion, sampler
        let texture_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                multisampled: false,
                view_dimension: wgpu::TextureViewDimension::D2,
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
            },
            count: None,
        };
        let material_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Material Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    texture_entry(1),
                    texture_entry(2),
                    texture_entry(3),
                    texture_entry(4),
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Material Sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });

        // 1x1 white texture bound to unused material texture slots
        let white_texture = Self::create_texture_view(
            device,
            queue,
            &crate::mesh::loader::TextureData {
                pixels: vec![255; 4],
                width: 1,
                height: 1,
            },
            false,
        );

        let default_material = Self::create_gpu_material(
            device,
            &material_bind_group_layout,
            &sampler,
            &MaterialUniforms::viewer_default(),
            [&white_texture; 4],
            AlphaMode::Opaque,
            false,
        );

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
            bind_group_layouts: &[
                &bind_group_layout,
                &material_bind_group_layout,
                &joint_palette_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let shadow_binding_layout = GroundShadow::binding_layout(device);
        let ground_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Ground Shadow Catcher Pipeline Layout"),
                bind_group_layouts: &[
                    &bind_group_layout,
                    &material_bind_group_layout,
                    &joint_palette_bind_group_layout,
                    &shadow_binding_layout,
                ],
                push_constant_ranges: &[],
            });
        let ground_shadow =
            GroundShadow::new(device, &shader, &pipeline_layout, &shadow_binding_layout);

        // Create depth texture
        let depth_texture = Self::create_depth_texture(device, config);

        let create_pipeline = |label: &str,
                               fs_entry: &str,
                               cull_mode: Option<wgpu::Face>,
                               polygon_mode: wgpu::PolygonMode,
                               blend: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Vertex::desc()],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs_entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.format,
                        blend: Some(if blend {
                            wgpu::BlendState::ALPHA_BLENDING
                        } else {
                            wgpu::BlendState::REPLACE
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode,
                    polygon_mode,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    // Blended surfaces are depth-tested but don't occlude each other
                    depth_write_enabled: !blend,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };

        // Solid pipelines for every (cull, blend) combination
        let mut solid_pipelines = Vec::with_capacity(6);
        for cull in [None, Some(wgpu::Face::Back), Some(wgpu::Face::Front)] {
            for blend in [false, true] {
                debug_assert_eq!(
                    solid_pipelines.len(),
                    Self::solid_pipeline_index(PassState { cull, blend })
                );
                solid_pipelines.push(create_pipeline(
                    "Solid Pipeline",
                    "fs_main",
                    cull,
                    wgpu::PolygonMode::Fill,
                    blend,
                ));
            }
        }

        let wireframe_pipeline = create_pipeline(
            "Wireframe Pipeline",
            "fs_wireframe",
            Some(wgpu::Face::Back),
            wgpu::PolygonMode::Line,
            false,
        );

        let backface_pipeline = create_pipeline(
            "Backface Pipeline",
            "fs_backface",
            Some(wgpu::Face::Back),
            wgpu::PolygonMode::Fill,
            false,
        );

        let id_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Visible Material and Backface IDs"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_ids"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R32Uint,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });

        // Environment draws use generated vertices and leave depth writes to the mesh.
        let create_environment_pipeline = |label, vs_entry, fs_entry, ground: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(if ground {
                    &ground_pipeline_layout
                } else {
                    &pipeline_layout
                }),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs_entry),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs_entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.format,
                        blend: Some(if ground {
                            wgpu::BlendState::ALPHA_BLENDING
                        } else {
                            wgpu::BlendState::REPLACE
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: if ground { Some(wgpu::Face::Back) } else { None },
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: false,
                    depth_compare: if ground {
                        wgpu::CompareFunction::Less
                    } else {
                        wgpu::CompareFunction::Always
                    },
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };
        let background_pipeline = create_environment_pipeline(
            "Background Pipeline",
            "vs_background",
            "fs_background",
            false,
        );
        let ground_pipeline =
            create_environment_pipeline("Ground Pipeline", "vs_ground", "fs_ground", true);

        Self {
            solid_pipelines,
            wireframe_pipeline,
            backface_pipeline,
            background_pipeline,
            ground_pipeline,
            id_pipeline,
            material_labels: vec![None],
            ground_shadow,
            ground_bounds: None,
            joint_shadow_bounds: [None; 256],
            shadow_bounds: std::cell::Cell::new(None),
            ground_visible: std::cell::Cell::new(false),
            vertex_buffer: None,
            index_buffer: None,
            backface_index_buffer: None,
            uniform_buffer,
            bind_group,
            num_indices: 0,
            num_backface_indices: 0,
            batches: Vec::new(),
            has_vertex_colors: false,
            material_bind_group_layout,
            default_material,
            materials: Vec::new(),
            white_texture,
            sampler,
            joint_palette_buffer,
            joint_palette_bind_group,
            joint_count: 0,
            skin_center: [0.0; 3],
            depth_texture,
            viewport_aspect: config.width as f32 / config.height.max(1) as f32,
            background: std::cell::Cell::new(Background::default()),
        }
    }

    fn solid_pipeline_index(pass: PassState) -> usize {
        let cull = match pass.cull {
            None => 0,
            Some(wgpu::Face::Back) => 1,
            Some(wgpu::Face::Front) => 2,
        };
        cull * 2 + pass.blend as usize
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
        self.viewport_aspect = config.width as f32 / config.height.max(1) as f32;
    }

    /// Upload an RGBA8 texture with a full box-filtered mip chain
    fn create_texture_view(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        tex: &crate::mesh::loader::TextureData,
        srgb: bool,
    ) -> wgpu::TextureView {
        let mips = mip_chain(tex);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Mesh Texture"),
            size: wgpu::Extent3d {
                width: tex.width,
                height: tex.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: mips.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: if srgb {
                wgpu::TextureFormat::Rgba8UnormSrgb
            } else {
                wgpu::TextureFormat::Rgba8Unorm
            },
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (level, (w, h, pixels)) in mips.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * w),
                    rows_per_image: Some(*h),
                },
                wgpu::Extent3d {
                    width: *w,
                    height: *h,
                    depth_or_array_layers: 1,
                },
            );
        }
        texture.create_view(&wgpu::TextureViewDescriptor::default())
    }

    #[allow(clippy::too_many_arguments)]
    fn create_gpu_material(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        uniforms: &MaterialUniforms,
        textures: [&wgpu::TextureView; 4],
        alpha_mode: AlphaMode,
        double_sided: bool,
    ) -> GpuMaterial {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Material Uniform Buffer"),
            contents: bytemuck::bytes_of(uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let [base, normal, mr, occlusion] = textures;
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Material Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(base),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(normal),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(mr),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(occlusion),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        GpuMaterial {
            bind_group,
            alpha_mode,
            double_sided,
        }
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
        let center = na::Matrix4::new_translation(&-na::Vector3::from(self.skin_center));
        for (target, matrix) in data.iter_mut().zip(matrices) {
            *target = (center * na::Matrix4::from(*matrix)).into();
        }
        queue.write_buffer(&self.joint_palette_buffer, 0, bytemuck::cast_slice(&data));
        // A union of each joint's transformed influence bounds conservatively contains
        // blended skin positions. Fit the shadow camera to the current pose without
        // reskinning every vertex on the CPU, while keeping the catcher at rest height.
        let mut bounds = self.ground_bounds;
        for (joint_bounds, matrix) in self.joint_shadow_bounds.iter().zip(&data[..count]) {
            if let Some(posed) =
                joint_bounds.and_then(|b| b.transformed(&na::Matrix4::from(*matrix)))
            {
                bounds = Some(bounds.map_or(posed, |b| b.union(posed)));
            }
        }
        self.shadow_bounds.set(bounds);
    }

    /// Set the joint count (number of active joints for skinning)
    pub fn set_joint_count(&mut self, count: u32) {
        self.joint_count = count;
    }

    /// Load a mesh and its materials
    pub fn load_mesh(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &RenderMesh,
        materials: &MeshMaterials,
    ) {
        let vertices = &mesh.vertices;
        self.ground_bounds = mesh.ground_bounds;
        self.skin_center = mesh.skeleton_data.as_ref().map_or([0.0; 3], |s| s.center);
        self.shadow_bounds.set(self.ground_bounds);
        self.joint_shadow_bounds = [None; 256];
        for vertex in vertices {
            let Some(point) = GroundBounds::from_positions([vertex.position]) else {
                continue;
            };
            for (&index, &weight) in vertex.joint_indices.iter().zip(&vertex.joint_weights) {
                if weight > 0.0
                    && let Some(bounds) = self.joint_shadow_bounds.get_mut(index as usize)
                {
                    *bounds = Some(bounds.map_or(point, |b| b.union(point)));
                }
            }
        }
        let indices = &mesh.indices;
        let backface_indices = &mesh.backface_indices;
        self.has_vertex_colors = mesh.has_vertex_colors;
        self.batches = mesh.batches.clone();

        // Upload each (texture, color space) pair once
        let mut views: HashMap<(usize, bool), wgpu::TextureView> = HashMap::new();
        let mut view_for = |idx: Option<usize>, srgb: bool| -> Option<wgpu::TextureView> {
            let idx = idx?;
            let tex = materials.textures.get(idx)?;
            Some(
                views
                    .entry((idx, srgb))
                    .or_insert_with(|| Self::create_texture_view(device, queue, tex, srgb))
                    .clone(),
            )
        };
        let use_viewer_color = materials_use_viewer_color(&materials.materials);
        self.material_labels = std::iter::once(None)
            .chain(materials.materials.iter().map(|m| m.name.clone()))
            .collect();
        self.materials = materials
            .materials
            .iter()
            .enumerate()
            .map(|(index, m)| {
                let mut uniforms = MaterialUniforms::from_material(m);
                uniforms.material_id = index as u32 + 2;
                uniforms.use_viewer_color = use_viewer_color as u32;
                let base = view_for(m.base_color_texture, true);
                let normal = view_for(m.normal_texture, false);
                let mr = view_for(m.metallic_roughness_texture, false);
                let occlusion = view_for(m.occlusion_texture, false);
                let white = &self.white_texture;
                Self::create_gpu_material(
                    device,
                    &self.material_bind_group_layout,
                    &self.sampler,
                    &uniforms,
                    [
                        base.as_ref().unwrap_or(white),
                        normal.as_ref().unwrap_or(white),
                        mr.as_ref().unwrap_or(white),
                        occlusion.as_ref().unwrap_or(white),
                    ],
                    m.alpha_mode,
                    m.double_sided,
                )
            })
            .collect();

        // Create vertex buffer
        self.vertex_buffer = Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
        );

        // Create index buffer
        self.index_buffer = Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Index Buffer"),
                contents: bytemuck::cast_slice(indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
        );

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
    pub fn load_mesh_dynamic(
        &mut self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        vertices: &[Vertex],
        indices: &[u32],
        backface_indices: &[u32],
        has_vertex_colors: bool,
    ) {
        self.ground_bounds = GroundBounds::from_positions(vertices.iter().map(|v| v.position));
        self.shadow_bounds.set(self.ground_bounds);
        self.joint_shadow_bounds = [None; 256];
        self.has_vertex_colors = has_vertex_colors;
        self.batches.clear();
        self.materials.clear();

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
        let posed = GroundBounds::from_positions(vertices.iter().map(|v| v.position));
        self.shadow_bounds.set(match (self.ground_bounds, posed) {
            (Some(rest), Some(pose)) => Some(rest.union(pose)),
            (rest, pose) => rest.or(pose),
        });
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
        background: Background,
        backface_unlit: bool,
    ) {
        self.background.set(background);
        let ground_plane = self
            .ground_bounds
            .map(|bounds| bounds.placement(model))
            .unwrap_or([0.0; 4]);
        let shadow_view_proj = self
            .shadow_bounds
            .get()
            .and_then(|bounds| bounds.shadow_view_projection(model, light_direction));
        // Hide the entire floor below its top surface, including orthographic views.
        // Transparent backgrounds are used for isolated assets and sprite baking.
        let opaque_background = match background {
            Background::Solid(c) => c[3] >= 1.0,
            Background::RadialGradient { center, edge } => center[3] >= 1.0 && edge[3] >= 1.0,
        };
        self.ground_visible.set(
            ground_plane[3] > 0.0
                && camera_pos.y > ground_plane[1]
                && opaque_background
                && shadow_view_proj.is_some(),
        );
        let (background_center, background_edge) = match background {
            Background::Solid(c) => (c, c),
            Background::RadialGradient { center, edge } => (center, edge),
        };
        let uniforms = Uniforms {
            view_proj: (*view_proj).into(),
            model: (*model).into(),
            camera_pos: [camera_pos.x, camera_pos.y, camera_pos.z],
            _padding: 0.0,
            shading_mode,
            has_vertex_colors: if self.has_vertex_colors { 1 } else { 0 },
            viewport_aspect: self.viewport_aspect,
            joint_count: self.joint_count,
            base_color,
            light_direction,
            backface_unlit: backface_unlit as u32,
            background_center,
            background_edge,
            ground_plane,
            shadow_view_proj: shadow_view_proj
                .unwrap_or_else(na::Matrix4::identity)
                .into(),
        };

        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }

    /// A dedicated integer pass: no UI, wireframe, lights, or ground in the counts.
    pub fn pixel_stats(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
    ) -> Result<crate::rpc::types::PixelStats, Box<dyn std::error::Error>> {
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Visible ID Target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Uint,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        if let Some(vertices) = &self.vertex_buffer {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Diagnostic ID Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
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
            pass.set_pipeline(&self.id_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_bind_group(2, &self.joint_palette_bind_group, &[]);
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.set_index_buffer(
                self.index_buffer.as_ref().unwrap().slice(..),
                wgpu::IndexFormat::Uint32,
            );
            let default = [DrawBatch {
                first_index: 0,
                index_count: self.num_indices,
                material: None,
            }];
            let batches = if self.batches.is_empty() {
                &default[..]
            } else {
                &self.batches
            };
            for batch in batches {
                let material = batch
                    .material
                    .and_then(|i| self.materials.get(i))
                    .unwrap_or(&self.default_material);
                pass.set_bind_group(1, &material.bind_group, &[]);
                pass.draw_indexed(
                    batch.first_index..batch.first_index + batch.index_count,
                    0,
                    0..1,
                );
            }
        } else {
            return Err("No mesh loaded".into());
        }
        queue.submit(Some(encoder.finish()));
        let pixels = super::readback::read_pixels(device, queue, &target)?;
        let mut counts = vec![0usize; self.material_labels.len()];
        let mut backface_px = 0;
        let mut covered_px = 0;
        let mut bbox: Option<[u32; 4]> = None;
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let value = u32::from_le_bytes(pixel.try_into().unwrap());
            if value == 0 {
                continue;
            }
            covered_px += 1;
            let id = value & 0x7fff_ffff;
            if let Some(count) = counts.get_mut(id.saturating_sub(1) as usize) {
                *count += 1;
            }
            if value & 0x8000_0000 != 0 {
                backface_px += 1;
                let x = index as u32 % width;
                let y = index as u32 / width;
                let b = bbox.get_or_insert([x, y, x, y]);
                b[0] = b[0].min(x);
                b[1] = b[1].min(y);
                b[2] = b[2].max(x);
                b[3] = b[3].max(y);
            }
        }
        let materials: Vec<_> = counts
            .iter()
            .enumerate()
            .map(|(index, &pixels)| crate::rpc::types::MaterialPixels {
                id: index as u32 + 1,
                name: self.material_labels[index].clone(),
                pixels,
            })
            .collect();
        // Friendly names aggregate collisions; the materials array retains every unique ID.
        let mut material_px = std::collections::BTreeMap::new();
        for material in &materials {
            *material_px
                .entry(material.name.clone().unwrap_or_else(|| "default".into()))
                .or_insert(0) += material.pixels;
        }
        Ok(crate::rpc::types::PixelStats {
            width,height,total_px: width as usize * height as usize,covered_px,backface_px,bbox,material_px,materials,
            blend_policy: "Nearest mesh fragment; BLEND alpha >= 0.5; MASK uses material cutoff; both sides counted; UI/wireframe/ground excluded".into(),
        })
    }

    /// Render the mesh
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        show_wireframe: bool,
        show_backfaces: bool,
        show_ground: bool,
    ) {
        let background = self.background.get();
        let clear_color = match background {
            Background::Solid(c) => c,
            Background::RadialGradient { center, .. } => center,
        };
        if self.vertex_buffer.is_none() {
            return; // No mesh loaded
        }

        if show_ground && self.ground_visible.get() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Ground Shadow Depth Pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.ground_shadow.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.ground_shadow.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_bind_group(2, &self.joint_palette_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.as_ref().unwrap().slice(..));
            pass.set_index_buffer(
                self.index_buffer.as_ref().unwrap().slice(..),
                wgpu::IndexFormat::Uint32,
            );
            if self.batches.is_empty() {
                pass.set_bind_group(1, &self.default_material.bind_group, &[]);
                pass.draw_indexed(0..self.num_indices, 0, 0..1);
            } else {
                for batch in &self.batches {
                    let material = batch
                        .material
                        .and_then(|i| self.materials.get(i))
                        .unwrap_or(&self.default_material);
                    pass.set_bind_group(1, &material.bind_group, &[]);
                    pass.draw_indexed(
                        batch.first_index..batch.first_index + batch.index_count,
                        0,
                        0..1,
                    );
                }
            }
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
        render_pass.set_bind_group(1, &self.default_material.bind_group, &[]);
        render_pass.set_bind_group(2, &self.joint_palette_bind_group, &[]);

        if matches!(background, Background::RadialGradient { .. }) {
            render_pass.set_pipeline(&self.background_pipeline);
            render_pass.draw(0..3, 0..1);
        }

        if show_ground && self.ground_visible.get() {
            render_pass.set_pipeline(&self.ground_pipeline);
            render_pass.set_bind_group(3, &self.ground_shadow.bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        render_pass.set_vertex_buffer(0, self.vertex_buffer.as_ref().unwrap().slice(..));

        // Draw backfaces first so they win the depth tie against double-sided materials
        if show_backfaces {
            render_pass.set_index_buffer(
                self.backface_index_buffer.as_ref().unwrap().slice(..),
                wgpu::IndexFormat::Uint32,
            );
            render_pass.set_pipeline(&self.backface_pipeline);
            if self.batches.is_empty() {
                render_pass.set_bind_group(1, &self.default_material.bind_group, &[]);
                render_pass.draw_indexed(0..self.num_backface_indices, 0, 0..1);
            } else {
                for batch in &self.batches {
                    let material = batch
                        .material
                        .and_then(|i| self.materials.get(i))
                        .unwrap_or(&self.default_material);
                    render_pass.set_bind_group(1, &material.bind_group, &[]);
                    render_pass.draw_indexed(
                        batch.first_index..batch.first_index + batch.index_count,
                        0,
                        0..1,
                    );
                }
            }
        }

        render_pass.set_index_buffer(
            self.index_buffer.as_ref().unwrap().slice(..),
            wgpu::IndexFormat::Uint32,
        );

        // Draw solid mesh: opaque/mask materials, then blended ones
        let default_batch = [DrawBatch {
            first_index: 0,
            index_count: self.num_indices,
            material: None,
        }];
        let batches: &[DrawBatch] = if self.batches.is_empty() {
            &default_batch
        } else {
            &self.batches
        };
        for blended in [false, true] {
            for batch in batches {
                let material = batch
                    .material
                    .and_then(|i| self.materials.get(i))
                    .unwrap_or(&self.default_material);
                if (material.alpha_mode == AlphaMode::Blend) != blended {
                    continue;
                }
                render_pass.set_bind_group(1, &material.bind_group, &[]);
                let range = batch.first_index..batch.first_index + batch.index_count;
                for &pass in material_passes(material.alpha_mode, material.double_sided) {
                    render_pass
                        .set_pipeline(&self.solid_pipelines[Self::solid_pipeline_index(pass)]);
                    render_pass.draw_indexed(range.clone(), 0, 0..1);
                }
            }
        }

        // Draw wireframe if enabled
        if show_wireframe {
            render_pass.set_pipeline(&self.wireframe_pipeline);
            render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
        }
    }
}

/// Box-filtered RGBA8 mip chain, level 0 first: (width, height, pixels) per level
fn mip_chain(tex: &crate::mesh::loader::TextureData) -> Vec<(u32, u32, Vec<u8>)> {
    let mut levels = vec![(tex.width, tex.height, tex.pixels.clone())];
    loop {
        let (w, h, src) = levels.last().unwrap();
        let (w, h) = (*w, *h);
        if w == 1 && h == 1 {
            break;
        }
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut dst = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                // Clamp so odd/1-pixel dimensions reuse the edge texel
                let xs = [(2 * x).min(w - 1), (2 * x + 1).min(w - 1)];
                let ys = [(2 * y).min(h - 1), (2 * y + 1).min(h - 1)];
                for c in 0..4 {
                    let mut sum = 0u32;
                    for &sy in &ys {
                        for &sx in &xs {
                            sum += src[((sy * w + sx) * 4 + c) as usize] as u32;
                        }
                    }
                    dst[((y * nw + x) * 4 + c) as usize] = ((sum + 2) / 4) as u8;
                }
            }
        }
        levels.push((nw, nh, dst));
    }
    levels
}

// Add wgpu::util for buffer_init
use wgpu::util::DeviceExt;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::loader::{load_mesh_with_colors, test_fixtures::two_material_glb};

    #[test]
    fn test_material_passes_follow_cull_and_blend_state() {
        let path = two_material_glb("msh_test_material_passes");
        let mesh = load_mesh_with_colors(&path, None).unwrap();
        std::fs::remove_file(&path).ok();
        let [hull, sail] = &mesh.materials.materials[..] else {
            panic!("expected two materials");
        };

        // Opaque single-sided: one back-face-culled, non-blended draw
        assert_eq!(
            material_passes(hull.alpha_mode, hull.double_sided),
            &[PassState {
                cull: Some(wgpu::Face::Back),
                blend: false
            }]
        );
        // Blended double-sided: far side first, then near side, both blended, nothing left culled
        assert_eq!(
            material_passes(sail.alpha_mode, sail.double_sided),
            &[
                PassState {
                    cull: Some(wgpu::Face::Front),
                    blend: true
                },
                PassState {
                    cull: Some(wgpu::Face::Back),
                    blend: true
                },
            ]
        );
    }

    #[test]
    fn test_material_passes_double_sided_opaque_and_mask() {
        let none = [PassState {
            cull: None,
            blend: false,
        }];
        assert_eq!(material_passes(AlphaMode::Opaque, true), &none);
        assert_eq!(material_passes(AlphaMode::Mask, true), &none);
        assert_eq!(
            material_passes(AlphaMode::Mask, false),
            &[PassState {
                cull: Some(wgpu::Face::Back),
                blend: false
            }]
        );
    }

    #[test]
    fn test_viewer_color_only_for_uniform_untextured_materials() {
        let flat = |factor: [f32; 4]| MaterialData {
            base_color_factor: factor,
            ..Default::default()
        };
        let grey = [0.5, 0.5, 0.5, 1.0];
        let red = [1.0, 0.0, 0.0, 1.0];

        assert!(materials_use_viewer_color(&[]));
        assert!(materials_use_viewer_color(&[flat(grey), flat(grey)]));
        assert!(!materials_use_viewer_color(&[flat(grey), flat(red)]));
        let textured = MaterialData {
            base_color_texture: Some(0),
            ..flat(grey)
        };
        assert!(!materials_use_viewer_color(&[textured, flat(grey)]));
    }

    #[test]
    fn test_mip_chain_halves_to_1x1() {
        let tex = crate::mesh::loader::TextureData {
            pixels: [[0u8, 0, 0, 255], [200, 100, 0, 255], [0, 0, 0, 255]].concat(),
            width: 3,
            height: 1,
        };
        let mips = mip_chain(&tex);
        assert_eq!(
            mips.iter().map(|(w, h, _)| (*w, *h)).collect::<Vec<_>>(),
            vec![(3, 1), (1, 1)]
        );
        assert_eq!(mips[1].2, vec![100, 50, 0, 255]);
    }
}
