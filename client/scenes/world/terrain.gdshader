shader_type spatial;
// Grayscale terrain: base shade from vertex colour, plus a faint world-space
// grid so movement and distance are easy to judge while testing.

uniform float grid_spacing = 10.0;
uniform float grid_strength : hint_range(0.0, 1.0) = 0.25;
uniform float line_width : hint_range(0.001, 0.2) = 0.03;

varying vec3 world_pos;

void vertex() {
	world_pos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
	float shade = COLOR.r;
	if (grid_spacing > 0.0) {
		vec2 cell = world_pos.xz / grid_spacing;
		vec2 dist = abs(fract(cell - 0.5) - 0.5) / fwidth(cell);
		float line = 1.0 - clamp(min(dist.x, dist.y) - line_width * 10.0, 0.0, 1.0);
		shade *= 1.0 - line * grid_strength;
	}
	ALBEDO = vec3(shade);
	ROUGHNESS = 0.95;
}
