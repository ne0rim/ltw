"""Create one 1.8 m base soldier from Blender's CC0 realistic male base mesh.

Run from the project root with Blender 5.3:
  blender --background --factory-startup --python art-source/scripts/blender/build_base_human_soldier.py
"""
import bpy
import bmesh
import math
import os
from mathutils import Matrix, Vector

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
SOURCE = os.path.join(ROOT, "art-source", "characters", "base_human_source", "extracted", "human-base-meshes-bundle-v1.4.1", "human_base_meshes_bundle.blend")
OUT_DIR = os.path.join(ROOT, "art-source", "characters", "base_soldier")
os.makedirs(OUT_DIR, exist_ok=True)

bpy.ops.object.select_all(action='SELECT')
bpy.ops.object.delete(use_global=False)

def material(name, color, roughness=.78, metallic=0):
    m=bpy.data.materials.new(name); m.diffuse_color=(*color,1); m.use_nodes=True
    bs=m.node_tree.nodes.get("Principled BSDF")
    bs.inputs["Base Color"].default_value=(*color,1)
    bs.inputs["Roughness"].default_value=roughness
    bs.inputs["Metallic"].default_value=metallic
    return m

skin=material("Skin | natural warm tone",(.42,.24,.16),.72)
iris_mat=material("Iris | dark brown",(.10,.045,.025),.42)
pupil_mat=material("Pupil",(.012,.009,.007),.3)
shirt=material("BDU | muted olive",(.18,.22,.13),.9)
trousers=material("BDU trousers | darker olive",(.12,.16,.09),.92)
boots=material("Combat boots | dark brown",(.055,.045,.032),.62)
solemat=material("Boot sole rubber",(.025,.027,.024),.8)
webbing=material("Canvas webbing",(.12,.15,.09),.88)
metal=material("Matte hardware",(.13,.14,.12),.55,.35)
floor_mat=material("Studio floor",(.13,.14,.13),.9)

with bpy.data.libraries.load(SOURCE, link=False) as (source_data, target_data):
    wanted={"GEO-body_male_realistic","GEO-body_male_realistic.eye.L","GEO-body_male_realistic.eye.R"}
    target_data.objects=[n for n in source_data.objects if n in wanted]

col=bpy.data.collections.new("Base soldier | 1.8 m"); bpy.context.scene.collection.children.link(col)
objects=[]
for obj in target_data.objects:
    if obj is None: continue
    col.objects.link(obj); objects.append(obj)
body=next(o for o in objects if o.name.startswith("GEO-body_male_realistic") and ".eye." not in o.name)
eyes=[o for o in objects if ".eye." in o.name]
body.name="Human male base | source mesh"
body.data.name="Human male base mesh | Blender CC0"
body.data.materials.clear(); body.data.materials.append(skin)
for o in eyes:
    o.name="Eye | "+("L" if o.name.endswith(".L") else "R")
    o.data.materials.clear(); o.data.materials.append(material("Eye white",(.72,.69,.61),.38))

# Measure the source mesh in world space, then scale its bare human height to 1.8 m.
deps=bpy.context.evaluated_depsgraph_get()
evaluated=body.evaluated_get(deps)
corners=[body.matrix_world @ Vector(c) for c in body.bound_box]
x0=min(v.x for v in corners); x1=max(v.x for v in corners)
y0=min(v.y for v in corners); y1=max(v.y for v in corners)
z0=min(v.z for v in corners); z1=max(v.z for v in corners)
height=z1-z0
scale=1.8/height
transform=Matrix.Translation(Vector((-(x0+x1)*.5*scale,-(y0+y1)*.5*scale,-z0*scale))) @ Matrix.Scale(scale,4)
for o in objects:
    o.matrix_world=transform @ o.matrix_world

def add_shell(name, predicate, mat, thickness=.008):
    mesh=body.data.copy(); mesh.name=name+" surface"
    bm=bmesh.new(); bm.from_mesh(mesh)
    keep=[]
    for f in bm.faces:
        p=body.matrix_world @ f.calc_center_median()
        if predicate(p): keep.append(f)
    delete=[f for f in bm.faces if f not in keep]
    bmesh.ops.delete(bm,geom=delete,context='FACES')
    bm.to_mesh(mesh); bm.free(); mesh.update()
    mesh.materials.clear(); mesh.materials.append(mat)
    for p in mesh.polygons: p.material_index=0; p.use_smooth=True
    shell=bpy.data.objects.new(name,mesh); col.objects.link(shell); shell.matrix_world=body.matrix_world.copy()
    sub=shell.modifiers.new("Smooth garment boundary",'SUBSURF'); sub.levels=1; sub.render_levels=1
    solid=shell.modifiers.new("Cloth thickness",'SOLIDIFY'); solid.thickness=thickness; solid.offset=1.0; solid.use_rim=True
    shell["source"]="Surface fitted over the Blender CC0 male base mesh"
    return shell

def coords(p):
    return p.x,p.y,p.z

add_shell("BDU blouse shell",lambda p: (.92<=p.z<=1.53 and abs(p.x)<=.29) or (.67<=p.z<=1.50 and .20<=abs(p.x)<=.52),shirt,.009)
add_shell("BDU trouser shell",lambda p: .07<=p.z<=.96 and abs(p.x)<=.24,trousers,.01)
add_shell("Boot upper shell",lambda p: p.z<=.28 and .02<=abs(p.x)<=.31,boots,.008)

def uv_sphere(name, loc, scale, mat):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=20,ring_count=12,location=loc)
    o=bpy.context.object; o.name=name; o.scale=scale; bpy.ops.object.transform_apply(location=False,rotation=False,scale=True)
    o.data.materials.append(mat)
    for f in o.data.polygons: f.use_smooth=True
    col.objects.link(o)
    for c in list(o.users_collection):
        if c != col: c.objects.unlink(o)
    return o

# Small irises give the realistic base face a natural focal point.
for side in (-1,1):
    uv_sphere("Iris",(side*.035,-.105,1.683),(.006,.003,.007),iris_mat)
    uv_sphere("Pupil",(side*.035,-.108,1.683),(.003,.002,.004),pupil_mat)

def cube(name, loc, dims, mat, bevel=.012):
    bpy.ops.mesh.primitive_cube_add(size=1,location=loc)
    o=bpy.context.object; o.name=name; o.dimensions=dims
    bpy.ops.object.transform_apply(location=False,rotation=False,scale=True)
    if bevel:
        m=o.modifiers.new("Rounded sewn edges",'BEVEL'); m.width=bevel; m.segments=2
        o.modifiers.new("Weighted corner normals",'WEIGHTED_NORMAL')
    o.data.materials.append(mat); col.objects.link(o)
    for c in list(o.users_collection):
        if c != col: c.objects.unlink(o)
    return o

def torus(name, loc, major, minor, mat):
    bpy.ops.mesh.primitive_torus_add(major_radius=major,minor_radius=minor,major_segments=32,minor_segments=8,location=loc)
    o=bpy.context.object; o.name=name; o.scale=(1,.72,1); bpy.ops.object.transform_apply(location=False,rotation=False,scale=True); o.data.materials.append(mat)
    for p in o.data.polygons: p.use_smooth=True
    col.objects.link(o)
    for c in list(o.users_collection):
        if c != col: c.objects.unlink(o)
    return o

# Simple late-Cold-War field uniform details sit on top of the fitted garment.
torus("Web belt",(0,0,.99),.175,.014,webbing)
torus("BDU collar",(0,-.02,1.48),.075,.01,shirt)
cube("Belt buckle",(0,-.177,.99),(.075,.025,.055),metal,.009)
for side in (-1,1):
    cube("BDU chest pocket",(side*.105,-.155,1.31),(.12,.035,.13),shirt,.012)
    cube("Chest pocket flap",(side*.105,-.177,1.375),(.122,.02,.035),webbing,.008)
    cube("Utility pouch",(side*.19,-.16,1.02),(.09,.06,.12),webbing,.012)

# Keep this file as the reusable bare base human only. Remove the temporary
# BDU shells and accessories so the green clothing material cannot remain on
# the MetaHuman/base mesh.
base_mesh_names={"Human male base | source mesh","Eye | L","Eye | R","Iris","Pupil","Ground"}
for obj in list(bpy.data.objects):
    if obj.type=='MESH' and obj.name not in base_mesh_names:
        bpy.data.objects.remove(obj,do_unlink=True)
# Upright inspection render, kept plain so body anatomy remains easy to judge.
ground=cube("Ground",(0,0,-.035),(200,200,.07),floor_mat,0)
bpy.ops.object.light_add(type='AREA',location=(-3,-4,5)); key=bpy.context.object; key.data.energy=700; key.data.size=4; key.rotation_euler=(Vector((0,0,.9))-key.location).to_track_quat('-Z','Y').to_euler()
bpy.ops.object.light_add(type='AREA',location=(3,-2,2.8)); fill=bpy.context.object; fill.data.energy=420; fill.data.size=3; fill.data.color=(.72,.82,1); fill.rotation_euler=(Vector((0,0,.9))-fill.location).to_track_quat('-Z','Y').to_euler()
bpy.ops.object.camera_add(location=(1.15,-6.5,2.15)); camera=bpy.context.object; camera.rotation_euler=(Vector((0,0,.91))-camera.location).to_track_quat('-Z','Y').to_euler(); camera.data.type='ORTHO'; camera.data.ortho_scale=2.35
scene=bpy.context.scene; scene.camera=camera; scene.render.engine='BLENDER_EEVEE'; scene.eevee.taa_render_samples=64
scene.unit_settings.system='METRIC'; scene.unit_settings.length_unit='METERS'
scene.render.resolution_x=1000; scene.render.resolution_y=1400; scene.render.resolution_percentage=100
scene.render.image_settings.file_format='PNG'; scene.render.image_settings.color_mode='RGBA'
scene.world.color=(.08,.09,.08); scene.view_settings.view_transform='AgX'; scene.view_settings.look='AgX - Medium High Contrast'
scene.render.filepath=os.path.join(OUT_DIR,"base_soldier_preview.png")
scene.frame_set(1)
scene["character_height_m"]="1.8 (ground to crown)"
scene["base_mesh_source"]="Blender Studio and community Human Base Meshes v1.4.1; CC0"
body["height_m"]="1.8"
for mat in list(bpy.data.materials):
    if mat.users==0:
        bpy.data.materials.remove(mat)
bpy.ops.wm.save_as_mainfile(filepath=os.path.join(OUT_DIR,"base_soldier.blend"))
bpy.ops.render.render(write_still=True)
print("Created 1.8 m base human soldier from Blender CC0 male mesh.")
