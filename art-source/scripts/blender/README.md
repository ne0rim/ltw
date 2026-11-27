# Models from code (Blender)

Weapons and the first-person arms are generated from code, then hand-edited in Blender.

| Build script | Model id | Category |
|---|---|---|
| `build_m16a2.py` | `m16a2` | weapons |
| `build_fp_arms.py` | `fp_arms` | characters |
| `build_base_body.py` | `base_body` | characters |
| `build_trousers_woodland.py` | `trousers_woodland` | characters |
| `build_boots_combat.py` | `boots_combat` | characters |
| `build_shirt_woodland.py` | `shirt_woodland` | characters |
| `build_soldier_rig.py` | `soldier_rig` | characters |

## Rigged body and modular clothing

`build_base_body.py` rigs the 1.8 m base human (from `base_soldier.blend`) with
the armature `Rig`. Bone names follow Godot's `SkeletonProfileHumanoid`
(`Hips`, `Spine`, `LeftUpperLeg`, ...); hands are a single bone for now.

Each clothing/gear piece (`build_<piece>.py`, helpers in `character_kit.py`)
is fitted to that body and copies its skin weights, so every piece shares the
same skeleton. A piece's `.glb` holds its own copy of `Rig` plus its mesh. In
Godot, point the piece's MeshInstance3D `skeleton` at the body's Skeleton3D.

Inside a piece's `.blend`, the body sits in the `reference (not exported)`
collection: it is there for fitting and checking, and is never exported.
Build the body first, and rebuild pieces after changing the body's rig.
Camouflage textures are generated into `art-source/textures/camo/`.

`build_soldier_rig.py` makes the game's soldier: the body split into regions
(`Body_Head`, `Body_Hands`, `Body_Torso`, `Body_Legs`, `Body_Feet`, so the
game hides what clothing covers), `socket_rifle` on the right hand, and the
in-place animations `idle`, `walk`, `run` (jog) and `aim`. Hands are put on
the M16A2's grips with IK and feet follow stepping paths, then everything is
baked to plain bone keys. In Godot, `SoldierBody` (client/scripts/soldier_body.gd)
dresses it, puts the weapon in the hand and drives the animations. Build
order: body, then clothing pieces, then the soldier rig.

Add `-- --previews <folder>` to a character build to render check images
(front, back, side, and a mid-stride pose).

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
