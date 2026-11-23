extends Node3D
## Test-map bootstrap: puts the player on the terrain and shows debug info.

@export var spawn_height_offset: float = 2.0

@onready var _sim: SimBridge = $SimBridge
@onready var _terrain: Terrain = $Terrain
@onready var _player: Player = $Player
@onready var _label: Label = $DebugHUD/Label


func _ready() -> void:
	var spawn := Vector3(0.0, _terrain.get_height_at(0.0, 0.0) + spawn_height_offset, 0.0)
	_player.spawn_point = spawn
	_player.respawn()


func _physics_process(_delta: float) -> void:
	# Godot owns player movement; the sim just needs to know where the player is.
	_sim.report_player_transform(_player.global_position, _player.get_node("Body").global_rotation.y)


func _process(_delta: float) -> void:
	var p := _player.global_position
	var sp := _sim.get_player_position()
	_label.text = "\n".join([
		"FPS %d" % Engine.get_frames_per_second(),
		"Sim tick %d   (%.0f Hz, x%.2f%s)" % [
			_sim.get_tick(), _sim.tick_rate_hz, _sim.time_scale,
			"  PAUSED" if _sim.paused else ""],
		"Sim time %.1f s" % _sim.get_sim_time(),
		"Pos %.1f, %.1f, %.1f   speed %.1f m/s" % [
			p.x, p.y, p.z, Vector2(_player.velocity.x, _player.velocity.z).length()],
		"Sim sees player at %.1f, %.1f, %.1f" % [sp.x, sp.y, sp.z],
		"WASD move · Space jump · Shift sprint · Esc free mouse · P pause sim · N step",
	])


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and not event.echo:
		match event.physical_keycode:
			KEY_P:
				_sim.paused = not _sim.paused
			KEY_N:
				_sim.step()
