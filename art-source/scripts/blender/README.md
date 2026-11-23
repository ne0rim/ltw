# Models from code (Blender)

Weapons and the first-person arms are generated from code, then hand-edited in Blender.

| Build script | Model id | Category |
|---|---|---|
| `build_m16a2.py` | `m16a2` | weapons |
| `build_fp_arms.py` | `fp_arms` | characters |

## Files per model

| File | Who owns it | Overwritten? |
|---|---|---|
| `art-source/<category>/<id>/<id>_generated.blend` | the build script | every build |
| `art-source/<category>/<id>/<id>.blend` | **you** (hand edits) | never |
| `client/assets/models/<category>/<id>.glb` | exported from **your** `.blend` | only when you export |

The first build of a weapon creates `<id>.blend` and the `.glb`. After that, a
build only refreshes `<id>_generated.blend`. To pull script changes into your
file, copy parts over from the generated file in Blender, or delete your
`<id>.blend` and build again to start fresh from the latest generated version.

## Commands (run from the project root)

Build (or rebuild) the generated file:

```powershell
& "C:\Program Files\Blender Foundation\Blender 5.3\blender.exe" --background --factory-startup --python art-source\scripts\blender\build_m16a2.py
```

Export your hand-edited file to the game (save in Blender first):

```powershell
.\art-source\scripts\export-weapon.ps1 m16a2
```

Or, inside Blender: open `export_weapon.py` in the Scripting tab and press Run Script.

## Rules for hand edits

- Keep object, bone and `socket_*` names. Godot and the game code look them up by name.
- A mesh must never have the same name as a bone (glTF would rename the bone).
- Moving parts are skinned 100% to their bone with an `Armature` modifier. New
  parts need a vertex group named after their bone, plus an `Armature` modifier
  pointing at the rig, and should be parented to the rig.
- Modifiers you add (Bevel, Subdivision, etc.) are applied on export.
- Real-world scale: metres, +Y is the muzzle direction, +Z is up.
