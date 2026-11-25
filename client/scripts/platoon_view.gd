class_name PlatoonView
extends Node3D
## Shows the soldiers the Rust sim knows about. Spawning, organisation, orders
## and all soldier data live in Rust (including their height on the map's
## ground); this node draws them and plays their shots.
## The node itself lives in main.tscn; its Soldier_N children are created at
## runtime from the sim's roster (however many soldiers it has).

## Capsule shade per unit kind / section number, so sections are easy to tell apart.
const SHADES := {"platoon_hq": 0.9, "rifle_section_1": 0.72, "rifle_section_2": 0.58, "rifle_section_3": 0.46}

## The player character commanding this platoon (its platoon commander).
@export var commander: Player

var soldiers: Array = []                 # latest snapshot (Array of Dictionary)
var units: Array = []
var player_info: Dictionary = {}
## The side the player fights for ("blue" or "red"). Lists and keys only show
## this side's units; visuals are made for everyone.
var my_side := "blue"

var _sim: SimBridge
var _terrain: Terrain
var _visuals := {}                       # soldier id -> SoldierVisual
var _effects: ShotEffects
var _selected := PackedInt32Array()


## Start showing the sim's soldiers (both sides). Visuals for soldiers added
## later, such as an enemy platoon, appear on the next refresh.
func attach(sim: SimBridge, terrain: Terrain) -> void:
	_sim = sim
	_terrain = terrain
	_effects = ShotEffects.new()
	_effects.name = "ShotEffects"
	add_child(_effects)
	my_side = _sim.get_player_side()
	refresh()
	_sim.ticked.connect(func(_t: int, _n: int) -> void: refresh())
	_sim.sim_event.connect(_on_sim_event)


## Pull a fresh snapshot from the sim and update the visuals.
func refresh() -> void:
	soldiers = _sim.get_soldiers()
	units = _sim.get_units()
	for info in soldiers:
		if info.is_player:
			player_info = info
			continue
		# The core keeps soldiers on the map's ground; use its height.
		var y: float = (info.position as Vector3).y
		if not _visuals.has(info.id):
			_add_visual(info, y)
		_visuals[info.id].update_state(info, y)


func _add_visual(info: Dictionary, ground_y: float) -> void:
	var v := SoldierVisual.new()
	add_child(v)
	v.setup(info, _unit_shades().get(info.unit_id, 0.7), ground_y)
	_visuals[info.id] = v


## Units on the player's side (not the platoon itself), in id order.
func my_units() -> Array:
	return units.filter(func(u: Dictionary) -> bool: return u.side == my_side and u.kind != "platoon")


func ground_height(x: float, z: float) -> float:
	return _terrain.get_height_at(x, z) if _terrain else 0.0


func set_selected_units(unit_ids: PackedInt32Array) -> void:
	_selected = unit_ids
	for info in soldiers:
		if _visuals.has(info.id):
			_visuals[info.id].set_selected(unit_ids.has(info.unit_id))


func unit(unit_id: int) -> Dictionary:
	for u in units:
		if u.id == unit_id:
			return u
	return {}


## e.g. "36 soldiers · HQ 6 · 1 Sect 10 · 2 Sect 10 · 3 Sect 10"
func summary() -> String:
	var parts := PackedStringArray()
	var total := 0
	for u in my_units():
		var n: int = u.soldier_ids.size()
		total += n
		parts.append("%s %d" % [_short_name(u), n])
	return "%d soldiers · %s" % [total, " · ".join(parts)]


## One line per unit: "[11A] 1 Sect · 10 men · moving · 2070 rds" (▶ = selected).
func unit_status_lines() -> PackedStringArray:
	var out := PackedStringArray()
	for u in my_units():
		var mark := "▶" if _selected.has(u.id) else " "
		out.append("%s[%s] %s · %d men · %s · %d rds" % [
			mark, u.callsign, _short_name(u), u.soldier_ids.size(), u.status, u.rounds])
	return out


## One line per enemy unit, for the dev panel (the player won't get this
## once sight exists): "[E1] Enemy Platoon · attack (fire base + assault) · moving".
func enemy_status_lines() -> PackedStringArray:
	var out := PackedStringArray()
	for u in units:
		if u.side == my_side:
			continue
		var how: String = (" (%s)" % u.playbook) if u.playbook != "" else ""
		out.append("[%s] %s · %s%s · %s · %s" % [u.callsign, u.name, u.task, how,
			u.task_stage, u.status])
	return out


func _short_name(u: Dictionary) -> String:
	return "HQ" if u.kind == "platoon_hq" else "%s Sect" % String(u.name).split(" ")[0]


func _on_sim_event(kind: String, data: Dictionary) -> void:
	if kind == "shot_fired" and _visuals.has(data.soldier_id):
		var v: SoldierVisual = _visuals[data.soldier_id]
		var to: Vector3 = data.target
		to.y = ground_height(to.x, to.z)
		v.play_shot()
		_effects.shot(v.muzzle_position(), to, data.rounds)


func _unit_shades() -> Dictionary:
	var out := {}
	var section_no := {}  # per side
	for u in units:
		if u.kind == "platoon_hq":
			out[u.id] = SHADES.platoon_hq
		elif u.kind == "rifle_section":
			section_no[u.side] = section_no.get(u.side, 0) + 1
			out[u.id] = SHADES.get("rifle_section_%d" % section_no[u.side], 0.6)
	return out
