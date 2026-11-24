//! Unit organisation: platoons, sections, fire teams, and each soldier's place.
//!
//!   Platoon (Unit) ─ Members ─▶ Sections (Unit + ParentUnit)
//!   Section        ─ Members ─▶ Soldiers (SquadMember + FormationSlot)

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum UnitKind {
    Platoon,
    PlatoonHeadquarters,
    RifleSection,
}

impl UnitKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Platoon => "platoon",
            Self::PlatoonHeadquarters => "platoon_hq",
            Self::RifleSection => "rifle_section",
        }
    }
}

/// A platoon or squad.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct Unit {
    pub id: u32,
    pub kind: UnitKind,
    pub name: String,
    pub callsign: String,
}

/// Units or soldiers that belong to this unit, in order.
#[derive(Component, Debug, Clone, PartialEq, Default)]
pub struct Members(pub Vec<Entity>);

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Leader(pub Option<Entity>);

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ParentUnit(pub Entity);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FireTeam {
    Alpha,
    Bravo,
}

impl FireTeam {
    pub fn name(self) -> &'static str {
        match self {
            Self::Alpha => "Alpha",
            Self::Bravo => "Bravo",
        }
    }
}

/// A soldier's place in their section (or the platoon HQ).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct SquadMember {
    pub squad: Entity,
    pub fire_team: Option<FireTeam>,
    /// Position in the section's formation (0 = commander).
    pub slot: u8,
}

/// A soldier's place in their unit's formation: metres (right, forward) from
/// the unit anchor, in the unit's facing. Used when the unit is ordered to move.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct FormationSlot {
    pub right: f32,
    pub forward: f32,
}
