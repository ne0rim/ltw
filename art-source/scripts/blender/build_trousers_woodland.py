"""Combat trousers in US M81 woodland, fitted and skinned to the base body.

Modular piece: the .glb holds the shared "Rig" and one skinned mesh,
"Trousers_Woodland". Cut like late-Cold-War BDU trousers: loose legs,
waistband with belt loops, fly, thigh cargo pockets with flaps, back pockets
with flaps and knee reinforcement patches. The hem stops above the ankle so
boots can cover it.

Needs the rigged body first (build_base_body.py). Run from the project root:
  & "C:\\Program Files\\Blender Foundation\\Blender 5.3\\blender.exe" --background --factory-startup --python art-source\\scripts\\blender\\build_trousers_woodland.py -- --previews art-source\\characters\\trousers_woodland\\previews
"""

import math
import os
import sys

import bpy
import numpy as np
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import character_kit as ck  # noqa: E402

MODEL_ID = "trousers_woodland"
MESH_NAME = "Trousers_Woodland"
WAIST_Z = 1.00     # top of the waistband
HEM_Z = 0.20       # bottom of the legs (inside the boot)
TILE_M = 0.80      # metres of fabric per camo repeat

# How far the cloth stands off the skin (metres) at each height: snug waist,
# roomy seat and legs, a little blousing just above the boot.
EASE_Z = [0.20, 0.24, 0.30, 0.40, 0.55, 0.75, 0.86, 0.93, 1.00]
EASE_M = [0.012, 0.020, 0.020, 0.019, 0.019, 0.018, 0.014, 0.009, 0.008]


def ease_at(z):
    return float(np.interp(z, EASE_Z, EASE_M))


def build_shell(body):
    """The trouser surface: the skin from waist to ankle, relaxed and stood off
    by the ease. Hands hang beside the thighs, so drop anything wide."""
    return ck.fitted_shell(
        body, lambda c, n: HEM_Z - 0.08 < c.z < WAIST_Z + 0.08 and abs(c.x) < 0.27,
        ck.box_planes(z=(HEM_Z, WAIST_Z)), lambda p: ease_at(p.z))


def belt_loops(shell_tree, mat):
    loops = []
    centre = Vector((0, 0.02, 0.972))
    for deg in (-150, -100, -45, 45, 100, 150, 180):
        a = math.radians(deg)
        d = Vector((math.sin(a), -math.cos(a), 0))
        hit, normal, _, _ = shell_tree.ray_cast(centre, d)
        if hit is None:
            continue
        normal = Vector((normal.x, normal.y, 0)).normalized()
        bpy.ops.mesh.primitive_cube_add(size=1)
        loop = bpy.context.object
        loop.name = "Belt loop"
        loop.dimensions = (0.013, 0.004, 0.052)
        bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
        rot = Vector((0, -1, 0)).rotation_difference(normal).to_matrix().to_4x4()
        loop.matrix_world = Matrix.Translation(hit + normal * 0.010) @ rot
        b = loop.modifiers.new("Rounded", 'BEVEL')
        b.width = 0.0012
        b.segments = 2
        loop.data.materials.append(mat)
        loops.append(loop)
    return loops


def build_trousers(body, mat):
    bm = build_shell(body)
    shell_tree = BVHTree.FromBMesh(bm)
    shell = ck.object_from_bmesh(MESH_NAME, bm, mat)
    ck.solidify(shell, 0.004)
    shell_mesh = shell.data

    parts = [shell]
    parts.append(ck.patch(shell_mesh, shell_tree, "Waistband", mat,
                       ck.box_planes(z=(0.952, WAIST_Z)), lambda c, n: True, 0.0025, 0.005))
    parts.append(ck.patch(shell_mesh, shell_tree, "Fly", mat,
                       ck.box_planes(x=(0.002, 0.042), z=(0.80, 0.95)),
                       lambda c, n: c.y < 0 and n.y < -0.3, 0.002, 0.003))
    for s in (1, -1):
        side = "L" if s > 0 else "R"
        xs = lambda lo, hi: (min(lo * s, hi * s), max(lo * s, hi * s))  # noqa: E731
        parts.append(ck.patch(shell_mesh, shell_tree, f"Cargo pocket {side}", mat,
                           ck.box_planes(y=(-0.085, 0.075), z=(0.56, 0.735)),
                           lambda c, n, s=s: c.x * s > 0.10 and n.x * s > 0.2, 0.005, 0.004))
        parts.append(ck.patch(shell_mesh, shell_tree, f"Cargo flap {side}", mat,
                           ck.box_planes(y=(-0.092, 0.082), z=(0.712, 0.758)),
                           lambda c, n, s=s: c.x * s > 0.10 and n.x * s > 0.2, 0.0105, 0.004))
        parts.append(ck.patch(shell_mesh, shell_tree, f"Back pocket {side}", mat,
                           ck.box_planes(x=xs(0.035, 0.155), z=(0.80, 0.895)),
                           lambda c, n: c.y > 0.03 and n.y > 0.2, 0.004, 0.003))
        parts.append(ck.patch(shell_mesh, shell_tree, f"Back flap {side}", mat,
                           ck.box_planes(x=xs(0.03, 0.16), z=(0.872, 0.905)),
                           lambda c, n: c.y > 0.03 and n.y > 0.2, 0.0085, 0.003))
        parts.append(ck.patch(shell_mesh, shell_tree, f"Knee patch {side}", mat,
                           ck.box_planes(x=xs(0.07, 0.225), z=(0.43, 0.60)),
                           lambda c, n: c.y < 0.04 and n.y < -0.2, 0.0025, 0.003))
    parts += belt_loops(shell_tree, mat)

    for p in parts:
        ck.box_uvs(p, TILE_M)
    return ck.join(parts, MESH_NAME)


def main(argv):
    previews = os.path.abspath(argv[argv.index("--previews") + 1]) if "--previews" in argv else None
    ck.kit.reset_scene()
    rig, body = ck.load_rigged_body()
    mat = ck.fabric_material("Fabric | US woodland", ck.woodland_texture())
    trousers = build_trousers(body, mat)
    ck.transfer_weights(trousers, body, rig)
    trousers["model_id"] = MODEL_ID
    trousers.data.calc_loop_triangles()
    print(f"[{MODEL_ID}] verts={len(trousers.data.vertices)} tris={len(trousers.data.loop_triangles)} "
          f"bones={[g.name for g in trousers.vertex_groups]}")
    ck.save_outputs(MODEL_ID)

    if previews:
        os.makedirs(previews, exist_ok=True)
        cam = ck.setup_preview_scene()
        ck.render_view(cam, os.path.join(previews, "front.png"), (0, -6.5, 0.9), (0, 0, 0.75), ortho=1.9)
        ck.render_view(cam, os.path.join(previews, "back.png"), (0, 6.5, 0.9), (0, 0, 0.75), ortho=1.9)
        ck.render_view(cam, os.path.join(previews, "side.png"), (6.5, 0, 0.9), (0, 0, 0.75), ortho=1.9)
        ck.render_view(cam, os.path.join(previews, "closeup.png"), (0.9, -1.4, 1.0), (0.05, 0, 0.75), lens=50)
        ck.pose_walk(rig)
        ck.render_view(cam, os.path.join(previews, "walk_34.png"), (2.6, -3.4, 1.4), (0, 0, 0.7), lens=50)
        ck.render_view(cam, os.path.join(previews, "walk_side.png"), (6.5, 0, 0.9), (0, 0, 0.75), ortho=1.9)


if __name__ == "__main__":
    main(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])
