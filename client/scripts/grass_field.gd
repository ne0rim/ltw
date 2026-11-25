class_name GrassField
extends Node3D
## Grass around the player: tufts drawn in batches (one MultiMesh per square
## patch), only within `radius_m` of `focus`, added and dropped as it moves.
## Each patch's tufts are placed from its own coordinates, so the same spot
## always has the same grass.
##
## Visual only for now: it doesn't hide anyone or stop anything. (Later the
## map can carry a ground-cover layer that gives concealment.)
##
## Uses `grass_models` if any are set (each tuft picks one; the first mesh
## found in the scene is used), else simple blade clumps.

@export var terrain: Terrain
## Usually the player.
@export var focus: Node3D
## Grass is drawn this far around the focus (m)...
@export var radius_m := 70.0
## ...and patches are dropped once this much further away (m).
@export var keep_margin_m := 15.0
## Side of a patch (m).
@export var patch_m := 16.0
## Tufts per patch.
@export var tufts_per_patch := 140
## Look again after the focus has moved this far (m).
@export var recheck_m := 6.0
@export var grass_color := Color(0.30, 0.42, 0.20)
## Optional tuft models (their first mesh is used).
@export var grass_models: Array[PackedScene] = []

var _patches := {}  # Vector2i -> MultiMeshInstance3D
var _checked_at := Vector3.INF
var _meshes: Array[Mesh] = []


func _ready() -> void:
	_meshes = _tuft_meshes()


func _process(_delta: float) -> void:
	if not focus or not terrain or terrain.get_sample_count() == 0:
		return
	if focus.global_position.distance_to(_checked_at) >= recheck_m:
		refresh()


## Add patches that came into range and drop those that left it.
func refresh() -> void:
	var here := focus.global_position
	_checked_at = here
	var reach := int(ceil((radius_m + keep_margin_m) / patch_m))
	var centre := Vector2i(floori(here.x / patch_m), floori(here.z / patch_m))
	var keep := {}
	for dz in range(-reach, reach + 1):
		for dx in range(-reach, reach + 1):
			var key := centre + Vector2i(dx, dz)
			var d := _patch_distance(key, here)
			if d <= radius_m + keep_margin_m:
				keep[key] = true
				if d <= radius_m and not _patches.has(key):
					_patches[key] = _make_patch(key)
	for key in _patches.keys():
		if not keep.has(key):
			_patches[key].queue_free()
			_patches.erase(key)


## How many patches are drawn right now (for tests and the dev panel).
func patch_count() -> int:
	return _patches.size()


func _patch_distance(key: Vector2i, p: Vector3) -> float:
	var x0 := key.x * patch_m
	var z0 := key.y * patch_m
	var dx := maxf(maxf(x0 - p.x, p.x - (x0 + patch_m)), 0.0)
	var dz := maxf(maxf(z0 - p.z, p.z - (z0 + patch_m)), 0.0)
	return Vector2(dx, dz).length()


## One patch: tufts at repeatable spots inside it, on the ground.
func _make_patch(key: Vector2i) -> Node3D:
	var rng := RandomNumberGenerator.new()
	rng.seed = hash(Vector2i(key.x * 73856093, key.y * 19349663))
	var holder := Node3D.new()
	holder.name = "Grass_%d_%d" % [key.x, key.y]
	add_child(holder)
	# One MultiMesh per tuft mesh, so each model is batched separately.
	var per_mesh: Array = []
	for m in _meshes:
		per_mesh.append([])
	var half := terrain.size_m * 0.5
	for i in tufts_per_patch:
		var x := (key.x + rng.randf()) * patch_m
		var z := (key.y + rng.randf()) * patch_m
		var which := rng.randi() % _meshes.size()
		var turn := rng.randf() * TAU
		var size := rng.randf_range(0.6, 1.3)
		if absf(x) > half or absf(z) > half:
			continue
		var basis := Basis(Vector3.UP, turn).scaled(Vector3(size, size * rng.randf_range(0.8, 1.4), size))
		per_mesh[which].append(Transform3D(basis, Vector3(x, terrain.get_height_at(x, z), z)))
	for j in _meshes.size():
		var xforms: Array = per_mesh[j]
		if xforms.is_empty():
			continue
		var mm := MultiMesh.new()
		mm.transform_format = MultiMesh.TRANSFORM_3D
		mm.mesh = _meshes[j]
		mm.instance_count = xforms.size()
		for k in xforms.size():
			mm.set_instance_transform(k, xforms[k])
		var mmi := MultiMeshInstance3D.new()
		mmi.multimesh = mm
		mmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		holder.add_child(mmi)
	return holder


## The meshes tufts are drawn with: the first mesh of each grass model, or a
## generated clump of blades.
func _tuft_meshes() -> Array[Mesh]:
	var out: Array[Mesh] = []
	for scene in grass_models:
		var inst := scene.instantiate()
		var found := inst.find_children("*", "MeshInstance3D", true, false)
		if not found.is_empty():
			out.append((found[0] as MeshInstance3D).mesh)
		inst.free()
	if out.is_empty():
		out.append(_blade_clump())
	return out


## A tuft of thin tapered blades leaning outward, darker at the base.
func _blade_clump() -> Mesh:
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	var rng := RandomNumberGenerator.new()
	rng.seed = 1985
	for b in 7:
		var a := rng.randf() * TAU
		var dir := Vector3(cos(a), 0.0, sin(a))
		var side := dir.cross(Vector3.UP) * 0.025
		var root := dir * rng.randf_range(0.0, 0.08)
		var h := rng.randf_range(0.25, 0.55)
		var tip := root + dir * h * 0.35 + Vector3.UP * h
		var base_c := grass_color.darkened(0.35)
		var tip_c := grass_color.lightened(0.15)
		for v in [[root - side, base_c], [root + side, base_c], [tip, tip_c]]:
			st.set_color(v[1])
			st.set_normal(Vector3.UP)
			st.add_vertex(v[0])
	var mat := StandardMaterial3D.new()
	mat.vertex_color_use_as_albedo = true
	mat.cull_mode = BaseMaterial3D.CULL_DISABLED
	mat.roughness = 1.0
	var mesh := st.commit()
	mesh.surface_set_material(0, mat)
	return mesh
