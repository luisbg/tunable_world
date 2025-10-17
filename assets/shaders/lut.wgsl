#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var post_source_tex: texture_2d<f32>;
@group(0) @binding(1) var post_source_sampler: sampler;

struct LutParams {
  enabled: u32,
  strength: f32,  // 0..1
  lut_size: f32,
}
@group(0) @binding(2) var<uniform> params: LutParams;

@group(0) @binding(3) var lut_tex: texture_2d<f32>;
@group(0) @binding(4) var lut_sampler: sampler; // linear clamp

// Map slice index -> (col,row) tile; then to pixel coords inside atlas
// Assumes an 8x8 grid
fn lut64_slice_to_px(slice: i32, r: i32, g: i32, tile: i32) -> vec2<i32> {
    let col = slice % 8;
    let row = slice / 8;
    let x   = col * tile + r;
    let y   = row * tile + g;
    return vec2<i32>(x, y);
}


fn sample_lut64_8x8_trilinear(lut: texture_2d<f32>, c_in: vec3<f32>) -> vec3<f32> {
    let n  = 64.0;
    let nf = n - 1.0;
    let c  = clamp(c_in, vec3(0.0), vec3(1.0));

    // Integer texture dims (expect 512×512, but we derive tile size anyway)
    let dims  = vec2<i32>(textureDimensions(lut, 0));
    let tile_x  = max(dims.x / 8, 1);
    let tile_y  = max(dims.y / 8, 1);
    let tile    = min(tile_x, tile_y); // if non-square by accident

    // Blue axis: choose slices b0 and b1, and blend factor bt
    let b  = c.z * nf;
    let b0 = i32(floor(b));
    let b1 = min(b0 + 1, 63);
    let bt = fract(b);

    // R/G in-texel coords (0..63) and their neighbors
    let rx = c.x * nf;
    let gx = c.y * nf;

    let r0 = i32(floor(rx)); let r1 = min(r0 + 1, 63);
    let g0 = i32(floor(gx)); let g1 = min(g0 + 1, 63);
    let rt = fract(rx);      let gt = fract(gx);

    // Bilinear in slice b0
    let p00 = textureLoad(lut, lut64_slice_to_px(b0, r0, g0, tile), 0).rgb;
    let p10 = textureLoad(lut, lut64_slice_to_px(b0, r1, g0, tile), 0).rgb;
    let p01 = textureLoad(lut, lut64_slice_to_px(b0, r0, g1, tile), 0).rgb;
    let p11 = textureLoad(lut, lut64_slice_to_px(b0, r1, g1, tile), 0).rgb;
    let c0  = mix(mix(p00, p10, rt), mix(p01, p11, rt), gt);

    // Bilinear in slice b1
    let q00 = textureLoad(lut, lut64_slice_to_px(b1, r0, g0, tile), 0).rgb;
    let q10 = textureLoad(lut, lut64_slice_to_px(b1, r1, g0, tile), 0).rgb;
    let q01 = textureLoad(lut, lut64_slice_to_px(b1, r0, g1, tile), 0).rgb;
    let q11 = textureLoad(lut, lut64_slice_to_px(b1, r1, g1, tile), 0).rgb;
    let c1  = mix(mix(q00, q10, rt), mix(q01, q11, rt), gt);

    // Lerp across blue
    return mix(c0, c1, bt);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
  let base_src = textureSample(post_source_tex, post_source_sampler, in.uv);

  if (params.enabled == 0u) {
    return base_src;
  }
    
  let remapped = sample_lut64_8x8_trilinear(lut_tex, base_src.rgb);

  let out_rgb = mix(base_src.rgb, remapped, clamp(params.strength, 0.0, 1.0));
  return vec4<f32>(out_rgb, 1.0);
}
