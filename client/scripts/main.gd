extends Node3D
## Test-map bootstrap: puts the player on the terrain, loads or spawns the
## platoons in the Rust sim, and keeps the sim informed about the player.
## For now it also plays the part of the mission script: it spawns the enemy
## platoon at the far end of the map and gives it its task.
## The on-screen panels live in the HUD (scripts/hud.gd).

@export var spawn_height_offset: float = 2.0
## How far from the map centre each platoon starts (metres): the player's to
## the south, the enemy's to the north, facing each other.
@export var start_distance_m: float = 170.0
## What the enemy platoon is tasked with: attack your start position, defend
## its own, or nothing.
@export_enum("attack", "defend", "none") var enemy_task: String = "attack"
## Loaded when the game starts and written when the window is closed.
@export var save_path: String = "user://save.json"

@onready var _sim: SimBridge = $SimBridge
@onready var _terrain: Terrain = $Terrain
@onready var _player: Player = $Player
@onready var _view: ViewModel = $Player/Head/Camera/ViewModel
@onready var _platoon: PlatoonView = $Platoon


func _ready() -> void:
	var home := _ground(0.0, start_distance_m)
	_player.spawn_point = home + Vector3.UP * spawn_height_offset
	_player.respawn()
	_player.global_rotation.y = 0.0  # facing north, toward the enemy
	_sim.sim_event.connect(_on_sim_event)

	# The platoons live in Rust; the player is their platoon's commander.
	# The Platoon node is in the scene; soldiers are built from the sim's roster.
	if _sim.load_game(_os_save_path()):
		_platoon.attach(_sim, _terrain)
		_restore_player()
	else:
		_platoon.spawn_platoon(_sim, _terrain, home, 0.0)
	if not _has_enemy():
		_spawn_enemy(home)

	# The player's magazines come from their Rust inventory.
	_view.magazine_source = func() -> int: return _sim.player_reload()
	_view.fired.connect(func(_muzzle: Transform3D) -> void: _sim.report_player_rounds(_view.ammo))


func _notification(what: int) -> void:
	# Closing the window saves. (The editor's Stop button kills the game
	# without this notification, so it doesn't save.)
	if what == NOTIFICATION_WM_CLOSE_REQUEST:
		_sim.save_game(_os_save_path())


func _ground(x: float, z: float) -> Vector3:
	return Vector3(x, _terrain.get_height_at(x, z), z)


func _has_enemy() -> bool:
	var mine := _sim.get_player_side()
	for u in _sim.get_units():
		if u.side != mine:
			return true
	return false


## The enemy platoon starts at the north end facing south, AI-commanded.
func _spawn_enemy(player_home: Vector3) -> void:
	var side := "red" if _sim.get_player_side() == "blue" else "blue"
	var origin := _ground(0.0, -start_distance_m)
	var id := _sim.spawn_ai_platoon(side, "Enemy Platoon", "E1", origin, PI)
	match enemy_task:
		"attack":
			_sim.assign_attack(id, player_home)
		"defend":
			_sim.assign_defend(id, origin, PI)


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
