//! Lead the Way simulation core. Pure Rust, no Godot dependency.
//!
//! - `components/` : data attached to entities (one file per area)
//! - `entities/`   : what each kind of entity is made of, and spawning
//! - `systems/`    : per-tick logic
//! - `data`        : game content loaded from `core/data/*.json`
//! - `sim`         : the fixed-tick simulation and its public API
//! - `events`      : messages back to the client
//! - `map`         : map definitions (forces, start positions) and the active map
//! - `rng`         : seeded random numbers
//! - `save`        : save files (world <-> plain data)

pub mod components;
pub mod data;
pub mod entities;
pub mod events;
pub mod map;
pub mod rng;
pub mod save;
pub mod sim;
pub mod systems;

pub use components::{Gait, PlayerControlled, PlayerReport, Stance, Transform, Velocity};
pub use events::SimEvent;
pub use sim::{Sim, SimClock, SimConfig, SoldierInfo, UnitInfo};
