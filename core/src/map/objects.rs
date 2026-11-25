//! Objects placed on a map: barricades (sandbag walls) that stop bullets and
//! that defenders fight from, ammo boxes, trees and boulders.
//!
//! Every object is a box standing on the ground (`length_m` across its front,
//! `thickness_m` front to back, `height_m` tall); that box is what stops
//! bullets. For a tree it is the trunk (the canopy is `canopy_m`, for sight).
//! `variant` picks which model the client shows (tree or rock number n).

use serde::{Deserialize, Serialize};

use crate::components::{place, yaw_towards};

use super::yaw_from_heading;

/// What an object is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ObjectKind {
    /// A low wall (sandbags): stops bullets, defenders crouch behind it.
    #[default]
    Barricade,
    /// A crate of ammunition that never runs out (walk up and press F).
    AmmoBox,
    /// A tree: the trunk stops bullets; the canopy hides (later, for sight).
    Tree,
    /// A large rock: stops bullets, cover to kneel behind.
    Boulder,
}

impl ObjectKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Barricade => "barricade",
            Self::AmmoBox => "ammo_box",
            Self::Tree => "tree",
            Self::Boulder => "boulder",
        }
    }
}

/// A box-shaped object standing on the ground. Its front faces `facing_deg`
/// (compass heading); for a barricade that is the side the enemy is on.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MapObject {
    pub kind: ObjectKind,
    /// Centre of the object's base, on the ground.
    pub position: [f32; 3],
    pub facing_deg: f32,
    /// Size across its front (m).
    pub length_m: f32,
    pub height_m: f32,
    /// Size front to back (m).
    pub thickness_m: f32,
    /// Which model of this kind to show (the client picks model n, wrapping).
    #[serde(default)]
    pub variant: u8,
    /// Trees: radius of the canopy (m). 0 for everything else.
    #[serde(default)]
    pub canopy_m: f32,
}

impl MapObject {
    pub fn yaw(&self) -> f32 {
        yaw_from_heading(self.facing_deg)
    }

    /// Unit directions (x, z): the way its front faces, and to its right.
    fn axes(&self) -> ([f32; 2], [f32; 2]) {
        let yaw = self.yaw();
        let (s, c) = yaw.sin_cos();
        ([-s, -c], [c, -s])
    }

    /// The two ends of a line `back` metres behind the object's rear face,
    /// running its full length: where defenders stand to fight from it.
    pub fn firing_line(&self, back: f32) -> ([f32; 3], [f32; 3]) {
        let behind = self.thickness_m * 0.5 + back;
        let half = self.length_m * 0.5;
        let yaw = self.yaw();
        (
            place(self.position, yaw, -half, -behind),
            place(self.position, yaw, half, -behind),
        )
    }

    /// The smallest x/z rectangle containing its footprint (its turned box
    /// seen from above): ([min x, min z], [max x, max z]).
    pub fn footprint_bounds(&self) -> ([f32; 2], [f32; 2]) {
        let (front, right) = self.axes();
        let (hl, ht) = (self.length_m * 0.5, self.thickness_m * 0.5);
        let mut min = [f32::MAX; 2];
        let mut max = [f32::MIN; 2];
        for (a, b) in [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
            let x = self.position[0] + right[0] * hl * a + front[0] * ht * b;
            let z = self.position[2] + right[1] * hl * a + front[1] * ht * b;
            min = [min[0].min(x), min[1].min(z)];
            max = [max[0].max(x), max[1].max(z)];
        }
        (min, max)
    }

    /// Facing (yaw) of someone standing behind it looking over the top.
    pub fn defender_yaw(&self) -> f32 {
        let (front, _) = self.axes();
        yaw_towards(front[0], front[1])
    }

    /// How far along `dir` (unit length) from `origin` a bullet hits this
    /// object's box, within `max` metres.
    pub fn raycast(&self, origin: [f32; 3], dir: [f32; 3], max: f32) -> Option<f32> {
        let (front, right) = self.axes();
        // The ray in the object's own frame: x across, y up, z front.
        let rel = [
            origin[0] - self.position[0],
            origin[1] - self.position[1],
            origin[2] - self.position[2],
        ];
        let local_o = [
            rel[0] * right[0] + rel[2] * right[1],
            rel[1],
            rel[0] * front[0] + rel[2] * front[1],
        ];
        let local_d = [
            dir[0] * right[0] + dir[2] * right[1],
            dir[1],
            dir[0] * front[0] + dir[2] * front[1],
        ];
        let min = [-self.length_m * 0.5, 0.0, -self.thickness_m * 0.5];
        let max_b = [self.length_m * 0.5, self.height_m, self.thickness_m * 0.5];
        // Slab test.
        let (mut t0, mut t1) = (0.0_f32, max);
        for a in 0..3 {
            if local_d[a].abs() < 1e-8 {
                if local_o[a] < min[a] || local_o[a] > max_b[a] {
                    return None;
                }
            } else {
                let inv = 1.0 / local_d[a];
                let (mut near, mut far) =
                    ((min[a] - local_o[a]) * inv, (max_b[a] - local_o[a]) * inv);
                if near > far {
                    std::mem::swap(&mut near, &mut far);
                }
                t0 = t0.max(near);
                t1 = t1.min(far);
                if t0 > t1 {
                    return None;
                }
            }
        }
        Some(t0)
    }
}
