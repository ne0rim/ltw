@tool
class_name Terrain
extends Node3D
## Generates a grayscale, mountainous test terrain with matching collision.
## Every setting is exported. Change it in the Inspector and press Regenerate.

@export_group("Size")
## Width and depth of the terrain, in metres.
@export var size_m: float = 512.0:
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

@export_tool_button("Regenerate") var _regen_button := generate

var _heights := PackedFloat32Array()
var _samples := 0
var _cell := 2.0
var _mesh_instance: MeshInstance3D
var _shape_node: CollisionShape3D
var _material: ShaderMaterial
var _regen_queued := false


func _ready() -> void:
	generate()


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
	_build_heights()
	_build_mesh()
	_build_collision()


func _queue_regen() -> void:
	if not is_inside_tree() or _regen_queued:
		return
	_regen_queued = true
	generate.call_deferred()


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
