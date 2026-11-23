class_name Player
extends CharacterBody3D
## Placeholder first-person player: a capsule with stances (stand / crouch /
## prone), movement speeds (tactical / walk / sprint), jumping and mouse look.
## Horizontal mouse turns the body; vertical mouse tilts the head.
## Every tuning value is exported.

## Must match `Stance` in core/src/components.rs.
enum Stance { STANDING, CROUCHING, PRONE }
## Must match `Gait` in core/src/components.rs.
enum Gait { STILL, TACTICAL, WALK, SPRINT }

@export_group("Speeds (m/s)")
## Slow, controlled movement with the weapon up (hold Ctrl).
@export var tactical_speed: float = 1.6
@export var walk_speed: float = 3.0
## Hold Shift while moving forward. Stamina and weight will scale this later.
@export var sprint_speed: float = 5.5
@export var crouch_speed: float = 1.5
@export var crawl_speed: float = 0.6
## Sprint only works if forward input is at least this strong (0..1).
@export_range(0.0, 1.0) var sprint_min_forward: float = 0.5

@export_group("Acceleration (m/s²)")
@export var ground_accel: float = 25.0
@export var air_accel: float = 4.0

@export_group("Jump & gravity")
@export var jump_velocity: float = 5.0
## Multiplier on the project's gravity (9.8 m/s²).
@export var gravity_scale: float = 1.0

@export_group("Stance heights (m)")
@export var standing_height: float = 1.8
@export var crouching_height: float = 1.2
@export var prone_height: float = 0.7
@export var standing_eye: float = 1.65
@export var crouching_eye: float = 1.05
@export var prone_eye: float = 0.35
## How fast the eye moves to a new stance height (higher is snappier).
@export var stance_change_speed: float = 10.0

@export_group("Look")
@export var mouse_sensitivity: float = 0.0025
@export var invert_y: bool = false
## Mouse sensitivity multiplier while aiming down sights.
@export var ads_sensitivity_scale: float = 0.6
## How far you can look down / up, in degrees.
@export_range(-89.0, 0.0) var min_pitch_deg: float = -85.0
@export_range(0.0, 89.0) var max_pitch_deg: float = 85.0

@export_group("Safety")
## Fall below this height and you respawn at the spawn point.
@export var kill_height: float = -200.0

var spawn_point := Vector3.ZERO
var stance: Stance = Stance.STANDING
var gait: Gait = Gait.STILL
## Set by the view model while the player aims down sights (no sprinting, slower).
var is_aiming := false

@onready var _head: Node3D = $Head
@onready var _collision: CollisionShape3D = $Collision
@onready var _body: Node3D = $Body
@onready var _gravity: float = ProjectSettings.get_setting("physics/3d/default_gravity")
var _shape: CapsuleShape3D


func _ready() -> void:
	# Own copy of the shape, so changing stance doesn't affect other instances.
	_shape = (_collision.shape as CapsuleShape3D).duplicate()
	_collision.shape = _shape
	_apply_stance_shape(stance)
	_head.position.y = _eye_height(stance)
	Input.mouse_mode = Input.MOUSE_MODE_CAPTURED


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseMotion and Input.mouse_mode == Input.MOUSE_MODE_CAPTURED:
		look(event.relative)
	elif event.is_action_pressed("crouch"):
		try_set_stance(Stance.STANDING if stance == Stance.CROUCHING else Stance.CROUCHING)
	elif event.is_action_pressed("prone"):
		try_set_stance(Stance.STANDING if stance == Stance.PRONE else Stance.PRONE)
	elif event.is_action_pressed("ui_cancel"):
		Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
	elif event is InputEventMouseButton and event.pressed:
		Input.mouse_mode = Input.MOUSE_MODE_CAPTURED


## Turn by a mouse delta in pixels: x turns the body, y tilts the head.
func look(mouse_delta: Vector2) -> void:
	var sens := mouse_sensitivity * (ads_sensitivity_scale if is_aiming else 1.0)
	rotate_y(-mouse_delta.x * sens)
	var dy := mouse_delta.y * sens * (-1.0 if invert_y else 1.0)
	_head.rotation.x = clampf(_head.rotation.x - dy,
		deg_to_rad(min_pitch_deg), deg_to_rad(max_pitch_deg))


## Recoil: nudge the view up (pitch) and sideways (yaw), in radians.
func add_view_kick(pitch: float, yaw: float) -> void:
	rotate_y(yaw)
	_head.rotation.x = clampf(_head.rotation.x + pitch,
		deg_to_rad(min_pitch_deg), deg_to_rad(max_pitch_deg))


## Change stance if there's room (you can't stand up under something).
## Returns true if the stance changed.
func try_set_stance(new_stance: Stance) -> bool:
	if new_stance == stance:
		return true
	if _height_of(new_stance) > _height_of(stance) and not _has_headroom(_height_of(new_stance)):
		return false
	stance = new_stance
	_apply_stance_shape(stance)
	return true


## Top speed for the current stance and gait.
func current_max_speed() -> float:
	match stance:
		Stance.CROUCHING: return crouch_speed
		Stance.PRONE: return crawl_speed
	match gait:
		Gait.TACTICAL: return tactical_speed
		Gait.SPRINT: return sprint_speed
	return walk_speed


func _physics_process(delta: float) -> void:
	var input := Input.get_vector("move_left", "move_right", "move_forward", "move_back")
	var wants_sprint := Input.is_action_pressed("sprint") and -input.y >= sprint_min_forward and not is_aiming

	# Jumping or sprinting from crouch/prone stands you up first.
	if stance != Stance.STANDING and (wants_sprint or Input.is_action_just_pressed("jump")):
		try_set_stance(Stance.STANDING)

	if not is_on_floor():
		velocity.y -= _gravity * gravity_scale * delta
	elif Input.is_action_just_pressed("jump") and stance == Stance.STANDING:
		velocity.y = jump_velocity

	gait = _pick_gait(input, wants_sprint)

	# Movement is relative to where the body faces.
	var dir := global_basis * Vector3(input.x, 0.0, input.y)
	dir.y = 0.0
	dir = dir.normalized() * input.length()
	var target := dir * current_max_speed()
	var accel := ground_accel if is_on_floor() else air_accel
	var horizontal := Vector2(velocity.x, velocity.z).move_toward(
		Vector2(target.x, target.z), accel * delta)
	velocity.x = horizontal.x
	velocity.z = horizontal.y

	move_and_slide()

	# Ease the eye toward the stance height so changes aren't a jump cut.
	_head.position.y = lerpf(_head.position.y, _eye_height(stance),
		1.0 - exp(-stance_change_speed * delta))

	if global_position.y < kill_height:
		respawn()


func respawn() -> void:
	global_position = spawn_point
	velocity = Vector3.ZERO


func _pick_gait(input: Vector2, wants_sprint: bool) -> Gait:
	if input.length_squared() < 0.01:
		return Gait.STILL
	if stance != Stance.STANDING:
		return Gait.WALK
	if wants_sprint:
		return Gait.SPRINT
	if is_aiming or Input.is_action_pressed("tactical"):
		return Gait.TACTICAL
	return Gait.WALK


func _height_of(s: Stance) -> float:
	match s:
		Stance.CROUCHING: return crouching_height
		Stance.PRONE: return prone_height
	return standing_height


func _eye_height(s: Stance) -> float:
	match s:
		Stance.CROUCHING: return crouching_eye
		Stance.PRONE: return prone_eye
	return standing_eye


func _apply_stance_shape(s: Stance) -> void:
	var h := _height_of(s)
	_shape.height = maxf(h, _shape.radius * 2.0)
	_collision.position.y = _shape.height * 0.5
	# Squash the placeholder body to match (only its shadow is visible in first person).
	_body.scale.y = h / standing_height


## Is there room for a capsule of this height where we stand?
func _has_headroom(height: float) -> bool:
	var probe := CapsuleShape3D.new()
	probe.radius = _shape.radius * 0.9
	probe.height = maxf(height - 0.45, probe.radius * 2.0)
	var params := PhysicsShapeQueryParameters3D.new()
	params.shape = probe
	# Lift the probe a little so the ground under our feet doesn't count.
	params.transform = Transform3D(Basis(), global_position + Vector3.UP * (0.4 + probe.height * 0.5))
	params.exclude = [get_rid()]
	params.collision_mask = collision_mask
	return get_world_3d().direct_space_state.intersect_shape(params, 1).is_empty()
