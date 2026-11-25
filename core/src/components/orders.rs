//! Orders given to units, and the order a unit is carrying out.

use bevy_ecs::component::Component;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Order {
    /// Walk to `target` and form up facing `facing` (yaw, radians).
    MoveTo { target: [f32; 3], facing: f32 },
    /// Stay put and fire at `target` until given another order.
    FireAt { target: [f32; 3] },
    /// Spread along an arc around `centre`, `radius` metres out, every soldier
    /// facing outward. The arc starts at yaw `start` and runs `sweep` radians
    /// clockwise (TAU = all the way round). Used for all-round security.
    Perimeter {
        centre: [f32; 3],
        radius: f32,
        start: f32,
        sweep: f32,
    },
    /// Spread evenly along the line from `from` to `to`, all facing `facing`
    /// (yaw); crouched if `crouch` (behind a barricade).
    Line {
        from: [f32; 3],
        to: [f32; 3],
        facing: f32,
        crouch: bool,
    },
}

impl Order {
    pub fn name(&self) -> &'static str {
        match self {
            Self::MoveTo { .. } => "move",
            Self::FireAt { .. } => "fire",
            Self::Perimeter { .. } => "perimeter",
            Self::Line { .. } => "line",
        }
    }
    pub fn target(&self) -> [f32; 3] {
        match *self {
            Self::MoveTo { target, .. } | Self::FireAt { target } => target,
            Self::Perimeter { centre, .. } => centre,
            Self::Line { from, to, .. } => midpoint(from, to),
        }
    }
    /// Orders that walk soldiers somewhere and end when they have all arrived.
    pub fn is_move(&self) -> bool {
        matches!(
            self,
            Self::MoveTo { .. } | Self::Perimeter { .. } | Self::Line { .. }
        )
    }
}

/// Halfway between two points.
pub fn midpoint(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        (a[0] + b[0]) * 0.5,
        (a[1] + b[1]) * 0.5,
        (a[2] + b[2]) * 0.5,
    ]
}

/// The order a unit is carrying out (on platoon/section entities).
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct CurrentOrder {
    pub order: Option<Order>,
    /// Set once the unit has reported "in position" for its current move.
    pub arrived_reported: bool,
}
