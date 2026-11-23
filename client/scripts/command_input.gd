class_name CommandInput
extends Node3D
## The player's command keys. Selection lives here (it's the player's UI
## state); orders go to the Rust sim, which carries them out.
##
##   Ctrl+1/2/3 = toggle 1/2/3 Section, Ctrl+4 = toggle Platoon HQ
##   Ctrl+M     = selected units move to the point under the crosshair
##   Ctrl+F     = selected units fire at the point under the crosshair

signal selection_changed(unit_ids: PackedInt32Array)
signal order_given(order: String, unit_ids: PackedInt32Array, target: Vector3)

@export var sim: SimBridge
@export var platoon: PlatoonView
@export var player: Player
## How far the crosshair can pick a point (metres).
@export var max_order_range := 1500.0
## How long order markers stay up (seconds).
@export var marker_time := 6.0

var selected := PackedInt32Array()
var last_message := ""

var _markers: Array = []    # [node, time_left]


## Unit ids in key order: 1-3 = rifle sections, 4 = platoon HQ.
func unit_for_key(n: int) -> int:
	var sections := []
	var hq := -1
	for u in platoon.units:
		if u.kind == "rifle_section":
			sections.append(u.id)
		elif u.kind == "platoon_hq":
			hq = u.id
	sections.sort()
	if n >= 1 and n <= sections.size():
		return sections[n - 1]
	return hq if n == 4 else -1


func _unhandled_input(event: InputEvent) -> void:
	for n in range(1, 5):
		if event.is_action_pressed("select_unit_%d" % n):
			toggle_unit(unit_for_key(n))
			get_viewport().set_input_as_handled()
			return
	if event.is_action_pressed("order_move"):
		order_move()
		get_viewport().set_input_as_handled()
	elif event.is_action_pressed("order_fire"):
		order_fire()
		get_viewport().set_input_as_handled()


func toggle_unit(unit_id: int) -> void:
	if unit_id < 0:
		return
	var i := selected.find(unit_id)
	if i >= 0:
		selected.remove_at(i)
	else:
		selected.append(unit_id)
	platoon.set_selected_units(selected)
	selection_changed.emit(selected)


func order_move() -> void:
	if not _check_selection():
		return
	var target := aim_point()
	sim.order_move(selected, target, player.global_rotation.y)
	_order_feedback("move", target)


func order_fire() -> void:
	if not _check_selection():
		return
	var target := aim_point()
	sim.order_fire_at(selected, target)
	_order_feedback("fire", target)


## Point on the ground under the crosshair (or far ahead if aiming at the sky).
func aim_point() -> Vector3:
	var cam := get_viewport().get_camera_3d()
	var from := cam.global_position
	var dir := -cam.global_basis.z
	var q := PhysicsRayQueryParameters3D.create(from, from + dir * max_order_range, 1)
	q.exclude = [player.get_rid()]
	var hit := get_world_3d().direct_space_state.intersect_ray(q)
	if hit:
		return hit.position
	var flat := Vector3(dir.x, 0.0, dir.z).normalized()
	var p := from + flat * 150.0
	return Vector3(p.x, platoon.ground_height(p.x, p.z), p.z)


func selection_text() -> String:
	if selected.is_empty():
		return "none"
	var names := PackedStringArray()
	for u in platoon.units:
		if selected.has(u.id):
			names.append("%s [%s]" % [u.name, u.callsign])
	return ", ".join(names)


func _check_selection() -> bool:
	if selected.is_empty():
		last_message = "No unit selected (Ctrl+1-4)"
		return false
	return true


func _order_feedback(order: String, target: Vector3) -> void:
	var calls := PackedStringArray()
	for u in platoon.units:
		if selected.has(u.id):
			calls.append(u.callsign)
	var dist := player.global_position.distance_to(target)
	last_message = "%s %s → %.0f m" % [order.to_upper(), " ".join(calls), dist]
	_spawn_marker(order, target, " ".join(calls))
	order_given.emit(order, selected, target)


func _spawn_marker(order: String, target: Vector3, calls: String) -> void:
	var m := Node3D.new()
	var pole := MeshInstance3D.new()
	var cyl := CylinderMesh.new()
	cyl.top_radius = 0.04
	cyl.bottom_radius = 0.04
	cyl.height = 2.5
	var mat := StandardMaterial3D.new()
	mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	mat.albedo_color = Color(0.05, 0.05, 0.05) if order == "move" else Color(1.0, 1.0, 1.0)
	cyl.material = mat
	pole.mesh = cyl
	pole.position.y = 1.25
	m.add_child(pole)
	var ring := MeshInstance3D.new()
	var torus := TorusMesh.new()
	torus.inner_radius = 1.6 if order == "move" else 2.6
	torus.outer_radius = torus.inner_radius + 0.25
	torus.material = mat
	ring.mesh = torus
	ring.position.y = 0.05
	m.add_child(ring)
	var label := Label3D.new()
	label.text = "%s\n%s" % [order.to_upper(), calls]
	label.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	label.fixed_size = true
	label.pixel_size = 0.0012
	label.font_size = 28
	label.outline_size = 8
	label.no_depth_test = true
	label.position.y = 2.9
	m.add_child(label)
	get_tree().current_scene.add_child(m)
	m.global_position = target
	_markers.append([m, marker_time])


func _process(delta: float) -> void:
	for i in range(_markers.size() - 1, -1, -1):
		_markers[i][1] -= delta
		if _markers[i][1] <= 0.0:
			_markers[i][0].queue_free()
			_markers.remove_at(i)
