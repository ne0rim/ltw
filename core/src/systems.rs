//! Per-tick gameplay systems.

use bevy_ecs::prelude::*;

use crate::components::{Gait, PlayerControlled, Stance, Velocity};
use crate::events::{SimEvent, SimEvents};
use crate::sim::SimClock;

/// Tells the client when the sim sees the player's stance or gait change.
/// This is a simple round trip for now. Later, AI and orders will react here too.
pub fn watch_player_movement(
    clock: Res<SimClock>,
    q: Query<(&Stance, &Gait, &Velocity), With<PlayerControlled>>,
    mut last: Local<Option<(Stance, Gait)>>,
    mut events: ResMut<SimEvents>,
) {
    let Ok((&stance, &gait, vel)) = q.single() else { return };
    if *last != Some((stance, gait)) {
        // Skip the very first tick: the default state isn't a "change".
        if last.is_some() {
            events.push(SimEvent::PlayerMovementChanged {
                tick: clock.tick,
                stance,
                gait,
                speed: vel.horizontal_speed(),
            });
        }
        *last = Some((stance, gait));
    }
}
