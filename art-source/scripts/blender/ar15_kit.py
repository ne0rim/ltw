"""Shared AR-15 family parts kit for Blender (run inside Blender's Python).

Builds an M16/C7/M4-family rifle from a spec dictionary, so variants differ
only in their spec (barrel length, handguard, stock, rear sight...).

Conventions (match the Godot client after glTF export):
  * Units are metres in Blender. Profiles below are written in millimetres.
  * +Y points out of the muzzle (becomes Godot -Z, i.e. "forward").
  * +Z is up. +X is the rifle's right side (ejection port side).
  * Origin is on the bore axis at the rear face of the upper receiver.

Every moving part is skinned 100% to its own bone, so animations only move
bones. Mesh names must never equal a bone name (glTF would rename the bone). Empties named socket_* mark attachment and handling points.
"""

import math
import os
import shutil

import bmesh
import bpy
from mathutils import Matrix, Vector

MM = 0.001


def mm(v):
    return v * MM


# --------------------------------------------------------------------------
# Scene helpers
# --------------------------------------------------------------------------

def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.unit_settings.system = 'METRIC'
    scene.unit_settings.scale_length = 1.0
    return scene


def material(name, color, metallic, roughness):
    mat = bpy.data.materials.get(name) or bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes.get("Principled BSDF")
    bsdf.inputs["Base Color"].default_value = (*color, 1.0)
    bsdf.inputs["Metallic"].default_value = metallic
    bsdf.inputs["Roughness"].default_value = roughness
    mat.diffuse_color = (*color, 1.0)  # shown by Workbench previews
    mat.metallic = metallic
    mat.roughness = roughness
    return mat


def standard_materials():
    return {
        # Black anodised aluminium (receivers)
        "receiver": material("ar_receiver_anodized", (0.040, 0.041, 0.043), 0.55, 0.42),
        # Black polymer (stock, grip, handguard)
        "polymer": material("ar_furniture_polymer", (0.028, 0.028, 0.028), 0.0, 0.68),
        # Parkerised / phosphated steel (barrel, sights, small parts)
        "steel": material("ar_steel_parkerized", (0.050, 0.050, 0.052), 0.85, 0.55),
        # Magazine body
        "mag": material("ar_magazine", (0.060, 0.060, 0.058), 0.5, 0.5),
        # Cartridge
        "brass": material("ammo_brass", (0.72, 0.52, 0.22), 1.0, 0.35),
        "copper": material("ammo_copper", (0.62, 0.32, 0.18), 1.0, 0.35),
    }


def _link(obj):
    bpy.context.scene.collection.objects.link(obj)
    return obj


def _mesh_obj(name, bm, mat):
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    for p in mesh.polygons:
        p.use_smooth = True
    obj = _link(bpy.data.objects.new(name, mesh))
    if mat is not None:
        obj.data.materials.append(mat)
    return obj


# --------------------------------------------------------------------------
# Primitive builders (all inputs in mm)
# --------------------------------------------------------------------------

def prism(name, profile_yz, width, mat, x=0.0):
    """Extrude a side-view (y, z) outline across X. Good for flat-sided parts."""
    bm = bmesh.new()
    hw = width / 2.0
    left = [bm.verts.new((mm(x - hw), mm(y), mm(z))) for y, z in profile_yz]
    right = [bm.verts.new((mm(x + hw), mm(y), mm(z))) for y, z in profile_yz]
    bm.faces.new(left)
    bm.faces.new(list(reversed(right)))
    n = len(profile_yz)
    for i in range(n):
        j = (i + 1) % n
        bm.faces.new([left[i], left[j], right[j], right[i]])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return _mesh_obj(name, bm, mat)


def taper_x(obj, y0, k0, y1, k1):
    """Scale an object's width (X) linearly from k0 at y0 to k1 at y1 (mm)."""
    for v in obj.data.vertices:
        t = min(1.0, max(0.0, (v.co.y / MM - y0) / (y1 - y0)))
        v.co.x *= k0 + (k1 - k0) * t
    return obj


def box(name, y0, y1, z0, z1, x0, x1, mat):
    return prism(name, [(y0, z0), (y1, z0), (y1, z1), (y0, z1)], x1 - x0, mat, x=(x0 + x1) / 2)


def tube_y(name, y0, y1, r0, mat, r1=None, z=0.0, x=0.0, segs=32):
    """Solid cylinder/cone along Y from y0 (radius r0) to y1 (radius r1)."""
    r1 = r0 if r1 is None else r1
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=segs, radius1=mm(r0), radius2=mm(r1), depth=mm(y1 - y0))
    bmesh.ops.rotate(bm, verts=bm.verts, cent=(0, 0, 0), matrix=Matrix.Rotation(math.radians(-90), 3, 'X'))
    bmesh.ops.translate(bm, verts=bm.verts, vec=(mm(x), mm((y0 + y1) / 2), mm(z)))
    return _mesh_obj(name, bm, mat)


def tube_x(name, x0, x1, r, y, z, mat, segs=24):
    """Cylinder along X (pins, knobs, buttons)."""
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=segs, radius1=mm(r), radius2=mm(r), depth=mm(x1 - x0))
    bmesh.ops.rotate(bm, verts=bm.verts, cent=(0, 0, 0), matrix=Matrix.Rotation(math.radians(90), 3, 'Y'))
    bmesh.ops.translate(bm, verts=bm.verts, vec=(mm((x0 + x1) / 2), mm(y), mm(z)))
    return _mesh_obj(name, bm, mat)


def lathe_y(name, stations, mat, segs=48, z=0.0, radius_pattern=None):
    """Revolve a list of (y, radius) stations around the Y axis.
    radius_pattern(i) -> multiplier per segment, for ribs and flats."""
    bm = bmesh.new()
    rings = []
    for y, r in stations:
        ring = []
        for i in range(segs):
            a = 2 * math.pi * i / segs
            k = radius_pattern(i) if radius_pattern else 1.0
            ring.append(bm.verts.new((mm(r * k * math.cos(a)), mm(y), mm(z + r * k * math.sin(a)))))
        rings.append(ring)
    for ra, rb in zip(rings, rings[1:]):
        for i in range(segs):
            j = (i + 1) % segs
            bm.faces.new([ra[i], ra[j], rb[j], rb[i]])
    bm.faces.new(list(reversed(rings[0])))
    bm.faces.new(rings[-1])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return _mesh_obj(name, bm, mat)


def torus_x(name, y, z, major, minor, mat, x=0.0, segs=24, rsegs=8):
    """Ring lying in the YZ plane (axis along X): sling swivels, loops."""
    bm = bmesh.new()
    rings = []
    for i in range(segs):
        a = 2 * math.pi * i / segs
        c = Vector((0, math.cos(a) * major, math.sin(a) * major))
        n = c.normalized()
        ring = []
        for j in range(rsegs):
            b = 2 * math.pi * j / rsegs
            p = c + n * (math.cos(b) * minor) + Vector((math.sin(b) * minor, 0, 0))
            ring.append(bm.verts.new((mm(x + p.x), mm(y + p.y), mm(z + p.z))))
        rings.append(ring)
    for i in range(segs):
        ra, rb = rings[i], rings[(i + 1) % segs]
        for j in range(rsegs):
            k = (j + 1) % rsegs
            bm.faces.new([ra[j], ra[k], rb[k], rb[j]])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return _mesh_obj(name, bm, mat)


def _section(hw, z0, z1, n, segs, x=0.0):
    """Superellipse cross-section (x, z) points: hw = half width, z0..z1 = bottom..top.
    n = squareness (2 is an ellipse, 4-6 is a rounded box)."""
    pts = []
    zc, hz = (z0 + z1) / 2.0, (z1 - z0) / 2.0
    for i in range(segs):
        a = 2 * math.pi * i / segs
        c, s_ = math.cos(a), math.sin(a)
        px = math.copysign(abs(c) ** (2.0 / n), c) * hw
        pz = math.copysign(abs(s_) ** (2.0 / n), s_) * hz
        pts.append((x + px, zc + pz))
    return pts


def loft_y(name, stations, mat, segs=48, x=0.0):
    """Loft rounded-box cross-sections along Y.
    stations: list of (y, half_width, z_bottom, z_top, squareness)."""
    bm = bmesh.new()
    rings = []
    for y, hw, z0, z1, n in stations:
        rings.append([bm.verts.new((mm(px), mm(y), mm(pz))) for px, pz in _section(hw, z0, z1, n, segs, x)])
    for ra, rb in zip(rings, rings[1:]):
        for i in range(segs):
            j = (i + 1) % segs
            bm.faces.new([ra[i], ra[j], rb[j], rb[i]])
    bm.faces.new(list(reversed(rings[0])))
    bm.faces.new(rings[-1])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return _mesh_obj(name, bm, mat)


def loft_z(name, stations, mat, segs=40):
    """Loft horizontal rounded slices along Z (for grips).
    stations: list of (z, y_back, y_front, half_width, squareness), top to bottom."""
    bm = bmesh.new()
    rings = []
    for z, yb, yf, hw, n in stations:
        yc, a = (yb + yf) / 2.0, (yf - yb) / 2.0
        ring = []
        for i in range(segs):
            t = 2 * math.pi * i / segs
            c, s_ = math.cos(t), math.sin(t)
            py = yc + math.copysign(abs(c) ** (2.0 / n), c) * a
            px = math.copysign(abs(s_) ** (2.0 / n), s_) * hw
            ring.append(bm.verts.new((mm(px), mm(py), mm(z))))
        rings.append(ring)
    for ra, rb in zip(rings, rings[1:]):
        for i in range(segs):
            j = (i + 1) % segs
            bm.faces.new([ra[i], ra[j], rb[j], rb[i]])
    bm.faces.new(rings[0])
    bm.faces.new(list(reversed(rings[-1])))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return _mesh_obj(name, bm, mat)


def prism_xy(name, profile_xy, z0, z1, mat):
    """Extrude a top-view (x, y) outline up along Z (for latches, flat levers)."""
    bm = bmesh.new()
    lo = [bm.verts.new((mm(x), mm(y), mm(z0))) for x, y in profile_xy]
    hi = [bm.verts.new((mm(x), mm(y), mm(z1))) for x, y in profile_xy]
    bm.faces.new(lo)
    bm.faces.new(list(reversed(hi)))
    n = len(profile_xy)
    for i in range(n):
        j = (i + 1) % n
        bm.faces.new([lo[i], lo[j], hi[j], hi[i]])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return _mesh_obj(name, bm, mat)


def tube_radial(name, y, z, angle_deg, r, depth, mat, x=0.0, segs=16, outer=40.0):
    """Cylinder pointing out from the Y axis at angle_deg (0 = +X, 90 = up).
    Used as a cutter for vent holes and for radial pins."""
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=segs, radius1=mm(r), radius2=mm(r), depth=mm(depth))
    a = math.radians(angle_deg)
    # Cone axis is Z; aim it along (cos a, 0, sin a).
    rot = Matrix.Rotation(math.radians(90) - a, 3, 'Y')
    bmesh.ops.rotate(bm, verts=bm.verts, cent=(0, 0, 0), matrix=rot)
    d = outer - depth / 2.0
    bmesh.ops.translate(bm, verts=bm.verts, vec=(mm(x + math.cos(a) * d), mm(y), mm(z + math.sin(a) * d)))
    return _mesh_obj(name, bm, mat)


def smooth(obj, angle=40.0):
    """Smooth shading with hard edges above `angle` (for lofted/curved parts)."""
    try:
        obj.data.set_sharp_from_angle(angle=math.radians(angle))
    except AttributeError:
        pass
    return obj


def curve_points(p0, p1, p2, n):
    """Quadratic Bezier from p0 to p2 with control p1, n points."""
    out = []
    for i in range(n):
        t = i / (n - 1)
        y = (1 - t) ** 2 * p0[0] + 2 * (1 - t) * t * p1[0] + t ** 2 * p2[0]
        z = (1 - t) ** 2 * p0[1] + 2 * (1 - t) * t * p1[1] + t ** 2 * p2[1]
        out.append((y, z))
    return out


# --------------------------------------------------------------------------
# Modifiers
# --------------------------------------------------------------------------

def bevel(obj, width, segments=3, angle=30.0):
    m = obj.modifiers.new("Bevel", 'BEVEL')
    m.width = mm(width)
    m.segments = segments
    m.limit_method = 'ANGLE'
    m.angle_limit = math.radians(angle)
    m.use_clamp_overlap = True
    m.harden_normals = True
    return obj


def cut(obj, cutter):
    """Boolean-subtract `cutter` from `obj` (cutter is deleted when applied)."""
    m = obj.modifiers.new("Cut_" + cutter.name, 'BOOLEAN')
    m.operation = 'DIFFERENCE'
    m.object = cutter
    m.solver = 'EXACT'
    cutter.hide_render = True
    cutter.display_type = 'WIRE'
    return obj


def apply_all(objs):
    """Bake every modifier into the mesh, then delete boolean cutters."""
    dg = bpy.context.evaluated_depsgraph_get()
    cutters = set()
    for obj in objs:
        if not obj.modifiers:
            continue
        for m in obj.modifiers:
            if m.type == 'BOOLEAN' and m.object:
                cutters.add(m.object)
        ev = obj.evaluated_get(dg)
        new_mesh = bpy.data.meshes.new_from_object(ev, preserve_all_data_layers=True, depsgraph=dg)
        old = obj.data
        obj.modifiers.clear()
        obj.data = new_mesh
        bpy.data.meshes.remove(old)
    for c in cutters:
        bpy.data.objects.remove(c, do_unlink=True)


# --------------------------------------------------------------------------
# The rifle
# --------------------------------------------------------------------------

def build_ar15(spec):
    """Build every part. Returns (parts dict name -> (object, bone), sockets dict)."""
    M = standard_materials()
    parts = {}

    def add(obj, bone="root"):
        parts[obj.name] = (obj, bone)
        return obj

    barrel_start = 168.0                      # barrel face inside upper receiver
    muzzle = barrel_start + spec["barrel_length_mm"]
    hider_len = 57.0

    # ---- Lower receiver ------------------------------------------------
    lower = prism("lower_receiver", [
        (0, -11), (0, -38), (6, -46), (40, -47), (94, -47), (96, -84), (100, -88),
        (164, -88), (167, -82), (165, -46), (172, -40), (180, -30), (180, -11),
    ], 23.0, M["receiver"])
    # Hollow magazine well, so it reads as an opening when the magazine is out
    cut(lower, box("cutter_magwell", 100.5, 161.5, -96, -38, -10.8, 10.8, None))
    bevel(lower, 1.6)
    add(lower)
    # Flared lip at the bottom of the magazine well
    flare = prism("magwell_flare", [(95, -78), (165, -78), (170, -86), (168, -92), (96, -92), (92, -86)],
                  26.0, M["receiver"])
    cut(flare, box("cutter_magwell_flare", 100.5, 161.5, -96, -70, -10.8, 10.8, None))
    add(bevel(flare, 1.0))
    # Front pivot-pin boss
    add(bevel(prism("lower_front_boss", [(165, -44), (176, -34), (183, -26), (183, -12), (171, -12), (165, -28)],
                    25.0, M["receiver"]), 1.5))
    # Raised fence around the magazine release (right side), button and bolt catch (left)
    add(bevel(prism("mag_release_fence", [(90, -18), (106, -18), (108, -26), (104, -34), (92, -34), (88, -26)],
                    1.6, M["receiver"], x=12.2), 0.5))
    add(bevel(tube_x("mag_release_button", 11.5, 15.5, 4.5, 98, -26, M["steel"]), 0.4))
    add(bevel(prism("bolt_catch", [(90, -15), (104, -12), (107, -17), (104, -22), (92, -21)], 2.2, M["steel"], x=-12.6), 0.5))
    add(bevel(tube_x("bolt_catch_pin", -13.2, -11.2, 1.3, 98, -28, M["steel"], segs=12), 0.2))
    # Pins: front pivot, rear takedown (with heads on the right), hammer and trigger
    for pname, y, z, r, head in (("pin_pivot", 175, -22, 3.0, True), ("pin_takedown", 9, -22, 3.0, True),
                                 ("pin_hammer", 61, -40, 2.3, False), ("pin_trigger", 78, -45, 2.3, False)):
        add(bevel(tube_x(pname, -12.4, 12.4, r, y, z, M["steel"], segs=16), 0.3))
        if head:
            add(bevel(tube_x(pname + "_head", 11.5, 13.2, r + 1.6, y, z, M["steel"], segs=20), 0.4))
    # Receiver extension (buffer tube) boss
    add(bevel(tube_y("buffer_tube", -40, 2, 13.5, M["receiver"], z=-2), 0.8))

    # ---- Upper receiver ------------------------------------------------
    # Lofted so the top corners are rounded and the front (barrel extension
    # housing) turns round, like the forging.
    upper = loft_y("upper_receiver", [
        (0, 10.5, -11, 13.0, 5), (2, 11.5, -11, 15.0, 7), (150, 11.5, -11, 15.0, 7),
        (160, 12.5, -11, 13.5, 3.6), (178, 12.5, -11, 12.0, 3.0), (182, 11.5, -10, 11.0, 3.0),
    ], M["receiver"], segs=48)
    # Ejection port (right side)
    cut(upper, box("cutter_ejection_port", 62, 124, -6.5, 7.0, 3.0, 20.0, None))
    smooth(upper, 35)
    add(upper)
    # Brass deflector bump behind the port
    add(bevel(prism("brass_deflector", [(44, -2), (60, -2), (60, 15), (50, 15)], 8.0, M["receiver"], x=13.5), 1.2))
    # Forward assist (right rear) with a grooved plunger button
    add(bevel(tube_x("forward_assist_housing", 9, 22, 7.0, 16, 5, M["receiver"]), 0.8))
    add(bevel(tube_x("forward_assist_button", 21, 27.5, 5.6, 16, 5, M["steel"]), 0.8))
    for k, x0 in enumerate((22.0, 23.8, 25.6)):
        add(tube_x(f"forward_assist_ridge_{k}", x0, x0 + 0.9, 6.3, 16, 5, M["steel"]))
    # Dust cover hinge rod
    add(bevel(tube_y("dust_cover_rod", 58, 128, 1.1, M["steel"], x=12.2, z=-8.2, segs=10), 0.2))

    # Carry handle with integrated rear sight
    # A2 profile: upright rear leg under the sight, flat grip bar, and a long
    # front leg that slants down to the receiver, giving the triangular look.
    handle = prism("carry_handle", [
        (12, 14), (12, 48), (20, 60), (104, 60), (112, 56), (154, 17), (156, 14),
        (136, 14), (101, 44), (42, 44), (34, 14),
    ], 15.0, M["receiver"])
    cut(handle, box("cutter_handle_slot", 50, 96, 49, 55, -20, 20, None))
    cut(handle, box("cutter_handle_groove", 46, 102, 58.6, 62, -1.6, 1.6, None))   # mounting groove on top
    bevel(handle, 1.5, segments=3)
    add(handle)
    add(bevel(tube_x("carry_handle_bolt", -9, 9, 3.2, 143, 21, M["steel"], segs=16), 0.3))

    sight = spec.get("rear_sight", "A2")
    rear = prism("rear_sight_housing", [(12, 44), (12, 62), (40, 62), (44, 56), (44, 44)], 24.0, M["receiver"])
    bevel(rear, 1.2)
    add(rear)
    aperture = prism("rear_sight_aperture", [(22, 56), (22, 70), (28, 70), (30, 56)], 18.0, M["steel"])
    cut(aperture, tube_y("cutter_peep", 15, 35, 1.8 if sight == "A2" else 2.5, None, z=64))
    bevel(aperture, 0.6)
    add(aperture)
    # Second (large, low-light) aperture folded down behind the first
    add(bevel(box("rear_sight_aperture_2", 30, 37, 56, 59.5, -8, 8, M["steel"]), 0.5))
    if sight == "A2":
        # A2 windage knob (right) and elevation drum (under the aperture), both ridged
        add(bevel(tube_x("windage_knob", 12, 19, 6.5, 26, 52, M["steel"], segs=20), 0.6))
        for k in range(4):
            add(tube_x(f"windage_knob_ridge_{k}", 13 + k * 1.6, 13.7 + k * 1.6, 7.1, 26, 52, M["steel"], segs=20))
        add(bevel(tube_x("elevation_drum", -11, 11, 9.5, 30, 50, M["steel"], segs=28), 0.8))
        for k in range(5):
            add(tube_x(f"elevation_drum_ridge_{k}", -10 + k * 4.6, -9.2 + k * 4.6, 10.1, 30, 50, M["steel"], segs=28))

    # Charging handle (moves): shaft plus T latch seen from above
    ch = prism("charging_handle_body", [(-22, 7), (-22, 15), (6, 15), (6, 7)], 12.0, M["receiver"])
    bevel(ch, 1.2)
    add(ch, "charging_handle")
    latch = prism_xy("charging_handle_latch", [
        (-26, -23), (26, -23), (26, -16), (8, -14), (8, -9), (-8, -9), (-8, -14),
        (-19, -14), (-19, -9), (-26, -9),
    ], 8.0, 15.0, M["receiver"])
    add(bevel(latch, 1.4), "charging_handle")

    # Bolt carrier, visible through the ejection port (moves)
    add(bevel(tube_y("bolt_carrier_body", 14, 150, 9.5, M["steel"], z=1, segs=32), 0.8), "bolt_carrier")

    # Dust cover over the port (hinged along its bottom edge), with detent bump
    add(bevel(box("dust_cover_plate", 61, 125, -7.5, 8.0, 11.0, 12.6, M["steel"]), 0.5), "dust_cover")
    add(bevel(box("dust_cover_detent", 66, 73, 1.5, 5.0, 12.4, 13.4, M["steel"]), 0.4), "dust_cover")

    # ---- Fire controls ------------------------------------------------
    add(bevel(prism("trigger_blade", [
        (79, -44), (82, -47), (80, -55), (75, -63), (71, -66), (69, -64), (72, -58), (75, -50), (75, -44),
    ], 5.0, M["steel"]), 0.6), "trigger")
    guard = prism("trigger_guard", [
        (40, -47), (40, -70), (44, -74), (94, -74), (98, -70), (98, -47),
        (94, -47), (94, -68), (91, -70), (47, -70), (44, -67), (44, -47),
    ], 12.0, M["receiver"])
    add(bevel(guard, 1.0))
    add(bevel(tube_x("trigger_guard_pin", -7, 7, 1.4, 96, -58, M["steel"], segs=10), 0.2))
    add(bevel(prism("selector_lever", [(48, -21), (48, -25), (66, -24), (68, -22), (66, -20)], 2.5, M["steel"], x=-13.0), 0.5), "selector")
    add(bevel(tube_x("selector_hub", -13.5, -11, 4.5, 56, -22, M["steel"]), 0.4), "selector")

    # ---- Pistol grip ---------------------------------------------------
    # Horizontal slices (z, y_back, y_front, half_width, squareness): raked
    # about 25 degrees, swelled back strap, finger swell on the front strap,
    # slight flare at the bottom.
    rake = 0.0   # rake is built into the slices below (about 24 degrees)
    grip_slices = [
        (-46, 12, 62, 12.5, 2.6), (-56, 8, 56, 13.2, 2.5), (-66, 3.5, 52, 14.0, 2.4),
        (-74, 0, 51, 14.4, 2.4), (-82, -3.5, 46, 14.6, 2.4), (-94, -9, 40, 14.8, 2.4),
        (-108, -15.5, 34, 15.0, 2.4), (-122, -22, 27.5, 15.2, 2.4), (-134, -27.5, 22, 15.4, 2.5),
        (-142, -31, 18.5, 15.4, 2.6), (-148, -32, 17, 14.8, 2.8), (-151, -30, 15, 13.2, 3.0),
    ]
    grip = loft_z("pistol_grip", [
        (z, yb + rake * (z + 46), yf + rake * (z + 46), hw, n) for z, yb, yf, hw, n in grip_slices
    ], M["polymer"], segs=40)
    smooth(grip, 50)
    add(grip)

    # ---- Stock ------------------------------------------------------------
    if spec.get("stock", "A2") == "A2":
        # Rounded A2 stock: narrow neck at the receiver, widening toward the butt.
        stock = loft_y("stock", [
            (2, 14.5, -42, 16, 4.0), (-40, 15.5, -46, 17, 4.0), (-75, 16.5, -53, 17.5, 4.2),
            (-150, 18.0, -76, 18.5, 4.4), (-222, 19.5, -99, 19.5, 4.6), (-258, 20.0, -105, 20.0, 5.0),
            (-264, 20.0, -106, 20.0, 5.0),
        ], M["polymer"], segs=48)
        smooth(stock, 45)
        add(stock)
        plate = loft_y("butt_plate", [
            (-263, 20.5, -107, 20.5, 5.0), (-272, 21.0, -109, 21.5, 5.0), (-276, 19.5, -107, 20.0, 5.0),
        ], M["polymer"], segs=48)
        smooth(plate, 45)
        add(plate)
        # Trapdoor (storage compartment) on the butt plate
        add(bevel(box("butt_trapdoor", -277.2, -276, -78, -10, -11, 11, M["polymer"]), 0.6))
        add(bevel(tube_y("butt_trapdoor_hinge", -278, -276, 2.0, M["steel"], z=-14, segs=12), 0.2))
        add(bevel(torus_x("sling_swivel_rear", -236, -112, 9, 1.6, M["steel"]), 0.2))
        add(bevel(box("sling_swivel_rear_base", -244, -228, -106, -100, -5, 5, M["steel"]), 0.8))

    # ---- Barrel and front end ----------------------------------------------
    add(bevel(tube_y("barrel", barrel_start, muzzle, 8.5, M["steel"], segs=32), 0.4))
    # Heavier profile forward of the front sight (A2 "government" profile)
    fsb_y = spec["front_sight_y"]
    add(bevel(tube_y("barrel_front_profile", fsb_y + 30, muzzle, 9.6, M["steel"], segs=32), 0.4))

    # Delta ring (with its spring grooves) + slip ring
    add(bevel(lathe_y("delta_ring", [
        (184, 22), (186, 31), (189, 31), (189.6, 30.2), (190.4, 30.2), (191, 31), (193, 31),
        (193.6, 30.2), (194.4, 30.2), (195, 31), (198, 31), (200, 27), (204, 26),
    ], M["steel"]), 0.4))

    hg_y0, hg_y1 = 204.0, spec["handguard_end_y"]
    # A2 handguard: rings of ribs around the circumference, with small
    # rectangular vents in the grooves along the lower sides.
    R, depth, pitch, ridge = 28.0, 1.4, 9.0, 5.0
    st = [(hg_y0, 26.0), (hg_y0 + 3, 27.6), (hg_y0 + 8, R)]
    y, grooves = hg_y0 + 8, []
    while y + pitch < hg_y1 - 14:
        st += [(y + ridge, R), (y + ridge + 0.6, R - depth), (y + pitch - 0.6, R - depth), (y + pitch, R)]
        grooves.append(y + ridge + (pitch - ridge) / 2)
        y += pitch
    st += [(hg_y1 - 10, R - 0.4), (hg_y1 - 4, 26.0), (hg_y1, 23.5)]
    hg = lathe_y("handguard", st, M["polymer"], segs=56, z=-4)
    for side, x0, x1 in (("r", 25.0, 32), ("l", -32, -25.0)):
        cut(hg, box(f"cutter_hg_seam_{side}", hg_y0 + 9, hg_y1 - 12, -4.5, -3.5, x0, x1, None))
    for row, ang in enumerate((-32, -148)):
        for k, gy in enumerate(grooves[1:-1]):
            c = box(f"cutter_hg_vent_{row}_{k}", gy - 1.0, gy + 1.0, -2.0, 2.0, 22.0, 34.0, None)
            c.rotation_euler = (0, math.radians(-ang), 0)
            c.location = (0, 0, mm(-4))
            cut(hg, c)
    smooth(hg, 35)
    add(hg)
    add(bevel(lathe_y("handguard_cap", [(hg_y1, 21), (hg_y1 + 1, 23.5), (hg_y1 + 8, 23.5), (hg_y1 + 9, 14)],
                      M["steel"], z=-4), 0.5))

    # Front sight base / gas block: rounded clamp, tower, ears, square post,
    # taper pins, bayonet lug and swivel
    fs0, fs1 = fsb_y - 18, fsb_y + 18
    clamp = loft_y("fsb_clamp", [
        (fs0, 12.5, -19, 11, 3.2), (fs0 + 1.5, 14, -20.5, 12, 3.2), (fs1 - 1.5, 14, -20.5, 12, 3.2), (fs1, 12.5, -19, 11, 3.2),
    ], M["steel"], segs=40)
    smooth(clamp, 40)
    add(clamp)
    tower = prism("fsb_tower", [(fs0 + 6, 10), (fs0 + 14, 50), (fs1 - 10, 50), (fs1 - 4, 10)], 13.0, M["steel"])
    add(bevel(tower, 1.2))
    for side, x in (("l", -6.5), ("r", 6.5)):
        add(bevel(prism(f"fsb_ear_{side}", [(fs0 + 13, 46), (fs0 + 15, 66), (fs1 - 11, 66), (fs1 - 9, 46)], 3.0,
                        M["steel"], x=x), 0.6))
    add(bevel(box("front_sight_post", fsb_y - 1.0, fsb_y + 1.0, 46, 64, -1.0, 1.0, M["steel"]), 0.2))
    add(bevel(tube_x("front_sight_detent", -2.6, 2.6, 1.6, fsb_y + 5, 48, M["steel"], segs=12), 0.2))
    for k, y in enumerate((fs0 + 8, fs1 - 8)):
        add(bevel(tube_x(f"fsb_taper_pin_{k}", -14.6, 14.6, 1.7, y, -13, M["steel"], segs=12), 0.2))
    add(bevel(box("bayonet_lug", fs0 + 4, fs0 + 24, -28, -19, -4, 4, M["steel"]), 0.8))
    add(bevel(torus_x("sling_swivel_front", fsb_y + 6, -34, 10, 1.6, M["steel"]), 0.2))
    add(bevel(tube_x("sling_swivel_front_rivet", -4, 4, 2.0, fsb_y + 6, -24, M["steel"], segs=12), 0.2))

    # A2 "birdcage" flash hider, slotted on top and sides (closed bottom)
    hl = hider_len
    fh = lathe_y("flash_hider", [(muzzle - 2, 10.4), (muzzle, 11.0), (muzzle + 8, 11.0), (muzzle + 9, 10.4),
                                 (muzzle + 10, 11.0), (muzzle + hl - 2.5, 11.0), (muzzle + hl, 9.6)],
                 M["steel"], segs=40)
    for k, ang in enumerate((90 - 72, 90, 90 + 72, 90 - 144, 90 + 144)):
        c = box(f"cutter_fh_slot_{k}", muzzle + 18, muzzle + hl - 6, -1.4, 1.4, 6.0, 14.0, None)
        c.rotation_euler = (0, math.radians(-ang), 0)
        cut(fh, c)
    cut(fh, tube_y("cutter_bore", muzzle + 10, muzzle + hl + 5, 4.2, None))
    bevel(fh, 0.4, segments=2, angle=40)
    add(fh)

    # ---- Magazine (moves) --------------------------------------------------
    rounds = spec.get("mag_rounds", 30)
    depth = 188.0 if rounds == 30 else 140.0
    curve = 40.0 if rounds == 30 else 18.0
    back = curve_points((101, -36), (100, -36 - depth * 0.45), (101 + curve, -36 - depth), 12)
    front = curve_points((161, -36), (161, -36 - depth * 0.40), (161 + curve + 3, -31 - depth), 12)
    mag = prism("magazine_body", back + list(reversed(front)), 22.0, M["mag"])
    bevel(mag, 2.0)
    add(mag, "magazine")
    add(bevel(prism("magazine_floorplate", [
        (back[-1][0] - 3, back[-1][1] + 2), (front[-1][0] + 3, front[-1][1] + 2),
        (front[-1][0] + 3, front[-1][1] - 6), (back[-1][0] - 3, back[-1][1] - 7),
    ], 25.0, M["mag"]), 1.2), "magazine")
    # Pressed stiffening ribs following the curve, both sides
    def lerp2(a, b, t):
        return (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t)
    for side, x in (("r", 11.3), ("l", -11.3)):
        for k, (f0, f1) in enumerate(((0.24, 0.31), (0.66, 0.73))):
            rib = [lerp2(back[i], front[i], f0) for i in range(3, 11)]
            rib += [lerp2(back[i], front[i], f1) for i in reversed(range(3, 11))]
            add(bevel(prism(f"magazine_rib_{side}{k}", rib, 1.2, M["mag"], x=x), 0.4), "magazine")
    # Top cartridge, visible when the magazine is out
    add(bevel(tube_y("magazine_round_case", 112, 150, 4.7, M["brass"], z=-33, segs=16), 0.3), "magazine")
    add(tube_y("magazine_round_bullet", 150, 160, 2.9, M["copper"], r1=0.8, z=-33, segs=16), "magazine")

    sockets = {
        "socket_muzzle": (0, muzzle + hider_len, 0),
        "socket_ads_eye": (0, -60, 64),          # eye behind the rear aperture, on the sight line
        "socket_rear_aperture": (0, 25, 64),
        "socket_front_post": (0, fsb_y, 64),
        "socket_grip_right_hand": (0, 40, -95),
        "socket_grip_left_hand": (0, (hg_y0 + hg_y1) / 2, -30),
        "socket_mag_well": (0, 131, -40),
        "socket_shell_eject": (16, 92, 2),
        "socket_underbarrel": (0, (hg_y0 + hg_y1) / 2, -32),   # M203 mount
        "socket_bayonet": (0, fsb_y - 4, -24),
        "socket_top_mount": (0, 90, 60),                        # carry-handle scope mount
        "socket_sling_front": (0, fsb_y + 6, -34),
        "socket_sling_rear": (0, -236, -112),
        "socket_shoulder": (0, -276, -40),
    }
    return parts, sockets


# --------------------------------------------------------------------------
# Rig, sockets, animations
# --------------------------------------------------------------------------

BONES = {
    # name: (head_mm (x, y, z), parent)
    "root": ((0, 0, 0), None),
    "magazine": ((0, 131, -40), "root"),
    "bolt_carrier": ((0, 82, 1), "root"),
    "charging_handle": ((0, -8, 12), "root"),
    "trigger": ((0, 78, -45), "root"),
    "selector": ((-13, 56, -22), "root"),
    "dust_cover": ((12, 93, -7.5), "root"),
}


def rig(name, parts, sockets, extras):
    arm_data = bpy.data.armatures.new(name + "_rig")
    arm = _link(bpy.data.objects.new(name, arm_data))
    for k, v in extras.items():
        arm[k] = v
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode='EDIT')
    for bname, (head, parent) in BONES.items():
        b = arm_data.edit_bones.new(bname)
        h = Vector((mm(head[0]), mm(head[1]), mm(head[2])))
        b.head = h
        b.tail = h + Vector((0, 0.03, 0))   # every bone points +Y, roll 0
        b.roll = 0.0
    for bname, (_, parent) in BONES.items():
        if parent:
            arm_data.edit_bones[bname].parent = arm_data.edit_bones[parent]
    bpy.ops.object.mode_set(mode='OBJECT')

    for obj, bone in parts.values():
        vg = obj.vertex_groups.new(name=bone)
        vg.add(list(range(len(obj.data.vertices))), 1.0, 'REPLACE')
        mod = obj.modifiers.new("Armature", 'ARMATURE')
        mod.object = arm
        obj.parent = arm

    for sname, (x, y, z) in sockets.items():
        e = _link(bpy.data.objects.new(sname, None))
        e.empty_display_type = 'ARROWS'
        e.empty_display_size = 0.02
        e.location = (mm(x), mm(y), mm(z))
        e.parent = arm
    return arm


def _key(arm, bone, frame, loc=None, rot=None):
    pb = arm.pose.bones[bone]
    pb.rotation_mode = 'QUATERNION'
    if loc is not None:
        pb.location = Vector(loc)
        pb.keyframe_insert("location", frame=frame)
    if rot is not None:
        axis, deg = rot
        pb.rotation_quaternion = Matrix.Rotation(math.radians(deg), 4, axis).to_quaternion()
        pb.keyframe_insert("rotation_quaternion", frame=frame)


def make_actions(arm, fps=30):
    """Simple mechanical animations. Bones point +Y with roll 0, so bone-local
    axes equal the rifle's axes (X right, Y forward, Z up)."""
    scene = bpy.context.scene
    scene.render.fps = fps
    arm.animation_data_create()
    made = []

    def action(name):
        act = bpy.data.actions.new(name)
        act.use_fake_user = True
        arm.animation_data.action = act
        for pb in arm.pose.bones:
            pb.location = (0, 0, 0)
            pb.rotation_quaternion = (1, 0, 0, 0)
        made.append(act)
        return act

    # One shot: trigger pull, bolt carrier cycles back 75 mm and returns.
    # Authored over 6 frames so it survives import; the real cycle is ~75 ms at
    # 800 rpm, so play it with speed_scale ~2.7 for true speed.
    action("fire")
    _key(arm, "trigger", 0, rot=('X', 0)); _key(arm, "trigger", 1, rot=('X', -14)); _key(arm, "trigger", 5, rot=('X', 0))
    _key(arm, "bolt_carrier", 0, loc=(0, 0, 0)); _key(arm, "bolt_carrier", 2, loc=(0, -0.075, 0))
    _key(arm, "bolt_carrier", 3, loc=(0, -0.075, 0)); _key(arm, "bolt_carrier", 6, loc=(0, 0, 0))
    _key(arm, "dust_cover", 0, rot=('Y', 100)); _key(arm, "dust_cover", 6, rot=('Y', 100))

    # Magazine out and back in.
    action("reload_magazine")
    _key(arm, "magazine", 0, loc=(0, 0, 0))
    _key(arm, "magazine", 8, loc=(0, 0.01, -0.06))
    _key(arm, "magazine", 18, loc=(0, 0.04, -0.30))
    _key(arm, "magazine", 40, loc=(0, 0.04, -0.30))
    _key(arm, "magazine", 52, loc=(0, 0.01, -0.06))
    _key(arm, "magazine", 60, loc=(0, 0, 0))

    # Pull and release the charging handle (carries the bolt with it).
    action("charge")
    for b, back in (("charging_handle", -0.068), ("bolt_carrier", -0.075)):
        _key(arm, b, 0, loc=(0, 0, 0)); _key(arm, b, 8, loc=(0, back, 0)); _key(arm, b, 12, loc=(0, back, 0)); _key(arm, b, 14, loc=(0, 0, 0))
    _key(arm, "dust_cover", 0, rot=('Y', 0)); _key(arm, "dust_cover", 4, rot=('Y', 100))

    # Selector: frame 0 safe, 10 semi, 20 burst/auto.
    action("selector")
    _key(arm, "selector", 0, rot=('X', 0)); _key(arm, "selector", 10, rot=('X', -90)); _key(arm, "selector", 20, rot=('X', -180))

    # Dust cover pops open.
    action("dust_cover_open")
    _key(arm, "dust_cover", 0, rot=('Y', 0)); _key(arm, "dust_cover", 3, rot=('Y', 100))

    arm.animation_data.action = None
    for pb in arm.pose.bones:
        pb.location = (0, 0, 0)
        pb.rotation_quaternion = (1, 0, 0, 0)
    return made


# --------------------------------------------------------------------------
# Output
# --------------------------------------------------------------------------

def export_glb(path):
    """Export the open scene as a game-ready .glb.
    Modifiers you add by hand (bevels, subdivision...) are applied on export;
    the Armature modifier is kept as skinning, so animations still work."""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    kwargs = dict(
        filepath=path, export_format='GLB', export_yup=True, export_apply=True,
        export_animations=True, export_animation_mode='ACTIONS', export_extras=True,
        export_skins=True, export_materials='EXPORT',
    )
    try:
        bpy.ops.export_scene.gltf(**kwargs)
    except TypeError as e:
        print("[export] some glTF options were not recognised, using defaults:", e)
        bpy.ops.export_scene.gltf(filepath=path, export_format='GLB')


# --------------------------------------------------------------------------
# Generated vs hand-edited files
# --------------------------------------------------------------------------
#
#   art-source/<category>/<id>/<id>_generated.blend   rebuilt by the script every run
#   art-source/<category>/<id>/<id>.blend             YOURS: created once, never overwritten
#   client/assets/models/<category>/<id>.glb          exported from YOUR .blend
#
# <category> is "weapons", "characters", ...
#
# The game model always comes from the hand-edited file. To pull script
# changes into it, copy parts over from the generated file in Blender (or
# delete your .blend to start again from the latest generated one).

def weapon_paths(root, weapon_id, category="weapons"):
    folder = os.path.join(root, "art-source", category, weapon_id)
    return {
        "folder": folder,
        "generated": os.path.join(folder, f"{weapon_id}_generated.blend"),
        "hand": os.path.join(folder, f"{weapon_id}.blend"),
        "glb": os.path.join(root, "client", "assets", "models", category, f"{weapon_id}.glb"),
    }


def save_outputs(root, weapon_id, category="weapons"):
    """Save the freshly built scene as the generated file. Only if the
    hand-edited file doesn't exist yet: create it and export the game .glb."""
    p = weapon_paths(root, weapon_id, category)
    os.makedirs(p["folder"], exist_ok=True)
    bpy.context.preferences.filepaths.save_version = 0     # no .blend1 backups
    bpy.ops.wm.save_as_mainfile(filepath=p["generated"])
    print(f"[{weapon_id}] saved generated file {p['generated']}")
    if os.path.exists(p["hand"]):
        print(f"[{weapon_id}] your hand-edited {p['hand']} was left untouched; the game .glb was not changed")
        return False
    shutil.copyfile(p["generated"], p["hand"])
    export_glb(p["glb"])
    print(f"[{weapon_id}] created {p['hand']} for hand editing and exported {p['glb']}")
    return True


def render_previews(out_prefix, look_at=(0.0, 0.25, -0.02)):
    """Grey-shaded Workbench previews for checking the shape."""
    scene = bpy.context.scene
    scene.render.engine = 'BLENDER_WORKBENCH'
    sh = scene.display.shading
    sh.light = 'STUDIO'
    sh.color_type = 'SINGLE'
    sh.single_color = (0.62, 0.62, 0.64)
    sh.show_cavity = True
    sh.cavity_type = 'BOTH'
    sh.show_shadows = False
    sh.show_specular_highlight = True
    world = scene.world or bpy.data.worlds.new("preview")
    scene.world = world
    world.color = (0.32, 0.33, 0.35)
    try:
        sh.background_type = 'WORLD'
    except (AttributeError, TypeError):
        pass
    scene.render.film_transparent = False
    scene.render.resolution_x, scene.render.resolution_y = 1800, 900

    target = _link(bpy.data.objects.new("preview_target", None))
    target.location = look_at
    cam_data = bpy.data.cameras.new("preview_cam")
    cam = _link(bpy.data.objects.new("preview_cam", cam_data))
    scene.camera = cam
    con = cam.constraints.new('TRACK_TO')
    con.target = target
    con.track_axis = 'TRACK_NEGATIVE_Z'
    con.up_axis = 'UP_Y'

    views = {
        "right": dict(loc=(1.6, look_at[1], look_at[2]), ortho=1.15),
        "left_34": dict(loc=(-1.0, -0.45, 0.35), lens=50),
        "receiver_closeup": dict(loc=(0.42, -0.08, 0.10), lens=50, target=(0.0, 0.07, -0.02)),
        "front_34": dict(loc=(0.55, 1.35, 0.22), lens=50, target=(0.0, 0.55, 0.0)),
        "right_material": dict(loc=(1.6, look_at[1], look_at[2]), ortho=1.15, material=True),
    }
    paths = []
    for vname, v in views.items():
        target.location = v.get("target", look_at)
        cam.location = v["loc"]
        if "ortho" in v:
            cam_data.type = 'ORTHO'
            cam_data.ortho_scale = v["ortho"]
        else:
            cam_data.type = 'PERSP'
            cam_data.lens = v["lens"]
        sh.color_type = 'MATERIAL' if v.get("material") else 'SINGLE'
        p = f"{out_prefix}_{vname}.png"
        scene.render.filepath = p
        bpy.ops.render.render(write_still=True)
        paths.append(p)
    bpy.data.objects.remove(cam, do_unlink=True)
    bpy.data.objects.remove(target, do_unlink=True)
    return paths
