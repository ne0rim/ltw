//! The weapon entity.

use bevy_ecs::bundle::Bundle;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::components::{Holder, WeaponSpec, WeaponState};

#[derive(Bundle)]
pub struct WeaponBundle {
    pub state: WeaponState,
    pub holder: Holder,
}

/// Spawn a weapon of type `spec` with a full magazine inserted.
pub fn spawn_weapon(world: &mut World, spec: WeaponSpec, holder: Option<Entity>) -> Entity {
    world
        .spawn(WeaponBundle {
            state: WeaponState {
                weapon_id: spec.id.to_string(),
                stats: spec.stats,
                magazine: Some(spec.stats.magazine_capacity),
            },
            holder: Holder(holder),
        })
        .id()
}
