@group(0) @binding(0) var y_plane: texture_2d<f32>;
@group(0) @binding(1) var u_plane: texture_2d<f32>;
@group(0) @binding(2) var v_plane: texture_2d<f32>;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4(positions[index], 0.0, 1.0);
}

@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    let chroma = pixel / 2;
    let y = (textureLoad(y_plane, pixel, 0).r * 255.0 - 16.0) / 219.0;
    let u = (textureLoad(u_plane, chroma, 0).r * 255.0 - 128.0) / 224.0;
    let v = (textureLoad(v_plane, chroma, 0).r * 255.0 - 128.0) / 224.0;
    let rgb = vec3(y + 1.402 * v, y - 0.344136 * u - 0.714136 * v, y + 1.772 * u);
    return vec4(clamp(rgb, vec3(0.0), vec3(1.0)), 1.0);
}
