class_name ViewModel
extends Node3D
## First-person weapon + arms. Lives under the player's Camera3D.
## Everything weapon-specific comes from `data` (a WeaponViewData) and the
## weapon model's socket_* nodes, so this works for any weapon.
##
## Motion is code-driven: hip/aim/sprint/reload poses blended by springs,
## plus sway, bob, breathing, landing dip and recoil.

signal fired(muzzle: Transform3D)
signal reloaded

@export var data: WeaponViewData

## Where the arms' shoulder line sits relative to the eye (camera space).
@export var shoulder_offset := Vector3(0.0, -0.22, 0.12)

@export_group("Feel")
@export var sway_strength := 0.0016        # radians per pixel of mouse motion
@export var sway_max_deg := 6.0
@export var sway_return := 9.0
@export var bob_walk_hz := 1.8
@export var bob_sprint_hz := 2.6
@export var bob_height := 0.012
@export var bob_side := 0.010
@export var breathe_amount := 0.0025
@export var land_dip := 0.035
## Aiming scales sway and bob by this much.
@export var ads_motion_scale := 0.2
@export var pose_blend_speed := 10.0

@export_group("Muzzle flash")
@export var flash_time := 0.035
@export var flash_energy := 6.0

var ammo := 0
var mode_index := 0
var reloading := false
## Optional: where new magazines come from. Called when a reload starts; must
## return the rounds in the magazine being inserted, or -1 if there are none
## (e.g. the Rust sim's inventory). If unset, magazines are unlimited and full.
var magazine_source: Callable
## Set when a reload is refused because there are no magazines left.
var out_of_magazines := false
var _incoming_rounds := 0

var _player: Player
var _camera: Camera3D
var _base_fov := 75.0
var _pivot: Node3D
var _gun: Node3D
var _gun_skel: Skeleton3D
var _gun_anim: AnimationPlayer
var _arms: FPArms
var _sockets := {}
var _flash_light: OmniLight3D
var _flash_mesh: MeshInstance3D
var _flash_left := 0.0

var _aim_t := 0.0
var _sprint_t := 0.0
var _reload_pose_t := 0.0
var _cooldown := 0.0
var _burst_left := 0.0
var _sway := Vector2.ZERO           # current sway (radians)
var _mouse := Vector2.ZERO          # mouse motion this frame
var _recoil_pos := 0.0
var _recoil_rot := Vector2.ZERO     # pitch, yaw (radians)
var _bob_phase := 0.0
var _land := 0.0
var _was_on_floor := true
var _last_vy := 0.0
var _time := 0.0
var _reload_time := 0.0
var _mag_local := Transform3D()     # hand point relative to the magazine bone
var _selector_rest := Quaternion()
var _trigger_pulled := false        # a fresh press of the fire button this frame


func _ready() -> void:
	_camera = get_parent() as Camera3D
	if _camera:
		_base_fov = _camera.fov
	var n := get_parent()
	while n and not (n is Player):
		n = n.get_parent()
	_player = n as Player
	_pivot = Node3D.new()
	_pivot.name = "WeaponPivot"
	add_child(_pivot)
	_arms = FPArms.new()
	_arms.name = "Arms"
	_arms.position = shoulder_offset
	add_child(_arms)
	if data:
		equip(data)


## Swap to another weapon (any WeaponViewData).
func equip(new_data: WeaponViewData) -> void:
	data = new_data
	if _gun:
		_gun.queue_free()
	_gun = data.model.instantiate()
	_pivot.add_child(_gun)
	_gun_skel = _gun.find_children("*", "Skeleton3D", true, false)[0]
	_gun_anim = _gun.find_children("*", "AnimationPlayer", true, false)[0]
	_strip_static_tracks()
	_sockets.clear()
	for s in _gun.find_children("socket_*", "", true, false):
		_sockets[s.name] = s
	for g in _gun.find_children("*", "GeometryInstance3D", true, false):
		(g as GeometryInstance3D).cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	_setup_flash()
	_setup_magazine_point()
	var sel := _gun_skel.find_bone("selector")
	if sel >= 0:
		_selector_rest = _gun_skel.get_bone_rest(sel).basis.get_rotation_quaternion()
	ammo = data.magazine_size
	mode_index = 0
	_apply_selector()


func fire_mode() -> String:
	return data.fire_modes[mode_index] if data and data.fire_modes.size() > 0 else "semi"


func _process(delta: float) -> void:
	if not data or not _gun:
		return
	_time += delta
	var captured := Input.mouse_mode == Input.MOUSE_MODE_CAPTURED
	var sprinting := _player != null and _player.gait == Player.Gait.SPRINT
	var wants_aim := captured and Input.is_action_pressed("aim") and not reloading and not sprinting
	if _player:
		_player.is_aiming = wants_aim

	_handle_fire(delta, captured, sprinting)
	_update_reload(delta)

	# Pose weights
	var k := 1.0 - exp(-pose_blend_speed * delta)
	_aim_t = move_toward(_aim_t, 1.0 if wants_aim else 0.0, delta / maxf(data.ads_time, 0.01))
	_sprint_t = lerpf(_sprint_t, 1.0 if (sprinting and not reloading) else 0.0, k)
	_reload_pose_t = lerpf(_reload_pose_t, 1.0 if reloading else 0.0, k)

	_update_motion(delta)
	_pivot.transform = _compute_pose()
	if _camera:
		_camera.fov = lerpf(_base_fov, data.ads_fov, smoothstep(0.0, 1.0, _aim_t))
	_update_flash(delta)
	_pose_hands()
	_mouse = Vector2.ZERO


func _input(event: InputEvent) -> void:
	if event is InputEventMouseMotion and Input.mouse_mode == Input.MOUSE_MODE_CAPTURED:
		_mouse += event.relative


func _unhandled_input(event: InputEvent) -> void:
	if Input.mouse_mode != Input.MOUSE_MODE_CAPTURED or not data:
		return
	if event.is_action_pressed("fire"):
		_trigger_pulled = true
	elif event.is_action_pressed("reload"):
		start_reload()
	elif event.is_action_pressed("fire_mode"):
		cycle_fire_mode()


func cycle_fire_mode() -> void:
	if data.fire_modes.size() > 1:
		mode_index = (mode_index + 1) % data.fire_modes.size()
		_apply_selector()


# ---------------------------------------------------------------- firing

func _handle_fire(delta: float, captured: bool, sprinting: bool) -> void:
	_cooldown -= delta
	var mode := fire_mode()
	var can_fire := captured and not reloading and not sprinting
	var pulled := _trigger_pulled
	_trigger_pulled = false
	if not can_fire:
		_burst_left = 0
		return
	if pulled:
		if ammo <= 0:
			return  # dry: the next step adds a click sound here
		if mode == "burst":
			_burst_left = data.burst_count
		elif mode == "semi" and _cooldown <= 0.0:
			_shoot()
	if mode == "auto" and Input.is_action_pressed("fire") and _cooldown <= 0.0 and ammo > 0:
		_shoot()
	if _burst_left > 0 and _cooldown <= 0.0:
		if ammo > 0:
			_shoot()
			_burst_left -= 1
		else:
			_burst_left = 0


func _shoot() -> void:
	ammo -= 1
	_cooldown = 60.0 / data.rounds_per_minute
	_gun_anim.stop()
	_gun_anim.play("fire", -1, data.fire_anim_speed)
	var aim_scale := lerpf(1.0, 0.55, _aim_t)
	_recoil_pos += data.recoil_kick_back * aim_scale
	_recoil_rot += Vector2(deg_to_rad(data.recoil_pitch_deg), deg_to_rad(randf_range(-1.0, 1.0) * data.recoil_yaw_deg)) * aim_scale
	if _player:
		_player.add_view_kick(deg_to_rad(data.recoil_view_pitch_deg),
			deg_to_rad(randf_range(-1.0, 1.0) * data.recoil_view_yaw_deg))
	_flash_left = flash_time
	var muzzle: Node3D = _sockets.get("socket_muzzle")
	fired.emit(muzzle.global_transform if muzzle else global_transform)


func _apply_selector() -> void:
	var sel := _gun_skel.find_bone("selector")
	if sel < 0:
		return
	var ang := data.selector_angles_deg[mode_index] if mode_index < data.selector_angles_deg.size() else 0.0
	_gun_skel.set_bone_pose_rotation(sel, _selector_rest * Quaternion(Vector3.RIGHT, -deg_to_rad(ang)))


## Model animations are exported with a constant key for every bone; drop those
## so playing "fire" doesn't reset the selector, magazine, etc.
func _strip_static_tracks() -> void:
	for lib_name in _gun_anim.get_animation_library_list():
		var lib := _gun_anim.get_animation_library(lib_name)
		for anim_name in lib.get_animation_list():
			var a := lib.get_animation(anim_name)
			for t in range(a.get_track_count() - 1, -1, -1):
				var n := a.track_get_key_count(t)
				var constant := true
				for k in range(1, n):
					if a.track_get_key_value(t, k) != a.track_get_key_value(t, 0):
						constant = false
						break
				if constant:
					a.remove_track(t)


# ---------------------------------------------------------------- reload

func start_reload() -> void:
	if reloading or ammo >= data.magazine_size or not _gun_anim.has_animation(data.reload_animation):
		return
	_incoming_rounds = data.magazine_size
	if magazine_source.is_valid():
		_incoming_rounds = magazine_source.call()
		out_of_magazines = _incoming_rounds < 0
		if out_of_magazines:
			return
	reloading = true
	_burst_left = 0
	_reload_time = 0.0


func _update_reload(delta: float) -> void:
	if not reloading:
		return
	var reach := data.reload_reach_time
	var prev := _reload_time
	_reload_time += delta
	if prev < reach and _reload_time >= reach:
		_gun_anim.stop()
		_gun_anim.play(data.reload_animation)
	if prev < reach + data.reload_seated_at and _reload_time >= reach + data.reload_seated_at:
		ammo = _incoming_rounds
		reloaded.emit()
	if _reload_time >= reach * 2.0 + data.reload_seated_at:
		reloading = false


## 0 = hand on the handguard, 1 = hand on the magazine.
func _left_hand_on_mag() -> float:
	if not reloading:
		return 0.0
	var reach := data.reload_reach_time
	var end := reach + data.reload_seated_at
	if _reload_time < reach:
		return smoothstep(0.0, 1.0, _reload_time / reach)
	if _reload_time < end:
		return 1.0
	return 1.0 - smoothstep(0.0, 1.0, (_reload_time - end) / reach)


func _setup_magazine_point() -> void:
	var bone := _gun_skel.find_bone(data.magazine_bone)
	var well: Node3D = _sockets.get("socket_mag_well")
	if bone < 0 or not well:
		return
	var b := FPArms.hand_basis(data.magazine_hand_fingers_dir, data.magazine_hand_back_dir)
	# Express the hand point in the magazine bone's rest frame, so it rides along with the bone.
	var in_gun := Transform3D(b, _socket_pos(well) + data.magazine_hand_offset)
	var in_skel := _gun_skel.global_transform.affine_inverse() * _gun.global_transform * in_gun
	_mag_local = _gun_skel.get_bone_global_rest(bone).affine_inverse() * in_skel


# ---------------------------------------------------------------- motion

func _update_motion(delta: float) -> void:
	var motion_scale := lerpf(1.0, ads_motion_scale, _aim_t)
	# Sway: lag behind the mouse, then spring back.
	var target := -_mouse * sway_strength * data.sway_amount
	var max_r := deg_to_rad(sway_max_deg)
	_sway += target
	_sway = _sway.clamp(Vector2(-max_r, -max_r), Vector2(max_r, max_r))
	_sway = _sway.lerp(Vector2.ZERO, 1.0 - exp(-sway_return * delta))

	# Recoil springs back.
	var r := 1.0 - exp(-data.recoil_return * delta)
	_recoil_pos = lerpf(_recoil_pos, 0.0, r)
	_recoil_rot = _recoil_rot.lerp(Vector2.ZERO, r)

	# Bob from ground speed.
	var speed := 0.0
	var on_floor := true
	var vy := 0.0
	if _player:
		speed = Vector2(_player.velocity.x, _player.velocity.z).length()
		on_floor = _player.is_on_floor()
		vy = _player.velocity.y
	var hz := lerpf(bob_walk_hz, bob_sprint_hz, _sprint_t)
	if on_floor:
		_bob_phase += delta * hz * TAU * clampf(speed / 3.0, 0.0, 1.6)
	# Landing dip.
	if on_floor and not _was_on_floor:
		_land = clampf(-_last_vy / 6.0, 0.0, 1.0) * land_dip
	_was_on_floor = on_floor
	_last_vy = vy
	_land = lerpf(_land, 0.0, 1.0 - exp(-8.0 * delta))
	_motion_scale_cache = motion_scale
	_speed_cache = speed


var _motion_scale_cache := 1.0
var _speed_cache := 0.0


func _compute_pose() -> Transform3D:
	var hip := Transform3D(Basis.from_euler(_deg3(data.hip_rotation_deg)), data.hip_position)
	var pose := hip.interpolate_with(_ads_pose(), smoothstep(0.0, 1.0, _aim_t))
	var sprint := Transform3D(Basis.from_euler(_deg3(data.sprint_rotation_deg)), data.sprint_position)
	pose = pose.interpolate_with(sprint, _sprint_t)
	var reload := Transform3D(Basis.from_euler(_deg3(data.reload_rotation_deg)), data.reload_position)
	pose = pose.interpolate_with(reload, _reload_pose_t)

	var ms := _motion_scale_cache
	var amp := clampf(_speed_cache / 3.0, 0.0, 1.5) * data.bob_amount * ms
	var bob := Vector3(sin(_bob_phase) * bob_side * amp, -absf(cos(_bob_phase)) * bob_height * amp, 0.0)
	var breathe := Vector3(0.0, sin(_time * 1.6) * breathe_amount * ms, 0.0)
	var offset_pos := bob + breathe + Vector3(0.0, -_land, _recoil_pos)
	# Rotations happen around the grip, so recoil rocks the gun in the hand.
	var grip: Node3D = _sockets.get("socket_grip_right_hand")
	var p := _socket_pos(grip) if grip else Vector3.ZERO
	var rot := Basis.from_euler(Vector3(_recoil_rot.x + _sway.y * ms, _recoil_rot.y + _sway.x * ms, -_sway.x * 0.5 * ms))
	var around_grip := Transform3D(Basis(), p) * Transform3D(rot, Vector3.ZERO) * Transform3D(Basis(), -p)
	return Transform3D(Basis(), offset_pos) * pose * around_grip


## Pose that lines the rear aperture and front post up with the eye.
func _ads_pose() -> Transform3D:
	var rear: Node3D = _sockets.get("socket_rear_aperture")
	var front: Node3D = _sockets.get("socket_front_post")
	if not rear or not front:
		return Transform3D(Basis(), data.hip_position)
	var rp := _socket_pos(rear)
	var sight_dir := (_socket_pos(front) - rp).normalized()
	var r := Basis.looking_at(sight_dir, Vector3.UP).inverse()
	return Transform3D(r, Vector3(0.0, 0.0, -data.ads_eye_relief) - r * rp)


## A socket's position in the weapon model's own space (independent of the current pose).
func _socket_pos(s: Node3D) -> Vector3:
	return (_gun.global_transform.affine_inverse() * s.global_transform).origin


# ---------------------------------------------------------------- hands

func _pose_hands() -> void:
	var rs: Node3D = _sockets.get(data.right_hand_socket)
	if rs:
		var rb := FPArms.hand_basis(data.right_hand_fingers_dir, data.right_hand_back_dir)
		_arms.pose_hand("r", rs.global_transform * Transform3D(rb, data.right_hand_offset), data.right_finger_curl_deg)
	var ls: Node3D = _sockets.get(data.left_hand_socket)
	if ls:
		var lb := FPArms.hand_basis(data.left_hand_fingers_dir, data.left_hand_back_dir)
		var guard := ls.global_transform * Transform3D(lb, data.left_hand_offset)
		var target := guard
		var curls := data.left_finger_curl_deg
		var w := _left_hand_on_mag()
		if w > 0.0:
			var bone := _gun_skel.find_bone(data.magazine_bone)
			var mag := _gun_skel.global_transform * _gun_skel.get_bone_global_pose(bone) * _mag_local
			target = guard.interpolate_with(mag, w)
			curls = _lerp_curls(curls, data.magazine_finger_curl_deg, w)
		_arms.pose_hand("l", target, curls)


# ---------------------------------------------------------------- flash

func _setup_flash() -> void:
	var muzzle: Node3D = _sockets.get("socket_muzzle")
	if not muzzle:
		return
	_flash_light = OmniLight3D.new()
	_flash_light.light_color = Color(1.0, 0.75, 0.45)
	_flash_light.omni_range = 4.0
	_flash_light.visible = false
	muzzle.add_child(_flash_light)
	var mat := StandardMaterial3D.new()
	mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	mat.albedo_color = Color(1.0, 0.85, 0.55)
	mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	var sphere := SphereMesh.new()
	sphere.radius = 0.5
	sphere.height = 1.0
	sphere.material = mat
	_flash_mesh = MeshInstance3D.new()
	_flash_mesh.mesh = sphere
	_flash_mesh.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	_flash_mesh.visible = false
	muzzle.add_child(_flash_mesh)


func _update_flash(delta: float) -> void:
	if not _flash_light:
		return
	_flash_left -= delta
	var on := _flash_left > 0.0
	_flash_light.visible = on
	_flash_mesh.visible = on
	if on:
		_flash_light.light_energy = flash_energy * randf_range(0.7, 1.0)
		var s := randf_range(0.8, 1.2)
		_flash_mesh.scale = Vector3(0.045 * s, 0.045 * s, 0.12 * s)
		_flash_mesh.position = Vector3(0.0, 0.0, -0.05)
		_flash_mesh.rotation.z = randf() * TAU


static func _deg3(v: Vector3) -> Vector3:
	return Vector3(deg_to_rad(v.x), deg_to_rad(v.y), deg_to_rad(v.z))


static func _lerp_curls(a: PackedFloat32Array, b: PackedFloat32Array, t: float) -> PackedFloat32Array:
	var out := PackedFloat32Array()
	for i in a.size():
		out.append(lerpf(a[i], b[i] if i < b.size() else a[i], t))
	return out
