@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;

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

// Normalized Gaussian (sigma=4, radius=12). Each bilinear fetch combines two
// adjacent taps, avoiding duplicated edges from widely spaced sparse samples.
// Blur the premultiplied result of the complete BGA layer stack, including alpha.
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let offsets = array<f32, 6>(1.4765796511, 3.4455295350, 5.4148988458, 7.3849121445, 9.3557748935, 11.3276683008);
    let weights = array<f32, 6>(0.1850033176, 0.1360122676, 0.0781768744, 0.0351278237, 0.0123383268, 0.0033872108);
    var color = textureSample(image, image_sampler, input.uv) * 0.0999083581;
    for (var i = 0u; i < 6u; i += 1u) {
        let offset = input.direction * offsets[i];
        color += (textureSample(image, image_sampler, input.uv + offset)
                + textureSample(image, image_sampler, input.uv - offset)) * weights[i];
    }
    return color;
}
