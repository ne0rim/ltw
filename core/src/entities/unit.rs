//! The unit entity (a platoon or a section).

use bevy_ecs::bundle::Bundle;

use crate::components::{Controller, CurrentOrder, Leader, Members, Side, Task, Unit, UnitKind};

/// A unit with no members, no leader, no orders and no task yet. Set `side`
/// and `controller` before spawning (they default to Blue / Player). Sections also get a
/// `ParentUnit` pointing at their platoon.
#[derive(Bundle)]
pub struct UnitBundle {
    pub unit: Unit,
    pub members: Members,
    pub leader: Leader,
    pub order: CurrentOrder,
    pub side: Side,
    pub controller: Controller,
    pub task: Task,
}

impl UnitBundle {
    pub fn new(id: u32, kind: UnitKind, name: &str, callsign: &str) -> Self {
        Self {
            unit: Unit {
                id,
                kind,
                name: name.into(),
                callsign: callsign.into(),
            },
            members: Members::default(),
            leader: Leader(None),
            order: CurrentOrder::default(),
            side: Side::default(),
            controller: Controller::default(),
            task: Task::default(),
        }
    }
}
