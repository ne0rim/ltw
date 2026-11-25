@tool
class_name MapObjects
extends Node3D
## Shows the map's objects (barricades, ammo boxes) around the player. The
## core owns where things are and what they do (for example barricades
## stopping bullets); this node only shows them and gives them collision so the
## player can't walk through.
##
## Objects are streamed: only those within `view_radius_m` of the player are
## built, asked for through the core's spatial index
## (`SimBridge.get_map_objects_near`). They are freed once the player is more
## than `view_radius_m + keep_margin_m` away. The check repeats whenever the
## player has moved `recheck_distance_m`, so the cost depends on what's nearby,
## not on how big the map is.
##
## In the editor it shows every object of the scene's map (Main's `map_id`)
## from the map files, so you can see them while editing.
##
## Trees and boulders use the models in `tree_models` / `boulder_models` if
## any are set (object variant n uses model n, wrapping), scaled to the size
## the core gives them; otherwise simple stand-in shapes. Models should stand
## on their origin with their front facing -Z. Collision always matches the
## core: the trunk for a tree, the box for a boulder.

const AMMO_BOX_SCENE := preload("res://scenes/props/ammo_box.tscn")
## Sandbag colour.
const BARRICADE_COLOR := Color(0.52, 0.47, 0.35)

@export var sim: SimBridge
@export var player: Player
## Objects closer than this are built (m).
@export var view_radius_m := 600.0
## Built objects are kept until this much further away, so objects at the edge
## don't flicker in and out (m).
@export var keep_margin_m := 50.0
## Look again after the player has moved this far (m).
@export var recheck_distance_m := 20.0
@export_tool_button("Reload map") var _reload_map_button := reload_editor_map

@export_group("Models")
## Tree models, one per variant (scaled to each tree's height).
@export var tree_models: Array[PackedScene] = []
## Boulder models, one per variant (scaled to each boulder's size).
@export var boulder_models: Array[PackedScene] = []
## Trees and boulders aren't drawn beyond this distance (m).
@export var nature_draw_distance_m := 500.0

var _shown := {}  # object id -> Node3D
var _checked_at := Vector3.INF
var _sandbags: StandardMaterial3D
# Stand-in meshes, shared by every tree and boulder (made on first use).
var _stand_ins := {}


func _ready() -> void:
	_sandbags = StandardMaterial3D.new()
	_sandbags.albedo_color = BARRICADE_COLOR
	_sandbags.roughness = 0.95
	if Engine.is_editor_hint():
		reload_editor_map()


## Editor: show every object of the scene's map, from the map files.
func reload_editor_map() -> void:
	for node in _shown.values():
		node.queue_free()
	_shown.clear()
	var root: Node = owner if owner else get_tree().edited_scene_root
	var id: String = str(root.get("map_id")) if root and root.get("map_id") else ""
	var file := MapFile.new()
	if id == "" or not file.open(id):
		return
	for o in file.get_map_objects():
		var node := _build(o)
		if node:
			_shown[o.id] = node


## Forget everything shown and look again (after a new map is loaded).
func reset() -> void:
	for node in _shown.values():
		node.queue_free()
	_shown.clear()
	_checked_at = Vector3.INF
	refresh()


func _process(_delta: float) -> void:
	if Engine.is_editor_hint():
		return  # no streaming while editing: everything is shown
	if player and player.global_position.distance_to(_checked_at) >= recheck_distance_m:
		refresh()


## Build objects that came into range and free those that left it.
func refresh() -> void:
	if Engine.is_editor_hint() or not sim or not player:
		return
	var here := player.global_position
	_checked_at = here
	var keep := {}
	for o in sim.get_map_objects_near(here, view_radius_m + keep_margin_m):
		keep[o.id] = true
		var close := Vector2(o.position.x - here.x, o.position.z - here.z).length() <= view_radius_m
		if close and not _shown.has(o.id):
			var node := _build(o)
			if node:
				_shown[o.id] = node
	for id in _shown.keys():
		if not keep.has(id):
			_shown[id].queue_free()
			_shown.erase(id)


## How many objects are built right now (for the dev panel and tests).
func shown_count() -> int:
	return _shown.size()


func _build(o: Dictionary) -> Node3D:
	match o.kind:
		"barricade":
			var b := _barricade(o)
			add_child(b)
			return b
		"tree":
			var t := _tree(o)
			add_child(t)
			return t
		"boulder":
			var r := _boulder(o)
			add_child(r)
			return r
		"ammo_box":
			var box: AmmoBox = AMMO_BOX_SCENE.instantiate()
			box.sim = sim
			box.player = player
			add_child(box)
			box.global_position = o.position
			box.rotation.y = o.yaw
			return box
	push_warning("MapObjects: no look for '%s'" % o.kind)
	return null


## A tree: a model if one is set for its variant, else a trunk and crown.
## Collision is the trunk (what the core says stops bullets).
func _tree(o: Dictionary) -> Node3D:
	var body := _body_at(o, "Tree_%d" % o.id)
	var height: float = o.height
	var trunk_r: float = o.length * 0.5
	if not tree_models.is_empty():
		body.add_child(_fitted_model(tree_models[o.variant % tree_models.size()],
			Vector3.ONE * height, true))
	else:
		var conifer := int(o.variant) % 2 == 0
		var trunk := _mesh_instance("trunk", Vector3(trunk_r * 2.0, height * 0.5, trunk_r * 2.0))
		trunk.position.y = height * 0.25
		body.add_child(trunk)
		var canopy: float = o.canopy
		var crown: MeshInstance3D
		if conifer:
			crown = _mesh_instance("cone", Vector3(canopy * 2.0, height * 0.8, canopy * 2.0))
			crown.position.y = height * 0.2 + height * 0.4
		else:
			crown = _mesh_instance("crown", Vector3(canopy * 2.0, canopy * 1.7, canopy * 2.0))
			crown.position.y = height - canopy * 0.85
		body.add_child(crown)
	var shape := CylinderShape3D.new()
	shape.radius = trunk_r
	shape.height = height
	var col := CollisionShape3D.new()
	col.shape = shape
	col.position.y = height * 0.5
	body.add_child(col)
	return body


## A boulder: a model if one is set for its variant, else a faceted rock.
## Collision is its box (what the core says stops bullets).
func _boulder(o: Dictionary) -> Node3D:
	var body := _body_at(o, "Boulder_%d" % o.id)
	var size := Vector3(o.length, o.height, o.thickness)
	if not boulder_models.is_empty():
		body.add_child(_fitted_model(boulder_models[o.variant % boulder_models.size()], size, false))
	else:
		var rock := _mesh_instance("rock_%d" % (int(o.variant) % 4), size)
		rock.position.y = size.y * 0.5
		body.add_child(rock)
	var shape := BoxShape3D.new()
	shape.size = size
	var col := CollisionShape3D.new()
	col.shape = shape
	col.position.y = size.y * 0.5
	body.add_child(col)
	return body


## A static body standing where the core put the object, turned to its yaw.
func _body_at(o: Dictionary, node_name: String) -> StaticBody3D:
	var body := StaticBody3D.new()
	body.name = node_name
	body.position = o.position
	body.rotation.y = o.yaw
	return body


## A model scaled to `size`: uniformly by height (`by_height`, for trees) or
## to all three sizes (boulders). Uses the model's own bounds.
func _fitted_model(scene: PackedScene, size: Vector3, by_height: bool) -> Node3D:
	var model: Node3D = scene.instantiate()
	var bounds := _bounds(model)
	var b := bounds.size
	if b.y > 0.001:
		if by_height:
			model.scale = Vector3.ONE * (size.y / b.y)
		elif b.x > 0.001 and b.z > 0.001:
			model.scale = Vector3(size.x / b.x, size.y / b.y, size.z / b.z)
	for v in model.find_children("*", "GeometryInstance3D", true, false):
		(v as GeometryInstance3D).visibility_range_end = nature_draw_distance_m
	return model


## Combined bounds of a model's meshes, in its own space.
func _bounds(node: Node3D) -> AABB:
	var out := AABB()
	var first := true
	for m in node.find_children("*", "MeshInstance3D", true, false):
		var mi := m as MeshInstance3D
		var box := mi.transform * mi.get_aabb()
		out = box if first else out.merge(box)
		first = false
	return out


## A stand-in mesh scaled to `size`, sharing one mesh per kind.
func _mesh_instance(kind: String, size: Vector3) -> MeshInstance3D:
	var mi := MeshInstance3D.new()
	mi.mesh = _stand_in(kind)
	mi.scale = size
	mi.visibility_range_end = nature_draw_distance_m
	return mi


## Unit-sized stand-in meshes (1 m across and tall), made once.
func _stand_in(kind: String) -> Mesh:
	if _stand_ins.has(kind):
		return _stand_ins[kind]
	var mesh: PrimitiveMesh
	var mat := StandardMaterial3D.new()
	mat.roughness = 0.95
	match kind:
		"trunk":
			var c := CylinderMesh.new()
			c.top_radius = 0.35
			c.bottom_radius = 0.5
			c.height = 1.0
			c.radial_segments = 8
			mesh = c
			mat.albedo_color = Color(0.32, 0.24, 0.17)
		"cone":
			var c := CylinderMesh.new()
			c.top_radius = 0.0
			c.bottom_radius = 0.5
			c.height = 1.0
			c.radial_segments = 8
			mesh = c
			mat.albedo_color = Color(0.17, 0.30, 0.17)
		"crown":
			var s := SphereMesh.new()
			s.radius = 0.5
			s.height = 1.0
			s.radial_segments = 10
			s.rings = 6
			mesh = s
			mat.albedo_color = Color(0.22, 0.36, 0.18)
		_:
			# Rocks: low-segment spheres look like faceted stones; the
			# variant changes the facets.
			var v := int(kind.get_slice("_", 1))
			var s := SphereMesh.new()
			s.radius = 0.5
			s.height = 1.0
			s.radial_segments = 5 + v
			s.rings = 3 + v % 2
			mesh = s
			mat.albedo_color = Color(0.42, 0.41, 0.39).darkened(0.08 * v)
	mesh.material = mat
	_stand_ins[kind] = mesh
	return mesh


## A low sandbag wall: length across its front (local X), its front facing
## local -Z (the object's yaw), standing on its position.
func _barricade(o: Dictionary) -> Node3D:
	var size := Vector3(o.length, o.height, o.thickness)
	var body := StaticBody3D.new()
	body.name = "Barricade_%d" % o.id
	body.position = o.position
	body.rotation.y = o.yaw

	var mesh := BoxMesh.new()
	mesh.size = size
	mesh.material = _sandbags
	var view := MeshInstance3D.new()
	view.mesh = mesh
	view.position.y = size.y * 0.5
	body.add_child(view)

	var shape := BoxShape3D.new()
	shape.size = size
	var col := CollisionShape3D.new()
	col.shape = shape
	col.position.y = size.y * 0.5
	body.add_child(col)
	return body
