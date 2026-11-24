extends Node3D
## Test-map bootstrap. The Rust core owns the mission: which map, which forces
## and where they start, and what they are doing. This script only:
##   - loads the save, or asks the core to start `map_id`
##   - sizes the terrain to the map and puts the player's body at its start
##   - keeps the sim informed about the player.
## The on-screen panels live in the HUD (scripts/hud.gd).

@export var spawn_height_offset: float = 2.0
## Map to start when there is no save (a file in core/data/maps/).
@export var map_id: String = "test_area"
## Loaded when the game starts and written when the window is closed.
@export var save_path: String = "user://save.json"

@onready var _sim: SimBridge = $SimBridge
@onready var _terrain: Terrain = $Terrain
@onready var _player: Player = $Player
@onready var _view: ViewModel = $Player/Head/Camera/ViewModel
@onready var _platoon: PlatoonView = $Platoon


func _ready() -> void:
	_sim.sim_event.connect(_on_sim_event)

	var loaded := _sim.load_game(_os_save_path())
	if not loaded and not _sim.start_map(map_id):
		push_error("Could not start map %s" % map_id)
	var map := _sim.get_map()
	_fit_terrain(map)

	# The player's body is Godot's: put it at the map's start for their platoon,
	# or where the save says it was.
	var start: Vector3 = map.get("player_start", Vector3.ZERO)
	_player.spawn_point = _ground(start.x, start.z) + Vector3.UP * spawn_height_offset
	if loaded:
		_restore_player()
	else:
		_player.respawn()
		_player.global_rotation.y = map.get("player_yaw", 0.0)

	# Soldiers (both sides) are built from the sim's roster.
	_platoon.attach(_sim, _terrain)

	# The player's magazines come from their Rust inventory.
	_view.magazine_source = func() -> int: return _sim.player_reload()
	_view.fired.connect(func(_muzzle: Transform3D) -> void: _sim.report_player_rounds(_view.ammo))


## The terrain covers the map: same size, regenerated if it differs.
func _fit_terrain(map: Dictionary) -> void:
	var size: float = map.get("size_m", _terrain.size_m)
	if not is_equal_approx(size, _terrain.size_m):
		_terrain.size_m = size
		_terrain.generate()


func _notification(what: int) -> void:
	# Closing the window saves. (The editor's Stop button kills the game
	# without this notification, so it doesn't save.)
	if what == NOTIFICATION_WM_CLOSE_REQUEST:
		_sim.save_game(_os_save_path())


func _ground(x: float, z: float) -> Vector3:
	return Vector3(x, _terrain.get_height_at(x, z), z)


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
