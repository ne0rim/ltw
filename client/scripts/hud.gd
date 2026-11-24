class_name Hud
extends CanvasLayer
## On-screen UI: the crosshair and three panels, each toggled with a key.
##
##   Ctrl+Alt+1  Dev       sim tick, Godot vs Rust player state, last sim event
##   Ctrl+Alt+2  Controls  every key
##   Ctrl+Alt+3  Game      you, your weapon, the platoon and each section
##   T           Tactical map (hides the panels while open)
##
## The compass at the top always shows which way the player faces.

@export var sim: SimBridge
@export var player: Player
@export var platoon: PlatoonView
@export var command: CommandInput
@export var view: ViewModel
@export var terrain: Terrain

@onready var crosshair: Control = $Crosshair
@onready var dev_panel: HudPanel = $DevPanel
@onready var controls_panel: HudPanel = $ControlsPanel
@onready var game_panel: HudPanel = $GamePanel
@onready var tactical_map: TacticalMap = $TacticalMap
@onready var compass: Compass = $Compass

const CONTROLS := [
	"Move",
	"  WASD move · Space jump",
	"  Ctrl tactical · Shift sprint",
	"  C crouch · X prone",
	"Weapon",
	"  LMB fire · RMB aim",
	"  R reload · B fire mode",
	"  F use (ammo box: refill magazines)",
	"Map",
	"  T tactical map",
	"Command",
	"  Ctrl+1/2/3 select section · Ctrl+4 HQ",
	"  Ctrl+M move here · Ctrl+F fire here",
	"Sim",
	"  P pause · N step one tick",
	"UI",
	"  Ctrl+Alt+1 dev · Ctrl+Alt+2 controls · Ctrl+Alt+3 game",
	"  Esc free mouse",
]

var _last_sim_event := "(none yet)"
## Panels that were showing before the map opened.
var _panels_before_map: Array[HudPanel] = []


func _ready() -> void:
	controls_panel.set_lines(PackedStringArray(CONTROLS))
	tactical_map.setup(terrain, platoon, player, command)
	sim.sim_event.connect(_on_sim_event)


func _input(event: InputEvent) -> void:
	var map_key := event.is_action_pressed("tactical_map", false, true)
	if map_key or (tactical_map.visible and event.is_action_pressed("ui_cancel")):
		_toggle_map()
		get_viewport().set_input_as_handled()
		return
	if tactical_map.visible:
		return  # panels stay hidden while the map is open
	# _input runs before the command keys' _unhandled_input, so Ctrl+Alt+1
	# doesn't also select 1 Section (Ctrl+1).
	for pair in [["hud_dev", dev_panel], ["hud_controls", controls_panel], ["hud_game", game_panel]]:
		if event.is_action_pressed(pair[0], false, true):
			pair[1].visible = not pair[1].visible
			get_viewport().set_input_as_handled()
			return


func _toggle_map() -> void:
	if tactical_map.visible:
		tactical_map.close()
		for p in _panels_before_map:
			p.visible = true
		_panels_before_map.clear()
	else:
		for p in [dev_panel, controls_panel, game_panel]:
			if p.visible:
				_panels_before_map.append(p)
				p.visible = false
		tactical_map.open()


func _process(_delta: float) -> void:
	crosshair.visible = not player.is_aiming and not tactical_map.visible
	compass.visible = not tactical_map.visible
	compass.heading_deg = Compass.heading_from_yaw(player.global_rotation.y)
	if dev_panel.visible:
		dev_panel.set_lines(_dev_lines())
	if game_panel.visible:
		game_panel.set_lines(_game_lines())


func _dev_lines() -> PackedStringArray:
	var p := player.global_position
	var s := sim.get_player_state()
	var sp: Vector3 = s.position
	return PackedStringArray([
		"FPS %d" % Engine.get_frames_per_second(),
		"Sim tick %d   (%.0f Hz, x%.2f%s)" % [
			sim.get_tick(), sim.tick_rate_hz, sim.time_scale, "  PAUSED" if sim.paused else ""],
		"Godot  pos %.1f, %.1f, %.1f   speed %.2f / %.2f m/s   %s, %s" % [
			p.x, p.y, p.z, Vector2(player.velocity.x, player.velocity.z).length(),
			player.current_max_speed(),
			Player.Stance.keys()[player.stance].to_lower(),
			Player.Gait.keys()[player.gait].to_lower()],
		"Rust   pos %.1f, %.1f, %.1f   speed %.2f m/s   %s, %s" % [
			sp.x, sp.y, sp.z, s.speed, s.stance_name, s.gait_name],
		"Last sim event: %s" % _last_sim_event,
		"Enemy (AI): %s" % " | ".join(platoon.enemy_status_lines()),
	])


func _game_lines() -> PackedStringArray:
	var me := platoon.player_info
	var mags: PackedInt32Array = me.get("magazines", PackedInt32Array())
	var lines := PackedStringArray([
		"%s %s · %s · %s" % [me.get("rank", ""), me.get("name", ""), me.get("role", ""),
			me.get("unit_name", "")],
		"Health %.0f (%s)" % [me.get("health", 0.0), me.get("life", "")],
		"%s  %d/%d  %s · spare mags %d (%d rds)%s%s" % [
			view.data.weapon_id.to_upper(), view.ammo, view.data.magazine_size,
			view.fire_mode().to_upper(), mags.size(), me.get("reserve_rounds", 0),
			"  RELOADING" if view.reloading else "",
			"  NO MAGAZINES" if view.out_of_magazines else ""],
		"",
		"Platoon: %s" % platoon.summary(),
		"Selected: %s" % command.selection_text(),
	])
	if command.last_message != "":
		lines.append("Last order: %s" % command.last_message)
	lines.append("")
	lines.append_array(platoon.unit_status_lines())
	return lines


func _on_sim_event(kind: String, data: Dictionary) -> void:
	# Shots and reloads come from the whole platoon; only show the player's.
	if kind in ["shot_fired", "reloaded", "out_of_ammo"] \
			and data.get("soldier_id", -1) != platoon.player_info.get("id", -2):
		return
	_last_sim_event = "%s %s" % [kind, data]
