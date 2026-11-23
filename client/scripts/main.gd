extends Node3D
## Test-map bootstrap: puts the player on the terrain, spawns the platoon in
## the Rust sim, keeps the sim informed about the player, and shows debug info.

@export var spawn_height_offset: float = 2.0

@onready var _sim: SimBridge = $SimBridge
@onready var _terrain: Terrain = $Terrain
@onready var _player: Player = $Player
@onready var _label: Label = $DebugHUD/Label
@onready var _view: ViewModel = $Player/Head/Camera/ViewModel

var _platoon: PlatoonView
var _last_sim_event := "(none yet)"


func _ready() -> void:
	var ground := Vector3(0.0, _terrain.get_height_at(0.0, 0.0), 0.0)
	_player.spawn_point = ground + Vector3.UP * spawn_height_offset
	_player.respawn()
	_sim.sim_event.connect(_on_sim_event)

	# The platoon lives in Rust; the player is its platoon leader.
	_platoon = PlatoonView.new()
	_platoon.name = "Platoon"
	add_child(_platoon)
	_platoon.spawn_platoon(_sim, _terrain, ground, _player.global_rotation.y)

	# The player's magazines come from their Rust inventory.
	_view.magazine_source = func() -> int: return _sim.player_reload()
	_view.fired.connect(func(_muzzle: Transform3D) -> void: _sim.report_player_rounds(_view.ammo))


func _physics_process(_delta: float) -> void:
	# Godot owns player movement; the sim just needs to know the result.
	_sim.report_player(_player.global_position, _player.global_rotation.y,
		_player.velocity, _player.stance, _player.gait)


func _on_sim_event(kind: String, data: Dictionary) -> void:
	_last_sim_event = "%s %s" % [kind, data]
	print("[sim] ", _last_sim_event)


func _process(_delta: float) -> void:
	$DebugHUD/Crosshair.visible = not _player.is_aiming
	var p := _player.global_position
	var s := _sim.get_player_state()
	var sp: Vector3 = s.position
	var me := _platoon.player_info
	var mags: PackedInt32Array = me.get("magazines", PackedInt32Array())
	_label.text = "\n".join([
		"FPS %d" % Engine.get_frames_per_second(),
		"Sim tick %d   (%.0f Hz, x%.2f%s)" % [
			_sim.get_tick(), _sim.tick_rate_hz, _sim.time_scale,
			"  PAUSED" if _sim.paused else ""],
		"Godot: pos %.1f, %.1f, %.1f   speed %.2f / %.2f m/s   %s, %s" % [
			p.x, p.y, p.z, Vector2(_player.velocity.x, _player.velocity.z).length(),
			_player.current_max_speed(),
			Player.Stance.keys()[_player.stance].to_lower(),
			Player.Gait.keys()[_player.gait].to_lower()],
		"Rust:  pos %.1f, %.1f, %.1f   speed %.2f m/s   %s, %s" % [
			sp.x, sp.y, sp.z, s.speed, s.stance_name, s.gait_name],
		"Last sim event: %s" % _last_sim_event,
		"Platoon: %s" % _platoon.summary(),
		"You: %s %s · %s · %s · health %.0f (%s)" % [me.get("rank", ""), me.get("name", ""),
			me.get("role", ""), me.get("unit_name", ""), me.get("health", 0.0), me.get("life", "")],
		"%s  %d/%d  %s · spare mags %d (%d rds)%s%s" % [_view.data.weapon_id.to_upper(), _view.ammo,
			_view.data.magazine_size, _view.fire_mode().to_upper(), mags.size(), me.get("reserve_rounds", 0),
			"  RELOADING" if _view.reloading else "", "  NO MAGAZINES" if _view.out_of_magazines else ""],
		"WASD move · Ctrl tactical · Shift sprint · C crouch · X prone · Space jump",
		"LMB fire · RMB aim · R reload · B fire mode",
		"Esc free mouse · P pause sim · N step sim",
	])


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and not event.echo:
		match event.physical_keycode:
			KEY_P:
				_sim.paused = not _sim.paused
			KEY_N:
				_sim.step()
