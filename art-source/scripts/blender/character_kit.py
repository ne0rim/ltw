"""Shared helpers for the rigged base body and modular clothing/gear (run inside Blender).

Conventions (match the Godot client after glTF export):
  * Metres. +Z up. The character faces -Y, so its left side is +X.
  * One skeleton for everything: the armature object "Rig" from base_body.
    Bone names follow Godot's SkeletonProfileHumanoid (Hips, LeftUpperLeg, ...)
    so animations can be retargeted without a custom bone map.
  * Every clothing/gear file holds its own copy of "Rig" plus its meshes,
    skinned to the same bone names. In Godot, give the piece's meshes the
    body's Skeleton3D and they follow the body.
  * The body inside a gear file sits in the "reference (not exported)"
    collection, so it is there for fitting and previews but never exported.
"""

import math
import os
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Vector
from mathutils.bvhtree import BVHTree
from mathutils.kdtree import KDTree

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)
import ar15_kit as kit  # noqa: E402  (shared save/export helpers)

ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
CATEGORY = "characters"
RIG_NAME = "Rig"
BODY_NAME = "Body"
REFERENCE_COLLECTION = kit.REFERENCE_COLLECTION
TEXTURE_DIR = os.path.join(ROOT, "art-source", "textures")


def paths(model_id):
    return kit.weapon_paths(ROOT, model_id, CATEGORY)


def save_outputs(model_id):
    return kit.save_outputs(ROOT, model_id, CATEGORY)


# --------------------------------------------------------------------------
# Loading the rigged body into a gear file
# --------------------------------------------------------------------------

def base_body_file():
    """The hand-edited base body if it exists, else the generated one."""
    p = paths("base_body")
    for f in (p["hand"], p["generated"]):
        if os.path.exists(f):
            return f
    raise SystemExit("[character_kit] Build the base body first: build_base_body.py")


def load_rigged_body():
    """Append Rig, Body and eyes. Rig stays exportable; the body goes into the
    reference collection. Returns (rig, body)."""
    src = base_body_file()
    with bpy.data.libraries.load(src, link=False) as (data_from, data_to):
        data_to.objects = [n for n in data_from.objects
                           if n in (RIG_NAME, BODY_NAME) or n.startswith(("Eye", "Iris"))]
    ref = bpy.data.collections.new(REFERENCE_COLLECTION)
    bpy.context.scene.collection.children.link(ref)
    rig = body = None
    for obj in data_to.objects:
        if obj is None:
            continue
        if obj.name == RIG_NAME:
            bpy.context.scene.collection.objects.link(obj)
            rig = obj
        else:
            ref.objects.link(obj)
            if obj.name == BODY_NAME:
                body = obj
    if rig is None or body is None:
        raise SystemExit(f"[character_kit] {src} has no '{RIG_NAME}' or '{BODY_NAME}'")
    print(f"[character_kit] body and rig from {src}")
    return rig, body


def load_reference_gear(rig, model_id):
    """Put another finished piece (e.g. the trousers) on the body for checking
    fit and layering. It goes in the reference collection, so it is never
    exported with this piece. Silently skipped if that piece isn't built."""
    p = paths(model_id)
    src = next((f for f in (p["hand"], p["generated"]) if os.path.exists(f)), None)
    if not src:
        print(f"[character_kit] no {model_id} to preview with; skipped")
        return []
    with bpy.data.libraries.load(src, link=False) as (data_from, data_to):
        data_to.objects = list(data_from.objects)
    ref = bpy.data.collections[REFERENCE_COLLECTION]
    kept = []
    for obj in data_to.objects:
        if obj is None:
            continue
        piece = obj.type == 'MESH' and obj.get("model_id") == model_id
        if piece:
            ref.objects.link(obj)
            obj.parent = rig
            for m in obj.modifiers:
                if m.type == 'ARMATURE':
                    m.object = rig
            kept.append(obj)
    for obj in data_to.objects:
        if obj is not None and obj not in kept:
            bpy.data.objects.remove(obj, do_unlink=True)
    print(f"[character_kit] reference {model_id} from {src}")
    return kept


# --------------------------------------------------------------------------
# Mesh helpers
# --------------------------------------------------------------------------

def mesh_world_coords(obj):
    n = len(obj.data.vertices)
    co = np.empty(n * 3)
    obj.data.vertices.foreach_get("co", co)
    co = co.reshape(-1, 3)
    mw = np.array(obj.matrix_world)
    return co @ mw[:3, :3].T + mw[:3, 3]


def bvh_from_object(obj):
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bm.transform(obj.matrix_world)
    bm.normal_update()
    tree = BVHTree.FromBMesh(bm)
    bm.free()
    return tree


def push_outside(bm, tree, ease_at, verts=None):
    """Move verts so each sits at least ease_at(z) outside the surface in tree."""
    for v in (verts if verts is not None else bm.verts):
        loc, normal, _, dist = tree.find_nearest(v.co)
        if loc is None:
            continue
        e = ease_at(v.co.z)
        side = (v.co - loc).dot(normal)
        if side < e:
            v.co = loc + normal * e


def bisect_keep(bm, planes):
    """Cut bm with planes [(co, no), ...] and keep only what is on the +no side
    of every plane. Gives clean straight edges on curved garments."""
    for co, no in planes:
        geom = bm.verts[:] + bm.edges[:] + bm.faces[:]
        bmesh.ops.bisect_plane(bm, geom=geom, plane_co=co, plane_no=no, clear_outer=False, clear_inner=True)


def box_planes(x=None, y=None, z=None):
    """Planes for keeping the inside of an axis-aligned box (None = unbounded)."""
    planes = []
    for axis, rng in ((0, x), (1, y), (2, z)):
        if rng is None:
            continue
        lo, hi = rng
        n = Vector((0, 0, 0))
        n[axis] = 1.0
        c = Vector((0, 0, 0))
        c[axis] = lo
        planes.append((c.copy(), n.copy()))
        c[axis] = hi
        planes.append((c.copy(), -n))
    return planes


def keep_faces(bm, predicate):
    """Delete faces whose (center, normal) fail predicate, then loose verts/edges."""
    bm.normal_update()
    dead = [f for f in bm.faces if not predicate(f.calc_center_median(), f.normal)]
    bmesh.ops.delete(bm, geom=dead, context='FACES')
    loose = [v for v in bm.verts if not v.link_faces]
    bmesh.ops.delete(bm, geom=loose, context='VERTS')


def smooth(bm, times, factor=0.5):
    inner = [v for v in bm.verts if not v.is_boundary]
    for _ in range(times):
        bmesh.ops.smooth_vert(bm, verts=inner, factor=factor,
                              use_axis_x=True, use_axis_y=True, use_axis_z=True)


def fitted_shell(body, rough_keep, planes, ease_at, skin_gap=0.006, unsubdivide=True, relax=5):
    """A garment surface made from the body's own skin:
    1. relax it into a smooth hull that bridges hollows (crotch, armpits)
       and loses muscle detail, without going under the skin;
    2. cut it with planes [(co, no), ...], keeping the +no side of each;
    3. stand it off the hull by ease_at(point) (a Vector, world space);
    4. make sure nothing is closer to the skin than skin_gap.
    relax sets how strongly step 1 bridges hollows (higher = wider bridges;
    keep it low where limbs sit close, e.g. armpits).
    rough_keep(center, normal) picks the faces to start from. Returns a bmesh
    in world space whose open edges lie exactly on the cut planes."""
    skin = bvh_from_object(body)
    bm = bmesh.new()
    bm.from_mesh(body.data)
    bm.transform(body.matrix_world)
    keep_faces(bm, rough_keep)
    # The body is a subdivided sculpt; step back down one level for a
    # lighter garment.
    if unsubdivide:
        bmesh.ops.unsubdivide(bm, verts=bm.verts, iterations=2)

    for _ in range(relax):
        smooth(bm, 8)
        push_outside(bm, skin, lambda z: 0.0)
    bm.normal_update()
    hull = BVHTree.FromBMesh(bm)

    bisect_keep(bm, planes)
    keep_faces(bm, lambda c, n: True)
    for v in bm.verts:
        loc, normal, _, _ = hull.find_nearest(v.co)
        if loc is not None:
            v.co = loc + normal * ease_at(v.co)
    smooth(bm, 3)
    push_outside(bm, skin, lambda z: skin_gap)
    # Put each open edge back exactly on the plane it was cut with.
    for v in bm.verts:
        if v.is_boundary:
            d = [(v.co - Vector(co)).dot(Vector(no)) for co, no in planes]
            k = min(range(len(d)), key=lambda i: abs(d[i]))
            v.co -= Vector(planes[k][1]) * d[k]
    bm.normal_update()
    return bm


def cut_away(bm, plane_co, plane_no, region):
    """Cut along a plane and delete the faces on its -no side that region(center)
    also selects (e.g. a V-neck: only the front, only one half)."""
    geom = bm.verts[:] + bm.edges[:] + bm.faces[:]
    bmesh.ops.bisect_plane(bm, geom=geom, plane_co=plane_co, plane_no=plane_no)
    co, no = Vector(plane_co), Vector(plane_no)
    dead = [f for f in bm.faces
            if (f.calc_center_median() - co).dot(no) < 0 and region(f.calc_center_median())]
    bmesh.ops.delete(bm, geom=dead, context='FACES')
    loose = [v for v in bm.verts if not v.link_faces]
    bmesh.ops.delete(bm, geom=loose, context='VERTS')


def plane_across(head, tail, t, keep_towards_head=True):
    """A plane square to the bone head->tail, a fraction t along it."""
    head, tail = Vector(head), Vector(tail)
    d = (tail - head).normalized()
    return (head.lerp(tail, t), -d if keep_towards_head else d)


def patch(shell_mesh, shell_tree, name, mat, planes, keep, offset, thickness):
    """A piece of material sewn onto a garment: cut from the garment's surface
    so it follows it exactly, lifted off it by offset and given thickness."""
    bm = bmesh.new()
    bm.from_mesh(shell_mesh)
    bisect_keep(bm, planes)
    keep_faces(bm, keep)
    # Plane cuts leave slivers that shade as a ragged edge; merge them away.
    bmesh.ops.dissolve_degenerate(bm, dist=0.0015, edges=bm.edges)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=0.001)
    push_outside(bm, shell_tree, lambda z: offset)
    obj = object_from_bmesh(name, bm, mat)
    solidify(obj, thickness)
    return obj


def join(parts, name):
    """Bake modifiers and join parts into one mesh object called name."""
    kit.apply_all(parts)
    bpy.ops.object.select_all(action='DESELECT')
    for p in parts:
        p.select_set(True)
    bpy.context.view_layer.objects.active = parts[0]
    bpy.ops.object.join()
    obj = parts[0]
    obj.name = name
    obj.data.name = name
    return obj


def object_from_bmesh(name, bm, mat, collection=None):
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    mesh.materials.append(mat)
    for p in mesh.polygons:
        p.use_smooth = True
    obj = bpy.data.objects.new(name, mesh)
    (collection or bpy.context.scene.collection).objects.link(obj)
    return obj


def solidify(obj, thickness, offset=1.0):
    m = obj.modifiers.new("Fabric thickness", 'SOLIDIFY')
    m.thickness = thickness
    m.offset = offset
    m.use_rim = True
    # Even offset blows up thin/folded faces into long spikes; clamp instead.
    m.use_even_offset = False
    m.thickness_clamp = 1.0
    return m


def box_uvs(obj, tile_m):
    """World-space box projection: tile_m metres of fabric per texture repeat.
    Seams between the three projections disappear inside a camouflage print."""
    me = obj.data
    uv = me.uv_layers.get("UVMap") or me.uv_layers.new(name="UVMap")
    mw = obj.matrix_world
    nm = mw.to_3x3().inverted().transposed()
    for poly in me.polygons:
        n = (nm @ poly.normal)
        ax = max(range(3), key=lambda i: abs(n[i]))
        for li in poly.loop_indices:
            p = mw @ me.vertices[me.loops[li].vertex_index].co
            if ax == 0:
                u, v = p.y, p.z
            elif ax == 1:
                u, v = p.x, p.z
            else:
                u, v = p.x, p.y
            uv.data[li].uv = (u / tile_m, v / tile_m)


# --------------------------------------------------------------------------
# Skin weights from the body
# --------------------------------------------------------------------------

def transfer_weights(target, body, rig, k=8, max_influences=4, exclude=None):
    """Copy the body's bone weights onto a garment: each garment vertex blends
    the weights of its k nearest body vertices (inverse-distance squared),
    then keeps the strongest max_influences. Adds the Armature modifier.
    exclude(point) may return bone names a vertex must not follow (e.g. a
    shirt's side panel lying near the hanging forearm)."""
    bco = mesh_world_coords(body)
    names = [g.name for g in body.vertex_groups]
    W = np.zeros((len(bco), len(names)), dtype=np.float32)
    for v in body.data.vertices:
        for g in v.groups:
            W[v.index, g.group] = g.weight
    tree = KDTree(len(bco))
    for i, c in enumerate(bco):
        tree.insert(c, i)
    tree.balance()

    target.vertex_groups.clear()
    groups = [target.vertex_groups.new(name=n) for n in names]
    tco = mesh_world_coords(target)
    used = set()
    for vi, c in enumerate(tco):
        acc = np.zeros(len(names), dtype=np.float32)
        for _, idx, d in tree.find_n(c, k):
            acc += W[idx] / max(d, 1e-4) ** 2
        if exclude:
            banned = [names.index(n) for n in exclude(Vector(c)) if n in names]
            kept = acc.copy()
            kept[banned] = 0.0
            if kept.sum() == 0:
                # Every nearby skin point is excluded: look further afield.
                for _, idx, d in tree.find_n(c, 96):
                    kept += W[idx] / max(d, 1e-4) ** 2
                kept[banned] = 0.0
            if kept.sum() > 0:  # never leave a vertex with nothing to follow
                acc = kept
        top = [t for t in np.argsort(acc)[::-1][:max_influences] if acc[t] > 0]
        total = sum(acc[t] for t in top)
        for t in top:
            groups[t].add([vi], float(acc[t] / total), 'REPLACE')
            used.add(int(t))
    for i in sorted(set(range(len(groups))) - used, reverse=True):
        target.vertex_groups.remove(groups[i])
    mod = target.modifiers.new("Armature", 'ARMATURE')
    mod.object = rig
    target.parent = rig
    target.matrix_parent_inverse = rig.matrix_world.inverted()


def smooth_weights(obj, iterations=4, factor=0.5, max_influences=4):
    """Blend each vertex's bone weights with its neighbours', so a garment
    doesn't tear where neighbouring vertices follow different bones."""
    me = obj.data
    n = len(me.vertices)
    groups = list(obj.vertex_groups)
    W = np.zeros((n, len(groups)), dtype=np.float64)
    for v in me.vertices:
        for g in v.groups:
            W[v.index, g.group] = g.weight
    ev = np.empty(len(me.edges) * 2, dtype=np.int64)
    me.edges.foreach_get("vertices", ev)
    ev = ev.reshape(-1, 2)
    deg = np.bincount(ev.ravel(), minlength=n).astype(np.float64)[:, None]
    for _ in range(iterations):
        acc = np.zeros_like(W)
        np.add.at(acc, ev[:, 0], W[ev[:, 1]])
        np.add.at(acc, ev[:, 1], W[ev[:, 0]])
        avg = np.where(deg > 0, acc / np.maximum(deg, 1), W)
        W = W * (1 - factor) + avg * factor
    for g in groups:
        g.remove(list(range(n)))
    for vi in range(n):
        top = [t for t in np.argsort(W[vi])[::-1][:max_influences] if W[vi, t] > 1e-4]
        total = sum(W[vi, t] for t in top)
        for t in top:
            groups[t].add([vi], float(W[vi, t] / total), 'REPLACE')


# --------------------------------------------------------------------------
# Camouflage textures
# --------------------------------------------------------------------------

def _tileable_field(rng, size, feature_px, aspect=1.0):
    """Smooth, seamlessly tiling noise (zero mean, unit variance). aspect > 1
    stretches the blobs horizontally."""
    n = rng.standard_normal((size, size))
    fy = np.fft.fftfreq(size)[:, None]
    fx = np.fft.fftfreq(size)[None, :]
    s = 1.0 / feature_px
    filt = np.exp(-((fx * aspect) ** 2 + fy ** 2) / (2 * s * s))
    f = np.real(np.fft.ifft2(np.fft.fft2(n) * filt))
    return (f - f.mean()) / f.std()


def _warp(field, wx, wy):
    """Sample field at (x + wx, y + wy) with wrap-around (bilinear)."""
    size = field.shape[0]
    yy, xx = np.mgrid[0:size, 0:size].astype(np.float64)
    x = (xx + wx) % size
    y = (yy + wy) % size
    x0 = np.floor(x).astype(int)
    y0 = np.floor(y).astype(int)
    fx = x - x0
    fy = y - y0
    x1 = (x0 + 1) % size
    y1 = (y0 + 1) % size
    return (field[y0, x0] * (1 - fx) * (1 - fy) + field[y0, x1] * fx * (1 - fy)
            + field[y1, x0] * (1 - fx) * fy + field[y1, x1] * fx * fy)


# US M81 woodland: four colours (sRGB 0-255) and roughly how much of the
# fabric each covers.
WOODLAND_COLOURS = {
    "light_green": (96, 103, 68),
    "brown": (97, 74, 52),
    "khaki": (150, 140, 106),
    "black": (36, 35, 32),
}


def woodland_pixels(size=1024, seed=81):
    """US M81 woodland-style pattern as an (size, size, 3) sRGB float array.
    Big horizontal green and brown blobs, smaller khaki patches, and bold
    black branch-like strokes on top. Tiles seamlessly."""
    rng = np.random.default_rng(seed)
    blob = size / 5.0
    detail = _tileable_field(rng, size, size / 45.0)
    wx = _tileable_field(rng, size, size / 8.0) * size * 0.03
    wy = _tileable_field(rng, size, size / 8.0) * size * 0.03

    def shape(feature, aspect):
        f = _tileable_field(rng, size, feature, aspect)
        return _warp(f, wx, wy) + 0.06 * detail

    brown = shape(blob, 2.0)
    khaki = shape(blob * 0.5, 1.7)
    black = shape(blob * 0.8, 1.8)
    black_breaks = shape(blob * 0.7, 1.4)

    c = WOODLAND_COLOURS
    img = np.empty((size, size, 3))
    img[:] = c["light_green"]
    img[brown > np.quantile(brown, 0.62)] = c["brown"]
    img[khaki > np.quantile(khaki, 0.88)] = c["khaki"]
    stroke = np.abs(black) < np.quantile(np.abs(black), 0.22)
    stroke &= black_breaks > np.quantile(black_breaks, 0.38)
    img[stroke] = c["black"]

    # Faded, slightly uneven dye plus a fine twill so it reads as cloth.
    fade = 1.0 + 0.035 * _tileable_field(rng, size, size / 12.0)
    grain = 1.0 + 0.025 * rng.standard_normal((size, size))
    yy, xx = np.mgrid[0:size, 0:size]
    twill = 1.0 + 0.02 * np.sin((xx + yy) * (2 * math.pi / 4.0))
    img *= (fade * grain * twill)[..., None]
    return np.clip(img / 255.0, 0.0, 1.0)


def save_texture(pixels, path):
    size = pixels.shape[0]
    os.makedirs(os.path.dirname(path), exist_ok=True)
    name = os.path.splitext(os.path.basename(path))[0]
    img = bpy.data.images.new(name, size, size, alpha=False)
    rgba = np.ones((size, size, 4), dtype=np.float32)
    rgba[..., :3] = pixels[::-1]  # Blender stores rows bottom-up
    img.pixels.foreach_set(rgba.ravel())
    img.filepath_raw = path
    img.file_format = 'PNG'
    img.save()
    bpy.data.images.remove(img)
    return path


def woodland_texture():
    path = os.path.join(TEXTURE_DIR, "camo", "us_woodland.png")
    save_texture(woodland_pixels(), path)
    print(f"[character_kit] camo texture {path}")
    return path


def plain_material(name, color, roughness, metallic=0.0):
    return kit.material(name, color, metallic, roughness)


def fabric_material(name, texture_path, roughness=0.9):
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    nt = mat.node_tree
    bsdf = nt.nodes.get("Principled BSDF")
    tex = nt.nodes.new("ShaderNodeTexImage")
    tex.image = bpy.data.images.load(texture_path, check_existing=True)
    tex.location = (-400, 300)
    nt.links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = roughness
    mat.diffuse_color = (0.12, 0.13, 0.09, 1.0)
    return mat


# --------------------------------------------------------------------------
# Previews
# --------------------------------------------------------------------------

def setup_preview_scene():
    scene = bpy.context.scene
    scene.render.engine = 'BLENDER_EEVEE'
    scene.eevee.taa_render_samples = 48
    scene.render.resolution_x, scene.render.resolution_y = 900, 1200
    scene.render.image_settings.file_format = 'PNG'
    scene.view_settings.view_transform = 'AgX'
    world = scene.world or bpy.data.worlds.new("preview")
    scene.world = world
    world.color = (0.08, 0.09, 0.08)

    def light(loc, energy, size, color=(1, 1, 1)):
        data = bpy.data.lights.new("preview_light", 'AREA')
        data.energy, data.size, data.color = energy, size, color
        obj = bpy.data.objects.new("preview_light", data)
        scene.collection.objects.link(obj)
        obj.location = loc
        obj.rotation_euler = (Vector((0, 0, 0.9)) - obj.location).to_track_quat('-Z', 'Y').to_euler()

    light((-3, -4, 5), 700, 4)
    light((3, -2, 2.8), 420, 3, (0.72, 0.82, 1))
    light((0, 4, 3), 350, 3)
    cam = bpy.data.objects.new("preview_cam", bpy.data.cameras.new("preview_cam"))
    scene.collection.objects.link(cam)
    scene.camera = cam
    return cam


def render_view(cam, path, loc, target, ortho=None, lens=50):
    cam.location = loc
    cam.rotation_euler = (Vector(target) - Vector(loc)).to_track_quat('-Z', 'Y').to_euler()
    if ortho:
        cam.data.type = 'ORTHO'
        cam.data.ortho_scale = ortho
    else:
        cam.data.type = 'PERSP'
        cam.data.lens = lens
    bpy.context.scene.render.filepath = path
    bpy.ops.render.render(write_still=True)
    print(f"[character_kit] preview {path}")
    return path


def pose_walk(rig, amount=1.0):
    """A mid-stride pose for checking how garments deform."""
    poses = {
        "LeftUpperLeg": (-35, 0, 0), "LeftLowerLeg": (20, 0, 0),
        "RightUpperLeg": (25, 0, 0), "RightLowerLeg": (55, 0, 0),
        "Spine": (0, 8, 0), "LeftFoot": (-15, 0, 0), "RightFoot": (25, 0, 0),
        "RightToes": (-30, 0, 0),
        "LeftUpperArm": (25, 0, 0), "RightUpperArm": (-25, 0, 0),
        "LeftLowerArm": (-35, 0, 0), "RightLowerArm": (-15, 0, 0),
    }
    for name, rot in poses.items():
        pb = rig.pose.bones.get(name)
        if pb:
            pb.rotation_mode = 'XYZ'
            pb.rotation_euler = [math.radians(a * amount) for a in rot]
    bpy.context.view_layer.update()


def pose_reach(rig):
    """Arms lifted forward and out, for checking shoulders and sleeves."""
    poses = {"LeftUpperArm": (-70, 0, 30), "RightUpperArm": (-40, 0, -50),
             "LeftLowerArm": (-40, 0, 0), "Spine": (10, 0, 0)}
    for name, rot in poses.items():
        pb = rig.pose.bones.get(name)
        if pb:
            pb.rotation_mode = 'XYZ'
            pb.rotation_euler = [math.radians(a) for a in rot]
    bpy.context.view_layer.update()


def pose_rest(rig):
    for pb in rig.pose.bones:
        pb.rotation_mode = 'XYZ'
        pb.rotation_euler = (0, 0, 0)
        pb.location = (0, 0, 0)
    bpy.context.view_layer.update()
