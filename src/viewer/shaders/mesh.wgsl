// Vertex shader

struct Uniforms {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = uniforms.model * vec4<f32>(in.position, 1.0);
    out.world_position = world_pos.xyz;
    out.clip_position = uniforms.view_proj * world_pos;
    return out;
}

// Fragment shader

struct FragmentInput {
    @location(0) world_position: vec3<f32>,
};

@fragment
fn fs_main(in: FragmentInput) -> @location(0) vec4<f32> {
    // Simple lighting calculation
    let light_dir = normalize(vec3<f32>(0.5, 1.0, 0.5));

    // Calculate normal from derivatives (per-pixel)
    let dpdx = dpdx(in.world_position);
    let dpdy = dpdy(in.world_position);
    let normal = normalize(cross(dpdx, dpdy));

    // Diffuse lighting
    let diffuse = max(dot(normal, light_dir), 0.0);
    let ambient = 0.3;
    let lighting = ambient + diffuse * 0.7;

    // Base color (gray)
    let base_color = vec3<f32>(0.8, 0.8, 0.8);
    let color = base_color * lighting;

    return vec4<f32>(color, 1.0);
}

// Fragment shader for wireframe (black lines)
@fragment
fn fs_wireframe(in: FragmentInput) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}

// Fragment shader for backfaces (red)
@fragment
fn fs_backface(in: FragmentInput) -> @location(0) vec4<f32> {
    // Calculate normal from derivatives
    let dpdx = dpdx(in.world_position);
    let dpdy = dpdy(in.world_position);
    let normal = normalize(cross(dpdx, dpdy));

    // Simple lighting
    let light_dir = normalize(vec3<f32>(0.5, 1.0, 0.5));
    let diffuse = max(dot(normal, light_dir), 0.0);
    let ambient = 0.3;
    let lighting = ambient + diffuse * 0.7;

    // Red color
    let base_color = vec3<f32>(1.0, 0.0, 0.0);
    let color = base_color * lighting;

    return vec4<f32>(color, 1.0);
}
