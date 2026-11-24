//! The player-controlled entity and what the client reports about it.

use bevy_ecs::component::Component;

use super::movement::{Gait, Stance};
use super::transform::{Transform, Velocity};

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
