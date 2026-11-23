//! Basic ECS components shared across systems.

use bevy_ecs::prelude::*;

/// World-space position (metres) and facing (yaw, radians around +Y).
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct Transform {
    pub position: [f32; 3],
    pub yaw: f32,
}

/// Marks the entity the player is currently controlling.
///
/// Its Transform belongs to the client: Godot moves the player and reports
/// where it ended up. The sim reads that position (for orders, perception,
/// AI) but never moves it.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct PlayerControlled;
