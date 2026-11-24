extends Node3D
## Test-map bootstrap: puts the player on the terrain, loads or spawns the
## platoon in the Rust sim, and keeps the sim informed about the player.
## The on-screen panels live in the HUD (scripts/hud.gd).

@export var spawn_height_offset: float = 2.0
## Loaded when the game starts and written when the window is closed.
@export var save_path: String = "user://save.json"

@onready var _sim: SimBridge = $SimBridge
@onready var _terrain: Terrain = $Terrain
@onready var _player: Player = $Player
@onready var _view: ViewModel = $Player/Head/Camera/ViewModel
@onready var _platoon: PlatoonView = $Platoon


func _ready() -> void:
	var ground := Vector3(0.0, _terrain.get_height_at(0.0, 0.0), 0.0)
	_player.spawn_point = ground + Vector3.UP * spawn_height_offset
	_player.respawn()
	_sim.sim_event.connect(_on_sim_event)

	# The platoon lives in Rust; the player is its platoon commander.
	# The Platoon node is in the scene; its soldiers are built from the sim's roster.
	if _sim.load_game(_os_save_path()):
		_platoon.attach(_sim, _terrain)
		_restore_player()
	else:
		_platoon.spawn_platoon(_sim, _terrain, ground, _player.global_rotation.y)

	# The player's magazines come from their Rust inventory.
	_view.magazine_source = func() -> int: return _sim.player_reload()
	_view.fired.connect(func(_muzzle: Transform3D) -> void: _sim.report_player_rounds(_view.ammo))


func _notification(what: int) -> void:
	# Closing the window saves. (The editor's Stop button kills the game
	# without this notification, so it doesn't save.)
	if what == NOTIFICATION_WM_CLOSE_REQUEST:
		_sim.save_game(_os_save_path())


func _os_save_path() -> String:
	return ProjectSettings.globalize_path(save_path)


## Godot owns the player's body, so after a load put it where the sim says.
func _restore_player() -> void:
	var s := _sim.get_player_state()
	_player.global_position = s.position
	_player.global_rotation.y = s.yaw
	_player.velocity = Vector3.ZERO
	_player.try_set_stance(s.stance as Player.Stance)
	_view.ammo = _platoon.player_info.get("rounds_in_weapon", _view.ammo)


func _physics_process(_delta: float) -> void:
	# Godot owns player movement; the sim just needs to know the result.
	_sim.report_player(_player.global_position, _player.global_rotation.y,
		_player.velocity, _player.stance, _player.gait)


func _on_sim_event(kind: String, data: Dictionary) -> void:
	if kind in ["shot_fired", "reloaded", "out_of_ammo"] and data.get("soldier_id", -1) != _platoon.player_info.get("id", -2):
		return  # too frequent to log for the whole platoon
	print("[sim] %s %s" % [kind, data])


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and not event.echo:
		match event.physical_keycode:
			KEY_P:
				_sim.paused = not _sim.paused
			KEY_N:
				_sim.step()
