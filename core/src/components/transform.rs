//! Where things are and how they move: position, facing, velocity, plus
//! helpers for the client's axis conventions.

use bevy_ecs::component::Component;

/// World-space position (metres) and facing (yaw, radians around +Y).
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct Transform {
    pub position: [f32; 3],
    pub yaw: f32,
}

/// World-space velocity in metres per second.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct Velocity(pub [f32; 3]);

impl Velocity {
    /// Speed across the ground, ignoring vertical motion.
    pub fn horizontal_speed(&self) -> f32 {
        let [x, _, z] = self.0;
        (x * x + z * z).sqrt()
    }
}

/// World position for a formation offset. Uses the client's axes: +Y up,
/// facing `yaw` (radians), where yaw 0 looks down -Z.
pub fn place(origin: [f32; 3], yaw: f32, right: f32, forward: f32) -> [f32; 3] {
    let (s, c) = yaw.sin_cos();
    let fwd = (-s, -c);
    let rgt = (c, -s);
    [
        origin[0] + rgt.0 * right + fwd.0 * forward,
        origin[1],
        origin[2] + rgt.1 * right + fwd.1 * forward,
    ]
}

/// Client yaw convention: yaw 0 looks down -Z.
pub fn yaw_towards(dir_x: f32, dir_z: f32) -> f32 {
    (-dir_x).atan2(-dir_z)
}
