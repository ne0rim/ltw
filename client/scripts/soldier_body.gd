class_name SoldierBody
extends Node3D
## A dressed, animated soldier: the rigged body, modular clothing pieces on the
## same skeleton, and a weapon in the right hand. It only plays what it's told
## (speed, aiming); it never decides anything itself.
##
## Models come from art-source/scripts/blender (soldier_rig.glb and one .glb
## per clothing piece). Animations are in place; the parent node moves.

const RIG_SCENE := preload("res://assets/models/characters/soldier_rig.glb")

## Clothing pieces and the body regions each one covers (hidden when worn, so
## skin never pokes through and covered skin isn't drawn).
const PIECES := {
	"trousers_woodland": ["Body_Legs"],
	"boots_combat": ["Body_Feet"],
	"shirt_woodland": ["Body_Torso"],
}
const DEFAULT_OUTFIT := ["trousers_woodland", "boots_combat", "shirt_woodland"]

## Speeds the walk and jog cycles were made for (metres per second); must
## match WALK_SPEED / RUN_SPEED in build_soldier_rig.py.
const WALK_SPEED := 1.3
const RUN_SPEED := 2.6
## Bones the aim pose takes over (the legs keep walking or standing).
const UPPER_BODY := ["Spine", "Chest", "UpperChest", "Neck", "Head",
	"LeftShoulder", "LeftUpperArm", "LeftLowerArm", "LeftHand",
	"RightShoulder", "RightUpperArm", "RightLowerArm", "RightHand"]

## How quickly the body eases into and out of the aim pose.
@export var aim_speed := 5.0

var _rig: Node3D
var _skeleton: Skeleton3D
var _player: AnimationPlayer
var _tree: AnimationTree
var _socket: Node3D
var _weapon: Node3D
var _aim := 0.0
var _stride := 0.0


func _init() -> void:
	_rig = RIG_SCENE.instantiate()
	add_child(_rig)
	_skeleton = _rig.find_child("Skeleton3D", true, false)
	_player = _rig.find_child("AnimationPlayer", true, false)
	_socket = _rig.find_child("socket_rifle", true, false)
	for anim_name in _player.get_animation_list():
		_player.get_animation(anim_name).loop_mode = Animation.LOOP_LINEAR
	_tree = _build_tree()
	_rig.add_child(_tree)
	dress(DEFAULT_OUTFIT)


## Put on these clothing pieces (ids from PIECES) instead of the current ones.
func dress(pieces: Array) -> void:
	for child in _skeleton.get_children():
		if child.has_meta("clothing"):
			child.queue_free()
	var hidden := {}
	for piece in pieces:
		var scene: PackedScene = load("res://assets/models/characters/%s.glb" % piece)
		var inst := scene.instantiate()
		for mesh in inst.find_children("*", "MeshInstance3D", true, false):
			mesh.get_parent().remove_child(mesh)
			mesh.owner = null
			mesh.set_meta("clothing", piece)
			_skeleton.add_child(mesh)
			mesh.skeleton = NodePath("..")
		inst.free()
		for region in PIECES.get(piece, []):
			hidden[region] = true
	for region in ["Body_Torso", "Body_Legs", "Body_Feet"]:
		var part := _skeleton.get_node_or_null(region) as MeshInstance3D
		if part:
			part.visible = not hidden.has(region)


## Put a weapon model in the right hand (null for empty hands). Returns it.
func hold(weapon_scene: PackedScene) -> Node3D:
	if _weapon:
		_weapon.queue_free()
		_weapon = null
	if weapon_scene:
		_weapon = weapon_scene.instantiate()
		_socket.add_child(_weapon)
	return _weapon


## Drive the animation: ground speed in m/s and whether the rifle is shouldered.
func animate(speed: float, aiming: bool, delta: float) -> void:
	_aim = move_toward(_aim, 1.0 if aiming else 0.0, delta * aim_speed)
	# Walk and jog are kept on the same step: both play one stride per
	# stride length travelled, whichever is showing.
	var run_t := clampf((speed - WALK_SPEED) / (RUN_SPEED - WALK_SPEED), 0.0, 1.0)
	var walk_len := _player.get_animation("walk").length
	var run_len := _player.get_animation("run").length
	var stride := lerpf(WALK_SPEED * walk_len, RUN_SPEED * run_len, run_t)
	var strides_per_sec := maxf(speed, 0.5) / stride
	_tree.set("parameters/walk_rate/scale", strides_per_sec * walk_len)
	_tree.set("parameters/run_rate/scale", strides_per_sec * run_len)
	_tree.set("parameters/gait/blend_amount", run_t)
	_tree.set("parameters/move/blend_amount", clampf(speed / 0.6, 0.0, 1.0))
	_tree.set("parameters/aim/blend_amount", smoothstep(0.0, 1.0, _aim))


## Stop animating and hold the current pose (e.g. when killed).
func freeze() -> void:
	_tree.active = false


func _build_tree() -> AnimationTree:
	var root := AnimationNodeBlendTree.new()
	for anim_name in ["idle", "walk", "run", "aim"]:
		var node := AnimationNodeAnimation.new()
		node.animation = anim_name
		root.add_node(anim_name + "_clip", node)
	for rate in ["walk_rate", "run_rate"]:
		root.add_node(rate, AnimationNodeTimeScale.new())
	root.connect_node("walk_rate", 0, "walk_clip")
	root.connect_node("run_rate", 0, "run_clip")

	var gait := AnimationNodeBlend2.new()
	gait.sync = true
	root.add_node("gait", gait)
	root.connect_node("gait", 0, "walk_rate")
	root.connect_node("gait", 1, "run_rate")

	var move := AnimationNodeBlend2.new()
	move.sync = true
	root.add_node("move", move)
	root.connect_node("move", 0, "idle_clip")
	root.connect_node("move", 1, "gait")

	var aim := AnimationNodeBlend2.new()
	aim.filter_enabled = true
	var skel_path := String(_rig.get_path_to(_skeleton))
	for bone in UPPER_BODY:
		aim.set_filter_path(NodePath("%s:%s" % [skel_path, bone]), true)
	root.add_node("aim", aim)
	root.connect_node("aim", 0, "move")
	root.connect_node("aim", 1, "aim_clip")
	root.connect_node("output", 0, "aim")

	var tree := AnimationTree.new()
	tree.tree_root = root
	tree.anim_player = NodePath("../AnimationPlayer")
	tree.root_node = NodePath("..")
	tree.active = true
	return tree
