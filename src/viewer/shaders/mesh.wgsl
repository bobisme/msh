// Vertex shader

struct Uniforms {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    camera_pos: vec3<f32>,
    _padding: f32,
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
    // Calculate normal from derivatives (per-pixel)
    let dpdx = dpdx(in.world_position);
    let dpdy = dpdy(in.world_position);
    let normal = normalize(cross(dpdy, dpdx));

    // Lighting setup - two lights for better contrast
    let light1_dir = normalize(vec3<f32>(0.5, 1.0, 0.5));  // Main light from above
    let light2_dir = normalize(vec3<f32>(-0.3, -0.5, 0.3)); // Fill light from below
    let view_dir = normalize(uniforms.camera_pos - in.world_position);

    // Diffuse lighting from both lights
    let diffuse1 = max(dot(normal, light1_dir), 0.0);
    let diffuse2 = max(dot(normal, light2_dir), 0.0);

    // Specular lighting (Blinn-Phong) from main light only
    let half_dir = normalize(light1_dir + view_dir);
    let specular = pow(max(dot(normal, half_dir), 0.0), 32.0);

    // Combine lighting components
    let ambient = 0.15;
    let lighting = ambient + diffuse1 * 0.7 + diffuse2 * 0.3 + specular * 0.5;

    // Base color (light gray, slightly warm)
    let base_color = vec3<f32>(0.85, 0.85, 0.85);
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
    let normal = normalize(cross(dpdy, dpdx));

    // Lighting setup - two lights for better contrast
    let light1_dir = normalize(vec3<f32>(0.5, 1.0, 0.5));  // Main light from above
    let light2_dir = normalize(vec3<f32>(-0.3, -0.5, 0.3)); // Fill light from below
    let view_dir = normalize(uniforms.camera_pos - in.world_position);

    // Diffuse lighting from both lights
    let diffuse1 = max(dot(normal, light1_dir), 0.0);
    let diffuse2 = max(dot(normal, light2_dir), 0.0);

    // Specular lighting (Blinn-Phong) from main light only
    let half_dir = normalize(light1_dir + view_dir);
    let specular = pow(max(dot(normal, half_dir), 0.0), 32.0);

    // Combine lighting
    let ambient = 0.15;
    let lighting = ambient + diffuse1 * 0.7 + diffuse2 * 0.3 + specular * 0.5;

    // Red color
    let base_color = vec3<f32>(1.0, 0.0, 0.0);
    let color = base_color * lighting;

    return vec4<f32>(color, 1.0);
}
