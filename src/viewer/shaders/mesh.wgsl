// Vertex shader

struct Uniforms {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    camera_pos: vec3<f32>,
    _padding: f32,
    // Shading parameters
    shading_mode: u32,    // 0=Lit, 1=Flat, 2=Unlit
    has_vertex_colors: u32, // 1=use per-vertex color, 0=use uniform base_color
    _pad2: u32,
    _pad3: u32,
    base_color: vec4<f32>,
    light_direction: vec3<f32>,
    _pad4: f32,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) vertex_color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = uniforms.model * vec4<f32>(in.position, 1.0);
    out.world_position = world_pos.xyz;
    out.clip_position = uniforms.view_proj * world_pos;
    out.vertex_color = in.color;
    return out;
}

// Fragment shader

struct FragmentInput {
    @location(0) world_position: vec3<f32>,
    @location(1) vertex_color: vec4<f32>,
};

// Resolve base color: use per-vertex color when available, otherwise uniform
fn resolve_base_color(vertex_color: vec4<f32>) -> vec4<f32> {
    if uniforms.has_vertex_colors == 1u {
        return vertex_color;
    }
    return uniforms.base_color;
}

@fragment
fn fs_main(in: FragmentInput) -> @location(0) vec4<f32> {
    let color = resolve_base_color(in.vertex_color);
    let base = color.rgb;

    // Unlit: just output base color
    if uniforms.shading_mode == 2u {
        return vec4<f32>(base, color.a);
    }

    // Calculate normal from derivatives (per-pixel)
    let dpdx = dpdx(in.world_position);
    let dpdy = dpdy(in.world_position);
    let normal = normalize(cross(dpdy, dpdx));

    let light_dir = normalize(uniforms.light_direction);

    // Flat: single directional light, no specular
    if uniforms.shading_mode == 1u {
        let diffuse = max(dot(normal, light_dir), 0.0);
        let ambient = 0.15;
        let lighting = ambient + diffuse * 0.85;
        return vec4<f32>(base * lighting, color.a);
    }

    // Lit: two lights + specular (original behavior)
    let light2_dir = normalize(vec3<f32>(-0.3, -0.5, 0.3));
    let view_dir = normalize(uniforms.camera_pos - in.world_position);

    let diffuse1 = max(dot(normal, light_dir), 0.0);
    let diffuse2 = max(dot(normal, light2_dir), 0.0);

    let half_dir = normalize(light_dir + view_dir);
    let specular = pow(max(dot(normal, half_dir), 0.0), 32.0);

    let ambient = 0.15;
    let lighting = ambient + diffuse1 * 0.7 + diffuse2 * 0.3 + specular * 0.5;

    let lit_color = base * lighting;
    return vec4<f32>(lit_color, color.a);
}

// Fragment shader for wireframe (black lines)
@fragment
fn fs_wireframe(in: FragmentInput) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}

// Fragment shader for backfaces (red) - uses same shading logic with red base
@fragment
fn fs_backface(in: FragmentInput) -> @location(0) vec4<f32> {
    let base = vec3<f32>(1.0, 0.0, 0.0);

    let dpdx = dpdx(in.world_position);
    let dpdy = dpdy(in.world_position);
    let normal = normalize(cross(dpdy, dpdx));

    let light_dir = normalize(uniforms.light_direction);
    let light2_dir = normalize(vec3<f32>(-0.3, -0.5, 0.3));
    let view_dir = normalize(uniforms.camera_pos - in.world_position);

    let diffuse1 = max(dot(normal, light_dir), 0.0);
    let diffuse2 = max(dot(normal, light2_dir), 0.0);

    let half_dir = normalize(light_dir + view_dir);
    let specular = pow(max(dot(normal, half_dir), 0.0), 32.0);

    let ambient = 0.15;
    let lighting = ambient + diffuse1 * 0.7 + diffuse2 * 0.3 + specular * 0.5;

    let color = base * lighting;
    return vec4<f32>(color, 1.0);
}
