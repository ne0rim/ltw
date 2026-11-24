//! The player-controlled entity.
//!
//! The client moves it and reports where it is. When a platoon is spawned,
//! this same entity also becomes the platoon commander (see `platoon`).

use bevy_ecs::bundle::Bundle;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::components::PlayerControlled;

use super::soldier::BodyBundle;

#[derive(Bundle, Default)]
pub struct PlayerBundle {
    pub player: PlayerControlled,
    pub body: BodyBundle,
}

pub fn spawn_player(world: &mut World) -> Entity {
    world.spawn(PlayerBundle::default()).id()
}
