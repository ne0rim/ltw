"""Build placeholder first-person arms: sleeves, gloved hands, full finger rig.

Rigid segments skinned 100% to their bones (joints hide under sleeves and
glove seams), so posing is reliable. The game drives these bones with IK and
finger curls; see client/scripts/fp_arms.gd.

Bone layout (per side, suffix _r / _l):
  arms_root
    upperarm -> forearm -> hand -> {thumb,index,middle,ring,pinky}_01 -> _02 -> _03
Rest pose: arms straight forward (+Y), palms down, fingers straight.
Every bone points along +Y with roll 0, so bone-local X is the curl axis.

Writes art-source/characters/fp_arms/fp_arms_generated.blend (and, the first
time, fp_arms.blend + client/assets/models/characters/fp_arms.glb).

  blender --background --factory-startup --python art-source/scripts/blender/build_fp_arms.py -- [--previews DIR]
"""

import os
import sys

import bpy
from mathutils import Vector

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import ar15_kit as kit  # noqa: E402  (shared mesh helpers)

ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
MODEL_ID = "fp_arms"

SHOULDER_X = 180.0          # mm from centre to each shoulder
UPPER_LEN = 330.0
FORE_LEN = 300.0
WRIST_Y = UPPER_LEN + FORE_LEN
KNUCKLE_Y = WRIST_Y + 95.0

# Right hand, palm down: thumb on the -X side. (x, phalanx lengths, radius)
FINGERS = {
    "index": (SHOULDER_X - 27, (44, 26, 21), 9.3),
    "middle": (SHOULDER_X - 9, (47, 29, 22), 9.6),
    "ring": (SHOULDER_X + 9, (44, 27, 21), 9.1),
    "pinky": (SHOULDER_X + 26, (35, 21, 18), 8.1),
}
THUMB = ((SHOULDER_X - 42, WRIST_Y + 25, -6), (38, 30, 24), 10.5)


def materials():
    return {
        "sleeve": kit.material("fp_sleeve_og107", (0.20, 0.22, 0.13), 0.0, 0.9),
        "cuff": kit.material("fp_sleeve_cuff", (0.17, 0.19, 0.11), 0.0, 0.9),
        "glove": kit.material("fp_glove_leather", (0.035, 0.032, 0.03), 0.0, 0.55),
    }


def build_side(side, M, parts, bones):
    """Build one arm. Geometry is authored for the right side and mirrored for the left."""
    sx = 1.0 if side == "r" else -1.0
    X = lambda x: sx * x  # noqa: E731

    def bone(name, head, length, parent):
        bones[name] = ((X(head[0]), head[1], head[2]), length, parent)

    bone(f"upperarm_{side}", (SHOULDER_X, 0, 0), UPPER_LEN, "arms_root")
    bone(f"forearm_{side}", (SHOULDER_X, UPPER_LEN, 0), FORE_LEN, f"upperarm_{side}")
    bone(f"hand_{side}", (SHOULDER_X, WRIST_Y, 0), KNUCKLE_Y - WRIST_Y, f"forearm_{side}")

    def add(obj, b):
        parts[obj.name] = (obj, b)

    # Sleeves
    up = kit.tube_y(f"upperarm_{side}_sleeve", -60, UPPER_LEN + 15, 58, M["sleeve"], r1=50, x=X(SHOULDER_X), segs=28)
    add(kit.bevel(up, 6, segments=3), f"upperarm_{side}")
    fore = kit.tube_y(f"forearm_{side}_sleeve", UPPER_LEN - 10, WRIST_Y - 28, 49, M["sleeve"], r1=41, x=X(SHOULDER_X), segs=28)
    add(kit.bevel(fore, 5, segments=3), f"forearm_{side}")
    cuff = kit.tube_y(f"forearm_{side}_cuff", WRIST_Y - 38, WRIST_Y - 10, 44, M["cuff"], r1=43, x=X(SHOULDER_X), segs=28)
    add(kit.bevel(cuff, 3, segments=2), f"forearm_{side}")

    # Glove: wrist cuff + palm
    gcuff = kit.tube_y(f"hand_{side}_glove_cuff", WRIST_Y - 24, WRIST_Y + 18, 37, M["glove"], r1=33, x=X(SHOULDER_X), segs=24)
    add(kit.bevel(gcuff, 3, segments=2), f"hand_{side}")
    palm = kit.loft_y(f"hand_{side}_palm", [
        (WRIST_Y + 5, 33, -15, 15, 3.0), (WRIST_Y + 40, 42, -16, 16, 3.4),
        (KNUCKLE_Y - 8, 44, -15, 15, 3.6), (KNUCKLE_Y + 2, 41, -12, 12, 3.2),
    ], M["glove"], segs=32, x=X(SHOULDER_X))
    add(kit.smooth(palm, 50), f"hand_{side}")

    # Fingers: three rounded segments each
    for fname, (fx, lens, r) in FINGERS.items():
        y = KNUCKLE_Y - 6
        parent = f"hand_{side}"
        for k, L in enumerate(lens):
            bname = f"{fname}_{k + 1:02d}_{side}"
            bone(bname, (fx, y, 0), L, parent)
            rr = r * (1.0 - 0.06 * k)
            seg = kit.tube_y(f"{bname}_glove", y - 3, y + L + 3, rr, M["glove"], r1=rr * 0.94, x=X(fx), segs=16)
            add(kit.bevel(seg, rr * 0.55, segments=3, angle=20), bname)
            parent = bname
            y += L

    # Thumb (straight alongside the hand in rest pose; the game swings it in)
    (tx, ty, tz), lens, r = THUMB
    y, parent = ty, f"hand_{side}"
    for k, L in enumerate(lens):
        bname = f"thumb_{k + 1:02d}_{side}"
        bone(bname, (tx, y, tz), L, parent)
        rr = r * (1.0 - 0.07 * k)
        seg = kit.tube_y(f"{bname}_glove", y - 3, y + L + 3, rr, M["glove"], r1=rr * 0.92, x=X(tx), z=tz, segs=16)
        add(kit.bevel(seg, rr * 0.55, segments=3, angle=20), bname)
        parent = bname
        y += L


def rig(parts, bones):
    arm_data = bpy.data.armatures.new("fp_arms_rig")
    arm = kit._link(bpy.data.objects.new("FP_Arms", arm_data))
    arm["model_id"] = MODEL_ID
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode='EDIT')
    eb = {}
    for name, (head, length, parent) in bones.items():
        b = arm_data.edit_bones.new(name)
        h = Vector((kit.mm(head[0]), kit.mm(head[1]), kit.mm(head[2])))
        b.head = h
        b.tail = h + Vector((0, kit.mm(length), 0))
        b.roll = 0.0
        eb[name] = b
    for name, (_, _, parent) in bones.items():
        if parent:
            eb[name].parent = eb[parent]
            # Chain joints (upperarm->forearm->hand, finger segments) stay connected.
            eb[name].use_connect = (eb[parent].tail - eb[name].head).length < 1e-6
    bpy.ops.object.mode_set(mode='OBJECT')

    for obj, b in parts.values():
        vg = obj.vertex_groups.new(name=b)
        vg.add(list(range(len(obj.data.vertices))), 1.0, 'REPLACE')
        mod = obj.modifiers.new("Armature", 'ARMATURE')
        mod.object = arm
        obj.parent = arm
    return arm


def main(argv):
    previews = argv[argv.index("--previews") + 1] if "--previews" in argv else None
    kit.reset_scene()
    M = materials()
    parts = {}
    bones = {"arms_root": ((0, 0, 0), 30, None)}
    for side in ("r", "l"):
        build_side(side, M, parts, bones)
    kit.apply_all([o for o, _ in parts.values()])
    rig(parts, bones)

    tris = 0
    for obj, _ in parts.values():
        obj.data.calc_loop_triangles()
        tris += len(obj.data.loop_triangles)
    print(f"[{MODEL_ID}] parts={len(parts)} bones={len(bones)} tris={tris}")

    kit.save_outputs(ROOT, MODEL_ID, category="characters")
    if previews:
        os.makedirs(previews, exist_ok=True)
        for p in kit.render_previews(os.path.join(previews, MODEL_ID), look_at=(0.0, 0.45, 0.0)):
            print(f"[{MODEL_ID}] preview", p)


if __name__ == "__main__":
    main(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])
