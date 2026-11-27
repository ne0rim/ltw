"""Black leather combat boots, fitted and skinned to the base body.

Modular piece: the .glb holds the shared "Rig" and one skinned mesh,
"Boots_Combat_Black". Leather upper with a padded collar, toe cap, heel
counter and laced front (criss-cross laces between speed hooks), on a rubber
sole. The shaft is roomy enough to take the trouser legs tucked inside.

Needs the rigged body first (build_base_body.py). The trousers, if built,
are shown in the previews but never exported with the boots. Run from the
project root:
  & "C:\\Program Files\\Blender Foundation\\Blender 5.3\\blender.exe" --background --factory-startup --python art-source\\scripts\\blender\\build_boots_combat.py -- --previews art-source\\characters\\boots_combat\\previews
"""

import math
import os
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import character_kit as ck  # noqa: E402

MODEL_ID = "boots_combat"
MESH_NAME = "Boots_Combat_Black"
TOP_Z = 0.27        # top of the shaft (trouser hem at 0.20 tucks inside)
UPPER_LO_Z = 0.015  # the upper runs down inside the sole to here
SOLE_Z = 0.030      # top of the sole
SOLE_WELT = 0.007   # how far the sole sticks out past the upper

# Leather stand-off from the skin at each height: close over the foot, a
# straight shaft over the ankle, and room for the trousers at the top.
EASE_Z = [0.015, 0.05, 0.10, 0.15, 0.19, 0.27]
EASE_M = [0.007, 0.008, 0.012, 0.020, 0.030, 0.032]


def ease_at(z):
    return float(np.interp(z, EASE_Z, EASE_M))


def materials():
    return {
        "leather": ck.plain_material("Leather | black polished", (0.022, 0.021, 0.020), 0.38),
        "sole": ck.plain_material("Rubber | boot sole", (0.016, 0.016, 0.015), 0.88),
        "lace": ck.plain_material("Laces | black nylon", (0.018, 0.018, 0.017), 0.7),
        "metal": ck.plain_material("Speed hooks | blackened steel", (0.05, 0.05, 0.05), 0.4, 0.8),
    }


def _convex_hull(points):
    """2D convex hull (counter-clockwise), Andrew's monotone chain."""
    pts = sorted(set(map(tuple, points)))

    def cross(o, a, b):
        return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])

    lower, upper = [], []
    for p in pts:
        while len(lower) >= 2 and cross(lower[-2], lower[-1], p) <= 0:
            lower.pop()
        lower.append(p)
    for p in reversed(pts):
        while len(upper) >= 2 and cross(upper[-2], upper[-1], p) <= 0:
            upper.pop()
        upper.append(p)
    return np.array(lower[:-1] + upper[:-1])


def _resample(loop, step=0.006):
    """Evenly spaced points around a closed outline, for clean rounded soles."""
    closed = np.vstack([loop, loop[:1]])
    seg = np.linalg.norm(np.diff(closed, axis=0), axis=1)
    dist = np.concatenate([[0], np.cumsum(seg)])
    n = max(24, int(dist[-1] / step))
    t = np.linspace(0, dist[-1], n, endpoint=False)
    return np.stack([np.interp(t, dist, closed[:, 0]), np.interp(t, dist, closed[:, 1])], axis=1)


def sole(loop, mat, name):
    """Rubber sole: the upper's bottom outline pushed out by the welt, from the
    ground up to SOLE_Z, with rounded edges."""
    c = loop.mean(axis=0)
    # Outward normals of the outline (works whichever way the loop runs).
    nxt = np.roll(loop, -1, axis=0)
    prv = np.roll(loop, 1, axis=0)
    t = nxt - prv
    n = np.stack([t[:, 1], -t[:, 0]], axis=1)
    n /= np.linalg.norm(n, axis=1, keepdims=True)
    if np.mean(np.sum(n * (loop - c), axis=1)) < 0:
        n = -n
    outline = loop + n * SOLE_WELT
    bm = bmesh.new()
    bottom = [bm.verts.new((x, y, 0.0)) for x, y in outline]
    top = [bm.verts.new((x, y, SOLE_Z)) for x, y in outline]
    k = len(outline)
    for i in range(k):
        j = (i + 1) % k
        bm.faces.new((bottom[i], bottom[j], top[j], top[i]))
    bm.faces.new(top)
    bm.faces.new(list(reversed(bottom)))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    obj = ck.object_from_bmesh(name, bm, mat)
    for p in obj.data.polygons:
        p.use_smooth = False
    b = obj.modifiers.new("Rounded sole edge", 'BEVEL')
    b.width = 0.004
    b.segments = 2
    b.limit_method = 'ANGLE'
    obj.modifiers.new("Weighted normals", 'WEIGHTED_NORMAL')
    return obj


RING_POINTS = 64
VERTICAL_SMOOTHING = 25
# Slice heights: close together over the foot, where the outline changes fast.
RING_Z = np.concatenate([np.arange(UPPER_LO_Z, 0.116, 0.004), np.arange(0.12, TOP_Z + 1e-6, 0.010)])


def _offset_outline(loop, dist):
    """Push a closed counter-clockwise outline outward by dist."""
    t = np.roll(loop, -1, axis=0) - np.roll(loop, 1, axis=0)
    n = np.stack([t[:, 1], -t[:, 0]], axis=1)
    n /= np.linalg.norm(n, axis=1, keepdims=True)
    return loop + n * dist


def _ring(outline, n):
    """n points around a closed CCW outline, evenly spaced by length, starting
    straight in front of its centre (-Y) so every ring lines up."""
    dense = _resample(outline, step=0.0005)
    c = dense.mean(axis=0)
    front = [i for i in range(len(dense)) if dense[i, 1] < c[1]]
    start = min(front, key=lambda i: abs(dense[i, 0] - c[0]))
    dense = np.roll(dense, -start, axis=0)
    closed = np.vstack([dense, dense[:1]])
    seg = np.linalg.norm(np.diff(closed, axis=0), axis=1)
    dist = np.concatenate([[0], np.cumsum(seg)])
    t = np.linspace(0, dist[-1], n, endpoint=False)
    return np.stack([np.interp(t, dist, closed[:, 0]), np.interp(t, dist, closed[:, 1])], axis=1)


def skin_slice(co, edges, z, side):
    """Points where the body's edges cross height z, on one leg."""
    z0 = co[edges[:, 0], 2]
    z1 = co[edges[:, 1], 2]
    hit = (z0 - z) * (z1 - z) < 0
    e = edges[hit]
    a, b = co[e[:, 0]], co[e[:, 1]]
    t = ((z - a[:, 2]) / (b[:, 2] - a[:, 2]))[:, None]
    p = a + (b - a) * t
    return p[(p[:, 0] * side > 0.04) & (np.abs(p[:, 0]) < 0.33)][:, :2]


def lofted_upper(body):
    """The boot upper as one clean tube per foot: at each height, the convex
    outline of the foot/leg skin pushed out by the ease. No toes, no folds."""
    co = ck.mesh_world_coords(body)
    edges = np.empty(len(body.data.edges) * 2, dtype=np.int64)
    body.data.edges.foreach_get("vertices", edges)
    edges = edges.reshape(-1, 2)
    bm = bmesh.new()
    bottoms = []
    for side in (1, -1):
        R = np.array([_ring(_offset_outline(_convex_hull(skin_slice(co, edges, z, side)), ease_at(z)),
                            RING_POINTS) for z in RING_Z])
        # Relax up and down the boot so the vamp runs smoothly from toe to shin
        # instead of stepping where the foot ends.
        # Fill-only: points may move out but never in, so the leather still
        # clears the foot without being pushed back against the toes.
        for _ in range(VERTICAL_SMOOTHING):
            S = R.copy()
            S[1:-1] = 0.25 * R[:-2] + 0.5 * R[1:-1] + 0.25 * R[2:]
            for k in range(1, len(R) - 1):
                t = np.roll(R[k], -1, axis=0) - np.roll(R[k], 1, axis=0)
                n = np.stack([t[:, 1], -t[:, 0]], axis=1)
                n /= np.linalg.norm(n, axis=1, keepdims=True)
                d = np.sum((S[k] - R[k]) * n, axis=1)
                S[k] -= n * np.minimum(d, 0.0)[:, None]
            R = S
        rings = [[bm.verts.new((x, y, z)) for x, y in ring] for ring, z in zip(R, RING_Z)]
        bottoms.append(_resample(_convex_hull(skin_slice(co, edges, UPPER_LO_Z, side))))
        bottoms[-1] = _offset_outline(bottoms[-1], ease_at(UPPER_LO_Z))
        for lo, hi in zip(rings, rings[1:]):
            for i in range(RING_POINTS):
                j = (i + 1) % RING_POINTS
                bm.faces.new((lo[i], lo[j], hi[j], hi[i]))  # CCW rings -> outward normals
    bm.normal_update()
    return bm, bottoms


def mid_x(bm, side, z):
    xs = [v.co.x for v in bm.verts if abs(v.co.z - z) < 0.01 and v.co.x * side > 0]
    return float(np.mean(xs))


def _aligned(obj, origin, axis):
    """Place obj (built along local +Z) at origin with its +Z along axis."""
    rot = Vector((0, 0, 1)).rotation_difference(axis).to_matrix().to_4x4()
    obj.matrix_world = Matrix.Translation(origin) @ rot


def lacing(shell_tree, cx_of_z, mats, side):
    """Criss-cross laces up the front, with a speed hook either side of each."""
    parts = []
    zs = np.arange(0.085, TOP_Z - 0.012, 0.0165)
    for i, z in enumerate(zs):
        cx = cx_of_z(z)
        hit, normal, _, _ = shell_tree.ray_cast(Vector((cx, -0.5, z)), Vector((0, 1, 0)))
        if hit is None:
            continue
        across = Vector((1, 0, 0))
        across = (across - normal * across.dot(normal)).normalized()
        tilt = math.radians(16 if i % 2 else -16)
        lace_axis = Matrix.Rotation(tilt, 3, normal) @ across
        bpy.ops.mesh.primitive_cylinder_add(vertices=8, radius=0.0022, depth=0.036)
        lace = bpy.context.object
        lace.name = f"Lace {side}"
        _aligned(lace, hit + normal * 0.004, lace_axis)
        lace.data.materials.append(mats["lace"])
        parts.append(lace)
        for s in (-1, 1):
            bpy.ops.mesh.primitive_uv_sphere_add(segments=10, ring_count=6, radius=0.0032)
            hook = bpy.context.object
            hook.name = f"Speed hook {side}"
            hook.scale = (1, 1, 0.6)
            _aligned(hook, hit + across * (0.0195 * s) + normal * 0.0025, normal)
            hook.data.materials.append(mats["metal"])
            parts.append(hook)
    for p in parts:
        for poly in p.data.polygons:
            poly.use_smooth = True
    return parts


def build_boots(body, mats):
    bm, loops = lofted_upper(body)
    z_samples = [0.09, 0.15, 0.22]
    x_samples = {s: [mid_x(bm, s, z) for z in z_samples] for s in (1, -1)}
    shell_tree = BVHTree.FromBMesh(bm)
    upper = ck.object_from_bmesh(MESH_NAME, bm, mats["leather"])
    ck.solidify(upper, 0.003)
    upper_mesh = upper.data
    leather = mats["leather"]

    parts = [upper]
    parts.append(ck.patch(upper_mesh, shell_tree, "Padded collar", leather,
                          ck.box_planes(z=(TOP_Z - 0.02, TOP_Z)), lambda c, n: True, 0.002, 0.006))
    parts.append(ck.patch(upper_mesh, shell_tree, "Toe cap", leather,
                          ck.box_planes(y=(-1.0, -0.05), z=(UPPER_LO_Z, 0.085)), lambda c, n: True,
                          0.0015, 0.002))
    parts.append(ck.patch(upper_mesh, shell_tree, "Heel counter", leather,
                          ck.box_planes(y=(0.075, 1.0), z=(UPPER_LO_Z, 0.09)),
                          lambda c, n: True, 0.0015, 0.002))
    for s in (1, -1):
        side = "L" if s > 0 else "R"

        def cx_of_z(z, zs=z_samples, xs=x_samples[s]):
            return float(np.interp(z, zs, xs))

        cx = float(np.mean(x_samples[s]))
        parts.append(ck.patch(upper_mesh, shell_tree, f"Tongue {side}", leather,
                              ck.box_planes(x=(cx - 0.023, cx + 0.023), z=(0.07, TOP_Z - 0.004)),
                              lambda c, n: n.y < -0.25, 0.0015, 0.002))
        parts += lacing(shell_tree, cx_of_z, mats, side)
    for i, loop in enumerate(loops):
        parts.append(sole(loop, mats["sole"], "Sole"))
    print(f"[{MODEL_ID}] soles={len(loops)}")
    for p in parts:
        ck.box_uvs(p, 0.3)
    return ck.join(parts, MESH_NAME)


def main(argv):
    previews = os.path.abspath(argv[argv.index("--previews") + 1]) if "--previews" in argv else None
    ck.kit.reset_scene()
    rig, body = ck.load_rigged_body()
    mats = materials()
    boots = build_boots(body, mats)
    ck.transfer_weights(boots, body, rig)
    boots["model_id"] = MODEL_ID
    boots.data.calc_loop_triangles()
    print(f"[{MODEL_ID}] verts={len(boots.data.vertices)} tris={len(boots.data.loop_triangles)} "
          f"bones={[g.name for g in boots.vertex_groups]}")
    ck.save_outputs(MODEL_ID)

    if previews:
        os.makedirs(previews, exist_ok=True)
        ck.load_reference_gear(rig, "trousers_woodland")
        cam = ck.setup_preview_scene()
        ck.render_view(cam, os.path.join(previews, "front.png"), (0, -6.5, 0.5), (0, 0, 0.45), ortho=1.1)
        ck.render_view(cam, os.path.join(previews, "side.png"), (6.5, 0, 0.3), (0, 0, 0.2), ortho=0.6)
        ck.render_view(cam, os.path.join(previews, "back.png"), (0, 6.5, 0.3), (0, 0, 0.2), ortho=0.7)
        ck.render_view(cam, os.path.join(previews, "closeup.png"), (0.75, -0.9, 0.45), (0.1, 0, 0.13), lens=50)
        ck.pose_walk(rig)
        ck.render_view(cam, os.path.join(previews, "walk_side.png"), (6.5, 0, 0.5), (0, 0, 0.45), ortho=1.3)
        ck.render_view(cam, os.path.join(previews, "walk_34.png"), (1.8, -2.2, 0.9), (0, 0, 0.35), lens=50)


if __name__ == "__main__":
    main(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])
