class_name SoldierVisual
extends StaticBody3D
## Placeholder look for an AI soldier: a capsule carrying their weapon, with a
## name tag and a selection ring. It only shows what the Rust sim says; it
## never decides anything itself.

const WEAPON_SCENES := {
	"m16a2": preload("res://assets/models/weapons/m16a2.glb"),
}
## Physics layer for soldiers (so order raycasts can ignore them).
const SOLDIER_LAYER := 2

## Weapon at low ready (idle, moving): position and rotation in degrees.
@export var weapon_position := Vector3(0.14, 1.22, -0.46)
@export var weapon_rotation_deg := Vector3(-30.0, 18.0, 0.0)
## Weapon shouldered (firing, reloading).
@export var aim_position := Vector3(0.12, 1.45, -0.34)
@export var aim_rotation_deg := Vector3(0.0, 0.0, 0.0)
@export var pose_speed := 8.0
## Name tags fade out beyond this distance (metres).
@export var label_range := 35.0
## How quickly the capsule glides to the sim's position (higher = snappier).
@export var follow_speed := 14.0

var soldier_id := -1
var activity := "idle"

var _body: MeshInstance3D
var _label: Label3D
var _ring: MeshInstance3D
var _weapon: Node3D
var _weapon_anim: AnimationPlayer
var _muzzle: Node3D
var _weapon_id := ""
var _target_pos := Vector3.ZERO
var _target_yaw := 0.0
var _aim_t := 0.0
var _placed := false


func _init() -> void:
	collision_layer = 1 << (SOLDIER_LAYER - 1)
	collision_mask = 0
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

	_ring = MeshInstance3D.new()
	var torus := TorusMesh.new()
	torus.inner_radius = 0.5
	torus.outer_radius = 0.62
	var rmat := StandardMaterial3D.new()
	rmat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	rmat.albedo_color = Color(0.03, 0.03, 0.03)
	torus.material = rmat
	_ring.mesh = torus
	_ring.position.y = 0.05
	_ring.visible = false
	add_child(_ring)

	_label = Label3D.new()
	_label.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	_label.position.y = 2.15
	_label.pixel_size = 0.004
	_label.font_size = 40
	_label.outline_size = 10
	add_child(_label)


func _ready() -> void:
	_label.visibility_range_end = label_range
	_label.visibility_range_end_margin = 5.0
	_label.visibility_range_fade_mode = GeometryInstance3D.VISIBILITY_RANGE_FADE_SELF


## Set everything that doesn't change often (called once).
func setup(info: Dictionary, shade: float, ground_y: float) -> void:
	soldier_id = info.id
	name = "Soldier_%d" % soldier_id
	var mat := StandardMaterial3D.new()
	mat.albedo_color = Color(shade, shade, shade)
	mat.roughness = 0.8
	_body.material_override = mat
	_set_weapon(info.weapon_id)
	update_state(info, ground_y)


## Apply a fresh snapshot from the sim. `ground_y` is the terrain height here.
func update_state(info: Dictionary, ground_y: float) -> void:
	var p: Vector3 = info.position
	_target_pos = Vector3(p.x, ground_y, p.z)
	_target_yaw = info.yaw
	if not _placed:
		position = _target_pos
		rotation.y = _target_yaw
		_placed = true
	activity = info.activity
	var team: String = info.fire_team
	var team_txt := (" %s" % team.substr(0, 1)) if team != "" else ""
	var doing := "" if activity == "idle" else " · %s" % activity
	_label.text = "%s %s\n%s%s · %d+%d%s" % [info.rank, info.name, info.role_abbrev, team_txt,
		info.rounds_in_weapon, info.reserve_rounds, doing]
	visible = info.life != "dead"
	if info.weapon_id != _weapon_id:
		_set_weapon(info.weapon_id)


func set_selected(on: bool) -> void:
	_ring.visible = on


## World position of the weapon's muzzle (for tracers).
func muzzle_position() -> Vector3:
	if _muzzle:
		return _muzzle.global_position
	return global_position + Vector3.UP * 1.45


## Play the weapon's firing animation.
func play_shot() -> void:
	if _weapon_anim and _weapon_anim.has_animation("fire"):
		_weapon_anim.stop()
		_weapon_anim.play("fire", -1, 2.7)


func _process(delta: float) -> void:
	var k := 1.0 - exp(-follow_speed * delta)
	position = position.lerp(_target_pos, k)
	rotation.y = lerp_angle(rotation.y, _target_yaw, k)
	var aiming := activity == "firing" or activity == "reloading"
	_aim_t = move_toward(_aim_t, 1.0 if aiming else 0.0, delta * pose_speed * 0.5)
	if _weapon:
		var t := smoothstep(0.0, 1.0, _aim_t)
		_weapon.position = weapon_position.lerp(aim_position, t)
		_weapon.rotation_degrees = weapon_rotation_deg.lerp(aim_rotation_deg, t)


func _set_weapon(weapon_id: String) -> void:
	_weapon_id = weapon_id
	if _weapon:
		_weapon.queue_free()
		_weapon = null
	_weapon_anim = null
	_muzzle = null
	if WEAPON_SCENES.has(weapon_id):
		_weapon = WEAPON_SCENES[weapon_id].instantiate()
		_weapon.position = weapon_position
		_weapon.rotation_degrees = weapon_rotation_deg
		add_child(_weapon)
		var anims := _weapon.find_children("*", "AnimationPlayer", true, false)
		_weapon_anim = anims[0] if anims.size() > 0 else null
		_muzzle = _weapon.find_child("socket_muzzle", true, false)
