@tool
class_name Terrain
extends Node3D
## Generates a grayscale, mountainous test terrain with matching collision.
## Every setting is exported. Change it in the Inspector and press Regenerate.

@export_group("Size")
## Width and depth of the terrain, in metres.
@export var size_m: float = 1024.0:
	set(v): size_m = maxf(v, 8.0); _queue_regen()
## Distance between height samples, in metres. Smaller means more detail and slower generation.
@export var cell_size_m: float = 2.0:
	set(v): cell_size_m = clampf(v, 0.25, 32.0); _queue_regen()

@export_group("Shape")
## Height of the tallest peaks, in metres.
@export var max_height_m: float = 70.0:
	set(v): max_height_m = v; _queue_regen()
@export var noise_seed: int = 1985:
	set(v): noise_seed = v; _queue_regen()
## Lower values give broader hills.
@export var noise_frequency: float = 0.004:
	set(v): noise_frequency = v; _queue_regen()
@export_range(1, 8) var octaves: int = 5:
	set(v): octaves = v; _queue_regen()
## 0 gives rolling hills, 1 gives sharp ridgelines.
@export_range(0.0, 1.0) var ridge_amount: float = 0.45:
	set(v): ridge_amount = v; _queue_regen()
## Radius of the gentler area around the centre where the player spawns.
@export var flat_center_radius_m: float = 60.0:
	set(v): flat_center_radius_m = maxf(v, 0.0); _queue_regen()
## Height of the centre area as a fraction of the full noise (0 is flat).
@export_range(0.0, 1.0) var center_roughness: float = 0.15:
	set(v): center_roughness = v; _queue_regen()

@export_group("Look")
## Spacing of the reference grid painted on the ground (0 turns it off).
@export var grid_spacing_m: float = 10.0:
	set(v): grid_spacing_m = maxf(v, 0.0); _update_material()

@export_group("Map terrain")
## Side of a terrain chunk (m).
@export var chunk_m: float = 64.0
## Chunks are drawn using every n-th height sample (4 at 0.5 m = 2 m detail).
@export var far_step: int = 4
## Chunks this close to the player are drawn at full detail (m)...
@export var detail_radius_m: float = 160.0
## ...and go back to coarse once this much further away (m).
@export var detail_keep_margin_m: float = 40.0
## Check again after the player has moved this far (m).
@export var detail_recheck_m: float = 10.0
## How far the edge skirts hang down to hide seams between chunks (m).
@export var skirt_m: float = 1.5
## In the editor, chunks use every n-th sample (2 at 0.5 m = 1 m detail).
@export var editor_step: int = 2
## Map to show in the editor; empty = the scene's (Main's) `map_id`.
@export var preview_map_id: String = ""
@export_tool_button("Reload map") var _reload_map_button := reload_editor_map

@export_tool_button("Regenerate") var _regen_button := generate

var _heights := PackedFloat32Array()
var _samples := 0
var _cell := 2.0
var _mesh_instance: MeshInstance3D
var _shape_node: CollisionShape3D
var _material: ShaderMaterial
var _regen_queued := false
# Chunked terrain from the map (see load_map).
var _source: Object  # SimBridge in the game, MapFile in the editor
var _focus: Node3D
var _lo := 0.0
var _hi := 1.0
var _chunk_cells := 128
var _chunks_per_side := 0
var _far := {}   # Vector2i -> MeshInstance3D (coarse, every chunk)
var _near := {}  # Vector2i -> MeshInstance3D (full detail, near the focus)
var _detail_checked_at := Vector3.INF
## True once the terrain comes from the map (the core's data): no noise
## generation, no regeneration when settings change.
var _from_map := false


func _ready() -> void:
	# In the game, main.gd loads the map (see load_map). In the editor, show the
	# same map from its files; the noise generator is only a fallback.
	if Engine.is_editor_hint():
		reload_editor_map()


## Editor: show the map named by Main's `map_id` (or `preview_map_id`) from the
## core's map files, through a MapFile. Falls back to the noise generator if
## the map can't be loaded. Press "Reload map" after regenerating a map.
func reload_editor_map() -> void:
	var id := _editor_map_id()
	var file := MapFile.new()
	if id != "" and file.open(id) and load_map(file, null):
		return
	_from_map = false
	_clear_chunks()
	generate()


## The map to preview: `preview_map_id` if set, else the scene root's
## `map_id` (Main's), else nothing.
func _editor_map_id() -> String:
	if preview_map_id != "":
		return preview_map_id
	var root: Node = owner
	if root == null and Engine.is_editor_hint():
		root = get_tree().edited_scene_root
	if root and root.get("map_id"):
		return str(root.get("map_id"))
	return ""


## Show a map's terrain. `source` is the running game's SimBridge or, in the
## editor, a MapFile (both answer get_terrain / get_terrain_chunk). The map is cut into
## square chunks. Every chunk is drawn coarsely (every `far_step`-th sample);
## chunks within `detail_radius_m` of `focus` (the player) are swapped for
## full detail, and swapped back once more than `detail_radius_m +
## detail_keep_margin_m` away. Collision is full detail everywhere.
## Returns false if the map has no terrain.
func load_map(source: Object, focus: Node3D) -> bool:
	var t: Dictionary = source.get_terrain()
	if not t.has("heights"):
		return false
	_from_map = true
	_regen_queued = false
	_source = source
	_focus = focus
	_samples = t.samples
	_cell = t.cell_m
	_heights = t.heights
	_lo = t.min
	_hi = t.max
	size_m = (_samples - 1) * _cell
	cell_size_m = _cell
	if _mesh_instance:
		_mesh_instance.queue_free()
		_mesh_instance = null
	_build_collision_from(t.collision_heights)
	_clear_chunks()
	_chunk_cells = maxi(1, int(round(chunk_m / _cell)))
	_chunks_per_side = ceili(float(_samples - 1) / _chunk_cells)
	# The editor has no player to follow: show everything a little finer.
	var coarse_step := editor_step if Engine.is_editor_hint() else far_step
	for cz in _chunks_per_side:
		for cx in _chunks_per_side:
			_far[Vector2i(cx, cz)] = _make_chunk(Vector2i(cx, cz), coarse_step)
	_detail_checked_at = Vector3.INF
	_update_detail()
	return true


## How many chunks are drawn at full detail right now (for tests and the dev
## panel).
func detail_chunk_count() -> int:
	return _near.size()


func _process(_delta: float) -> void:
	if _focus and _focus.global_position.distance_to(_detail_checked_at) >= detail_recheck_m:
		_update_detail()


## Swap chunks near the focus to full detail, and far ones back to coarse.
func _update_detail() -> void:
	if not _focus:
		return
	var here := _focus.global_position
	_detail_checked_at = here
	for cz in _chunks_per_side:
		for cx in _chunks_per_side:
			var key := Vector2i(cx, cz)
			var d := _chunk_distance(key, here)
			if d <= detail_radius_m and not _near.has(key):
				_near[key] = _make_chunk(key, 1)
				_far[key].visible = false
			elif d > detail_radius_m + detail_keep_margin_m and _near.has(key):
				_near[key].queue_free()
				_near.erase(key)
				_far[key].visible = true


## Ground distance from a point to the nearest edge of a chunk (0 inside it).
func _chunk_distance(key: Vector2i, p: Vector3) -> float:
	var half := size_m * 0.5
	var size := _chunk_cells * _cell
	var x0 := global_position.x - half + key.x * size
	var z0 := global_position.z - half + key.y * size
	var dx := maxf(maxf(x0 - p.x, p.x - (x0 + size)), 0.0)
	var dz := maxf(maxf(z0 - p.z, p.z - (z0 + size)), 0.0)
	return Vector2(dx, dz).length()


## One chunk's mesh from the core (built in Rust), at `step` detail.
func _make_chunk(key: Vector2i, step: int) -> MeshInstance3D:
	var m: Dictionary = _source.get_terrain_chunk(key.x * _chunk_cells, key.y * _chunk_cells,
		_chunk_cells, step, _lo, _hi, skirt_m)
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = m.vertices
	arrays[Mesh.ARRAY_NORMAL] = m.normals
	arrays[Mesh.ARRAY_COLOR] = m.colors
	arrays[Mesh.ARRAY_INDEX] = m.indices
	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	var mi := MeshInstance3D.new()
	mi.name = "Chunk_%d_%d_%s" % [key.x, key.y, "near" if step == 1 else "far"]
	mi.mesh = mesh
	mi.material_override = _terrain_material()
	add_child(mi, false, Node.INTERNAL_MODE_FRONT)
	# Vertices are in world space; undo this node's own offset.
	mi.global_transform = Transform3D.IDENTITY
	return mi


func _clear_chunks() -> void:
	for mi in _far.values():
		mi.queue_free()
	for mi in _near.values():
		mi.queue_free()
	_far.clear()
	_near.clear()


func _terrain_material() -> ShaderMaterial:
	if _material == null:
		_material = ShaderMaterial.new()
		_material.shader = preload("res://shaders/terrain.gdshader")
	_material.set_shader_parameter("grid_spacing", grid_spacing_m)
	return _material


## Terrain height in metres at a world-space XZ position (bilinear).
func get_height_at(world_x: float, world_z: float) -> float:
	if _samples == 0:
		return 0.0
	var local := to_local(Vector3(world_x, 0.0, world_z))
	var half := size_m * 0.5
	var fx := clampf((local.x + half) / _cell, 0.0, _samples - 1.001)
	var fz := clampf((local.z + half) / _cell, 0.0, _samples - 1.001)
	var ix := int(fx)
	var iz := int(fz)
	var tx := fx - ix
	var tz := fz - iz
	var h00 := _heights[iz * _samples + ix]
	var h10 := _heights[iz * _samples + ix + 1]
	var h01 := _heights[(iz + 1) * _samples + ix]
	var h11 := _heights[(iz + 1) * _samples + ix + 1]
	return global_position.y + lerpf(lerpf(h00, h10, tx), lerpf(h01, h11, tx), tz)


## World-space XZ area the terrain covers (x = world x, y = world z).
func get_bounds() -> Rect2:
	var half := size_m * 0.5
	var c := global_position
	return Rect2(c.x - half, c.z - half, size_m, size_m)


## Height samples (metres, relative to this node), row by row along +Z.
## There are `get_sample_count()` samples per side, `get_cell_size()` metres apart.
func get_height_grid() -> PackedFloat32Array:
	return _heights


func get_sample_count() -> int:
	return _samples


func get_cell_size() -> float:
	return _cell


func generate() -> void:
	_regen_queued = false
	_from_map = false
	_clear_chunks()
	_build_heights()
	_build_mesh()
	_build_collision()


func _queue_regen() -> void:
	if not is_inside_tree() or _regen_queued or _from_map:
		return
	_regen_queued = true
	_regen_if_queued.call_deferred()


## A queued regeneration is skipped if `generate()` already ran in between.
func _regen_if_queued() -> void:
	if _regen_queued:
		generate()


func _build_heights() -> void:
	var cells := maxi(1, int(round(size_m / cell_size_m)))
	_cell = size_m / cells  # actual spacing, snapped so the grid covers size_m exactly
	_samples = cells + 1

	var noise := FastNoiseLite.new()
	noise.noise_type = FastNoiseLite.TYPE_SIMPLEX_SMOOTH
	noise.seed = noise_seed
	noise.frequency = noise_frequency
	noise.fractal_type = FastNoiseLite.FRACTAL_FBM
	noise.fractal_octaves = octaves

	var ridge := FastNoiseLite.new()
	ridge.noise_type = FastNoiseLite.TYPE_SIMPLEX_SMOOTH
	ridge.seed = noise_seed + 7
	ridge.frequency = noise_frequency * 1.3
	ridge.fractal_type = FastNoiseLite.FRACTAL_RIDGED
	ridge.fractal_octaves = maxi(1, octaves - 1)

	var half := size_m * 0.5
	_heights.resize(_samples * _samples)
	for z in _samples:
		for x in _samples:
			var wx := x * _cell - half
			var wz := z * _cell - half
			var hills := noise.get_noise_2d(wx, wz) * 0.5 + 0.5        # 0..1
			var ridges := ridge.get_noise_2d(wx, wz) * 0.5 + 0.5        # 0..1
			var h := lerpf(hills, ridges, ridge_amount)
			# Mountains rise toward the edges; the centre stays walkable.
			var dist := Vector2(wx, wz).length()
			var edge := smoothstep(flat_center_radius_m, half, dist)
			var mask := lerpf(center_roughness, 1.0, edge)
			_heights[z * _samples + x] = h * h * max_height_m * mask
	# Shift down so the centre sits at roughly y = 0.
	var c := int(_samples / 2.0)  # centre sample (must be a whole index)
	var offset := _heights[c * _samples + c]
	for i in _heights.size():
		_heights[i] -= offset


func _build_mesh() -> void:
	var half := size_m * 0.5
	var lo := INF
	var hi := -INF
	for h in _heights:
		lo = minf(lo, h)
		hi = maxf(hi, h)
	var span := maxf(hi - lo, 0.001)

	var verts := PackedVector3Array()
	var normals := PackedVector3Array()
	var colors := PackedColorArray()
	verts.resize(_samples * _samples)
	normals.resize(verts.size())
	colors.resize(verts.size())

	for z in _samples:
		for x in _samples:
			var i := z * _samples + x
			var h := _heights[i]
			verts[i] = Vector3(x * _cell - half, h, z * _cell - half)
			var hl := _heights[z * _samples + maxi(x - 1, 0)]
			var hr := _heights[z * _samples + mini(x + 1, _samples - 1)]
			var hd := _heights[maxi(z - 1, 0) * _samples + x]
			var hu := _heights[mini(z + 1, _samples - 1) * _samples + x]
			var n := Vector3(hl - hr, 2.0 * _cell, hd - hu).normalized()
			normals[i] = n
			# Grayscale: higher is lighter, steeper is darker (rock vs snowy flats).
			var height01 := (h - lo) / span
			var steep := 1.0 - n.y
			var g := clampf(0.22 + height01 * 0.6 - steep * 1.4, 0.08, 0.95)
			colors[i] = Color(g, g, g)

	var indices := PackedInt32Array()
	indices.resize((_samples - 1) * (_samples - 1) * 6)
	var k := 0
	for z in _samples - 1:
		for x in _samples - 1:
			var a := z * _samples + x
			var b := a + 1
			var c := a + _samples
			var d := c + 1
			indices[k] = a; indices[k + 1] = b; indices[k + 2] = c
			indices[k + 3] = b; indices[k + 4] = d; indices[k + 5] = c
			k += 6

	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = verts
	arrays[Mesh.ARRAY_NORMAL] = normals
	arrays[Mesh.ARRAY_COLOR] = colors
	arrays[Mesh.ARRAY_INDEX] = indices
	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)

	if _mesh_instance == null:
		_mesh_instance = MeshInstance3D.new()
		_mesh_instance.name = "TerrainMesh"
		add_child(_mesh_instance, false, Node.INTERNAL_MODE_FRONT)
	_mesh_instance.mesh = mesh
	_update_material()


func _update_material() -> void:
	if _mesh_instance == null:
		return
	if _material == null:
		_material = ShaderMaterial.new()
		_material.shader = preload("res://shaders/terrain.gdshader")
	_material.set_shader_parameter("grid_spacing", grid_spacing_m)
	_mesh_instance.material_override = _material


## Collision from heights already divided by the cell size (see below).
func _build_collision_from(scaled: PackedFloat32Array) -> void:
	if _shape_node == null:
		var body := StaticBody3D.new()
		body.name = "TerrainBody"
		add_child(body, false, Node.INTERNAL_MODE_FRONT)
		_shape_node = CollisionShape3D.new()
		_shape_node.name = "Shape"
		body.add_child(_shape_node)
	var shape := HeightMapShape3D.new()
	shape.map_width = _samples
	shape.map_depth = _samples
	shape.map_data = scaled
	_shape_node.shape = shape
	_shape_node.scale = Vector3.ONE * _cell


func _build_collision() -> void:
	if _shape_node == null:
		var body := StaticBody3D.new()
		body.name = "TerrainBody"
		add_child(body, false, Node.INTERNAL_MODE_FRONT)
		_shape_node = CollisionShape3D.new()
		_shape_node.name = "Shape"
		body.add_child(_shape_node)
	var shape_node := _shape_node

	# HeightMapShape3D samples are 1 unit apart, so scale the node uniformly by
	# cell size and store heights in those scaled units.
	var scaled := PackedFloat32Array()
	scaled.resize(_heights.size())
	for i in _heights.size():
		scaled[i] = _heights[i] / _cell
	var shape := HeightMapShape3D.new()
	shape.map_width = _samples
	shape.map_depth = _samples
	shape.map_data = scaled
	shape_node.shape = shape
	shape_node.scale = Vector3.ONE * _cell
