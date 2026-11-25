//! Keeping soldiers on the map's ground.

use bevy_ecs::query::{With, Without};
use bevy_ecs::system::{Query, Res};
use bevy_ecs::world::World;

use crate::components::{PlayerControlled, Soldier, Transform};
use crate::map::ActiveMap;

/// AI soldiers stand on the ground: their height follows the terrain as they
/// move. (The player's body is Godot's, which reports its own height.)
pub fn keep_on_ground(
    map: Option<Res<ActiveMap>>,
    mut soldiers: Query<&mut Transform, (With<Soldier>, Without<PlayerControlled>)>,
) {
    let Some(map) = map else { return };
    if map.0.heights.is_none() {
        return;
    }
    for mut t in &mut soldiers {
        let y = map.0.height_at(t.position[0], t.position[2]);
        if t.position[1] != y {
            t.position[1] = y;
        }
    }
}

/// The same, run once outside the schedule (right after spawning).
pub fn put_on_ground(world: &mut World) {
    let Some(map) = world.get_resource::<ActiveMap>().cloned() else {
        return;
    };
    let mut q =
        world.query_filtered::<&mut Transform, (With<Soldier>, Without<PlayerControlled>)>();
    for mut t in q.iter_mut(world) {
        t.position[1] = map.0.height_at(t.position[0], t.position[2]);
    }
}
