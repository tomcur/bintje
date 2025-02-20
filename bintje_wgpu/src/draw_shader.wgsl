// This shader takes the wide tile commands (and their positions) as vertex
// instance data. The vertex buffer steps per index.

struct Instance {
    @location(0) x: u32,
    @location(1) y: u32,
    @location(2) width: u32,
    @location(3) alpha_idx: u32,
    @location(4) color: u32,
}

struct VertexOutput {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) alpha_idx: u32,
    @location(2) x: u32,
    @location(3) y: u32,
}

@vertex
fn vs(
    @builtin(vertex_index) idx: u32,
    instance: Instance,
) -> VertexOutput {
    let x0 = -1 + 2 * f32(instance.x) / 256;
    let x1 = -1 + 2 * f32(instance.x + instance.width) / 256;
    let y0 = 1 - 2 * f32(instance.y) / 256;
    let y1 = 1 - 2 * f32(instance.y + 4) / 256;
    let vertex = array(
        vec2(x0, y0),
        vec2(x1, y0),
        vec2(x0, y1),
        vec2(x1, y1),
    );

    var output: VertexOutput;
    output.pos = vec4(vertex[idx], 0.0, 1.0);
    output.color = unpack4x8unorm(instance.color);
    output.alpha_idx = instance.alpha_idx;
    output.x = instance.x;
    output.y = instance.y;
    return output;
}

@group(0) @binding(0)
var<uniform> alpha_masks: array<vec4<u32>, 1024>;

struct FragOut {
    @location(0) color: vec4<f32>,
}

@fragment
fn fs(in: VertexOutput) -> FragOut {
    let in_x = floor(in.pos.x);
    let in_y = floor(in.pos.y);

    var output: FragOut;
    let alpha_idx = in.alpha_idx + (u32(in_x) - in.x) / 4;
    let alpha_mask = unpack4x8unorm(alpha_masks[alpha_idx][u32(in_x) % 4]);
    output.color = (f32(in.alpha_idx == 0xffff) + f32(in.alpha_idx != 0xffff) * alpha_mask[u32(in_y) - in.y]) * in.color;
    return output;
}
