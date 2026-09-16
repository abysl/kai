#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

struct FoilParams {
    strength: f32,
    frequency: f32,
    uv_drift: f32,
    cell_density: f32,
    spark_strength: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> foil: FoilParams;

fn hue_to_rgb(h: f32) -> vec3<f32> {
    let r = abs(h * 6.0 - 3.0) - 1.0;
    let g = 2.0 - abs(h * 6.0 - 2.0);
    let b = 2.0 - abs(h * 6.0 - 4.0);
    return clamp(vec3(r, g, b), vec3(0.0), vec3(1.0));
}

fn hash2(p: vec2<f32>) -> vec2<f32> {
    let q = vec2(dot(p, vec2(127.1, 311.7)), dot(p, vec2(269.5, 183.3)));
    return fract(sin(q) * 43758.5453);
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);

    let n = normalize(pbr_input.N);
    let v = normalize(pbr_input.V);
    let vt = v - n * dot(n, v);

    let phase = vt.x * foil.frequency * 6.0 + vt.y * foil.frequency * 4.0
        + in.uv.x * foil.uv_drift * 4.0 + in.uv.y * foil.uv_drift * 2.6;
    let rainbow = hue_to_rgb(fract(phase));
    let vis = clamp(length(vt) * 4.0, 0.0, 1.0);
    let sheen = rainbow * foil.strength * (0.45 + 0.55 * vis);

    let view_phase = dot(vt, vec3(3.0, 2.2, 2.6)) * foil.frequency;
    let cell_uv = in.uv * foil.cell_density;
    let base_cell = floor(cell_uv);
    let f = fract(cell_uv);
    var glint = 0.0;
    var glint_hue = 0.0;
    for (var j: i32 = -1; j <= 1; j++) {
        for (var i: i32 = -1; i <= 1; i++) {
            let offset = vec2(f32(i), f32(j));
            let rnd = hash2(base_cell + offset);
            let distance_to_point = length(offset + rnd - f);
            let align = fract(rnd.x + rnd.y + view_phase);
            let window = smoothstep(0.32, 0.02, abs(align - 0.5));
            let spot = smoothstep(0.5, 0.08, distance_to_point);
            let s = spot * window;
            if (s > glint) {
                glint = s;
                glint_hue = fract(rnd.x + align * 0.7);
            }
        }
    }
    let spark = hue_to_rgb(glint_hue) * glint * foil.spark_strength;

    out.color = vec4<f32>(out.color.rgb + sheen + spark, out.color.a);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif

    return out;
}
