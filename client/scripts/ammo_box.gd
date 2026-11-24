class_name AmmoBox
extends StaticBody3D
## An ammo box that never runs out. Walk up to it and press F (interact) to
## refill your spare magazines to a full basic load. Magazines live in the
## Rust sim, so the box asks the sim to resupply the player.

@export var sim: SimBridge
@export var player: Player
## Optional: the box sits on this terrain's surface when the game starts.
@export var terrain: Terrain
## How close you must be (metres, measured across the ground).
@export var reach_m: float = 2.0
## How long "Resupplied" stays up after use (seconds).
@export var feedback_time: float = 1.5

@onready var _prompt: Label3D = $Prompt

var _feedback_left := 0.0


func _ready() -> void:
	if terrain:
		global_position.y = terrain.get_height_at(global_position.x, global_position.z)
	_prompt.visible = false


func in_reach() -> bool:
	if not player:
		return false
	var d := player.global_position - global_position
	return Vector2(d.x, d.z).length() <= reach_m and absf(d.y) < 2.5


func _process(delta: float) -> void:
	_feedback_left = maxf(_feedback_left - delta, 0.0)
	var near := in_reach()
	_prompt.visible = near or _feedback_left > 0.0
	if _feedback_left > 0.0:
		_prompt.text = "Resupplied"
	elif near:
		_prompt.text = "F  Resupply"


func _unhandled_input(event: InputEvent) -> void:
	# Exact match, so Ctrl+F (fire order) doesn't also count as F.
	if event.is_action_pressed("interact", false, true) and in_reach():
		use()
		get_viewport().set_input_as_handled()


## Refill the player's spare magazines.
func use() -> void:
	if sim.player_resupply() < 0:
		return
	var view := player.get_node_or_null("Head/Camera/ViewModel") as ViewModel
	if view:
		view.out_of_magazines = false
	_feedback_left = feedback_time
