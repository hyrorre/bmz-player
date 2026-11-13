@group(0) @binding(0) var y_plane: texture_2d<f32>;
@group(0) @binding(1) var u_plane: texture_2d<f32>;
@group(0) @binding(2) var v_plane: texture_2d<f32>;
@group(0) @binding(3) var rgb_bytes: texture_2d<u32>;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4(positions[index], 0.0, 1.0);
}

@fragment
fn yuv420(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    let chroma = pixel / 2;
    let y = (textureLoad(y_plane, pixel, 0).r * 255.0 - 16.0) / 219.0;
    let u = (textureLoad(u_plane, chroma, 0).r * 255.0 - 128.0) / 224.0;
    let v = (textureLoad(v_plane, chroma, 0).r * 255.0 - 128.0) / 224.0;
    let rgb = vec3(y + 1.402 * v, y - 0.344136 * u - 0.714136 * v, y + 1.772 * u);
    return vec4(clamp(rgb, vec3(0.0), vec3(1.0)), 1.0);
}

@fragment
fn rgb24(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    let byte = pixel.x * 3;
    let texel = vec2(byte / 4, pixel.y);
    let a = textureLoad(rgb_bytes, texel, 0);
    let offset = byte % 4;
    var rgb: vec3<u32>;
    if offset <= 1 {
        rgb = vec3(a[offset], a[offset + 1], a[offset + 2]);
    } else {
        let b = textureLoad(rgb_bytes, texel + vec2(1, 0), 0);
        if offset == 2 {
            rgb = vec3(a.z, a.w, b.x);
        } else {
            rgb = vec3(a.w, b.x, b.y);
        }
    }
    return vec4(vec3<f32>(rgb) / 255.0, 1.0);
}
