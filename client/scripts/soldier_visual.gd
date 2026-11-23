class_name SoldierVisual
extends StaticBody3D
## Placeholder look for an AI soldier: a capsule carrying their weapon at low
## ready, with a name tag. It only shows what the Rust sim says; it never
## decides anything itself.

const WEAPON_SCENES := {
	"m16a2": preload("res://assets/models/weapons/m16a2.glb"),
}

## Weapon pose on the capsule (low ready): position and rotation in degrees.
@export var weapon_position := Vector3(0.14, 1.22, -0.46)
@export var weapon_rotation_deg := Vector3(-30.0, 18.0, 0.0)
## Name tags fade out beyond this distance (metres).
@export var label_range := 35.0

var soldier_id := -1

var _body: MeshInstance3D
var _label: Label3D
var _weapon: Node3D
var _weapon_id := ""


func _init() -> void:
	var shape := CapsuleShape3D.new()
	shape.radius = 0.35
	shape.height = 1.8
	var col := CollisionShape3D.new()
	col.shape = shape
	col.position.y = 0.9
	add_child(col)

	var mesh := CapsuleMesh.new()
	mesh.radius = 0.35
	mesh.height = 1.8
	_body = MeshInstance3D.new()
	_body.mesh = mesh
	_body.position.y = 0.9
	add_child(_body)

	var visor := MeshInstance3D.new()
	var vbox := BoxMesh.new()
	vbox.size = Vector3(0.4, 0.12, 0.2)
	var vmat := StandardMaterial3D.new()
	vmat.albedo_color = Color(0.1, 0.1, 0.1)
	vbox.material = vmat
	visor.mesh = vbox
	visor.position = Vector3(0.0, 1.5, -0.3)
	add_child(visor)

	_label = Label3D.new()
	_label.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	_label.position.y = 2.15
	_label.pixel_size = 0.004
	_label.font_size = 40
	_label.outline_size = 10
	_label.no_depth_test = false
	add_child(_label)


func _ready() -> void:
	_label.visibility_range_end = label_range
	_label.visibility_range_end_margin = 5.0
	_label.visibility_range_fade_mode = GeometryInstance3D.VISIBILITY_RANGE_FADE_SELF


## Set everything that doesn't change often (called once, then on changes).
func setup(info: Dictionary, shade: float) -> void:
	soldier_id = info.id
	name = "Soldier_%d" % soldier_id
	var mat := StandardMaterial3D.new()
	mat.albedo_color = Color(shade, shade, shade)
	mat.roughness = 0.8
	_body.material_override = mat
	_set_weapon(info.weapon_id)
	update_state(info)


## Apply a fresh snapshot from the sim.
func update_state(info: Dictionary) -> void:
	position = info.position
	rotation.y = info.yaw
	var team: String = info.fire_team
	var team_txt := (" %s" % team.substr(0, 1)) if team != "" else ""
	_label.text = "%s %s\n%s%s · %d+%d" % [info.rank, info.name, info.role_abbrev, team_txt,
		info.rounds_in_weapon, info.reserve_rounds]
	visible = info.life != "dead"
	if info.weapon_id != _weapon_id:
		_set_weapon(info.weapon_id)


func _set_weapon(weapon_id: String) -> void:
	_weapon_id = weapon_id
	if _weapon:
		_weapon.queue_free()
		_weapon = null
	if WEAPON_SCENES.has(weapon_id):
		_weapon = WEAPON_SCENES[weapon_id].instantiate()
		_weapon.position = weapon_position
		_weapon.rotation_degrees = weapon_rotation_deg
		add_child(_weapon)
