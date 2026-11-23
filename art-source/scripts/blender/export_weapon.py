"""Export the open model .blend to the game as client/assets/models/<category>/<id>.glb.

The weapon id comes from the file name (m16a2.blend -> m16a2.glb). Use it on
your hand-edited file:
  blender --background art-source/weapons/m16a2/m16a2.blend --python art-source/scripts/blender/export_weapon.py
or simply:  .\\art-source\\scripts\\export-weapon.ps1 m16a2

Inside Blender's GUI you can also open this file in the Scripting tab and
press Run Script; it exports the file you have open.
"""

import os
import sys

import bpy

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import ar15_kit as kit  # noqa: E402

ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))


def main():
    blend = bpy.data.filepath
    if not blend:
        raise SystemExit("[export] Save the .blend first; the weapon id comes from its file name.")
    weapon_id = os.path.splitext(os.path.basename(blend))[0]
    if weapon_id.endswith("_generated"):
        print("[export] WARNING: this is the script-generated file. Edit and export "
              f"{weapon_id[:-len('_generated')]}.blend instead, or your export will be "
              "replaced the next time you export the hand-edited file.")
        weapon_id = weapon_id[:-len("_generated")]

    # Make sure the rifle is exported in its rest pose, not mid-animation.
    for obj in bpy.data.objects:
        if obj.type == 'ARMATURE' and obj.animation_data:
            obj.animation_data.action = None
            for pb in obj.pose.bones:
                pb.location = (0, 0, 0)
                pb.rotation_quaternion = (1, 0, 0, 0)
                pb.scale = (1, 1, 1)

    # art-source/<category>/<id>/<id>.blend -> client/assets/models/<category>/<id>.glb
    category = os.path.basename(os.path.dirname(os.path.dirname(os.path.abspath(blend))))
    out = kit.weapon_paths(ROOT, weapon_id, category)["glb"]
    kit.export_glb(out)
    print(f"[export] {blend} -> {out}")


main()
