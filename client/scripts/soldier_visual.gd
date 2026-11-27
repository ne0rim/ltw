class_name SoldierVisual
extends StaticBody3D
## How an AI soldier looks: a dressed, animated soldier (SoldierBody) holding
## their weapon, with a name tag, a side ring (blue/red, notch = facing) and a
## selection ring. It only shows what the Rust sim says; it never decides
## anything itself.

const WEAPON_SCENES := {
	"m16a2": preload("res://assets/models/weapons/m16a2.glb"),
}
## Physics layer for soldiers (so order raycasts can ignore them).
const SOLDIER_LAYER := 2

## Name tags fade out beyond this distance (metres).
@export var label_range := 35.0
## How quickly the capsule glides to the sim's position (higher = snappier).
@export var follow_speed := 14.0

var soldier_id := -1
var activity := "idle"

var _body: SoldierBody
var _label: Label3D
var _ring: MeshInstance3D
var _side_ring: SideRing
var _weapon: Node3D
var _weapon_anim: AnimationPlayer
var _muzzle: Node3D
var _weapon_id := ""
var _target_pos := Vector3.ZERO
var _target_yaw := 0.0
var _placed := false
var _dead := false
var _speed := 0.0
var _last_pos := Vector3.ZERO


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

	_body = SoldierBody.new()
	# The model faces +Z (glTF); a soldier's facing is -Z.
	_body.rotation.y = PI
	add_child(_body)

	_ring = MeshInstance3D.new()
	var torus := TorusMesh.new()
	torus.inner_radius = 0.82
	torus.outer_radius = 0.9
	var rmat := StandardMaterial3D.new()
	rmat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	rmat.albedo_color = Color(1.0, 1.0, 1.0)
	torus.material = rmat
	_ring.mesh = torus
	_ring.position.y = 0.05
	_ring.visible = false
	add_child(_ring)

	_side_ring = SideRing.new()
	add_child(_side_ring)

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
## `_shade` was the capsule's unit tint; the uniform is the same for everyone now.
func setup(info: Dictionary, _shade: float, ground_y: float) -> void:
	soldier_id = info.id
	name = "Soldier_%d" % soldier_id
	_set_weapon(info.weapon_id)
	_side_ring.set_side(info.side)
	update_state(info, ground_y)


## Apply a fresh snapshot from the sim. `ground_y` is the terrain height here.
func update_state(info: Dictionary, ground_y: float) -> void:
	var p: Vector3 = info.position
	_target_pos = Vector3(p.x, ground_y, p.z)
	_target_yaw = info.yaw
	if not _placed:
		position = _target_pos
		rotation.y = _target_yaw
		_last_pos = position
		_placed = true
	activity = info.activity
	var team: String = info.fire_team
	var team_txt := (" %s" % team.substr(0, 1)) if team != "" else ""
	var doing := "" if activity == "idle" else " · %s" % activity
	_label.text = "%s %s\n%s%s · %d+%d%s" % [info.rank, info.name, info.role_abbrev, team_txt,
		info.rounds_in_weapon, info.reserve_rounds, doing]
	if info.life == "dead" and not _dead:
		_lie_down()
	if info.weapon_id != _weapon_id:
		_set_weapon(info.weapon_id)


func set_selected(on: bool) -> void:
	_ring.visible = on and not _dead


## Killed: the body falls forward and lies face down where they stood. The
## rifle drops beside them. Nothing about them changes after this.
## (A placeholder until there's a proper death animation.)
func _lie_down() -> void:
	_dead = true
	activity = "idle"
	_body.freeze()
	# Pivot about the feet: upright -> lying along their facing, chest down
	# (the model faces +Z, so tip it +90 about X, then turn it to face -Z).
	_body.rotation = Vector3(deg_to_rad(90.0), PI, 0.0)
	_body.position = Vector3(0.0, 0.14, 0.0)
	_ring.visible = false
	_side_ring.visible = false
	if _weapon:
		var t := _weapon.global_transform
		_weapon.get_parent().remove_child(_weapon)
		add_child(_weapon)
		_weapon.global_transform = t
		_weapon.position = Vector3(0.55, 0.04, -0.6)
		_weapon.rotation_degrees = Vector3(0.0, 80.0, 90.0)
	_label.text = _label.text.get_slice("\n", 0) + "\nKIA"
	# The body is no longer an obstacle or a target for order raycasts.
	collision_layer = 0


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
	if _dead or delta <= 0.0:
		return
	# Ground speed as seen on screen, smoothed so the gait doesn't flicker.
	var moved := position - _last_pos
	_last_pos = position
	var ground_speed := Vector2(moved.x, moved.z).length() / delta
	_speed = lerpf(_speed, ground_speed, 1.0 - exp(-6.0 * delta))
	var aiming := activity == "firing" or activity == "reloading"
	_body.animate(_speed, aiming, delta)


func _set_weapon(weapon_id: String) -> void:
	_weapon_id = weapon_id
	if _weapon:
		_body.hold(null)
		_weapon = null
	_weapon_anim = null
	_muzzle = null
	if WEAPON_SCENES.has(weapon_id):
		_weapon = _body.hold(WEAPON_SCENES[weapon_id])
		var anims := _weapon.find_children("*", "AnimationPlayer", true, false)
		_weapon_anim = anims[0] if anims.size() > 0 else null
		_muzzle = _weapon.find_child("socket_muzzle", true, false)
