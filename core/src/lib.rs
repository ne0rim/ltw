//! Lead the Way simulation core. Pure Rust, no Godot dependency.
//!
//! - `components/` : data attached to entities (one file per area)
//! - `entities/`   : what each kind of entity is made of, and spawning
//! - `systems/`    : per-tick logic
//! - `sim`         : the fixed-tick simulation and its public API
//! - `events`      : messages back to the client
//! - `rng`         : seeded random numbers

pub mod components;
pub mod entities;
pub mod events;
pub mod rng;
pub mod sim;
pub mod systems;

pub use components::{Gait, PlayerControlled, PlayerReport, Stance, Transform, Velocity};
pub use events::SimEvent;
pub use sim::{Sim, SimClock, SimConfig, SoldierInfo, UnitInfo};
