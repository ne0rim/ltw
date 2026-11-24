//! Systems: the logic that runs every tick, in the order set up in `Sim::new`.
//!
//! - `command`: AI commanders turn tasks into sub-unit tasks and orders
//! - `orders` : apply queued orders, move soldiers, fire, report in position
//! - `player` : watch the player-controlled soldier

pub mod command;
pub mod orders;
pub mod player;
