class_name FPArms
extends Node3D
## First-person arms. Weapon-agnostic: give it a world-space wrist target for
## each hand plus finger curls, and it solves the arm with two-bone IK.
##
## Bone conventions (from art-source/scripts/blender/build_fp_arms.py):
## every bone's local +Y points down the limb/finger, local +Z is the back of
## the hand, local X is the bend/curl axis.

const FINGERS := ["thumb", "index", "middle", "ring", "pinky"]

@export var arms_scene: PackedScene = preload("res://assets/models/characters/fp_arms.glb")
## Elbow hint per side (camera space): where the elbow should point.
@export var right_elbow_hint := Vector3(0.7, -1.0, 0.25)
@export var left_elbow_hint := Vector3(-0.6, -1.0, 0.1)
## Thumb opposition (degrees): swing in under the palm, and twist.
@export var thumb_swing_deg := 28.0
@export var thumb_twist_deg := -55.0

var skeleton: Skeleton3D
var _model: Node3D
var _bones := {}         # name -> index
var _upper_len := 0.0
var _fore_len := 0.0


func _ready() -> void:
	_model = arms_scene.instantiate()
	add_child(_model)
	skeleton = _model.find_children("*", "Skeleton3D", true, false)[0]
	for i in skeleton.get_bone_count():
		_bones[skeleton.get_bone_name(i)] = i
	_upper_len = skeleton.get_bone_rest(_bones["forearm_r"]).origin.length()
	_fore_len = skeleton.get_bone_rest(_bones["hand_r"]).origin.length()
	for g in _model.find_children("*", "GeometryInstance3D", true, false):
		(g as GeometryInstance3D).cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


## Place one hand. `wrist` is the wrist transform in world space (basis: +Y
## along the straight fingers, +Z out of the back of the hand).
## `curls` = degrees per joint for thumb, index, middle, ring, pinky.
func pose_hand(side: String, wrist: Transform3D, curls: PackedFloat32Array) -> void:
	var to_skel := skeleton.global_transform.affine_inverse()
	var target := to_skel * wrist
	var hint_cam := right_elbow_hint if side == "r" else left_elbow_hint
	var hint := (to_skel.basis * (global_transform.basis * hint_cam)).normalized()
	_solve_arm(side, target, hint)
	_curl_fingers(side, curls)


func _solve_arm(side: String, target: Transform3D, hint: Vector3) -> void:
	var upper: int = _bones["upperarm_" + side]
	var fore: int = _bones["forearm_" + side]
	var hand: int = _bones["hand_" + side]
	var parent_g := skeleton.get_bone_global_rest(skeleton.get_bone_parent(upper))
	var shoulder := (parent_g * skeleton.get_bone_rest(upper)).origin

	var a := _upper_len
	var b := _fore_len
	var to_t := target.origin - shoulder
	var dist := clampf(to_t.length(), 0.02, a + b - 0.002)
	var dir := to_t.normalized()
	var cos_s := clampf((a * a + dist * dist - b * b) / (2.0 * a * dist), -1.0, 1.0)
	var bend := hint - dir * hint.dot(dir)
	if bend.length_squared() < 1e-6:
		bend = Vector3.DOWN - dir * Vector3.DOWN.dot(dir)
	bend = bend.normalized()
	var elbow := shoulder + (dir * cos_s + bend * sqrt(1.0 - cos_s * cos_s)) * a
	var wrist := shoulder + dir * dist

	var y_u := (elbow - shoulder).normalized()
	var y_f := (wrist - elbow).normalized()
	var hinge := y_u.cross(y_f)
	if hinge.length_squared() < 1e-6:
		hinge = y_u.cross(bend)
	hinge = hinge.normalized()
	var g_upper := Basis(hinge, y_u, hinge.cross(y_u))
	var g_fore := Basis(hinge, y_f, hinge.cross(y_f))
	var g_hand := target.basis.orthonormalized()

	skeleton.set_bone_pose_rotation(upper, (parent_g.basis.inverse() * g_upper).get_rotation_quaternion())
	skeleton.set_bone_pose_rotation(fore, (g_upper.inverse() * g_fore).get_rotation_quaternion())
	skeleton.set_bone_pose_rotation(hand, (g_fore.inverse() * g_hand).get_rotation_quaternion())


func _curl_fingers(side: String, curls: PackedFloat32Array) -> void:
	var mirror := 1.0 if side == "r" else -1.0
	for f in FINGERS.size():
		var curl := deg_to_rad(curls[f] if f < curls.size() else 0.0)
		for j in 3:
			var name := "%s_%02d_%s" % [FINGERS[f], j + 1, side]
			if not _bones.has(name):
				continue
			var idx: int = _bones[name]
			var q := skeleton.get_bone_rest(idx).basis.get_rotation_quaternion()
			if f == 0 and j == 0:
				q = q * Quaternion(Vector3.BACK, deg_to_rad(thumb_swing_deg) * mirror) \
					* Quaternion(Vector3.UP, deg_to_rad(thumb_twist_deg) * mirror)
			skeleton.set_bone_pose_rotation(idx, q * Quaternion(Vector3.RIGHT, -curl))


## Build a wrist basis from "fingers point this way" and "back of hand faces this way".
static func hand_basis(fingers_dir: Vector3, back_dir: Vector3) -> Basis:
	var y := fingers_dir.normalized()
	var z := (back_dir - y * back_dir.dot(y)).normalized()
	return Basis(y.cross(z), y, z)
