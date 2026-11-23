@tool
class_name Crosshair
extends Control
## Simple four-line crosshair drawn at the centre of this control.
## Fill the screen with it (full-rect anchors) to centre it on screen.

@export var color: Color = Color(0.05, 0.05, 0.05, 0.9):
	set(v): color = v; queue_redraw()
@export var outline_color: Color = Color(1, 1, 1, 0.35):
	set(v): outline_color = v; queue_redraw()
## Length of each arm, in pixels.
@export var arm_length: float = 8.0:
	set(v): arm_length = v; queue_redraw()
## Empty space between the centre and each arm, in pixels.
@export var gap: float = 4.0:
	set(v): gap = v; queue_redraw()
@export var thickness: float = 2.0:
	set(v): thickness = v; queue_redraw()
@export var center_dot: bool = false:
	set(v): center_dot = v; queue_redraw()


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	resized.connect(queue_redraw)


func _draw() -> void:
	var c := size * 0.5
	var dirs := [Vector2.RIGHT, Vector2.LEFT, Vector2.DOWN, Vector2.UP]
	for pass_color in [outline_color, color]:
		var w := thickness + (2.0 if pass_color == outline_color else 0.0)
		for d in dirs:
			draw_line(c + d * gap, c + d * (gap + arm_length), pass_color, w)
		if center_dot:
			draw_circle(c, w * 0.5, pass_color)
