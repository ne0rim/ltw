"""Build the M16A2 rifle from code.

Always writes art-source/weapons/m16a2/m16a2_generated.blend. The first run
also creates m16a2.blend (yours to hand-edit) and exports the game .glb from
it. After that, your m16a2.blend and the game .glb are never overwritten;
export your edits with art-source/scripts/export-weapon.ps1 m16a2.

Run from anywhere:
  blender --background --factory-startup --python art-source/scripts/blender/build_m16a2.py -- [--previews DIR]
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import ar15_kit as kit  # noqa: E402

ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))

SPEC = {
    "id": "m16a2",
    "display_name": "M16A2",
    "barrel_length_mm": 508.0,     # 20 in
    "front_sight_y": 535.0,        # ~510 mm sight radius
    "handguard_end_y": 492.0,      # full-length round ribbed A2 handguard
    "rear_sight": "A2",            # adjustable windage + elevation drum
    "stock": "A2",
    "mag_rounds": 30,
}

EXTRAS = {
    "weapon_id": SPEC["id"],
    "display_name": SPEC["display_name"],
    "caliber": "5.56x45mm",
    "fire_modes": "semi,burst3",
    "twist": "1:7",
}


def main(argv):
    previews = None
    if "--previews" in argv:
        previews = argv[argv.index("--previews") + 1]

    kit.reset_scene()
    parts, sockets = kit.build_ar15(SPEC)
    kit.apply_all([o for o, _ in parts.values()])
    arm = kit.rig("M16A2", parts, sockets, EXTRAS)
    actions = kit.make_actions(arm)

    # Report overall size so it's easy to check against the real rifle (~1006 mm).
    import bpy
    from mathutils import Vector
    lo = Vector((9, 9, 9)); hi = Vector((-9, -9, -9))
    tris = 0
    for obj, _ in parts.values():
        for c in obj.bound_box:
            w = obj.matrix_world @ Vector(c)
            lo = Vector(map(min, lo, w)); hi = Vector(map(max, hi, w))
        obj.data.calc_loop_triangles()
        tris += len(obj.data.loop_triangles)
    size = (hi - lo) * 1000
    print(f"[m16a2] parts={len(parts)} tris={tris} size_mm L={size.y:.0f} H={size.z:.0f} W={size.x:.0f}")
    print(f"[m16a2] actions={[a.name for a in actions]}")

    kit.save_outputs(ROOT, SPEC["id"])

    if previews:
        os.makedirs(previews, exist_ok=True)
        for p in kit.render_previews(os.path.join(previews, "m16a2")):
            print("[m16a2] preview", p)


if __name__ == "__main__":
    main(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])
