class_name Compass
extends Control
## A compass strip: headings slide past a fixed centre marker as you turn.
## A tick every `tick_deg` degrees, each labelled (N, E, S, W at the cardinal
## points, the bearing everywhere else), and the exact heading under the marker.
##
## Set `heading_deg` every frame (0 = north, 90 = east, clockwise). Use
## `heading_from_yaw()` to convert a Godot yaw (yaw 0 looks down -Z = north).

const CARDINALS := {0: "N", 90: "E", 180: "S", 270: "W"}

## Current heading in degrees, 0..360, clockwise from north.
var heading_deg := 0.0:
	set(v):
		heading_deg = fposmod(v, 360.0)
		queue_redraw()

## How many degrees of heading the strip shows, edge to edge.
@export var span_deg := 100.0
@export var tick_deg := 5
@export var font_size := 13
@export var cardinal_font_size := 18
@export var color := Color(1, 1, 1, 0.95)
@export var background := Color(0, 0, 0, 0.45)


## Compass heading for a Godot yaw (radians, around +Y, yaw 0 looks down -Z).
static func heading_from_yaw(yaw: float) -> float:
	return fposmod(-rad_to_deg(yaw), 360.0)


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	clip_contents = true


func _draw() -> void:
	var font := get_theme_default_font()
	var w := size.x
	var bar_h := size.y - font_size - 8.0  # room under the bar for the readout
	draw_rect(Rect2(0, 0, w, bar_h), background)
	var px_per_deg := w / span_deg
	var cx := w * 0.5

	# Every tick within half a span of the heading.
	var first := int(floor((heading_deg - span_deg * 0.5) / tick_deg)) * tick_deg
	var last := int(ceil((heading_deg + span_deg * 0.5) / tick_deg)) * tick_deg
	for deg in range(first, last + 1, tick_deg):
		var x := cx + (deg - heading_deg) * px_per_deg
		if x < -20.0 or x > w + 20.0:
			continue
		var bearing := posmod(deg, 360)
		var fade := 1.0 - absf(x - cx) / (w * 0.5) * 0.6   # dimmer toward the edges
		var c := Color(color, color.a * fade)
		var cardinal := CARDINALS.has(bearing)
		var major := cardinal or bearing % 45 == 0
		var tick_h := 12.0 if major else (8.0 if bearing % 10 == 0 else 5.0)
		draw_line(Vector2(x, bar_h - tick_h), Vector2(x, bar_h), c, 2.0 if major else 1.0)
		var text: String = CARDINALS.get(bearing, str(bearing))
		var fs := cardinal_font_size if cardinal else font_size
		var tw := font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, fs).x
		var y := bar_h - tick_h - 4.0
		draw_string_outline(font, Vector2(x - tw * 0.5, y), text, HORIZONTAL_ALIGNMENT_LEFT, -1, fs, 3, Color(0, 0, 0, c.a))
		draw_string(font, Vector2(x - tw * 0.5, y), text, HORIZONTAL_ALIGNMENT_LEFT, -1, fs, c)

	# Centre marker and exact heading.
	var tri := PackedVector2Array([Vector2(cx - 6, bar_h + 7), Vector2(cx + 6, bar_h + 7), Vector2(cx, bar_h)])
	draw_colored_polygon(tri, color)
	draw_line(Vector2(cx, 0), Vector2(cx, bar_h), Color(color, 0.5), 1.0)
	var readout := "%03d°" % (int(round(heading_deg)) % 360)
	var rw := font.get_string_size(readout, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size).x
	var ry := bar_h + 8.0 + font_size
	draw_string_outline(font, Vector2(cx - rw * 0.5, ry), readout, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, 3, Color.BLACK)
	draw_string(font, Vector2(cx - rw * 0.5, ry), readout, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, color)
