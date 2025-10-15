#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var texture_sampler: sampler;

struct GradientParams {
    enabled: u32,      // 0 = off, 1 = on
    additive: u32,     // 0 = off, 1 = on
    strength: f32,     // 0..1, blend toward tint

    // Colors as RGBA; alpha is ignored but keeps alignment simple.
    color_top_right: vec4<f32>,
    color_bottom_left: vec4<f32>,
};
@group(0) @binding(2) var<uniform> grad: GradientParams;

// Helper: clamp to [0,1]
fn saturate(x: f32) -> f32 {
    return clamp(x, 0.0, 1.0);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
   let base = textureSample(screen_texture, texture_sampler, in.uv);

   if (grad.enabled == 0u) {
       return base;
   }

   // --- Diagonal gradient factor -----------------------------------------
   // We want: t = 0 at bottom-left (0,1), t = 1 at top-right (1,0).
   // Using f = uv.x - uv.y:
   //   f(1,0) = 1, f(0,1) = -1  → map to [0,1] via (f + 1)/2.
   let f = in.uv.x - in.uv.y;
   let t = saturate(0.5 * (f + 1.0));

   // Interpolate between bottom-left and top-right colors
   let tint = mix(grad.color_bottom_left.rgb, grad.color_top_right.rgb, t);

   // --- Apply tint ---------------------------------------------------------
   if (grad.additive == 0u ) {
      // Multiplicative tint feels like a "filter" over the scene.
      // Blend toward (base * tint) by `strength`.
      let tinted = base.rgb * tint;
      let out_rgb = mix(base.rgb, tinted, saturate(grad.strength));

      return vec4<f32>(out_rgb, base.a);
   } else {
       // --- Additive tint ---------------------------------------------------
       // Add tint and clamp to [0,1] to prevent overbright results
       let tinted = clamp(base.rgb + tint * grad.strength, vec3<f32>(0.0), vec3<f32>(1.0));

       return vec4<f32>(tinted, base.a);
   }
}
