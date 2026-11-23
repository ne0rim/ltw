//! Basic ECS components shared across systems.

use bevy_ecs::prelude::*;

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

/// Body posture. Affects speed, profile (how visible or hittable) and accuracy.
/// The numbers must match `Player.Stance` in the Godot client.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum Stance {
    #[default]
    Standing = 0,
    Crouching = 1,
    Prone = 2,
}

impl Stance {
    pub fn from_index(i: i64) -> Option<Self> {
        Some(match i {
            0 => Self::Standing,
            1 => Self::Crouching,
            2 => Self::Prone,
            _ => return None,
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Standing => "standing",
            Self::Crouching => "crouching",
            Self::Prone => "prone",
        }
    }
}

/// How the soldier is moving. Crouched and prone movement reports `Walk`;
/// the stance sets the speed.
/// The numbers must match `Player.Gait` in the Godot client.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum Gait {
    #[default]
    Still = 0,
    Tactical = 1,
    Walk = 2,
    Sprint = 3,
}

impl Gait {
    pub fn from_index(i: i64) -> Option<Self> {
        Some(match i {
            0 => Self::Still,
            1 => Self::Tactical,
            2 => Self::Walk,
            3 => Self::Sprint,
            _ => return None,
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Still => "still",
            Self::Tactical => "tactical",
            Self::Walk => "walk",
            Self::Sprint => "sprint",
        }
    }
}

/// Marks the entity the player is currently controlling.
///
/// Its movement belongs to the client: Godot moves the player and reports the
/// result. The sim reads it (for orders, perception, AI) but never moves it.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct PlayerControlled;

/// Everything the client reports about the player in one update.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PlayerReport {
    pub transform: Transform,
    pub velocity: Velocity,
    pub stance: Stance,
    pub gait: Gait,
}
