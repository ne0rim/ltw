class_name TacticalMap
extends Control
## Full-screen 2D map of the current map area: a height map with contours,
## every soldier in the platoon, unit orders, and a panel of unit stats.
## North (-Z) is up. Open and close it with `toggle()` (the HUD binds T).
##
## Enemy soldiers are shown in red. (Until sight exists the map shows all of
## them; later it will only show what your side has spotted.)
##
## Mouse: left-click a soldier to select their unit (Shift+click adds or
## removes), left-click empty ground to clear the selection, right-click to
## move the selected units there. They form up facing the way they travel.
##
## The "area" is the whole test terrain for now. Later it becomes one part of
## a larger map, and the map gains terrain types and objects.

## Colour per unit on your side: platoon HQ, then 1, 2, 3 Section.
const UNIT_COLORS := [Color("e8e8e8"), Color("e0b84c"), Color("5fb3c9"), Color("8fc98a")]
## Every enemy unit is red (HQ a little darker).
const ENEMY_COLOR := Color("e2574c")
const DEAD_COLOR := Color(0.45, 0.45, 0.45)

@export var area_name: String = "Test area"
## Faint grid line spacing (metres).
@export var grid_m: float = 50.0
## Length of the scale bar (metres).
@export var scale_bar_m: float = 100.0
## Space around the map (pixels).
@export var margin: float = 24.0
## How close a click must be to a soldier to pick their unit (pixels).
@export var pick_radius_px: float = 14.0

@onready var _image: TextureRect = $MapImage
@onready var _markers: Control = $Markers
@onready var _panel: HudPanel = $SquadPanel

## Map objects (from `MapObjects.objects`): trees, boulders and barricades
## are drawn on the map.
var objects: Array = []:
	set(v):
		objects = v
		if _static:
			_static.queue_redraw()

var _static: Control
var _static_rect := Rect2()  # map rect the still layer was drawn for

var _terrain: Terrain
var _platoon: PlatoonView
var _player: Player
var _command: CommandInput
var _bounds := Rect2()          # world XZ shown on the map
var _map_rect := Rect2()        # where it sits on screen
var _built := false


func setup(terrain: Terrain, platoon: PlatoonView, player: Player, command: CommandInput) -> void:
	_terrain = terrain
	_platoon = platoon
	_player = player
	_command = command


func _ready() -> void:
	visible = false
	# The still layer sits between the height map and the moving markers.
	_static = Control.new()
	_static.name = "StaticLayer"
	_static.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_static)
	move_child(_static, _markers.get_index())
	_static.draw.connect(_draw_static)
	_markers.draw.connect(_draw_markers)
	resized.connect(_layout)


func toggle() -> void:
	if visible:
		close()
	else:
		open()


func open() -> void:
	if not _built:
		_build_height_texture()
	visible = true
	_layout()
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE


func close() -> void:
	visible = false
	Input.mouse_mode = Input.MOUSE_MODE_CAPTURED


func _gui_input(event: InputEvent) -> void:
	var mb := event as InputEventMouseButton
	if not mb or not mb.pressed or not _built or not _map_rect.has_point(mb.position):
		return
	if mb.button_index == MOUSE_BUTTON_LEFT:
		_click_select(mb.position, mb.shift_pressed)
		accept_event()
	elif mb.button_index == MOUSE_BUTTON_RIGHT:
		_click_move(mb.position)
		accept_event()


func _click_select(at: Vector2, add: bool) -> void:
	var unit_id := _unit_at(at)
	if add:
		_command.toggle_unit(unit_id)
	else:
		_command.select_only(unit_id)


func _click_move(at: Vector2) -> void:
	var target := _to_world(at)
	# Face the way the units travel: from their centre toward the target.
	var from := Vector3.ZERO
	var n := 0
	for s in _platoon.soldiers:
		if _command.selected.has(s.unit_id) and not s.is_player:
			from += s.position
			n += 1
	var facing := _player.global_rotation.y
	if n > 0:
		var d := target - from / n
		if Vector2(d.x, d.z).length() > 0.5:
			facing = atan2(-d.x, -d.z)  # yaw 0 looks down -Z
	_command.order_move_to(target, facing)


## Unit of the soldier nearest to a screen point, or -1 if none is close.
func _unit_at(at: Vector2) -> int:
	var best := -1
	var best_d := pick_radius_px
	for s in _platoon.soldiers:
		if s.life == "dead" or s.side != _platoon.my_side:
			continue
		var p := _to_map(_player.global_position if s.is_player else s.position)
		var d := p.distance_to(at)
		if d < best_d:
			best_d = d
			best = s.unit_id
	return best


func _process(_delta: float) -> void:
	if not visible:
		return
	_layout()  # the unit panel's width changes with its text
	_markers.queue_redraw()
	_panel.set_lines(_squad_lines())


# ------------------------------------------------------------------ height map

## Upload the terrain's height grid as a float texture; the shader does the rest.
func _build_height_texture() -> void:
	var n := _terrain.get_sample_count()
	var grid := _terrain.get_height_grid()
	if n < 2 or grid.size() != n * n:
		return
	var lo := INF
	var hi := -INF
	for h in grid:
		lo = minf(lo, h)
		hi = maxf(hi, h)
	var img := Image.create_from_data(n, n, false, Image.FORMAT_RF, grid.to_byte_array())
	var mat := _image.material as ShaderMaterial
	mat.set_shader_parameter("heights", ImageTexture.create_from_image(img))
	mat.set_shader_parameter("min_height", lo)
	mat.set_shader_parameter("max_height", hi)
	mat.set_shader_parameter("texel", Vector2.ONE / n)
	mat.set_shader_parameter("cell_m", _terrain.get_cell_size())
	# Any texture works as the TextureRect's own; the shader reads `heights`.
	_image.texture = ImageTexture.create_from_image(img)
	_bounds = _terrain.get_bounds()
	_built = true
	_static.queue_redraw()


func _layout() -> void:
	# Square map on the left; the unit panel sits top-right.
	var side := minf(size.y - margin * 2.0, size.x - _panel.size.x - margin * 3.0)
	side = maxf(side, 64.0)
	_map_rect = Rect2(Vector2(margin, (size.y - side) * 0.5), Vector2(side, side))
	_image.position = _map_rect.position
	_image.size = _map_rect.size
	_markers.position = Vector2.ZERO
	_markers.size = size
	var moved := _static.size != size or _static_rect != _map_rect
	_static.position = Vector2.ZERO
	_static.size = size
	if moved:
		_static_rect = _map_rect
		_static.queue_redraw()


## Screen to world, on the ground.
func _to_world(screen: Vector2) -> Vector3:
	var uv := (screen - _map_rect.position) / _map_rect.size
	var xz := _bounds.position + uv * _bounds.size
	return Vector3(xz.x, _terrain.get_height_at(xz.x, xz.y), xz.y)


## World XZ to screen.
func _to_map(world: Vector3) -> Vector2:
	var uv := (Vector2(world.x, world.z) - _bounds.position) / _bounds.size
	return _map_rect.position + uv * _map_rect.size


# ------------------------------------------------------------------ markers

## The parts of the map that don't move (grid, trees, boulders, barricades),
## drawn once into their own layer and redrawn only when the map is opened,
## resized or its objects change. Drawing thousands of trees every frame
## would make the map crawl.
func _draw_static() -> void:
	if not _built:
		return
	var m := _static
	var px_per_m := _map_rect.size.x / _bounds.size.x

	# Grid.
	var x := ceilf(_bounds.position.x / grid_m) * grid_m
	while x < _bounds.end.x:
		var sx := _to_map(Vector3(x, 0, 0)).x
		m.draw_line(Vector2(sx, _map_rect.position.y), Vector2(sx, _map_rect.end.y), Color(1, 1, 1, 0.08))
		x += grid_m
	var z := ceilf(_bounds.position.y / grid_m) * grid_m
	while z < _bounds.end.y:
		var sy := _to_map(Vector3(0, 0, z)).y
		m.draw_line(Vector2(_map_rect.position.x, sy), Vector2(_map_rect.end.x, sy), Color(1, 1, 1, 0.08))
		z += grid_m
	m.draw_rect(_map_rect, Color(0, 0, 0, 0.9), false, 2.0)

	# Trees (dark green, sized by canopy) and boulders (grey), under everything.
	for o in objects:
		if o.kind == "tree":
			var r := maxf(1.2, float(o.canopy) * px_per_m)
			m.draw_circle(_to_map(o.position), r, Color(0.12, 0.25, 0.12, 0.85))
		elif o.kind == "boulder":
			m.draw_circle(_to_map(o.position), maxf(1.0, float(o.length) * 0.5 * px_per_m), Color(0.6, 0.6, 0.58))

	# Barricades: a thick line along their front.
	for o in objects:
		if o.kind != "barricade":
			continue
		var yaw: float = o.yaw
		var half: float = o.length * 0.5
		var centre: Vector3 = o.position
		var right := Vector3(cos(yaw), 0.0, -sin(yaw)) * half
		var a := _to_map(centre - right)
		var b := _to_map(centre + right)
		m.draw_line(a, b, Color.BLACK, 5.0)
		m.draw_line(a, b, Color("c9b27c"), 3.0)


func _draw_markers() -> void:
	if not _built:
		return
	var font := get_theme_default_font()
	var m := _markers
	var px_per_m := _map_rect.size.x / _bounds.size.x

	var colors := _unit_colors()
	var by_id := {}
	for s in _platoon.soldiers:
		by_id[s.id] = s

	# Orders: a line from each unit to its objective.
	for u in _platoon.my_units():
		if u.order == "none":
			continue
		var c: Color = colors.get(u.id, Color.WHITE)
		var from := _unit_centre(u, by_id)
		var to := _to_map(u.order_target)
		m.draw_dashed_line(from, to, Color(c, 0.8), 1.5, 6.0)
		if u.order == "move":
			m.draw_arc(to, 6.0, 0.0, TAU, 20, c, 2.0)
		else:
			m.draw_line(to + Vector2(-6, -6), to + Vector2(6, 6), c, 2.0)
			m.draw_line(to + Vector2(-6, 6), to + Vector2(6, -6), c, 2.0)

	# Soldiers.
	for s in _platoon.soldiers:
		if s.is_player:
			continue
		var p := _to_map(s.position)
		var c: Color = colors.get(s.unit_id, Color.WHITE)
		if s.life == "dead":
			m.draw_line(p + Vector2(-3, -3), p + Vector2(3, 3), DEAD_COLOR, 1.5)
			m.draw_line(p + Vector2(-3, 3), p + Vector2(3, -3), DEAD_COLOR, 1.5)
			continue
		if s.life != "healthy":
			c = c.darkened(0.45)
		if _command.selected.has(s.unit_id):
			m.draw_arc(p, 7.5, 0.0, TAU, 16, Color.WHITE, 1.5)
		if s.slot == 0:  # section commander
			m.draw_rect(Rect2(p - Vector2(4.5, 4.5), Vector2(9, 9)), Color.BLACK)
			m.draw_rect(Rect2(p - Vector2(3.5, 3.5), Vector2(7, 7)), c)
		else:
			m.draw_circle(p, 4.5, Color.BLACK)
			m.draw_circle(p, 3.5, c)

	# Unit labels (▶ = selected).
	for u in _platoon.units:
		if u.kind == "platoon" or u.soldier_ids.is_empty():
			continue
		var at := _unit_centre(u, by_id) + Vector2(10, -10)
		var text: String = ("▶ " if _command.selected.has(u.id) else "") + u.callsign
		_label(font, at, text, colors.get(u.id, Color.WHITE), 14)

	# The player: an arrow pointing where they face (yaw 0 looks north).
	var pp := _to_map(_player.global_position)
	var yaw := _player.global_rotation.y
	var fwd := Vector2(-sin(yaw), -cos(yaw))
	var right := Vector2(-fwd.y, fwd.x)
	var tri := PackedVector2Array([pp + fwd * 11.0, pp - fwd * 6.0 + right * 6.5, pp - fwd * 6.0 - right * 6.5])
	m.draw_colored_polygon(tri, Color.WHITE)
	m.draw_polyline(tri + PackedVector2Array([tri[0]]), Color.BLACK, 1.5)
	_label(font, pp + Vector2(12, 14), "YOU", Color.WHITE, 12)

	# North arrow, scale bar, title.
	var top := _map_rect.position
	var north_w := font.get_string_size("N ↑", HORIZONTAL_ALIGNMENT_LEFT, -1, 16).x
	_label(font, top + Vector2(_map_rect.size.x - north_w - 16, 26), "N ↑", Color.WHITE, 16)
	var bar := scale_bar_m * px_per_m
	var b0 := Vector2(top.x + 16, _map_rect.end.y - 18)
	m.draw_line(b0, b0 + Vector2(bar, 0), Color.BLACK, 5.0)
	m.draw_line(b0, b0 + Vector2(bar, 0), Color.WHITE, 2.0)
	_label(font, b0 + Vector2(0, -8), "%d m" % int(scale_bar_m), Color.WHITE, 12)
	_label(font, top + Vector2(16, 26), "%s · %d × %d m" % [area_name.to_upper(),
		int(_bounds.size.x), int(_bounds.size.y)], Color.WHITE, 16)


func _label(font: Font, at: Vector2, text: String, color: Color, size_px: int) -> void:
	_markers.draw_string_outline(font, at, text, HORIZONTAL_ALIGNMENT_LEFT, -1, size_px, 4, Color.BLACK)
	_markers.draw_string(font, at, text, HORIZONTAL_ALIGNMENT_LEFT, -1, size_px, color)


## Screen position of a unit's centre (average of its living soldiers).
func _unit_centre(u: Dictionary, by_id: Dictionary) -> Vector2:
	var sum := Vector2.ZERO
	var n := 0
	for id in u.soldier_ids:
		var s: Dictionary = by_id.get(id, {})
		if s.is_empty() or s.life == "dead":
			continue
		sum += _to_map(_player.global_position if s.is_player else s.position)
		n += 1
	return sum / n if n > 0 else _to_map(u.order_target)


## HQ first, then the rifle sections in order.
func _unit_colors() -> Dictionary:
	var out := {}
	var section_no := 0
	for u in _platoon.units:
		if u.side != _platoon.my_side:
			out[u.id] = ENEMY_COLOR.darkened(0.25) if u.kind == "platoon_hq" else ENEMY_COLOR
		elif u.kind == "platoon_hq":
			out[u.id] = UNIT_COLORS[0]
		elif u.kind == "rifle_section":
			section_no += 1
			out[u.id] = UNIT_COLORS[mini(section_no, UNIT_COLORS.size() - 1)]
	return out


# ------------------------------------------------------------------ unit panel

func _squad_lines() -> PackedStringArray:
	var by_id := {}
	for s in _platoon.soldiers:
		by_id[s.id] = s
	var lines := PackedStringArray()
	var total := 0
	var effective := 0
	for s in _platoon.soldiers:
		if s.side != _platoon.my_side:
			continue
		total += 1
		if s.life in ["healthy", "wounded"]:
			effective += 1
	lines.append("1 Platoon · %d/%d effective" % [effective, total])
	for u in _platoon.my_units():
		var fit := 0
		var wounded := 0
		var down := 0
		for id in u.soldier_ids:
			var s: Dictionary = by_id.get(id, {})
			match s.get("life", ""):
				"healthy": fit += 1
				"wounded": fit += 1; wounded += 1
				"incapacitated", "dead": down += 1
		var leader: Dictionary = by_id.get(u.leader_soldier_id, {})
		var mark := "▶ " if _command.selected.has(u.id) else ""
		lines.append("")
		lines.append("%s[%s] %s · %s %s" % [mark, u.callsign, u.name,
			leader.get("rank", "?"), leader.get("name", "no leader")])
		var order := ""
		if u.order != "none":
			var dist := _player.global_position.distance_to(u.order_target)
			order = " · %s, %.0f m from you" % [u.order, dist]
		lines.append("   %d/%d effective · %d wounded · %d down" % [
			fit, u.soldier_ids.size(), wounded, down])
		lines.append("   %s%s · %d rds" % [u.status, order, u.rounds])
	lines.append("")
	lines.append("Click a unit to select · Shift+click to add")
	lines.append("Right-click to move selected · T close")
	return lines
