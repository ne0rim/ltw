```
ltw-prototype/
  core/                   # Pure Rust simulation (no Godot dependency)
    entities/components   # Components representing objects in the game
    schemas               # Base resource types: WeaponData, SoldierData...
    systems               # Combat, morale, veterancy, perception, logistics
    ai/                   # Drills, HTN planner, generate-and-score, queries
    save/                 # Save/load: serialize world, versioning + migrations, JSON (debug) / binary (release)
  godot-extension/        # Rust ↔ Godot bridge (builds the .dll)
    sim_bridge            # New game, tick, save/load/list saves
    commands / state_out / events / data_api / conversions / debug
  godot-client/           # Godot project: everything the player sees and hears
    assets/               # Exported, game-ready files
      models/ textures/ audio/ animations/ ui/
      maps/               # Per map: terrain chunks, heightmaps, vegetation
    scenes/               # Base Scenes & inherited scenes HUD, tactical map, menus
    presentation/         # Godot resources for art, linked to data/ by ID
    generated/            # Build output from tools/ (baked tactical data)
    lt_way.gdextension    # Points Godot to the debug/release .dll
  data/                   #values for weapons, ammo, roles, veterancy, missions
  art-source/             # .blend, World Machine projects, raw audio/textures
  tools/                  # Rust CLI: data validation, tactical bake, ID checks
```
