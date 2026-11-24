//! Messages from the sim back to the client.
//!
//! Systems push events into the [`SimEvents`] resource during a tick. The
//! client drains them after each update (see `Sim::drain_events`). This is
//! the channel for order acknowledgements, AI reports, contact reports and
//! so on.

use bevy_ecs::resource::Resource;

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
    /// A platoon was created.
    PlatoonSpawned { unit_id: u32, soldiers: u32 },
    /// A soldier swapped magazines.
    Reloaded {
        soldier_id: u32,
        rounds: u32,
        magazines_left: u32,
    },
    /// A soldier refilled their pouches from an ammo box.
    Resupplied { soldier_id: u32, magazines: u32 },
    /// A unit accepted an order ("move", "fire").
    OrderAcknowledged {
        unit_id: u32,
        callsign: String,
        order: &'static str,
    },
    /// Every soldier of a unit reached their place after a move order.
    UnitInPosition { unit_id: u32, callsign: String },
    /// A soldier fired `rounds` rounds; `target` is where they land.
    ShotFired {
        soldier_id: u32,
        target: [f32; 3],
        rounds: u32,
    },
    /// A soldier has fired their last magazine.
    OutOfAmmo { soldier_id: u32 },
}

/// Outbox of events produced this tick and not yet collected by the client.
#[derive(Resource, Debug, Default)]
pub struct SimEvents(pub Vec<SimEvent>);

impl SimEvents {
    pub fn push(&mut self, e: SimEvent) {
        self.0.push(e);
    }
}
