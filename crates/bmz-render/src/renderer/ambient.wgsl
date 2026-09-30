@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;
struct Kernel { taps: array<vec4<f32>, 13>, }
@group(0) @binding(2) var<uniform> kernel: Kernel;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) direction: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32, @location(2) direction: vec4<f32>) -> VertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0)
    );
    let uv = corners[index];
    var out: VertexOutput;
    out.position = vec4(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    out.direction = direction.xy;
    return out;
}

// Normalized Gaussian, at most radius 24. CPU-prepared adjacent tap pairs
// avoid duplicated edges from widely spaced sparse samples.
// Blur the premultiplied result of the complete BGA layer stack, including alpha.
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var color = textureSampleLevel(image, image_sampler, input.uv, 0.0) * kernel.taps[0].x;
    for (var i = 1u; i <= u32(kernel.taps[0].y); i += 1u) {
        let offset = input.direction * kernel.taps[i].x;
        color += (textureSampleLevel(image, image_sampler, input.uv + offset, 0.0)
                + textureSampleLevel(image, image_sampler, input.uv - offset, 0.0)) * kernel.taps[i].y;
    }
    return color;
}
