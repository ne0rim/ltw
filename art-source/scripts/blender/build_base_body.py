"""Rig the 1.8 m base human: skeleton + automatic skin weights.

Every clothing and gear piece is fitted to, and skinned from, this body.
Input: art-source/characters/base_soldier/base_soldier.blend
       (made by build_base_human_soldier.py from Blender's CC0 base mesh).

Run from the project root:
  & "C:\\Program Files\\Blender Foundation\\Blender 5.3\\blender.exe" --background --factory-startup --python art-source\\scripts\\blender\\build_base_body.py -- --previews art-source\\characters\\base_body\\previews

Bone names follow Godot's SkeletonProfileHumanoid. Hands are one bone each
for now (no finger bones yet).
"""

import os
import sys

import bpy
from mathutils import Vector

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import character_kit as ck  # noqa: E402

MODEL_ID = "base_body"
SOURCE = os.path.join(ck.ROOT, "art-source", "characters", "base_soldier", "base_soldier.blend")
SOURCE_BODY = "Human male base | source mesh"
MULTIRES_LEVEL = 1  # sculpt detail kept in the game mesh (0 = lightest)

# Joint positions measured from the 1.8 m body (left side, +X).
# name: (head, tail, parent). Right side is mirrored.
SPINE = {
    "Root": ((0, 0, 0), (0, 0.15, 0), None),
    "Hips": ((0, 0.015, 0.92), (0, 0.02, 1.02), "Root"),
    "Spine": ((0, 0.02, 1.02), (0, 0.025, 1.14), "Hips"),
    "Chest": ((0, 0.025, 1.14), (0, 0.025, 1.27), "Spine"),
    "UpperChest": ((0, 0.025, 1.27), (0, 0.02, 1.46), "Chest"),
    "Neck": ((0, 0.02, 1.46), (0, 0.0, 1.575), "UpperChest"),
    "Head": ((0, 0.0, 1.575), (0, 0.0, 1.80), "Neck"),
}
LIMBS = {
    "Shoulder": ((0.025, -0.005, 1.43), (0.175, 0.03, 1.42), "UpperChest"),
    "UpperArm": ((0.175, 0.03, 1.42), (0.305, 0.035, 1.165), "Shoulder"),
    "LowerArm": ((0.305, 0.035, 1.165), (0.40, -0.025, 0.935), "UpperArm"),
    "Hand": ((0.40, -0.025, 0.935), (0.452, -0.08, 0.80), "LowerArm"),
    "UpperLeg": ((0.09, 0.01, 0.91), (0.145, 0.045, 0.50), "Hips"),
    "LowerLeg": ((0.145, 0.045, 0.50), (0.19, 0.09, 0.085), "UpperLeg"),
    "Foot": ((0.19, 0.09, 0.085), (0.21, -0.035, 0.025), "LowerLeg"),
    "Toes": ((0.21, -0.035, 0.025), (0.22, -0.10, 0.02), "Foot"),
}


def bone_table():
    bones = dict(SPINE)
    for side, sx in (("Left", 1), ("Right", -1)):
        for name, (h, t, parent) in LIMBS.items():
            mh = (h[0] * sx, h[1], h[2])
            mt = (t[0] * sx, t[1], t[2])
            p = parent if parent in SPINE else side + parent
            bones[side + name] = (mh, mt, p)
    return bones


def load_body():
    with bpy.data.libraries.load(SOURCE, link=False) as (data_from, data_to):
        data_to.objects = [n for n in data_from.objects
                           if n == SOURCE_BODY or n.startswith("Eye |")]
    body = eyes = None
    eyes = []
    for obj in data_to.objects:
        bpy.context.scene.collection.objects.link(obj)
        if obj.name == SOURCE_BODY:
            body = obj
        else:
            eyes.append(obj)
    # Bake the sculpt at a game-friendly level.
    for m in body.modifiers:
        if m.type == 'MULTIRES':
            m.levels = MULTIRES_LEVEL
    bpy.context.view_layer.update()  # child world matrices are stale until now
    for obj in [body] + eyes:
        mw = obj.matrix_world.copy()
        obj.parent = None
        obj.matrix_world = mw
    ck.kit.apply_all([body])
    bpy.context.view_layer.objects.active = body
    body.select_set(True)
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    body.name = ck.BODY_NAME
    body.data.name = ck.BODY_NAME
    for e in eyes:
        bpy.ops.object.select_all(action='DESELECT')
        e.select_set(True)
        bpy.context.view_layer.objects.active = e
        bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    return body, eyes


def add_irises(eyes):
    """Dark iris + pupil discs on the front of each eyeball (the character looks along -Y)."""
    iris_mat = ck.kit.material("Iris | dark brown", (0.10, 0.045, 0.025), 0.0, 0.4)
    parts = []
    for eye in eyes:
        co = ck.mesh_world_coords(eye)
        c = co.mean(axis=0)
        r = (co[:, 0].max() - co[:, 0].min()) / 2
        front = co[:, 1].min()
        bpy.ops.mesh.primitive_uv_sphere_add(segments=24, ring_count=12, radius=1,
                                             location=(c[0], front + r * 0.12, c[2]))
        iris = bpy.context.object
        iris.name = "Iris " + eye.name[-1]
        iris.scale = (r * 0.55, r * 0.16, r * 0.55)
        bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
        iris.data.materials.append(iris_mat)
        for p in iris.data.polygons:
            p.use_smooth = True
        parts.append(iris)
    return parts


def build_rig():
    data = bpy.data.armatures.new(ck.RIG_NAME)
    rig = bpy.data.objects.new(ck.RIG_NAME, data)
    bpy.context.scene.collection.objects.link(rig)
    data.display_type = 'STICK'
    rig.show_in_front = True
    bpy.ops.object.select_all(action='DESELECT')
    bpy.context.view_layer.objects.active = rig
    rig.select_set(True)
    bpy.ops.object.mode_set(mode='EDIT')
    table = bone_table()
    eb = {}
    for name, (h, t, _) in table.items():
        b = data.edit_bones.new(name)
        b.head = Vector(h)
        b.tail = Vector(t)
        d = (b.tail - b.head).normalized()
        b.align_roll(Vector((0, 0, 1)) if abs(d.y) > 0.7 else Vector((0, -1, 0)))
        eb[name] = b
    for name, (h, _, parent) in table.items():
        if parent:
            eb[name].parent = eb[parent]
            eb[name].use_connect = (eb[parent].tail - Vector(h)).length < 1e-6
    eb["Root"].use_deform = False
    bpy.ops.object.mode_set(mode='OBJECT')
    rig["model_id"] = MODEL_ID
    return rig


# How much of the flesh around each bone to keep (1 = unchanged). The base
# mesh is a heavy, gym-built figure; a field soldier is leaner. Head, hands
# and feet stay as they are.
SLIM = {
    "Hips": 0.92, "Spine": 0.86, "Chest": 0.88, "UpperChest": 0.90, "Neck": 0.96, "Head": 1.0,
    "Shoulder": 0.92, "UpperArm": 0.86, "LowerArm": 0.92, "Hand": 1.0,
    "UpperLeg": 0.90, "LowerLeg": 0.94, "Foot": 1.0, "Toes": 1.0,
}


def slim(rig, body):
    """Pull the flesh in towards the bones. Each bone squeezes the vertices it
    moves, perpendicular to its own length (so nothing gets shorter), in
    proportion to its skin weight, so the change blends smoothly."""
    bones = {b.name: (b.head_local.copy(), (b.tail_local - b.head_local).normalized())
             for b in rig.data.bones if b.use_deform}
    names = {g.index: g.name for g in body.vertex_groups}
    for v in body.data.vertices:
        move = Vector((0, 0, 0))
        for g in v.groups:
            name = names[g.group]
            if name not in bones:
                continue
            keep = SLIM.get(name.replace("Left", "").replace("Right", ""), 1.0)
            head, axis = bones[name]
            rel = v.co - head
            perp = rel - axis * rel.dot(axis)
            move -= perp * (1.0 - keep) * g.weight
        v.co += move
    body.data.update()


def skin(rig, body, rigid_head_parts):
    bpy.ops.object.select_all(action='DESELECT')
    body.select_set(True)
    rig.select_set(True)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.parent_set(type='ARMATURE_AUTO')
    empty = [g.name for g in body.vertex_groups
             if not any(g.index == x.group and x.weight > 0 for v in body.data.vertices for x in v.groups)]
    if empty:
        print(f"[{MODEL_ID}] WARNING bones with no weights: {empty}")
    unweighted = sum(1 for v in body.data.vertices if not any(x.weight > 0 for x in v.groups))
    print(f"[{MODEL_ID}] body verts={len(body.data.vertices)} unweighted={unweighted}")
    slim(rig, body)
    for obj in rigid_head_parts:
        g = obj.vertex_groups.new(name="Head")
        g.add(list(range(len(obj.data.vertices))), 1.0, 'REPLACE')
        mod = obj.modifiers.new("Armature", 'ARMATURE')
        mod.object = rig
        obj.parent = rig


def main(argv):
    previews = os.path.abspath(argv[argv.index("--previews") + 1]) if "--previews" in argv else None
    ck.kit.reset_scene()
    body, eyes = load_body()
    irises = add_irises(eyes)
    rig = build_rig()
    skin(rig, body, eyes + irises)
    ck.save_outputs(MODEL_ID)

    if previews:
        os.makedirs(previews, exist_ok=True)
        cam = ck.setup_preview_scene()
        ck.render_view(cam, os.path.join(previews, "front.png"), (0, -6.5, 1.0), (0, 0, 0.9), ortho=2.1)
        rig.data.display_type = 'STICK'
        ck.pose_walk(rig)
        ck.render_view(cam, os.path.join(previews, "walk_side.png"), (6.5, 0, 1.0), (0, 0, 0.9), ortho=2.1)
        ck.render_view(cam, os.path.join(previews, "walk_34.png"), (2.6, -3.4, 1.5), (0, 0, 0.85), lens=50)
        ck.render_view(cam, os.path.join(previews, "face.png"), (0.25, -1.2, 1.68), (0, 0, 1.66), lens=85)


if __name__ == "__main__":
    main(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])
