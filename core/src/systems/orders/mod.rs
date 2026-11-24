//! Unit orders. The client queues them (`Sim::order_move`, `Sim::order_fire_at`);
//! they are applied at the start of the next tick so every system sees the
//! same state. Units acknowledge orders and report back through `SimEvent`s.
//!
//! - `apply`   : take queued orders, hand each to its order type, acknowledge
//! - `move_to` : `Order::MoveTo` - formation goals, walking, "in position";
//!   the order ends when everyone has arrived
//! - `fire_at` : `Order::FireAt` - fire missions, shooting, magazine changes;
//!   each soldier fires one magazine's worth, then the order ends
//! - `queries` : named queries shared by the order systems
//!
//! To add an order type: add a variant to `Order`, a file here with an
//! `assign` function (and any per-tick systems), and a match arm in `apply`.
//!
//! The player-controlled soldier is never moved or fired by these systems.

mod apply;
mod fire_at;
mod move_to;
mod queries;

pub use apply::{OrderRequest, PendingOrders, apply_orders};
pub use fire_at::{fire_control, report_fire_complete};
pub use move_to::{move_soldiers, report_in_position};

/// Tuning values for orders. They'll move into data files later.
pub mod tuning {
    /// Walking pace for ordered moves (m/s).
    pub const MOVE_SPEED: f32 = 2.5;
    /// Close enough to a move goal to count as arrived (m).
    pub const ARRIVE_DISTANCE: f32 = 0.35;
    /// Time to change magazines (s).
    pub const RELOAD_SECONDS: f32 = 3.0;
    /// Seconds between aimed single shots for riflemen (min, max).
    pub const RIFLE_INTERVAL: (f32, f32) = (1.1, 2.4);
    /// Seconds between bursts for machine gunners (min, max).
    pub const GUNNER_INTERVAL: (f32, f32) = (0.9, 1.7);
    pub const GUNNER_BURST: u32 = 3;
    /// Impact scatter around the target: base radius + per metre of range.
    pub const SPREAD_BASE: f32 = 0.8;
    pub const SPREAD_PER_METRE: f32 = 0.03;
    /// Longest random delay before a unit's first shots (s), so its soldiers
    /// don't all fire in unison.
    pub const FIRST_SHOT_STAGGER: f32 = 1.2;
}
