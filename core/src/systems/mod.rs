//! Systems: the logic that runs every tick, in the order set up in `Sim::new`.
//!
//! - `combat` : resolve every shot (hits, kills, suppression)
//! - `command`: AI commanders turn tasks into sub-unit tasks and orders
//! - `orders` : apply queued orders, move soldiers, fire, report in position
//! - `player` : watch the player-controlled soldier
//! - `terrain`: keep soldiers on the map's ground

pub mod combat;
pub mod command;
pub mod orders;
pub mod player;
pub mod terrain;
