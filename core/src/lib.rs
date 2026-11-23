//! Lead the Way simulation core. Pure Rust, no Godot dependency.

pub mod components;
pub mod events;
pub mod sim;
pub mod systems;

pub use components::{Gait, PlayerControlled, PlayerReport, Stance, Transform, Velocity};
pub use events::SimEvent;
pub use sim::{Sim, SimClock, SimConfig};
