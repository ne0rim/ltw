//! Entities: what each kind of thing in the sim is made of (its component
//! bundle) and how to spawn it. Matches the "Base Entities" in the design doc.
//!
//! - `player`  : the player-controlled body
//! - `soldier` : any soldier (body + identity, health, orders state, inventory)
//! - `weapon`  : a weapon entity, held by a soldier
//! - `unit`    : a platoon or section
//! - `platoon` : the default Canadian rifle platoon and its formation

pub mod platoon;
pub mod player;
pub mod soldier;
pub mod unit;
pub mod weapon;
