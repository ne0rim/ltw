//! Messages from the sim back to the client.
//!
//! Systems push events into the [`SimEvents`] resource during a tick. The
//! client drains them after each update (see `Sim::drain_events`). This is
//! the channel for order acknowledgements, AI reports, contact reports and
//! so on.

use bevy_ecs::prelude::*;

use crate::components::{Gait, Stance};

#[derive(Debug, Clone, PartialEq)]
pub enum SimEvent {
    /// The sim saw the player's stance or gait change.
    PlayerMovementChanged {
        tick: u64,
        stance: Stance,
        gait: Gait,
        speed: f32,
    },
}

/// Outbox of events produced this tick and not yet collected by the client.
#[derive(Resource, Debug, Default)]
pub struct SimEvents(pub Vec<SimEvent>);

impl SimEvents {
    pub fn push(&mut self, e: SimEvent) {
        self.0.push(e);
    }
}
