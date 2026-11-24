//! AI command: how an AI-controlled unit turns its task into action.
//!
//! - `platoon` : a commander with sub-units picks a playbook and gives each
//!   sub-unit a task, then moves the plan through its phases
//! - `section` : a unit without sub-units carries out its task by giving
//!   itself orders (move, fire)
//! - `geometry`: points around an objective (fire base, form-up, frontage)
//!
//! These systems only *decide*. They write orders into `PendingOrders` with
//! `Issuer::Ai`, and those go through the same checks as the player's orders.
//! They run before `apply_orders`, so their orders take effect the same tick.
//!
//! Units are processed in unit-id order so random choices happen in the same
//! order every run (and after loading a save).

mod geometry;
mod platoon;
mod section;

pub use platoon::command_platoons;
pub use section::command_sections;

/// Tuning for AI commanders. They'll move into data files later.
pub mod tuning {
    /// How far back from the objective a fire base sets up (m).
    pub const FIRE_BASE_RANGE: f32 = 150.0;
    /// Form-up point for an assault: distance back from the objective (m)...
    pub const FORM_UP_BACK: f32 = 110.0;
    /// ...and out to the flank (m).
    pub const FORM_UP_FLANK: f32 = 90.0;
    /// Spacing between two sections moving side by side (m).
    pub const SECTION_SPACING: f32 = 30.0;
    /// Reserve and HQ stay this far behind the leading sections (m).
    pub const RESERVE_BACK: f32 = 70.0;
    /// Width of a platoon's defensive frontage, centre to flank (m).
    pub const DEFENCE_FRONTAGE: f32 = 40.0;
    /// Beyond this distance a fire base is worth the time; closer, a direct
    /// assault is favoured. Only a weighting: either playbook can be picked.
    pub const LONG_ATTACK: f32 = 120.0;
}
