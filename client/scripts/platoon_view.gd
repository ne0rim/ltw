class_name PlatoonView
extends Node3D
## Shows the soldiers the Rust sim knows about. Spawning, organisation, orders
## and all soldier data live in Rust; this node draws them, plays their shots,
## and keeps the sim told about ground heights (the sim doesn't have the
## terrain yet).
## The node itself lives in main.tscn; its Soldier_N children are created at
## runtime from the sim's roster (however many soldiers it has).

## Capsule shade per unit kind / section number, so sections are easy to tell apart.
const SHADES := {"platoon_hq": 0.9, "rifle_section_1": 0.72, "rifle_section_2": 0.58, "rifle_section_3": 0.46}

## The player character commanding this platoon (its platoon commander).
@export var commander: Player

var soldiers: Array = []                 # latest snapshot (Array of Dictionary)
var units: Array = []
var player_info: Dictionary = {}

var _sim: SimBridge
var _terrain: Terrain
var _visuals := {}                       # soldier id -> SoldierVisual
var _effects: ShotEffects
var _selected := PackedInt32Array()


## Spawn the platoon in the sim and build visuals for everyone but the player.
func spawn_platoon(sim: SimBridge, terrain: Terrain, origin: Vector3, yaw: float) -> void:
	sim.spawn_platoon(origin, yaw)
	attach(sim, terrain)


## Build visuals for the platoon the sim already has (e.g. after loading a save).
func attach(sim: SimBridge, terrain: Terrain) -> void:
	_sim = sim
	_terrain = terrain
	_effects = ShotEffects.new()
	_effects.name = "ShotEffects"
	add_child(_effects)
	units = _sim.get_units()
	var shade_for := _unit_shades()
	refresh()
	for info in soldiers:
		if info.is_player:
			continue
		var v := SoldierVisual.new()
		add_child(v)
		var p: Vector3 = info.position
		v.setup(info, shade_for.get(info.unit_id, 0.7), ground_height(p.x, p.z))
		_visuals[info.id] = v
	_sim.ticked.connect(func(_t: int, _n: int) -> void: refresh())
	_sim.sim_event.connect(_on_sim_event)


## Pull a fresh snapshot from the sim, update visuals, and report ground heights.
func refresh() -> void:
	soldiers = _sim.get_soldiers()
	units = _sim.get_units()
	var ids := PackedInt32Array()
	var heights := PackedFloat32Array()
	for info in soldiers:
		if info.is_player:
			player_info = info
			continue
		var p: Vector3 = info.position
		var y := ground_height(p.x, p.z)
		ids.append(info.id)
		heights.append(y)
		if _visuals.has(info.id):
			_visuals[info.id].update_state(info, y)
	_sim.set_soldier_heights(ids, heights)


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
	for u in units:
		if u.kind == "platoon":
			continue
		var n: int = u.soldier_ids.size()
		total += n
		parts.append("%s %d" % [_short_name(u), n])
	return "%d soldiers · %s" % [total, " · ".join(parts)]


## One line per unit: "[11A] 1 Sect · 10 men · moving · 2070 rds" (▶ = selected).
func unit_status_lines() -> PackedStringArray:
	var out := PackedStringArray()
	for u in units:
		if u.kind == "platoon":
			continue
		var mark := "▶" if _selected.has(u.id) else " "
		out.append("%s[%s] %s · %d men · %s · %d rds" % [
			mark, u.callsign, _short_name(u), u.soldier_ids.size(), u.status, u.rounds])
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
	var section_no := 0
	for u in units:
		if u.kind == "platoon_hq":
			out[u.id] = SHADES.platoon_hq
		elif u.kind == "rifle_section":
			section_no += 1
			out[u.id] = SHADES.get("rifle_section_%d" % section_no, 0.6)
	return out
