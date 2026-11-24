shader_type canvas_item;
// Height map for the tactical map: elevation tint, hillshade and contour lines,
// all from one height texture (metres in the red channel), so it stays sharp
// at any size.

uniform sampler2D heights : filter_linear, repeat_disable;
uniform float min_height = 0.0;
uniform float max_height = 1.0;
// Size of one height sample, in UV and in metres.
uniform vec2 texel = vec2(0.004);
uniform float cell_m = 2.0;
// Contour interval, and how often a darker "index" contour is drawn.
uniform float contour_m = 5.0;
uniform float index_every = 5.0;
uniform float low_tone = 0.16;
uniform float high_tone = 0.82;
// Light comes from the north-west, as on printed maps.
uniform vec3 light_dir = vec3(-1.0, 1.4, -1.0);

float h_at(vec2 uv) {
	return texture(heights, uv).r;
}

// 1 on a contour line, 0 away from it; about `px` pixels wide on screen.
float contour(float h, float interval, float px) {
	float f = h / interval;
	float d = abs(fract(f - 0.5) - 0.5) / max(fwidth(f), 1e-5);
	return 1.0 - smoothstep(px * 0.5, px * 0.5 + 1.0, d);
}

void fragment() {
	float h = h_at(UV);
	float t = clamp((h - min_height) / max(max_height - min_height, 0.001), 0.0, 1.0);

	// Normal from neighbouring samples (same convention as terrain.gd).
	float hl = h_at(UV - vec2(texel.x, 0.0));
	float hr = h_at(UV + vec2(texel.x, 0.0));
	float hn = h_at(UV - vec2(0.0, texel.y));
	float hs = h_at(UV + vec2(0.0, texel.y));
	vec3 n = normalize(vec3(hl - hr, 2.0 * cell_m, hn - hs));
	float shade = clamp(dot(n, normalize(light_dir)), 0.0, 1.0);

	float tone = mix(low_tone, high_tone, t) * mix(0.55, 1.15, shade);
	float minor = contour(h, contour_m, 1.0);
	float major = contour(h, contour_m * index_every, 1.6);
	tone = mix(tone, tone * 0.62, minor * 0.55);
	tone = mix(tone, 0.08, major * 0.7);
	COLOR = vec4(vec3(tone), 1.0);
}
