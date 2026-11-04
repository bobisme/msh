# Research: wgpu Buffer Strategy for Joint Palette Matrices

## Context

Skeletal animation requires uploading a **joint palette** (array of `mat4x4<f32>`,
one per skeleton joint) to the GPU each frame. Typical skeletons have 50-150
joints; complex ones may reach 256.

Each `mat4x4<f32>` is **64 bytes**, 16-byte aligned. In an array they pack
tightly (64 is a multiple of 16, so no inter-element padding).

| Joint count | Palette size |
|-------------|-------------|
| 50          | 3.1 KiB     |
| 150         | 9.4 KiB     |
| 256         | 16.0 KiB    |

---

## maxUniformBufferBindingSize across backends

| Backend / tier           | max_uniform_buffer_binding_size | Source |
|--------------------------|--------------------------------|--------|
| **wgpu default**         | 64 KiB (65536)                 | WebGPU spec / wgpu Limits::default() |
| **wgpu downlevel (WebGL2)** | 16 KiB (16384)              | wgpu Limits::downlevel_webgl2_defaults() |
| **Vulkan min guaranteed**| 16 KiB (maxUniformBufferRange) | Vulkan spec required minimum |
| **Vulkan desktop typical** | 64 KiB - 4 GiB              | Hardware-dependent |
| **Metal**                | 64 KiB (uniform), no hard cap with argument buffers | Apple docs |
| **DX12**                 | 64 KiB per CBV root descriptor | D3D12 spec |
| **WebGPU spec**          | 64 KiB (65536)                 | GPUAdapterLimits |

**Key takeaway**: 64 KiB is the safe cross-platform limit via wgpu defaults.
Even 256 joints (16 KiB) fits comfortably within the Vulkan minimum of 16 KiB.
Since msh targets desktop Vulkan, the effective limit is 64 KiB = 1024 joints,
far more than needed.

---

## Uniform buffer vs storage buffer tradeoffs

| Property | Uniform buffer | Storage buffer |
|----------|---------------|----------------|
| **Max binding size (wgpu default)** | 64 KiB | 128 MiB |
| **Read/write** | Read-only in shader | Read-write |
| **GPU read performance** | Slightly faster (hardware constant cache) | Slightly slower (general memory path) |
| **Alignment (WGSL)** | 16-byte member alignment (similar to std140) | 16-byte member alignment (same rules in WGSL) |
| **WebGL2 support** | Yes | No (max_storage_buffer_binding_size = 0) |
| **Dynamic sizing in shader** | Fixed-size array only | Runtime-sized array (`array<mat4x4<f32>>`) |

### Alignment details

WGSL does **not** use std140/std430 directly. Instead it defines its own layout
rules, which are closest to std430 for storage and std140 for uniform:

- `mat4x4<f32>`: alignment 16, size 64 (columns are `vec4<f32>`, each 16-byte aligned)
- `array<mat4x4<f32>, N>`: element stride = 64 bytes, no padding between elements
- Both uniform and storage buffers use the same alignment for `mat4x4<f32>` arrays

The Rust-side struct needs `#[repr(C)]` with `[[f32; 4]; 4]` per matrix (64 bytes),
which matches the WGSL layout exactly. bytemuck can cast `&[[[f32; 4]; 4]]` directly.

### Performance notes

Uniform buffers are backed by **constant memory / constant cache** on most GPUs,
which is optimized for broadcast reads (all invocations reading the same value).
Joint palette access in a vertex shader is exactly this pattern: each vertex reads
from the same small set of joint matrices. This makes uniform buffers the ideal
choice for joint palettes.

Storage buffers go through the general L1/L2 cache path, which is optimized for
scattered reads. The difference is small (single-digit percent) but uniform
buffers are strictly better for this access pattern.

---

## Recommendation

**Use a uniform buffer** for the joint palette.

Rationale:
1. **Size fits easily**: 256 joints = 16 KiB, well within the 64 KiB wgpu default limit.
   Even combined with the current Uniforms struct (192 bytes), 256 joints only needs 16.2 KiB.
2. **Better performance**: Constant cache is ideal for the broadcast-read pattern
   of skeletal animation vertex shaders.
3. **Simpler**: No need to request storage buffer features or handle WebGL2 fallback.
4. **Max joint count to support**: 256 joints (16 KiB). This covers all practical
   skeleton sizes. If we ever needed more, storage buffer is a straightforward upgrade.

---

## Current msh bind group layout

### Group 0: Mesh uniforms (binding 0)
- **Type**: Uniform buffer, no dynamic offset
- **Visibility**: VERTEX | FRAGMENT
- **Rust struct**: `Uniforms` (192 bytes)
  - `view_proj: mat4x4<f32>` (64 bytes)
  - `model: mat4x4<f32>` (64 bytes)
  - `camera_pos: vec3<f32>` + padding (16 bytes)
  - `shading_mode: u32`, `has_vertex_colors: u32`, `has_texture: u32`, `_pad3: u32` (16 bytes)
  - `base_color: vec4<f32>` (16 bytes)
  - `light_direction: vec3<f32>` + padding (16 bytes)

### Group 1: Texture (bindings 0-1)
- **Binding 0**: `texture_2d<f32>` (FRAGMENT only)
- **Binding 1**: `sampler` (FRAGMENT only)

### Pipeline layout
- `bind_group_layouts: &[group0_uniforms, group1_texture]`
- No push constant ranges

---

## Proposed bind group changes for joint palette

### Option A: Separate bind group (recommended)

Add **Group 2** for the joint palette:

```
Group 0: Uniforms (unchanged)
  binding 0: uniform buffer (Uniforms struct, 192 bytes)

Group 1: Texture (unchanged)
  binding 0: texture_2d<f32>
  binding 1: sampler

Group 2: Joint palette (new)
  binding 0: uniform buffer (array<mat4x4<f32>, 256>, 16384 bytes)
```

**Why a separate group**: The joint palette changes every frame independently of
texture data. Separating it avoids rebuilding the texture bind group. Also, when
no skeleton is present, we can bind a dummy 1-joint identity buffer (same pattern
as the existing 1x1 white texture placeholder).

WGSL additions:
```wgsl
struct JointPalette {
    joints: array<mat4x4<f32>, 256>,  // or smaller fixed size
};

@group(2) @binding(0)
var<uniform> joint_palette: JointPalette;
```

The `Uniforms` struct would also need:
```wgsl
joint_count: u32,  // number of active joints (0 = no skinning)
```

### Option B: Add to existing Group 0

Add a second binding to Group 0:

```
Group 0:
  binding 0: uniform buffer (Uniforms struct)
  binding 1: uniform buffer (joint palette)
```

This is slightly simpler but means the entire Group 0 bind group must be
recreated whenever joint data or uniforms change layout. Option A is cleaner.

### Vertex attributes needed

Skinned vertices will need two additional attributes:
- `joint_indices: vec4<u32>` (4 joint indices per vertex)
- `joint_weights: vec4<f32>` (4 weights per vertex, sum to 1.0)

These go in the vertex buffer, expanding the `Vertex` struct.

---

## wgpu 27 version-specific considerations

- wgpu 27 uses the `wgpu::BufferDescriptor` API with `size` field; no changes
  needed for buffer creation.
- `BindGroupLayoutEntry` with `BufferBindingType::Uniform` works as-is.
- `min_binding_size` can be set to `Some(NonZeroU64::new(16384).unwrap())` for
  the joint palette binding to catch size mismatches at bind group creation time.
- wgpu 28 (released after v27) added mesh shaders but no changes to uniform/storage
  buffer APIs. Our approach is forward-compatible.
- `queue.write_buffer()` for per-frame joint palette updates is the same pattern
  already used for the Uniforms struct; no new API needed.
