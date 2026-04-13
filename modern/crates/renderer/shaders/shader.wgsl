// 3DMMEx renderer — Blinn-Phong shader
//
// Reproduces BRender's lighting model:
//   ambient + diffuse (Lambertian) + specular (Blinn-Phong)

// ---- Bind group 0: per-frame uniforms ----

struct CameraUniform {
    view_proj: mat4x4<f32>,
    eye_pos:   vec3<f32>,
    _pad:      f32,
};

struct LightUniform {
    direction: vec3<f32>,
    _pad0:     f32,
    color:     vec3<f32>,
    intensity: f32,
    ambient:   vec3<f32>,
    _pad1:     f32,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> light:  LightUniform;

// ---- Bind group 1: per-instance uniforms ----

struct ModelUniform {
    model:       mat4x4<f32>,
    normal_mat:  mat4x4<f32>,  // transpose(inverse(model)) — for normals
};

struct MaterialUniform {
    base_color:     vec4<f32>,
    ambient:        f32,
    diffuse:        f32,
    specular:       f32,
    specular_power: f32,
};

@group(1) @binding(0) var<uniform> model_u:    ModelUniform;
@group(1) @binding(1) var<uniform> material_u: MaterialUniform;

// ---- Vertex stage ----

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal:   vec3<f32>,
    @location(2) uv:       vec2<f32>,
    @location(3) color:    vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos:   vec4<f32>,
    @location(0)       world_pos:  vec3<f32>,
    @location(1)       world_norm: vec3<f32>,
    @location(2)       uv:         vec2<f32>,
    @location(3)       vert_color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let world_pos = model_u.model * vec4<f32>(in.position, 1.0);
    out.clip_pos   = camera.view_proj * world_pos;
    out.world_pos  = world_pos.xyz;
    out.world_norm = normalize((model_u.normal_mat * vec4<f32>(in.normal, 0.0)).xyz);
    out.uv         = in.uv;
    out.vert_color = in.color;

    return out;
}

// ---- Fragment stage ----

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let N = normalize(in.world_norm);
    let L = normalize(-light.direction);          // towards the light
    let V = normalize(camera.eye_pos - in.world_pos);
    let H = normalize(L + V);                     // half-vector

    // Ambient
    let ambient = light.ambient * material_u.ambient;

    // Diffuse (Lambertian)
    let NdotL   = max(dot(N, L), 0.0);
    let diffuse = light.color * light.intensity * material_u.diffuse * NdotL;

    // Specular (Blinn-Phong)
    let NdotH    = max(dot(N, H), 0.0);
    let spec     = pow(NdotH, material_u.specular_power);
    let specular = light.color * light.intensity * material_u.specular * spec;

    let base = material_u.base_color.rgb * in.vert_color.rgb;
    let lit  = base * (ambient + diffuse) + specular;

    return vec4<f32>(lit, material_u.base_color.a * in.vert_color.a);
}
