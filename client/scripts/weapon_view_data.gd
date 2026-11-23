class_name WeaponViewData
extends Resource
## How one weapon looks, feels and handles in first person.
## One file per weapon (e.g. res://presentation/weapons/m16a2.tres); the view
## model, arms and reload code are shared and read everything from here.
## Hand and aim points come from the weapon model's socket_* nodes.

@export var weapon_id := ""
@export var model: PackedScene

@export_group("Hip, aim and sprint poses")
## Where the weapon's origin sits in camera space when held at the hip.
@export var hip_position := Vector3(0.10, -0.14, -0.20)
@export var hip_rotation_deg := Vector3(0.0, 3.0, 0.0)
## Distance from the eye to the rear sight when aiming (metres).
@export var ads_eye_relief := 0.045
@export var ads_fov := 55.0
## Seconds to go from hip to fully aimed.
@export var ads_time := 0.22
@export var sprint_position := Vector3(0.07, -0.17, -0.18)
@export var sprint_rotation_deg := Vector3(-10.0, 30.0, 12.0)
## Weapon pose while reloading (tilted to show the magazine well).
@export var reload_position := Vector3(0.03, -0.11, -0.27)
@export var reload_rotation_deg := Vector3(16.0, 14.0, -38.0)

@export_group("Fire")
@export var rounds_per_minute := 800.0
## Any of "semi", "burst", "auto", in the order the selector cycles.
@export var fire_modes := PackedStringArray(["semi", "burst"])
@export var burst_count := 3
@export var magazine_size := 30
## The model's fire animation is authored slow; this plays it at true speed.
@export var fire_anim_speed := 2.7
## Selector bone angle (degrees) for each entry in fire_modes.
@export var selector_angles_deg := PackedFloat32Array([90.0, 180.0])

@export_group("Recoil")
@export var recoil_kick_back := 0.022
@export var recoil_pitch_deg := 2.2
@export var recoil_yaw_deg := 0.7
## How far the view itself climbs per shot (the player must pull down).
@export var recoil_view_pitch_deg := 0.45
@export var recoil_view_yaw_deg := 0.15
## Higher returns to rest faster.
@export var recoil_return := 16.0

@export_group("Weight and motion")
@export var weight_kg := 3.8
@export var sway_amount := 1.0
@export var bob_amount := 1.0

@export_group("Right hand (grip)")
@export var right_hand_socket := "socket_grip_right_hand"
## Wrist position relative to the socket, in weapon space (metres).
@export var right_hand_offset := Vector3(0.032, 0.03, 0.075)
## Direction the straight fingers point, and the back of the hand faces (weapon space).
@export var right_hand_fingers_dir := Vector3(-0.15, -0.45, -1.0)
@export var right_hand_back_dir := Vector3(1.0, 0.1, 0.0)
## Curl per joint in degrees: thumb, index, middle, ring, pinky.
@export var right_finger_curl_deg := PackedFloat32Array([30.0, 22.0, 78.0, 82.0, 84.0])

@export_group("Left hand (support)")
@export var left_hand_socket := "socket_grip_left_hand"
@export var left_hand_offset := Vector3(-0.045, -0.042, 0.08)
@export var left_hand_fingers_dir := Vector3(1.0, 0.1, -0.45)
@export var left_hand_back_dir := Vector3(-0.2, -1.0, 0.0)
@export var left_finger_curl_deg := PackedFloat32Array([20.0, 36.0, 38.0, 40.0, 44.0])

@export_group("Reload")
## Animation on the weapon model that moves the magazine bone out and back in.
@export var reload_animation := "reload_magazine"
@export var magazine_bone := "magazine"
## Where the left hand holds the magazine, relative to socket_mag_well at rest.
@export var magazine_hand_offset := Vector3(-0.03, -0.075, 0.0)
@export var magazine_hand_fingers_dir := Vector3(0.2, -0.3, -1.0)
@export var magazine_hand_back_dir := Vector3(-1.0, 0.0, 0.0)
@export var magazine_finger_curl_deg := PackedFloat32Array([20.0, 45.0, 50.0, 50.0, 50.0])
## Seconds for the hand to reach the magazine / return to the handguard.
@export var reload_reach_time := 0.25
## When (seconds into the reload animation) the new magazine is seated.
@export var reload_seated_at := 2.0
