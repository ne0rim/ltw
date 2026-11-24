//! Orders given to units, and the order a unit is carrying out.

use bevy_ecs::component::Component;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Order {
    /// Walk to `target` and form up facing `facing` (yaw, radians).
    MoveTo { target: [f32; 3], facing: f32 },
    /// Stay put and fire at `target` until given another order.
    FireAt { target: [f32; 3] },
}

impl Order {
    pub fn name(&self) -> &'static str {
        match self {
            Self::MoveTo { .. } => "move",
            Self::FireAt { .. } => "fire",
        }
    }
    pub fn target(&self) -> [f32; 3] {
        match *self {
            Self::MoveTo { target, .. } | Self::FireAt { target } => target,
        }
    }
}

/// The order a unit is carrying out (on platoon/section entities).
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct CurrentOrder {
    pub order: Option<Order>,
    /// Set once the unit has reported "in position" for its current move.
    pub arrived_reported: bool,
}
