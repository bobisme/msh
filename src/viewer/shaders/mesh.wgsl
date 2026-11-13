// Vertex shader

struct Uniforms {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    camera_pos: vec3<f32>,
    override_base_color: u32,
    // Shading parameters
    shading_mode: u32,      // 0=Lit, 1=Flat, 2=Unlit
    has_vertex_colors: u32, // 1=use per-vertex color, 0=use uniform base_color
    viewport_aspect: f32,   // width / height
    joint_count: u32,       // number of active joints (0 = no skinning)
    base_color: vec4<f32>,
    light_direction: vec3<f32>,
    backface_unlit: u32,
    background_center: vec4<f32>, // linear RGBA at the middle of the frame
    background_edge: vec4<f32>,   // linear RGBA at the corners
    ground_plane: vec4<f32>,     // world center x, height, center z, radius
    shadow_view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct Material {
    base_color_factor: vec4<f32>,
    has_base_color_texture: u32,
    has_normal_texture: u32,
    has_metallic_roughness_texture: u32,
    has_occlusion_texture: u32,
    alpha_mode: u32,       // 0=Opaque, 1=Mask, 2=Blend
    use_viewer_color: u32, // 1=vertex color / uniforms.base_color instead of base_color_factor
    alpha_cutoff: f32,
    metallic_factor: f32,
    roughness_factor: f32,
    normal_scale: f32,
    occlusion_strength: f32,
    material_id: u32,
    emissive_factor: vec4<f32>, // RGB factor times strength; W indicates a texture
    uv_transforms: array<vec4<f32>,10>,
};

@group(1) @binding(0)
var<uniform> material: Material;
@group(1) @binding(1)
var base_color_texture: texture_2d<f32>;
@group(1) @binding(2)
var normal_texture: texture_2d<f32>;
@group(1) @binding(3)
var metallic_roughness_texture: texture_2d<f32>;
@group(1) @binding(4)
var occlusion_texture: texture_2d<f32>;
@group(1) @binding(5)
var material_sampler: sampler;
@group(1) @binding(6)
var emissive_texture: texture_2d<f32>;
@group(1) @binding(7)
var environment_texture: texture_2d<f32>;
@group(1) @binding(8)
var environment_sampler: sampler;
@group(1) @binding(9)
var environment_diffuse: texture_2d<f32>;

struct JointPalette {
    joints: array<mat4x4<f32>, 256>,
};
@group(2) @binding(0)
var<uniform> joint_palette: JointPalette;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) texcoord: vec2<f32>,
    @location(3) joint_indices: vec4<u32>,
    @location(4) joint_weights: vec4<f32>,
    @location(5) normal: vec3<f32>,
    @location(6) tangent: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) vertex_color: vec4<f32>,
    @location(2) texcoord: vec2<f32>,
    @location(3) normal: vec3<f32>,
    @location(4) tangent: vec4<f32>,
};

fn mat3_of(m: mat4x4<f32>) -> mat3x3<f32> {
    return mat3x3<f32>(m[0].xyz, m[1].xyz, m[2].xyz);
}

fn mesh_vertex(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Apply GPU skinning when joints are present
    var local_pos: vec4<f32>;
    var local_normal = in.normal;
    var local_tangent = in.tangent.xyz;
    if uniforms.joint_count > 0u {
        var skinned_pos = vec4<f32>(0.0, 0.0, 0.0, 0.0);
        var skinned_normal = vec3<f32>(0.0);
        var skinned_tangent = vec3<f32>(0.0);
        for (var i = 0u; i < 4u; i = i + 1u) {
            let weight = in.joint_weights[i];
            if weight > 0.0 {
                let joint_mat = joint_palette.joints[in.joint_indices[i]];
                skinned_pos = skinned_pos + weight * (joint_mat * vec4<f32>(in.position, 1.0));
                skinned_normal = skinned_normal + weight * (mat3_of(joint_mat) * in.normal);
                skinned_tangent = skinned_tangent + weight * (mat3_of(joint_mat) * in.tangent.xyz);
            }
        }
        // If no skinning weights applied (all weights zero), use original position
        if skinned_pos.w == 0.0 {
            skinned_pos = vec4<f32>(in.position, 1.0);
        } else {
            local_normal = skinned_normal;
            local_tangent = skinned_tangent;
        }
        local_pos = skinned_pos;
    } else {
        local_pos = vec4<f32>(in.position, 1.0);
    }

    let world_pos = uniforms.model * local_pos;
    out.world_position = world_pos.xyz;
    out.clip_position = uniforms.view_proj * world_pos;
    out.vertex_color = in.color;
    out.texcoord = in.texcoord;
    // Model matrix is rotation * uniform scale, so it transforms normals directly.
    // Zero normals/tangents stay zero and signal "absent" to the fragment shader.
    out.normal = mat3_of(uniforms.model) * local_normal;
    out.tangent = vec4<f32>(mat3_of(uniforms.model) * local_tangent, in.tangent.w);
    return out;
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    return mesh_vertex(in);
}

@vertex
fn vs_shadow(in: VertexInput) -> VertexOutput {
    var out = mesh_vertex(in);
    out.clip_position = uniforms.shadow_view_proj * vec4<f32>(out.world_position, 1.0);
    return out;
}

// Fragment shader

struct FragmentInput {
    @builtin(front_facing) front_facing: bool,
    @location(0) world_position: vec3<f32>,
    @location(1) vertex_color: vec4<f32>,
    @location(2) texcoord: vec2<f32>,
    @location(3) normal: vec3<f32>,
    @location(4) tangent: vec4<f32>,
};

fn material_uv(uv: vec2<f32>, slot: u32) -> vec2<f32> {
    let p=vec3<f32>(uv,1.0);
    return vec2<f32>(dot(material.uv_transforms[slot*2u].xyz,p),dot(material.uv_transforms[slot*2u+1u].xyz,p));
}

// Resolve base color: material factor (or vertex color / viewer color) * baseColorTexture
fn resolve_base_color(vertex_color: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    var color = material.base_color_factor;
    if material.use_viewer_color == 1u || (material.use_viewer_color == 2u && uniforms.override_base_color == 1u) {
        let authored_alpha = color.a;
        if uniforms.has_vertex_colors == 1u {
            color = vertex_color;
        } else {
            color = uniforms.base_color;
        }
        color.a *= authored_alpha;
    }
    if material.has_base_color_texture == 1u {
        color = color * textureSample(base_color_texture, material_sampler, material_uv(uv,0u));
    }
    return color;
}

// Face normal from screen-space derivatives; always faces the camera
fn face_normal(world_position: vec3<f32>) -> vec3<f32> {
    return normalize(cross(dpdy(world_position), dpdx(world_position)));
}

// Shading normal: interpolated vertex normal (+ normal map) when available, else face normal
fn shading_normal(in: FragmentInput) -> vec3<f32> {
    if dot(in.normal, in.normal) < 1e-12 {
        return face_normal(in.world_position);
    }
    var n = normalize(in.normal);
    if material.has_normal_texture == 1u {
        let uv = material_uv(in.texcoord, 1u);
        var tangent = in.tangent.xyz;
        var tangent_sign = in.tangent.w;
        // glTF permits normals and UVs without authored tangents. Reconstruct a
        // local frame from screen derivatives for those meshes.
        if dot(tangent, tangent) < 1e-12 {
            let dp_x = dpdx(in.world_position);
            let dp_y = dpdy(in.world_position);
            let duv_x = dpdx(uv);
            let duv_y = dpdy(uv);
            let determinant = duv_x.x * duv_y.y - duv_x.y * duv_y.x;
            if abs(determinant) > 1e-10 {
                tangent = (dp_x * duv_y.y - dp_y * duv_x.y) / determinant;
                let bitangent = (dp_y * duv_x.x - dp_x * duv_y.x) / determinant;
                tangent_sign = sign(dot(cross(n, tangent), bitangent));
            }
        }
        if dot(tangent, tangent) > 1e-12 {
            let t = normalize(tangent - n * dot(n, tangent));
            let b = cross(n, t) * tangent_sign;
            var tn = textureSample(normal_texture, material_sampler, uv).xyz * 2.0 - 1.0;
            tn = vec3<f32>(tn.xy * material.normal_scale, tn.z);
            n = normalize(t * tn.x + b * tn.y + n * tn.z);
        }
    }
    // Back faces are only rasterized for double-sided materials; light them from their own side
    if !in.front_facing {
        n = -n;
    }
    return n;
}

// Output alpha: opaque surfaces are fully opaque, masked ones are cut out
fn resolve_alpha(alpha: f32) -> f32 {
    if material.alpha_mode == 0u {
        return 1.0;
    }
    if material.alpha_mode == 1u {
        if alpha < material.alpha_cutoff {
            discard;
        }
        return 1.0;
    }
    return alpha;
}

const PI: f32 = 3.141592653589793;

fn fresnel_schlick(f0: vec3<f32>, v_dot_h: f32) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - clamp(v_dot_h, 0.0, 1.0), 5.0);
}

fn smith_visibility(n_dot_v: f32, n_dot_l: f32, alpha: f32) -> f32 {
    let a2 = alpha * alpha;
    let gv = n_dot_l * sqrt(n_dot_v * n_dot_v * (1.0 - a2) + a2);
    let gl = n_dot_v * sqrt(n_dot_l * n_dot_l * (1.0 - a2) + a2);
    return 0.5 / max(gv + gl, 1e-6);
}

fn direct_brdf(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, diffuse: vec3<f32>, f0: vec3<f32>, roughness: f32) -> vec3<f32> {
    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_v = max(dot(n, v), 1e-4);
    if n_dot_l <= 0.0 { return vec3<f32>(0.0); }
    let h = normalize(v + l);
    let n_dot_h = max(dot(n, h), 0.0);
    let v_dot_h = max(dot(v, h), 0.0);
    let alpha = roughness * roughness;
    let a2 = alpha * alpha;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    let distribution = a2 / max(PI * d * d, 1e-7);
    let f = fresnel_schlick(f0, v_dot_h);
    let specular = f * distribution * smith_visibility(n_dot_v, n_dot_l, alpha);
    return ((vec3<f32>(1.0) - f) * diffuse / PI + specular) * n_dot_l;
}

// Pre-filtered equirectangular environment and split-sum GGX approximation.
fn environment_uv(dir: vec3<f32>) -> vec2<f32> {
    return vec2<f32>(atan2(dir.x, dir.z) / (2.0 * PI) + 0.5,
                     acos(clamp(dir.y, -1.0, 1.0)) / PI);
}

fn environment_brdf(n: vec3<f32>, v: vec3<f32>, diffuse: vec3<f32>, f0: vec3<f32>, roughness: f32) -> vec3<f32> {
    let n_dot_v = max(dot(n, v), 1e-4);
    let reflected = reflect(-v, n);
    let irradiance = textureSampleLevel(environment_diffuse, environment_sampler, environment_uv(n), 0.0).rgb;
    let radiance = textureSampleLevel(environment_texture, environment_sampler, environment_uv(reflected), roughness * 7.0).rgb;
    // Analytic fit to the integrated GGX/Fresnel term of the split-sum BRDF.
    let r = roughness * vec4<f32>(-1.0, -0.0275, -0.572, 0.022) + vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let a004 = min(r.x * r.x, exp2(-9.28 * n_dot_v)) * r.x + r.y;
    let ab = vec2<f32>(-1.04, 1.04) * a004 + r.zw;
    let specular = radiance * (f0 * ab.x + ab.y);
    let fresnel = fresnel_schlick(f0, n_dot_v);
    return (vec3<f32>(1.0) - fresnel) * diffuse * irradiance + specular;
}

@fragment
fn fs_main(in: FragmentInput) -> @location(0) vec4<f32> {
    let color = resolve_base_color(in.vertex_color, in.texcoord);
    let alpha = resolve_alpha(color.a);
    let base = color.rgb;

    // Unlit: just output base color
    if uniforms.shading_mode == 2u {
        return vec4<f32>(base, alpha);
    }

    let light_dir = normalize(uniforms.light_direction);

    // Flat: single directional light, no specular
    if uniforms.shading_mode == 1u {
        let normal = face_normal(in.world_position);
        let diffuse = max(dot(normal, light_dir), 0.0);
        let ambient = 0.15;
        let lighting = ambient + diffuse * 0.85;
        return vec4<f32>(base * lighting, alpha);
    }

    // glTF metallic-roughness: GGX distribution, correlated Smith visibility,
    // Schlick Fresnel, and an energy-conserving Lambertian diffuse lobe.
    let normal = shading_normal(in);
    let view_dir = normalize(uniforms.camera_pos - in.world_position);
    var metallic = material.metallic_factor;
    var roughness = material.roughness_factor;
    if material.has_metallic_roughness_texture == 1u {
        let mr = textureSample(metallic_roughness_texture, material_sampler, material_uv(in.texcoord, 2u));
        roughness *= mr.g;
        metallic *= mr.b;
    }
    metallic = clamp(metallic, 0.0, 1.0);
    roughness = clamp(roughness, 0.045, 1.0);
    var occlusion = 1.0;
    if material.has_occlusion_texture == 1u {
        let ao = textureSample(occlusion_texture, material_sampler, material_uv(in.texcoord, 3u)).r;
        occlusion = mix(1.0, ao, material.occlusion_strength);
    }
    let f0 = mix(vec3<f32>(0.04), base, metallic);
    let diffuse_color = base * (1.0 - metallic);
    let key = direct_brdf(normal, view_dir, light_dir, diffuse_color, f0, roughness) * 2.0;
    let fill_dir = normalize(vec3<f32>(-0.3, -0.5, 0.3));
    let fill = direct_brdf(normal, view_dir, fill_dir, diffuse_color, f0, roughness) * 0.5;
    let indirect = environment_brdf(normal, view_dir, diffuse_color, f0, roughness) * occlusion;
    var emissive = material.emissive_factor.rgb;
    if material.emissive_factor.w > 0.5 {
        emissive *= textureSample(emissive_texture, material_sampler, material_uv(in.texcoord, 4u)).rgb;
    }
    let radiance = key + fill + indirect + emissive;
    // Reinhard mapping handles radiance above display white while remaining monotonic.
    let lit_color = radiance / (vec3<f32>(1.0) + radiance);
    return vec4<f32>(lit_color, alpha);
}

// Integer visibility diagnostics are independent of display color and lighting.
@fragment
fn fs_ids(in: FragmentInput) -> @location(0) u32 {
    let color = resolve_base_color(in.vertex_color, in.texcoord);
    let alpha = resolve_alpha(color.a);
    if material.alpha_mode == 2u && alpha < 0.5 { discard; }
    return material.material_id | select(0u, 0x80000000u, !in.front_facing);
}

// Fragment shader for wireframe (black lines)
@fragment
fn fs_wireframe(in: FragmentInput) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}

// Fragment shader for backfaces (red) - uses same shading logic with red base
@fragment
fn fs_backface(in: FragmentInput) -> @location(0) vec4<f32> {
    let alpha = resolve_alpha(resolve_base_color(in.vertex_color, in.texcoord).a);
    if alpha <= 0.0 { discard; }
    let base = vec3<f32>(1.0, 0.0, 0.0);
    if uniforms.backface_unlit == 1u { return vec4<f32>(base, 1.0); }

    let normal = face_normal(in.world_position);

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

// Background: fullscreen radial gradient

struct BackgroundOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_background(@builtin(vertex_index) index: u32) -> BackgroundOutput {
    // One triangle covering the screen: (-1,-1), (3,-1), (-1,3)
    let xy = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u)) * 2.0 - 1.0;
    var out: BackgroundOutput;
    out.clip_position = vec4<f32>(xy, 0.0, 1.0);
    out.ndc = xy;
    return out;
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055, c * 12.92, c <= vec3<f32>(0.0031308));
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_background(in: BackgroundOutput) -> @location(0) vec4<f32> {
    // Distance from center with square pixels, normalized so the corners are 1
    let p = vec2<f32>(in.ndc.x * uniforms.viewport_aspect, in.ndc.y);
    let corner = length(vec2<f32>(uniforms.viewport_aspect, 1.0));
    // Broad, soft falloff from the center color to the edge color at the corners
    let t = smoothstep(0.0, 1.0, length(p) / corner);

    // Interpolate in sRGB so the falloff looks even, and dither by +-0.5 of an 8-bit step
    // so the dark ramp doesn't band
    let center = linear_to_srgb(uniforms.background_center.rgb);
    let edge = linear_to_srgb(uniforms.background_edge.rgb);
    let noise = fract(52.9829189 * fract(dot(in.clip_position.xy, vec2<f32>(0.06711056, 0.00583715)))) - 0.5;
    let srgb = clamp(mix(center, edge, t) + noise / 255.0, vec3<f32>(0.0), vec3<f32>(1.0));
    let alpha = mix(uniforms.background_center.a, uniforms.background_edge.a, t);
    return vec4<f32>(srgb_to_linear(srgb), alpha);
}

// Invisible, upward-facing shadow catcher. Only occluded pixels darken the background.
@group(3) @binding(0) var shadow_map: texture_depth_2d;
@group(3) @binding(1) var shadow_sampler: sampler_comparison;

struct GroundOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) offset: vec2<f32>,
};

@vertex
fn vs_ground(@builtin(vertex_index) index: u32) -> GroundOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(1.0, -1.0),
    );
    let offset = corners[index] * uniforms.ground_plane.w;
    let world = vec3<f32>(
        uniforms.ground_plane.x + offset.x,
        uniforms.ground_plane.y,
        uniforms.ground_plane.z + offset.y,
    );
    var out: GroundOutput;
    out.clip_position = uniforms.view_proj * vec4<f32>(world, 1.0);
    out.offset = offset;
    return out;
}

// Preserve material cutouts in the silhouette. Ordered coverage approximates
// translucent casters without treating fully transparent triangles as solid.
@fragment
fn fs_shadow(in: FragmentInput, @builtin(position) pixel: vec4<f32>) {
    let color = resolve_base_color(in.vertex_color, in.texcoord);
    if material.alpha_mode == 1u && color.a < material.alpha_cutoff {
        discard;
    }
    if material.alpha_mode == 2u {
        let bayer = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
        let cell = vec2<u32>(pixel.xy) % vec2<u32>(4u);
        if color.a <= (bayer[cell.y * 4u + cell.x] + 0.5) / 16.0 {
            discard;
        }
    }
}

@fragment
fn fs_ground(in: GroundOutput) -> @location(0) vec4<f32> {
    let world = vec3<f32>(uniforms.ground_plane.x + in.offset.x, uniforms.ground_plane.y, uniforms.ground_plane.z + in.offset.y);
    let light_clip = uniforms.shadow_view_proj * vec4<f32>(world, 1.0);
    let projected = light_clip.xyz / light_clip.w;
    let uv = projected.xy * vec2<f32>(0.5, -0.5) + 0.5;
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || projected.z < 0.0 || projected.z > 1.0 {
        return vec4<f32>(0.0);
    }

    // Filter a fixed-size depth map for a soft silhouette. Explicit-level comparison
    // sampling keeps this valid across the receiver's nonuniform silhouette edges.
    let texel = 1.0 / vec2<f32>(textureDimensions(shadow_map));
    var visibility = 0.0;
    for (var y = -2; y <= 2; y += 1) {
        for (var x = -2; x <= 2; x += 1) {
            visibility += textureSampleCompareLevel(shadow_map, shadow_sampler,
                uv + vec2<f32>(f32(x), f32(y)) * texel * 6.0, projected.z - 0.00005);
        }
    }
    let shadow = 1.0 - visibility / 25.0;
    let radius = length(in.offset) / uniforms.ground_plane.w;
    let fade = 1.0 - smoothstep(0.6, 1.0, radius);
    let to_camera = uniforms.camera_pos - world;
    let grazing = smoothstep(0.0, 0.12, to_camera.y / max(length(to_camera), 0.00001));
    let light_fade = smoothstep(0.1, 0.25, normalize(uniforms.light_direction).y);
    return vec4<f32>(0.0, 0.0, 0.0, shadow * 0.5 * fade * grazing * light_fade);
}
