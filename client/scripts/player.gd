class_name Player
extends CharacterBody3D
## Placeholder player: a capsule with WASD movement, Space to jump and a
## third-person mouse-look camera. Every tuning value is exported.

@export_group("Movement")
@export var walk_speed: float = 5.0
@export var sprint_speed: float = 8.5
## How quickly the player reaches top speed on the ground (m/s²).
@export var ground_accel: float = 40.0
## How much control the player has while airborne (m/s²).
@export var air_accel: float = 8.0
@export var jump_velocity: float = 5.0
## Multiplier on the project's gravity.
@export var gravity_scale: float = 1.0

@export_group("Camera")
@export var mouse_sensitivity: float = 0.003
@export_range(-89.0, 0.0) var min_pitch_deg: float = -80.0
@export_range(0.0, 89.0) var max_pitch_deg: float = 60.0

@export_group("Safety")
## Fall below this height and you respawn at the spawn point.
@export var kill_height: float = -200.0

var spawn_point := Vector3.ZERO

@onready var _pivot: Node3D = $CameraPivot
@onready var _gravity: float = ProjectSettings.get_setting("physics/3d/default_gravity")


func _ready() -> void:
	$CameraPivot/SpringArm.add_excluded_object(get_rid())  # don't let the camera hit our own capsule
	Input.mouse_mode = Input.MOUSE_MODE_CAPTURED


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseMotion and Input.mouse_mode == Input.MOUSE_MODE_CAPTURED:
		_pivot.rotation.y -= event.relative.x * mouse_sensitivity
		_pivot.rotation.x = clampf(
			_pivot.rotation.x - event.relative.y * mouse_sensitivity,
			deg_to_rad(min_pitch_deg), deg_to_rad(max_pitch_deg))
	elif event.is_action_pressed("ui_cancel"):
		Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
	elif event is InputEventMouseButton and event.pressed:
		Input.mouse_mode = Input.MOUSE_MODE_CAPTURED


func _physics_process(delta: float) -> void:
	if not is_on_floor():
		velocity.y -= _gravity * gravity_scale * delta
	elif Input.is_action_just_pressed("jump"):
		velocity.y = jump_velocity

	# Movement is relative to where the camera is looking (yaw only).
	var input := Input.get_vector("move_left", "move_right", "move_forward", "move_back")
	var dir := (_pivot.global_basis * Vector3(input.x, 0.0, input.y))
	dir.y = 0.0
	dir = dir.normalized() * input.length()

	var speed := sprint_speed if Input.is_action_pressed("sprint") else walk_speed
	var accel := ground_accel if is_on_floor() else air_accel
	var target := dir * speed
	var horizontal := Vector2(velocity.x, velocity.z).move_toward(
		Vector2(target.x, target.z), accel * delta)
	velocity.x = horizontal.x
	velocity.z = horizontal.y

	# Turn the body (and its facing marker) toward the movement direction.
	if dir.length_squared() > 0.01:
		$Body.rotation.y = lerp_angle($Body.rotation.y, atan2(-dir.x, -dir.z), 12.0 * delta)

	move_and_slide()

	if global_position.y < kill_height:
		respawn()


func respawn() -> void:
	global_position = spawn_point
	velocity = Vector3.ZERO
