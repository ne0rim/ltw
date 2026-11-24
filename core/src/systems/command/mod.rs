//! AI command: how an AI-controlled unit turns its task into action.
//!
//! - `platoon` : a commander with sub-units picks a playbook for its task
//!   (harbour when idle, linear or all-round defence, fire base + assault,
//!   two up, bounding advance, patrol, withdraw) and gives each sub-unit a
//!   task, then moves the plan through its phases
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
#[cfg(test)]
mod tests;

pub use platoon::command_platoons;
pub use section::command_sections;

/// Tuning for AI commanders. They'll move into data files later.
pub mod tuning {
    /// How far back from the objective a fire base sets up (m).
    pub const FIRE_BASE_RANGE: f32 = 150.0;
    /// Once the assault goes in, the fire base shifts its fire this far
    /// beyond the objective (m).
    pub const SHIFT_FIRE_BEYOND: f32 = 80.0;
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
    /// Beyond this distance a fire base or bounding is worth the time; closer,
    /// a direct assault is favoured. Only weightings: any playbook can happen.
    pub const LONG_ATTACK: f32 = 120.0;
    /// Beyond this, bounding is as likely as a fire base.
    pub const VERY_LONG_ATTACK: f32 = 250.0;
    /// How far each bound carries the moving section past the furthest line (m).
    pub const BOUND: f32 = 50.0;
    /// When bounding gets this close to the objective, the lead assaults (m).
    pub const FINAL_ASSAULT_RANGE: f32 = 60.0;
    /// Radius of an idle platoon's all-round perimeter (m).
    pub const HARBOUR_RADIUS: f32 = 25.0;
    /// Radius of an all-round defence (m).
    pub const ALL_ROUND_RADIUS: f32 = 40.0;
    /// Radius of the HQ's small circle in the middle of a perimeter (m).
    pub const HQ_HOLD_RADIUS: f32 = 6.0;
    /// Radius a lone section holds all-round in (m).
    pub const SECTION_HOLD_RADIUS: f32 = 8.0;
    /// Distance between sections in a patrol wedge (m).
    pub const PATROL_SPACING: f32 = 30.0;
}
