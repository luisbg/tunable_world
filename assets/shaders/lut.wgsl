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

// Compute UV into a 256x16 LUT arranged as 16 tiles (slices) across, each 16x16.
// `slice` is 0..15; `r`,`g` are already scaled to 0..15.
// tex_dims is textureDimensions(lut, 0) as vec2<f32>.
fn lut16_uv(slice: f32, r: f32, g: f32, tex_dims: vec2<f32>) -> vec2<f32> {
    let tile     = tex_dims.y;              // 16
    let half_px  = vec2(0.5) / tex_dims;
    let px       = (slice * tile + r) + 0.5;
    let py       = g + 0.5;
    let uv       = vec2(px, py) / tex_dims;
    return clamp(uv, half_px, vec2(1.0) - half_px);
}

fn sample_lut16x(lut: texture_2d<f32>, samp: sampler, c: vec3<f32>) -> vec3<f32> {
    let n  = 16.0;
    let nf = n - 1.0;

    // Clamp domain to [0,1]
    let cin = clamp(c, vec3(0.0), vec3(1.0));
    
    let b  = cin.z * nf;    
    let b0 = floor(b);
    let b1 = min(b0 + 1.0, nf);
    let bt = fract(b);

    // In-slice coordinates (0..15)
    let rx = cin.x * nf;
    let gy = cin.y * nf;

    // tile size in pixels is the texture height (16)
    let dims = vec2<f32>(textureDimensions(lut, 0)); // (256, 16)

    let uv0 = lut16_uv(b0, rx, gy, dims);
    let uv1 = lut16_uv(b1, rx, gy, dims);

    let c0 = textureSampleLevel(lut, samp, uv0, 0.0).rgb;
    let c1 = textureSampleLevel(lut, samp, uv1, 0.0).rgb;

    return mix(c0, c1, bt);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
  let base_src = textureSample(post_source_tex, post_source_sampler, in.uv).rgb;

  if (params.enabled == 0u) {
    return base;
  }
    
  let remapped = sample_lut16x(lut_tex, lut_sampler, base_src);

  let out_rgb = mix(base_src, remapped, clamp(params.strength, 0.0, 1.0));
  return vec4<f32>(out_rgb, 1.0);
}
