//! Systems: the logic that runs every tick, in the order set up in `Sim::new`.
//!
//! - `orders` : apply queued orders, move soldiers, fire, report in position
//! - `player` : watch the player-controlled soldier

pub mod orders;
pub mod player;
