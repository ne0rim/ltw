class_name HudPanel
extends PanelContainer
## A HUD box with a title and lines of text on a dark, see-through background.
## Reusable for any on-screen readout: set `title`, then call `set_lines()`.

@export var title: String = "":
	set(v):
		title = v
		if _title:
			_title.text = v
			_title.visible = v != ""
@export var font_size: int = 14
@export var background: Color = Color(0.0, 0.0, 0.0, 0.55)

var _title: Label
var _body: Label


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	var box := StyleBoxFlat.new()
	box.bg_color = background
	box.set_corner_radius_all(4)
	box.set_content_margin_all(10)
	add_theme_stylebox_override("panel", box)

	var col := VBoxContainer.new()
	col.mouse_filter = Control.MOUSE_FILTER_IGNORE
	col.add_theme_constant_override("separation", 4)
	add_child(col)

	_title = _make_label(font_size + 2)
	_title.text = title
	_title.visible = title != ""
	col.add_child(_title)
	_body = _make_label(font_size)
	col.add_child(_body)


func set_lines(lines: PackedStringArray) -> void:
	_body.text = "\n".join(lines)


func _make_label(size: int) -> Label:
	var l := Label.new()
	l.mouse_filter = Control.MOUSE_FILTER_IGNORE
	l.add_theme_font_size_override("font_size", size)
	l.add_theme_color_override("font_outline_color", Color.BLACK)
	l.add_theme_constant_override("outline_size", 3)
	return l
