class_name ShotEffects
extends Node3D
## Cheap visual effects for AI gunfire: muzzle flash, a tracer streak flying
## to the impact point, and a dust puff where it lands. Purely cosmetic; the
## sim decides where shots go.

@export var tracer_speed := 900.0        # m/s
@export var tracer_length := 4.5
@export var burst_gap := 0.075           # seconds between rounds in a burst
@export var puff_time := 0.6
@export var flash_time := 0.045

var _flash_mesh: SphereMesh
var _tracer_mesh: BoxMesh
var _puff_mesh: SphereMesh
var _puff_mat: StandardMaterial3D
var _active: Array = []                  # [kind, node, t, life, from, to]
var _queued: Array = []                  # [delay, from, to]


func _ready() -> void:
	var flash_mat := StandardMaterial3D.new()
	flash_mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	flash_mat.albedo_color = Color(1.0, 0.9, 0.7)
	_flash_mesh = SphereMesh.new()
	_flash_mesh.radius = 0.06
	_flash_mesh.height = 0.12
	_flash_mesh.material = flash_mat

	var tracer_mat := StandardMaterial3D.new()
	tracer_mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	tracer_mat.albedo_color = Color(1.0, 0.95, 0.8)
	_tracer_mesh = BoxMesh.new()
	_tracer_mesh.size = Vector3(0.045, 0.045, tracer_length)
	_tracer_mesh.material = tracer_mat

	_puff_mat = StandardMaterial3D.new()
	_puff_mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	_puff_mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	_puff_mat.albedo_color = Color(0.28, 0.27, 0.26, 0.8)
	_puff_mesh = SphereMesh.new()
	_puff_mesh.radius = 0.35
	_puff_mesh.height = 0.5
	_puff_mesh.material = _puff_mat


## One shot event: `rounds` rounds from `from` (muzzle) toward `to` (impact).
func shot(from: Vector3, to: Vector3, rounds: int) -> void:
	for i in rounds:
		_queued.append([i * burst_gap, from, to])


func _process(delta: float) -> void:
	for i in range(_queued.size() - 1, -1, -1):
		_queued[i][0] -= delta
		if _queued[i][0] <= 0.0:
			_fire_one(_queued[i][1], _queued[i][2])
			_queued.remove_at(i)

	for i in range(_active.size() - 1, -1, -1):
		var e: Array = _active[i]
		e[2] += delta
		var node: Node3D = e[1]
		var done: bool = e[2] >= e[3]
		match e[0]:
			"tracer":
				var p: Vector3 = (e[4] as Vector3).lerp(e[5], minf(e[2] / e[3], 1.0))
				node.global_position = p
				if done:
					_spawn_puff(e[5])
			"puff":
				var k: float = e[2] / e[3]
				node.scale = Vector3.ONE * lerpf(0.4, 1.8, k)
				var m: StandardMaterial3D = (node as MeshInstance3D).material_override
				m.albedo_color.a = 0.8 * (1.0 - k)
		if done:
			node.queue_free()
			_active.remove_at(i)


func _fire_one(from: Vector3, to: Vector3) -> void:
	var flash := MeshInstance3D.new()
	flash.mesh = _flash_mesh
	flash.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(flash)
	flash.global_position = from
	_active.append(["flash", flash, 0.0, flash_time, from, from])

	var dist := from.distance_to(to)
	if dist < 0.5:
		return
	var tracer := MeshInstance3D.new()
	tracer.mesh = _tracer_mesh
	tracer.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(tracer)
	tracer.global_position = from
	tracer.look_at(to, Vector3.UP if absf((to - from).normalized().y) < 0.99 else Vector3.RIGHT)
	_active.append(["tracer", tracer, 0.0, dist / tracer_speed, from, to])


func _spawn_puff(at: Vector3) -> void:
	var puff := MeshInstance3D.new()
	puff.mesh = _puff_mesh
	puff.material_override = _puff_mat.duplicate()
	puff.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(puff)
	puff.global_position = at + Vector3.UP * 0.2
	_active.append(["puff", puff, 0.0, puff_time, at, at])
