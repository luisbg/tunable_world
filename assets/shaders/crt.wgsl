#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var texture_sampler: sampler;

struct PostProcessSettings {
	enabled: u32,
	intensity: f32,
	scanline_freq: f32,
	line_intensity: f32,
};
@group(0) @binding(2) var<uniform> settings: PostProcessSettings;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
	let base = textureSample(screen_texture, texture_sampler, in.uv);

	if (settings.enabled == 0u) {
		return base;
	}

	let uv = in.uv;

	let centered_uv = uv * 2.0 - vec2<f32>(1.0);
	let k = settings.intensity;
	let r2 = dot(centered_uv, centered_uv);
	let distorted_uv = uv + centered_uv * r2 * k;

	if (distorted_uv.x < 0.0 || distorted_uv.x > 1.0 || distorted_uv.y < 0.0 || distorted_uv.y > 1.0) {
		return vec4<f32>(0.0, 0.0, 0.0, 1.0);
	}

	let color = textureSample(screen_texture, texture_sampler, distorted_uv).rgb;

	let scanline = 1.0 - settings.line_intensity * sin(distorted_uv.y * settings.scanline_freq * 3.14159);
	let final_color = color * scanline;

	return vec4<f32>(final_color, 1.0);
}
