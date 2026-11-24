class_name SideRing
extends Node3D
## A flat ring on the ground showing which side a soldier fights for (blue or
## red), with a notch on the side they face. Make it a child of the soldier's
## node: the notch points along the node's facing (-Z), which follows the yaw
## the sim reports.

const COLORS := {"blue": Color(0.25, 0.5, 1.0), "red": Color(0.95, 0.22, 0.2)}

@export var radius := 0.55
@export var width := 0.09
@export var height := 0.04

var _ring: MeshInstance3D
var _notch: MeshInstance3D
var _mat: StandardMaterial3D


func _init() -> void:
	_mat = StandardMaterial3D.new()
	_mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	_mat.albedo_color = COLORS.blue

	var torus := TorusMesh.new()
	torus.inner_radius = radius - width * 0.5
	torus.outer_radius = radius + width * 0.5
	torus.rings = 32
	torus.material = _mat
	_ring = MeshInstance3D.new()
	_ring.mesh = torus
	_ring.position.y = height
	_ring.scale.y = 0.4
	_ring.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(_ring)

	# A small wedge just outside the ring, in front of the soldier.
	var box := BoxMesh.new()
	box.size = Vector3(0.16, 0.03, 0.22)
	box.material = _mat
	_notch = MeshInstance3D.new()
	_notch.mesh = box
	_notch.position = Vector3(0.0, height, -(radius + 0.12))
	_notch.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(_notch)


## "blue" or "red" (as the sim reports it).
func set_side(side: String) -> void:
	_mat.albedo_color = COLORS.get(side, Color.WHITE)
