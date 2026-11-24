//! Posture and movement state: stance, gait, and where a soldier is walking to.

use bevy_ecs::component::Component;

/// Body posture. Affects speed, profile (how visible or hittable) and accuracy.
/// The numbers must match `Player.Stance` in the Godot client.
#[derive(
    Component,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
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
#[derive(
    Component,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
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

/// Which way the soldier turns to face on reaching their move goal (yaw).
/// None = keep facing the way they walked.
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize,
)]
pub struct GoalFacing(pub Option<f32>);

/// Where the soldier is walking to (None = not moving).
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct MoveGoal(pub Option<[f32; 3]>);
