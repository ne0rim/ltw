"""The animated soldier: base body + skeleton + rifle socket + animations.

Exports client/assets/models/characters/soldier_rig.glb with:
  * the rigged body (clothing pieces are separate .glbs that attach to this
    skeleton in Godot),
  * "socket_rifle", an empty on the RightHand bone: put the rifle model here
    and it sits in the hand in every animation,
  * animations (in place, 30 fps):
      idle  - standing, rifle at low ready, breathing
      walk  - rifle at low ready, about WALK_SPEED m/s
      run   - jog with the rifle at low ready, about RUN_SPEED m/s
      aim   - standing, rifle shouldered (firing, reloading)

How they are made: both hands are placed on the rifle's grips with IK, the
feet follow procedural stepping paths with IK, and the result is baked to
plain bone keys, so the game needs no IK.

Needs build_base_body.py and the M16A2 (build_m16a2.py) first. Run from the
project root:
  & "C:\\Program Files\\Blender Foundation\\Blender 5.3\\blender.exe" --background --factory-startup --python art-source\\scripts\\blender\\build_soldier_rig.py -- --previews art-source\\characters\\soldier_rig\\previews
"""

import math
import os
import sys

import bpy
import numpy as np
from mathutils import Matrix, Vector

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import character_kit as ck  # noqa: E402

MODEL_ID = "soldier_rig"
FPS = 30
RIFLE_FILE = os.path.join(ck.ROOT, "art-source", "weapons", "m16a2", "m16a2.blend")
RIFLE_RIG = "M16A2"

# Rifle sockets (rifle space: +Y muzzle, +Z up, metres), from the M16A2 model.
GRIP_R = Vector((0.0, 0.040, -0.095))
GRIP_L = Vector((0.0, 0.348, -0.030))
SHOULDER = Vector((0.0, -0.276, -0.040))

# Where each wrist sits relative to its grip socket, which way the fingers
# point and which way the back of the hand faces (rifle space). Converted
# from the first-person view's tuned values (weapon_view_data.gd).
HANDS = {
    "Right": dict(socket=GRIP_R, offset=Vector((0.032, -0.075, 0.03)),
                  fingers=Vector((-0.15, 1.0, -0.45)), back=Vector((1.0, 0.0, 0.1))),
    "Left": dict(socket=GRIP_L, offset=Vector((-0.045, -0.08, -0.042)),
                 fingers=Vector((1.0, 0.45, 0.1)), back=Vector((-0.2, 0.0, -1.0))),
}

# How the rifle is carried, in character space (the soldier faces -Y, left
# is +X). butt = where socket_shoulder goes; pitch/yaw/roll in degrees
# (pitch < 0 points the muzzle down, yaw > 0 swings it to the soldier's left).
LOW_READY = dict(butt=Vector((-0.14, -0.12, 1.33)), pitch=-38.0, yaw=14.0, roll=-10.0,
                 left_slide=-0.10)
AIM = dict(butt=Vector((-0.12, -0.08, 1.47)), pitch=0.0, yaw=0.0, roll=0.0,
           left_slide=-0.04)

WALK_SPEED = 1.3
RUN_SPEED = 2.6


# --------------------------------------------------------------------------
# Scene
# --------------------------------------------------------------------------

def load_body_and_rig():
    src = ck.base_body_file()
    with bpy.data.libraries.load(src, link=False) as (data_from, data_to):
        data_to.objects = [n for n in data_from.objects
                           if n in (ck.RIG_NAME, ck.BODY_NAME) or n.startswith(("Eye", "Iris"))]
    rig = None
    for obj in data_to.objects:
        bpy.context.scene.collection.objects.link(obj)
        if obj.name == ck.RIG_NAME:
            rig = obj
    return rig


def load_rifle():
    """The M16A2 for previews only (the game attaches its own copy)."""
    with bpy.data.libraries.load(RIFLE_FILE, link=False) as (data_from, data_to):
        data_to.objects = list(data_from.objects)
    col = bpy.data.collections.new(ck.REFERENCE_COLLECTION)
    bpy.context.scene.collection.children.link(col)
    root = None
    for obj in data_to.objects:
        if obj is None:
            continue
        col.objects.link(obj)
        if obj.name == RIFLE_RIG:
            root = obj
    return root


def empty(name, size=0.05):
    e = bpy.data.objects.new(name, None)
    e.empty_display_size = size
    bpy.context.scene.collection.objects.link(e)
    return e


def parent_to_bone(obj, rig, bone):
    world = obj.matrix_world.copy()
    obj.parent = rig
    obj.parent_type = 'BONE'
    obj.parent_bone = bone
    bpy.context.view_layer.update()
    obj.matrix_world = world


def rifle_matrix(spec):
    """World matrix of the rifle origin for a carry spec."""
    rot = (Matrix.Rotation(math.radians(spec["yaw"]), 4, 'Z')
           @ Matrix.Rotation(math.radians(-spec["pitch"]), 4, 'X')
           @ Matrix.Rotation(math.radians(180.0), 4, 'Z')    # rifle +Y -> soldier forward (-Y)
           @ Matrix.Rotation(math.radians(spec["roll"]), 4, 'Y'))
    return Matrix.Translation(spec["butt"]) @ rot @ Matrix.Translation(-SHOULDER)


def frame_from(fingers, back):
    """Rotation whose +Y points along fingers and whose `back` side faces back."""
    y = fingers.normalized()
    b = (back - y * back.dot(y)).normalized()
    return y, b


# --------------------------------------------------------------------------
# Controls: IK targets, poles and hand aim points
# --------------------------------------------------------------------------

class Controls:
    def __init__(self, rig):
        self.rig = rig
        pose = rig.pose.bones
        self.rifle = empty("ctrl_rifle", 0.1)
        self.hand = {}
        self.foot = {}
        for side, s in (("Left", 1), ("Right", -1)):
            wrist = empty(f"ctrl_{side}_wrist")
            fingers = empty(f"ctrl_{side}_fingers", 0.02)
            back = empty(f"ctrl_{side}_back", 0.02)
            for e in (wrist, fingers, back):
                e.parent = self.rifle
            self.hand[side] = (wrist, fingers, back)

            elbow = empty(f"ctrl_{side}_elbow_pole", 0.03)
            elbow.location = (0.45 * s, 0.35, 0.95) if side == "Right" else (0.35 * s, 0.25, 0.85)
            parent_to_bone(elbow, rig, "UpperChest")

            foot = empty(f"ctrl_{side}_foot")
            foot.rotation_mode = 'QUATERNION'
            knee = empty(f"ctrl_{side}_knee_pole", 0.03)
            knee.location = (0.14 * s, -0.6, 0.55)
            parent_to_bone(knee, rig, "Hips")
            self.foot[side] = foot

            ik = pose[f"{side}LowerLeg"].constraints.new('IK')
            ik.target = foot
            ik.pole_target = knee
            ik.pole_angle = math.radians(-90)
            ik.chain_count = 2
            cr = pose[f"{side}Foot"].constraints.new('COPY_ROTATION')
            cr.target = foot

            ik = pose[f"{side}LowerArm"].constraints.new('IK')
            ik.target = wrist
            ik.pole_target = elbow
            ik.pole_angle = math.radians(-90)
            ik.chain_count = 2
            hand = pose[f"{side}Hand"]
            dt = hand.constraints.new('DAMPED_TRACK')
            dt.target = fingers
            dt.track_axis = 'TRACK_Y'
            lt = hand.constraints.new('LOCKED_TRACK')
            lt.target = back
            lt.lock_axis = 'LOCK_Y'
            lt.track_axis = self._back_axis(side)

        rest = {b.name: rig.matrix_world @ b.matrix_local for b in rig.data.bones}
        self.foot_rest = {side: rest[f"{side}Foot"] for side in ("Left", "Right")}
        self.toe_rest = {side: rest[f"{side}Toes"].translation for side in ("Left", "Right")}

    def _back_axis(self, side):
        """Which local axis of the hand bone is the back of the hand at rest.
        In the A-pose the palms face the thighs, so the back faces outward."""
        m = self.rig.data.bones[f"{side}Hand"].matrix_local.to_3x3()
        out = Vector((1.0 if side == "Left" else -1.0, 0, 0))
        options = {"TRACK_X": m.col[0], "TRACK_NEGATIVE_X": -m.col[0],
                   "TRACK_Z": m.col[2], "TRACK_NEGATIVE_Z": -m.col[2]}
        return max(options, key=lambda k: options[k].dot(out))

    def carry(self, spec):
        """Put the rifle in its carry position (follows the chest from here
        on) and the hand targets on its grips."""
        self.rifle.parent = None
        self.rifle.matrix_world = rifle_matrix(spec)
        parent_to_bone(self.rifle, self.rig, "UpperChest")
        for side, h in HANDS.items():
            wrist, fingers, back = self.hand[side]
            socket = h["socket"].copy()
            if side == "Left":
                socket.y += spec["left_slide"]
            y, b = frame_from(h["fingers"], h["back"])
            wrist.location = socket + h["offset"]
            fingers.location = wrist.location + y * 0.12
            back.location = wrist.location + b * 0.12

    def place_foot(self, side, ground_y, lift, pitch_deg, frame):
        """Ankle and foot rotation for a foot whose sole is at ground_y
        (forward/back) and lifted `lift` metres, pitched about the ball
        (pitch < 0, toe-off) or the heel (pitch > 0, heel strike)."""
        rest = self.foot_rest[side]
        ankle = rest.translation.copy()
        ball = self.toe_rest[side].copy()
        heel = Vector((ankle.x, ankle.y + 0.035, 0.0))
        pivot = ball if pitch_deg < 0 else heel
        r = Matrix.Rotation(math.radians(-pitch_deg), 4, 'X')
        offset = Vector((0, ground_y, lift))
        new_ankle = pivot + (r @ (ankle - pivot).to_4d()).to_3d() + offset
        rot = (r @ rest).to_quaternion()
        foot = self.foot[side]
        foot.location = new_ankle
        foot.rotation_quaternion = rot
        foot.keyframe_insert("location", frame=frame)
        foot.keyframe_insert("rotation_quaternion", frame=frame)


# --------------------------------------------------------------------------
# Bone keys
# --------------------------------------------------------------------------

def key_bone(rig, name, frame, rot=None, loc=None):
    pb = rig.pose.bones[name]
    pb.rotation_mode = 'XYZ'
    if rot is not None:
        pb.rotation_euler = [math.radians(a) for a in rot]
        pb.keyframe_insert("rotation_euler", frame=frame)
    if loc is not None:
        pb.location = loc
        pb.keyframe_insert("location", frame=frame)


def upper_body(rig, frame, twist, lean, breathe=0.0, head_pitch=0.0, head_yaw=0.0):
    """Torso turned `twist` degrees to the right (left shoulder forward, a
    rifleman's bladed stance), leaning forward `lean` degrees; the neck turns
    the head back to the front."""
    key_bone(rig, "Spine", frame, rot=(lean * 0.5, -twist * 0.35, 0))
    key_bone(rig, "Chest", frame, rot=(lean * 0.3 + breathe, -twist * 0.35, 0))
    key_bone(rig, "UpperChest", frame, rot=(lean * 0.2 - breathe * 0.5, -twist * 0.30, 0))
    key_bone(rig, "Neck", frame, rot=(-lean * 0.4 + head_pitch * 0.4, twist * 0.5, 0))
    key_bone(rig, "Head", frame, rot=(-lean * 0.4 + head_pitch * 0.6, twist * 0.5 + head_yaw, 0))


def reset_pose(rig):
    rig.animation_data_clear()
    for pb in rig.pose.bones:
        pb.rotation_mode = 'XYZ'
        pb.rotation_euler = (0, 0, 0)
        pb.location = (0, 0, 0)
    for obj in bpy.data.objects:
        if obj.name.startswith("ctrl_"):
            obj.animation_data_clear()
    bpy.context.view_layer.update()


# --------------------------------------------------------------------------
# The animations
# --------------------------------------------------------------------------

def stand_feet(ctl, frame):
    for side in ("Left", "Right"):
        ctl.place_foot(side, 0.0, 0.0, 0.0, frame)


def anim_idle(rig, ctl):
    n = 60
    for f in range(n + 1):
        t = f / n * 2 * math.pi
        breathe = 1.2 * math.sin(t)
        key_bone(rig, "Root", f, loc=(0.004 * math.sin(t * 0.5), 0, -0.015 + 0.003 * math.sin(t)))
        upper_body(rig, f, twist=18, lean=3, breathe=breathe)
        stand_feet(ctl, f)
    return n


def anim_aim(rig, ctl):
    n = 60
    for f in range(n + 1):
        t = f / n * 2 * math.pi
        breathe = 0.5 * math.sin(t)
        key_bone(rig, "Root", f, loc=(0, 0, -0.03))
        upper_body(rig, f, twist=30, lean=6, breathe=breathe, head_pitch=20, head_yaw=-10)
        stand_feet(ctl, f)
    return n


def anim_gait(rig, ctl, speed, frames, stance, lift, bob, sink, lean, heel, toe):
    """One full stride (two steps) in place. stance = fraction of the cycle a
    foot is on the ground; the ground slides back under it at `speed`."""
    n = frames
    cycle = n / FPS
    reach = speed * cycle * stance / 2       # foot travels +-reach while planted
    for f in range(n + 1):
        u = f / n
        for side, phase in (("Left", 0.0), ("Right", 0.5)):
            p = (u + phase) % 1.0
            if p < stance:
                k = p / stance                           # 0 heel strike -> 1 toe off
                y = -reach + 2 * reach * k               # forward (-Y) to back
                z = 0.0
                pitch = heel * max(0.0, 1 - k / 0.2) - toe * max(0.0, (k - 0.75) / 0.25)
            else:
                k = (p - stance) / (1 - stance)          # 0 toe off -> 1 heel strike
                s = 0.5 - 0.5 * math.cos(math.pi * k)
                y = reach - 2 * reach * s
                z = lift * math.sin(math.pi * k) ** 1.5
                pitch = -toe * max(0.0, 1 - k / 0.3) + heel * max(0.0, (k - 0.7) / 0.3)
            ctl.place_foot(side, y, z, pitch, f)
        # Body: lowest just after each foot lands, highest mid-step.
        w = 2 * math.pi * u
        z = -sink - bob * math.cos(2 * w - 2 * math.pi * stance * 0.25)
        key_bone(rig, "Root", f, loc=(0.012 * math.sin(w), 0, z))
        key_bone(rig, "Hips", f, rot=(0, 5 * math.sin(w), -2 * math.sin(w)))
        upper_body(rig, f, twist=18 - 3 * math.sin(w), lean=lean, breathe=0.0)
    return n


ANIMS = {
    "idle": (anim_idle, LOW_READY),
    "walk": (lambda r, c: anim_gait(r, c, WALK_SPEED, 32, stance=0.62, lift=0.07, bob=0.012,
                                    sink=0.03, lean=4, heel=14, toe=22), LOW_READY),
    "run": (lambda r, c: anim_gait(r, c, RUN_SPEED, 22, stance=0.42, lift=0.15, bob=0.025,
                                   sink=0.045, lean=9, heel=8, toe=30), LOW_READY),
    "aim": (anim_aim, AIM),
}


def bake(rig, ctl, name, make, carry):
    reset_pose(rig)
    n = make(rig, ctl)
    # Place the rifle against the posed chest (frame 0), so the stance's twist
    # and lean don't swing it out of the hands.
    bpy.context.scene.frame_set(0)
    bpy.context.view_layer.update()
    ctl.carry(carry)
    bpy.context.scene.frame_start, bpy.context.scene.frame_end = 0, n
    bpy.ops.object.select_all(action='DESELECT')
    bpy.context.view_layer.objects.active = rig
    rig.select_set(True)
    bpy.ops.object.mode_set(mode='POSE')
    bpy.ops.pose.select_all(action='SELECT')
    bpy.ops.nla.bake(frame_start=0, frame_end=n, only_selected=True, visual_keying=True,
                     clear_constraints=False, use_current_action=False, bake_types={'POSE'})
    bpy.ops.object.mode_set(mode='OBJECT')
    action = rig.animation_data.action
    action.name = name
    action.use_fake_user = True
    print(f"[{MODEL_ID}] baked {name}: {n} frames")
    return action


def reach_report(rig, ctl):
    """How far each wrist ended up from where the rifle wants it (frame 0)."""
    bpy.context.view_layer.update()
    for side in ("Left", "Right"):
        wrist = ctl.hand[side][0].matrix_world.translation
        hand = (rig.matrix_world @ rig.pose.bones[f"{side}Hand"].matrix).translation
        print(f"[{MODEL_ID}]   {side} wrist miss {(wrist - hand).length * 100:.1f} cm")


def add_rifle_socket(rig, ctl):
    """socket_rifle on the RightHand bone, at the rifle origin as held in the
    idle pose. The rifle is rigid in the hand, so this holds in every pose."""
    bpy.context.scene.frame_set(0)
    bpy.context.view_layer.update()
    hand = rig.matrix_world @ rig.pose.bones["RightHand"].matrix
    offset = hand.inverted() @ ctl.rifle.matrix_world
    rig.data.pose_position = 'REST'
    bpy.context.view_layer.update()
    hand_rest = rig.matrix_world @ rig.data.bones["RightHand"].matrix_local
    sock = empty("socket_rifle", 0.05)
    sock.matrix_world = hand_rest @ offset
    parent_to_bone(sock, rig, "RightHand")
    rig.data.pose_position = 'POSE'
    bpy.context.view_layer.update()
    return sock


# Body regions the game hides under clothing (each a separate mesh):
#   Body_Head (always shown; includes the skin in the shirt's V-neck),
#   Body_Hands (always shown), Body_Torso (shirt), Body_Legs (trousers),
#   Body_Feet (boots).
FEET_TOP_Z = 0.22    # boots reach 0.27
LEGS_TOP_Z = 1.00    # trousers' waistband
NECK_Z = 1.47        # shirt collar reaches 1.49
CUFF_T = 0.86        # shirt cuff is at 0.90 along the forearm


def body_region(c, rig):
    for side in ("Left", "Right"):
        b = rig.data.bones[f"{side}LowerArm"]
        co, no = ck.plane_across(b.head_local, b.tail_local, CUFF_T, keep_towards_head=False)
        if abs(c.x) > 0.25 and (c - co).dot(no) > 0:
            return "Hands"
    if c.z >= NECK_Z or (c.z > 1.30 and c.y < 0 and abs(c.x) < 0.09):
        return "Head"
    if c.z < FEET_TOP_Z:
        return "Feet"
    if c.z < LEGS_TOP_Z and abs(c.x) < 0.27:
        return "Legs"
    return "Torso"


def split_body(rig):
    """One mesh per body region, all skinned to the same skeleton."""
    import bmesh
    body = bpy.data.objects[ck.BODY_NAME]
    regions = ("Head", "Hands", "Torso", "Legs", "Feet")
    for region in regions:
        part = body.copy()
        part.data = body.data.copy()
        part.name = part.data.name = f"Body_{region}"
        bpy.context.scene.collection.objects.link(part)
        bm = bmesh.new()
        bm.from_mesh(part.data)
        mw = part.matrix_world
        dead = [f for f in bm.faces if body_region(mw @ f.calc_center_median(), rig) != region]
        bmesh.ops.delete(bm, geom=dead, context='FACES')
        bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context='VERTS')
        bm.to_mesh(part.data)
        bm.free()
        print(f"[{MODEL_ID}] {part.name}: {len(part.data.vertices)} verts")
    bpy.data.objects.remove(body, do_unlink=True)


def strip_for_export(rig):
    for pb in rig.pose.bones:
        for c in list(pb.constraints):
            pb.constraints.remove(c)
    for obj in list(bpy.data.objects):
        if obj.name.startswith("ctrl_"):
            bpy.data.objects.remove(obj, do_unlink=True)
    rig.animation_data_clear()
    reset_pose(rig)
    # Only the baked clips go to the game (not the keys they were baked from).
    for a in list(bpy.data.actions):
        if a.name not in ANIMS:
            bpy.data.actions.remove(a)


# --------------------------------------------------------------------------
# Previews
# --------------------------------------------------------------------------

def previews(rig, rifle, out):
    """Each animation at a few moments, with the clothes on and the rifle in
    the right hand (via socket_rifle, as the game does)."""
    os.makedirs(out, exist_ok=True)
    for piece in ("trousers_woodland", "boots_combat", "shirt_woodland"):
        ck.load_reference_gear(rig, piece)
    for region in ("Torso", "Legs", "Feet"):     # covered by the clothes, as in the game
        bpy.data.objects[f"Body_{region}"].hide_render = True
    sock = bpy.data.objects["socket_rifle"]
    rifle.parent = sock
    rifle.matrix_parent_inverse = Matrix.Identity(4)
    rifle.matrix_basis = Matrix.Identity(4)
    cam = ck.setup_preview_scene()
    scene = bpy.context.scene
    scene.render.resolution_x, scene.render.resolution_y = 700, 900
    rig.animation_data_create()
    for name in ANIMS:
        action = bpy.data.actions[name]
        rig.animation_data.action = action
        if hasattr(rig.animation_data, "action_slot") and action.slots:
            rig.animation_data.action_slot = action.slots[0]
        end = int(action.frame_range[1])
        for i, f in enumerate((0, end // 4, end // 2, 3 * end // 4) if name in ("walk", "run") else (0,)):
            scene.frame_set(f)
            ck.render_view(cam, os.path.join(out, f"{name}_{i}_side.png"), (4.5, -0.4, 1.0), (0, 0, 0.9), ortho=2.1)
        scene.frame_set(0)
        ck.render_view(cam, os.path.join(out, f"{name}_34.png"), (1.9, -2.6, 1.6), (0, -0.1, 1.0), lens=45)


def main(argv):
    out = os.path.abspath(argv[argv.index("--previews") + 1]) if "--previews" in argv else None
    ck.kit.reset_scene()
    scene = bpy.context.scene
    scene.render.fps = FPS
    rig = load_body_and_rig()
    rifle = load_rifle()
    ctl = Controls(rig)
    for name, (make, carry) in ANIMS.items():
        bake(rig, ctl, name, make, carry)
        reach_report(rig, ctl)
        if name == "idle":
            add_rifle_socket(rig, ctl)
    strip_for_export(rig)
    split_body(rig)
    rig["model_id"] = MODEL_ID
    # The glTF exporter finds actions through the rig's NLA tracks: one muted
    # track per animation, so each exports as its own clip.
    ad = rig.animation_data_create()
    for name in ANIMS:
        a = bpy.data.actions[name]
        a["loop"] = True
        track = ad.nla_tracks.new()
        track.name = name
        strip = track.strips.new(name, 0, a)
        if hasattr(strip, "action_slot") and a.slots:
            strip.action_slot = a.slots[0]
        track.mute = True
    ck.save_outputs(MODEL_ID)
    if out:
        previews(rig, rifle, out)


if __name__ == "__main__":
    main(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])
