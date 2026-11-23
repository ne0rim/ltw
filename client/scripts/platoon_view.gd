class_name PlatoonView
extends Node3D
## Shows the soldiers the Rust sim knows about. Spawning, organisation and all
## soldier data live in Rust; this node only draws them and keeps the sim told
## about ground heights (the sim doesn't have the terrain yet).

## Capsule shade per unit kind / section number, so sections are easy to tell apart.
const SHADES := {"platoon_hq": 0.9, "rifle_section_1": 0.72, "rifle_section_2": 0.58, "rifle_section_3": 0.46}

var soldiers: Array = []                 # latest snapshot (Array of Dictionary)
var units: Array = []
var player_info: Dictionary = {}

var _sim: SimBridge
var _terrain: Terrain
var _visuals := {}                       # soldier id -> SoldierVisual


## Spawn the platoon in the sim and build visuals for everyone but the player.
func spawn_platoon(sim: SimBridge, terrain: Terrain, origin: Vector3, yaw: float) -> void:
	_sim = sim
	_terrain = terrain
	_sim.spawn_platoon(origin, yaw)
	units = _sim.get_units()
	var shade_for := _unit_shades()
	for info in _sim.get_soldiers():
		if info.is_player:
			continue
		var p: Vector3 = info.position
		_sim.set_soldier_height(info.id, _terrain.get_height_at(p.x, p.z))
	refresh()
	for info in soldiers:
		if info.is_player:
			continue
		var v := SoldierVisual.new()
		add_child(v)
		v.setup(info, shade_for.get(info.unit_id, 0.7))
		_visuals[info.id] = v
	_sim.ticked.connect(func(_t: int, _n: int) -> void: refresh())


## Pull a fresh snapshot from the sim and update the visuals.
func refresh() -> void:
	soldiers = _sim.get_soldiers()
	for info in soldiers:
		if info.is_player:
			player_info = info
		elif _visuals.has(info.id):
			_visuals[info.id].update_state(info)


## e.g. "36 soldiers · HQ 6 · 1 Sect 10 · 2 Sect 10 · 3 Sect 10"
func summary() -> String:
	var parts := PackedStringArray()
	var total := 0
	for u in units:
		if u.kind == "platoon":
			continue
		var n: int = u.soldier_ids.size()
		total += n
		var short: String = "HQ" if u.kind == "platoon_hq" else "%s Sect" % String(u.name).split(" ")[0]
		parts.append("%s %d" % [short, n])
	return "%d soldiers · %s" % [total, " · ".join(parts)]


func _unit_shades() -> Dictionary:
	var out := {}
	var squad_no := 0
	for u in units:
		if u.kind == "platoon_hq":
			out[u.id] = SHADES.platoon_hq
		elif u.kind == "rifle_section":
			squad_no += 1
			out[u.id] = SHADES.get("rifle_section_%d" % squad_no, 0.6)
	return out
