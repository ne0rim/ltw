//! Lead the Way simulation core. Pure Rust, no Godot dependency.

pub mod components;
pub mod sim;

pub use components::{PlayerControlled, Transform};
pub use sim::{Sim, SimClock, SimConfig};
