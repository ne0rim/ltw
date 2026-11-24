//! What a soldier is doing right now, and their firing state.

use bevy_ecs::component::Component;

/// What the soldier is doing right now (for the client and AI).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Activity {
    #[default]
    Idle,
    Moving,
    Firing,
    Reloading,
}

impl Activity {
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Moving => "moving",
            Self::Firing => "firing",
            Self::Reloading => "reloading",
        }
    }
}

/// Firing state for an ordered fire mission.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct FireControl {
    /// Point the soldier is firing at (None = not firing).
    pub target: Option<[f32; 3]>,
    /// Seconds until the next shot or burst.
    pub cooldown: f32,
    /// Seconds left in a magazine change (0 = not reloading).
    pub reload_left: f32,
    pub out_of_ammo: bool,
}
