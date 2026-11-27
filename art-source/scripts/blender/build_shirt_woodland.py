"""Combat shirt in US M81 woodland, fitted and skinned to the base body.

Modular piece: the .glb holds the shared "Rig" and one skinned mesh,
"Shirt_Woodland". Cut like a late-Cold-War BDU blouse, worn outside the
trousers: loose body, full-length sleeves with cuffs, open V-neck with a
collar, button placket, two chest and two lower pockets with flaps.

Needs the rigged body first (build_base_body.py). The trousers and boots, if
built, are shown in the previews but never exported with the shirt. Run from
the project root:
  & "C:\\Program Files\\Blender Foundation\\Blender 5.3\\blender.exe" --background --factory-startup --python art-source\\scripts\\blender\\build_shirt_woodland.py -- --previews art-source\\characters\\shirt_woodland\\previews
"""

import os
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Vector
from mathutils.bvhtree import BVHTree

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import character_kit as ck  # noqa: E402

MODEL_ID = "shirt_woodland"
MESH_NAME = "Shirt_Woodland"
HEM_Z = 0.84        # hangs over the trouser waistband (top at 1.00)
NECK_Z = 1.49       # top of the collar at the back
V_BOTTOM_Z = 1.33   # bottom of the open neck at the front
V_HALF_WIDTH = 0.075  # half the neck opening's width at NECK_Z
CUFF_T = 0.90       # sleeve ends this far down the forearm (0 elbow, 1 wrist)
TILE_M = 0.80

# Torso ease by height: roomy over the trousers and chest, settling onto the
# shoulders. Sleeves get their own, slimmer ease.
TORSO_Z = [0.84, 0.95, 1.10, 1.30, 1.40, 1.46, 1.49]
TORSO_M = [0.024, 0.022, 0.020, 0.018, 0.013, 0.009, 0.007]
SLEEVE_M = 0.015
CUFF_M = 0.011


def make_ease(rig):
    wrist = {s: rig.data.bones[f"{s}LowerArm"].tail_local for s in ("Left", "Right")}

    def ease(p):
        torso = float(np.interp(p.z, TORSO_Z, TORSO_M))
        # 0 on the torso, 1 out along the arm.
        arm = min(max((abs(p.x) - 0.21) / 0.07, 0.0), 1.0)
        w = wrist["Left" if p.x > 0 else "Right"]
        sleeve = CUFF_M if (p - w).length < 0.09 else SLEEVE_M
        return torso * (1 - arm) + sleeve * arm
    return ease


def cut_planes(rig):
    planes = [((0, 0, HEM_Z), (0, 0, 1)), ((0, 0, NECK_Z), (0, 0, -1))]
    for s in ("Left", "Right"):
        b = rig.data.bones[f"{s}LowerArm"]
        planes.append(ck.plane_across(b.head_local, b.tail_local, CUFF_T))
    return planes


def v_neck(bm):
    """Open the front of the neck in a V."""
    rise = NECK_Z - V_BOTTOM_Z
    for s in (1, -1):
        no = Vector((rise * s, 0, -V_HALF_WIDTH)).normalized()  # keeps below the V edge
        ck.cut_away(bm, (0, 0, V_BOTTOM_Z), no,
                    lambda c, s=s: c.y < 0 and c.x * s >= -0.001 and c.z > V_BOTTOM_Z - 0.01)


DRAPE_Z = 1.10  # below this (lower ribs) the shirt hangs straight
DRAPE_FADE = 0.10  # metres over which it eases from following the body to hanging
SKIRT_TOP_Z = 1.06  # below this the shirt is a straight tube down to the hem
HIP_EASE = 0.030    # the tube clears the hips/seat by at least this much
LEG_WEIGHT_KEPT = 0.4  # how much of the thighs' pull the skirt keeps (rest goes to Hips)


def _hull_2d(points):
    pts = sorted(set(map(tuple, np.round(points, 5))))

    def cross(o, a, b):
        return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])

    lower, upper = [], []
    for q in pts:
        while len(lower) >= 2 and cross(lower[-2], lower[-1], q) <= 0:
            lower.pop()
        lower.append(q)
    for q in reversed(pts):
        while len(upper) >= 2 and cross(upper[-2], upper[-1], q) <= 0:
            upper.pop()
        upper.append(q)
    return np.array(lower[:-1] + upper[:-1])


def _offset(loop, dist):
    t = np.roll(loop, -1, axis=0) - np.roll(loop, 1, axis=0)
    n = np.stack([t[:, 1], -t[:, 0]], axis=1)
    n /= np.linalg.norm(n, axis=1, keepdims=True)
    return loop + n * dist


def drape(bm, body):
    """A blouse worn outside the trousers falls straight from the chest: below
    DRAPE_Z, push the torso out to one outline (never in), fading in from
    DRAPE_Z. The outline covers the lower torso and clears the hips and seat."""
    torso = [v for v in bm.verts if v.co.z < DRAPE_Z and abs(v.co.x) < 0.30]
    # Outline from the torso only (sleeves hang out past |x| = 0.23) ...
    pts = [(v.co.x, v.co.y) for v in torso if abs(v.co.x) < 0.23]
    # ... and from the hips/seat the skirt has to hang over.
    co = ck.mesh_world_coords(body)
    hips = co[(co[:, 2] > HEM_Z - 0.02) & (co[:, 2] < SKIRT_TOP_Z) & (np.abs(co[:, 0]) < 0.23)]
    pts += [tuple(p) for p in _offset(_hull_2d(hips[:, :2]), HIP_EASE)]
    hull = _hull_2d(np.array(pts))
    c = hull.mean(axis=0)
    edges = list(zip(hull, np.roll(hull, -1, axis=0)))

    def reach(d):
        """Distance from c to the outline along unit direction d."""
        best = None
        for a, b in edges:
            e = b - a
            den = d[0] * e[1] - d[1] * e[0]
            if abs(den) < 1e-12:
                continue
            w = a - c
            t = (w[0] * e[1] - w[1] * e[0]) / den
            u = (w[0] * d[1] - w[1] * d[0]) / den
            if t > 0 and -1e-9 <= u <= 1 + 1e-9:
                best = t if best is None else min(best, t)
        return best

    for v in torso:
        r = np.array((v.co.x, v.co.y)) - c
        dist = np.linalg.norm(r)
        if dist < 1e-6:
            continue
        target = reach(r / dist)
        if target is None or target <= dist:
            continue
        w = min(max((DRAPE_Z - v.co.z) / DRAPE_FADE, 0.0), 1.0)
        new = c + r / dist * (dist + (target - dist) * w)
        v.co.x, v.co.y = float(new[0]), float(new[1])


def _longest_loop(edges):
    """The connected run of edges with the greatest total length."""
    left = set(edges)
    best, best_len = [], 0.0
    while left:
        group, todo = [], [left.pop()]
        while todo:
            e = todo.pop()
            group.append(e)
            for v in e.verts:
                for n in v.link_edges:
                    if n in left:
                        left.remove(n)
                        todo.append(n)
        length = sum(e.calc_length() for e in group)
        if length > best_len:
            best, best_len = group, length
    return best


def skirt(bm):
    """Replace the torso below SKIRT_TOP_Z with its edge extended straight
    down to the hem: a clean open tube, so nothing wraps between the legs.
    Only the torso is cut; the sleeves hang lower and are left alone."""
    ck.cut_away(bm, (0, 0, SKIRT_TOP_Z), (0, 0, 1), lambda c: abs(c.x) < 0.26)
    edges = _longest_loop([e for e in bm.edges if e.is_boundary
                           and all(abs(v.co.z - SKIRT_TOP_Z) < 1e-4 for v in e.verts)])
    steps = 11
    dz = (SKIRT_TOP_Z - HEM_Z) / steps
    new_faces = []
    for _ in range(steps):
        res = bmesh.ops.extrude_edge_only(bm, edges=edges)
        verts = [g for g in res["geom"] if isinstance(g, bmesh.types.BMVert)]
        for v in verts:
            v.co.z -= dz
        edges = [g for g in res["geom"] if isinstance(g, bmesh.types.BMEdge)
                 and all(v in verts for v in g.verts)]
        new_faces += [g for g in res["geom"] if isinstance(g, bmesh.types.BMFace)]
    bm.normal_update()
    # Make the new faces point outward like the rest of the shirt.
    c = Vector((0, 0.02, 0))
    inward = sum(1 for f in new_faces
                 if f.normal.dot((f.calc_center_median() - c) * Vector((1, 1, 0))) < 0)
    if inward > len(new_faces) / 2:
        bmesh.ops.reverse_faces(bm, faces=new_faces)
    for v in bm.verts:
        if v.is_boundary and abs(v.co.z - HEM_Z) < 1e-4:
            v.co.z = HEM_Z
    bm.normal_update()


ARM_BONES = [f"{s}{b}" for s in ("Left", "Right") for b in ("UpperArm", "LowerArm", "Hand")]
SLEEVE_REACH = 0.09  # a vertex this close to an arm bone is part of a sleeve
ARMPIT_Z = 1.20     # body panels below this never follow the arms


def _segment_distance(p, a, b):
    ab = b - a
    t = max(0.0, min(1.0, (p - a).dot(ab) / ab.length_squared))
    return (p - (a + ab * t)).length


def arm_exclusion(rig):
    """Body panels below the armpits must not follow the arms, even where a
    hanging forearm is closer to them than the torso is."""
    segs = [(rig.data.bones[n].head_local, rig.data.bones[n].tail_local)
            for n in ARM_BONES if "Hand" not in n]

    def exclude(p):
        # Only below the armpits: above, the shoulders must follow the arms.
        if p.z > ARMPIT_Z or any(_segment_distance(p, a, b) < SLEEVE_REACH for a, b in segs):
            return ()
        return ARM_BONES
    return exclude


def soften_leg_weights(obj):
    """The skirt shouldn't be dragged around by the thighs: below the waist,
    keep only part of the legs' weight and give the rest to Hips."""
    names = {g.index: g.name for g in obj.vertex_groups}
    hips = obj.vertex_groups.get("Hips") or obj.vertex_groups.new(name="Hips")
    mw = obj.matrix_world
    for v in obj.data.vertices:
        if (mw @ v.co).z > 1.02:
            continue
        moved = 0.0
        for g in v.groups:
            if "UpperLeg" in names.get(g.group, ""):
                moved += g.weight * (1 - LEG_WEIGHT_KEPT)
                g.weight *= LEG_WEIGHT_KEPT
        if moved:
            hips.add([v.index], moved, 'ADD')


def build_shirt(body, rig, mat):
    planes = cut_planes(rig)
    wrists = [rig.data.bones[f"{s}LowerArm"].tail_local for s in ("Left", "Right")]

    def rough(c, n):
        # Torso and arms, not the head or hands.
        return (HEM_Z - 0.08 < c.z < NECK_Z + 0.05
                and all((c - w).length > 0.05 or c.z > w.z + 0.03 for w in wrists))

    bm = ck.fitted_shell(body, rough, planes, make_ease(rig), relax=1)
    drape(bm, body)
    skirt(bm)
    v_neck(bm)
    bm.normal_update()
    shell_tree = BVHTree.FromBMesh(bm)
    shell = ck.object_from_bmesh(MESH_NAME, bm, mat)
    ck.solidify(shell, 0.004)
    m = shell.data

    def front(c, n):
        return c.y < 0 and n.y < -0.3

    parts = [shell]
    parts.append(ck.patch(m, shell_tree, "Collar", mat,
                          ck.box_planes(x=(-0.12, 0.12), z=(NECK_Z - 0.035, NECK_Z)),
                          lambda c, n: True, 0.003, 0.005))
    parts.append(ck.patch(m, shell_tree, "Placket", mat,
                          ck.box_planes(x=(-0.014, 0.022), z=(HEM_Z, V_BOTTOM_Z)), front, 0.002, 0.003))
    for s in (1, -1):
        side = "L" if s > 0 else "R"

        def xs(lo, hi, s=s):
            return (min(lo * s, hi * s), max(lo * s, hi * s))

        parts.append(ck.patch(m, shell_tree, f"Chest pocket {side}", mat,
                              ck.box_planes(x=xs(0.04, 0.15), z=(1.22, 1.345)), front, 0.004, 0.003))
        parts.append(ck.patch(m, shell_tree, f"Chest flap {side}", mat,
                              ck.box_planes(x=xs(0.035, 0.155), z=(1.32, 1.37)), front, 0.0085, 0.003))
        parts.append(ck.patch(m, shell_tree, f"Lower pocket {side}", mat,
                              ck.box_planes(x=xs(0.035, 0.165), z=(0.87, 1.02)), front, 0.004, 0.003))
        parts.append(ck.patch(m, shell_tree, f"Lower flap {side}", mat,
                              ck.box_planes(x=xs(0.03, 0.17), z=(0.995, 1.045)), front, 0.0085, 0.003))
        arm = rig.data.bones[f"{'Left' if s > 0 else 'Right'}LowerArm"]
        cuff_lo = ck.plane_across(arm.head_local, arm.tail_local, CUFF_T)
        cuff_hi = ck.plane_across(arm.head_local, arm.tail_local, CUFF_T - 0.24, keep_towards_head=False)
        parts.append(ck.patch(m, shell_tree, f"Cuff {side}", mat, [cuff_lo, cuff_hi],
                              lambda c, n, s=s: c.x * s > 0.28, 0.0025, 0.004))
    for p in parts:
        ck.box_uvs(p, TILE_M)
    return ck.join(parts, MESH_NAME)


def main(argv):
    previews = os.path.abspath(argv[argv.index("--previews") + 1]) if "--previews" in argv else None
    ck.kit.reset_scene()
    rig, body = ck.load_rigged_body()
    mat = ck.fabric_material("Fabric | US woodland", ck.woodland_texture())
    shirt = build_shirt(body, rig, mat)
    ck.transfer_weights(shirt, body, rig, exclude=arm_exclusion(rig))
    soften_leg_weights(shirt)
    ck.smooth_weights(shirt)
    shirt["model_id"] = MODEL_ID
    shirt.data.calc_loop_triangles()
    print(f"[{MODEL_ID}] verts={len(shirt.data.vertices)} tris={len(shirt.data.loop_triangles)} "
          f"bones={[g.name for g in shirt.vertex_groups]}")
    ck.save_outputs(MODEL_ID)

    if previews:
        os.makedirs(previews, exist_ok=True)
        ck.load_reference_gear(rig, "trousers_woodland")
        ck.load_reference_gear(rig, "boots_combat")
        cam = ck.setup_preview_scene()
        ck.render_view(cam, os.path.join(previews, "front.png"), (0, -6.5, 1.0), (0, 0, 0.95), ortho=2.0)
        ck.render_view(cam, os.path.join(previews, "back.png"), (0, 6.5, 1.0), (0, 0, 0.95), ortho=2.0)
        ck.render_view(cam, os.path.join(previews, "side.png"), (6.5, 0, 1.0), (0, 0, 0.95), ortho=2.0)
        ck.render_view(cam, os.path.join(previews, "closeup.png"), (0.7, -1.5, 1.35), (0.05, 0, 1.15), lens=50)
        ck.pose_walk(rig)
        ck.render_view(cam, os.path.join(previews, "walk_34.png"), (2.6, -3.4, 1.4), (0, 0, 0.95), lens=40)
        ck.pose_rest(rig)
        ck.pose_reach(rig)
        ck.render_view(cam, os.path.join(previews, "reach_34.png"), (2.2, -2.8, 1.6), (0, 0, 1.15), lens=40)


if __name__ == "__main__":
    main(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])
