//! Soldier, weapon and unit components.
//!
//! Layout (all ECS entities):
//!   Platoon (Unit)  ─ Members ─▶ Squads (Unit + ParentUnit)
//!   Squad           ─ Members ─▶ Soldiers (Soldier + SquadMember + ...)
//!   Soldier         ─ Inventory.primary ─▶ Weapon (WeaponState + Holder)
//!
//! Units and soldiers carry a stable `u32` id so the client can refer to them
//! without knowing ECS entity ids.

use bevy_ecs::prelude::*;

// ------------------------------------------------------------------ identity

/// Who a soldier is. `id` is stable for the whole session.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct Soldier {
    pub id: u32,
    pub name: String,
    pub rank: Rank,
    pub role: Role,
}

/// Canadian Forces army ranks (1985).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rank {
    Private,
    Corporal,
    MasterCorporal,
    Sergeant,
    WarrantOfficer,
    MasterWarrantOfficer,
    ChiefWarrantOfficer,
    SecondLieutenant,
    Lieutenant,
    Captain,
    Major,
}

impl Rank {
    pub fn abbrev(self) -> &'static str {
        match self {
            Self::Private => "Pte",
            Self::Corporal => "Cpl",
            Self::MasterCorporal => "MCpl",
            Self::Sergeant => "Sgt",
            Self::WarrantOfficer => "WO",
            Self::MasterWarrantOfficer => "MWO",
            Self::ChiefWarrantOfficer => "CWO",
            Self::SecondLieutenant => "2Lt",
            Self::Lieutenant => "Lt",
            Self::Captain => "Capt",
            Self::Major => "Maj",
        }
    }
    pub fn is_officer(self) -> bool {
        matches!(self, Self::SecondLieutenant | Self::Lieutenant | Self::Captain | Self::Major)
    }
}

/// Canadian infantry platoon appointments (1985).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    PlatoonCommander,
    PlatoonWarrant,
    Signaller,
    MedicalAssistant,
    SectionCommander,
    Section2IC,
    GroupLeader,
    LightMachineGunner,
    MachineGunner,
    AntiArmourGunner,
    Rifleman,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Self::PlatoonCommander => "Platoon Commander",
            Self::PlatoonWarrant => "Platoon Warrant",
            Self::Signaller => "Signaller",
            Self::MedicalAssistant => "Medical Assistant",
            Self::SectionCommander => "Section Commander",
            Self::Section2IC => "Section 2IC",
            Self::GroupLeader => "Group Leader",
            Self::LightMachineGunner => "C2 Gunner",
            Self::MachineGunner => "C5 Gunner",
            Self::AntiArmourGunner => "Carl G Gunner",
            Self::Rifleman => "Rifleman",
        }
    }
    pub fn abbrev(self) -> &'static str {
        match self {
            Self::PlatoonCommander => "Pl Comd",
            Self::PlatoonWarrant => "Pl WO",
            Self::Signaller => "Sig",
            Self::MedicalAssistant => "Med A",
            Self::SectionCommander => "Sect Comd",
            Self::Section2IC => "2IC",
            Self::GroupLeader => "Gp Ldr",
            Self::LightMachineGunner => "C2",
            Self::MachineGunner => "C5",
            Self::AntiArmourGunner => "Carl G",
            Self::Rifleman => "Rfn",
        }
    }
}

/// Only these soldiers can be controlled by the player (see the GDD).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct CommissionedOfficer;

// ------------------------------------------------------------------ health

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Default for Health {
    fn default() -> Self {
        Self { current: 100.0, max: 100.0 }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LifeState {
    #[default]
    Healthy,
    Wounded,
    Incapacitated,
    Dead,
}

impl LifeState {
    pub fn name(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Wounded => "wounded",
            Self::Incapacitated => "incapacitated",
            Self::Dead => "dead",
        }
    }
    /// Derive the state from health (thresholds will become data later).
    pub fn from_health(h: &Health) -> Self {
        let f = h.current / h.max.max(1.0);
        if f <= 0.0 {
            Self::Dead
        } else if f < 0.25 {
            Self::Incapacitated
        } else if f < 1.0 {
            Self::Wounded
        } else {
            Self::Healthy
        }
    }
}

// ------------------------------------------------------------------ weapons & inventory

/// Static facts about a weapon type. Later this comes from data files.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeaponSpec {
    pub id: &'static str,
    pub caliber: &'static str,
    pub magazine_capacity: u32,
}

pub const M16A2: WeaponSpec = WeaponSpec { id: "m16a2", caliber: "5.56x45mm", magazine_capacity: 30 };

pub fn weapon_spec(id: &str) -> Option<WeaponSpec> {
    match id {
        "m16a2" => Some(M16A2),
        _ => None,
    }
}

/// A weapon entity. Weapons are their own entities so they can be dropped,
/// picked up or handed over.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct WeaponState {
    pub weapon_id: String,
    pub magazine_capacity: u32,
    /// Rounds in the inserted magazine (None = no magazine inserted).
    pub magazine: Option<u32>,
}

/// Which soldier holds a weapon (None if dropped).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Holder(pub Option<Entity>);

/// What a soldier carries.
#[derive(Component, Debug, Clone, PartialEq, Default)]
pub struct Inventory {
    pub primary: Option<Entity>,
    /// Spare magazines, each entry = rounds in that magazine.
    pub magazines: Vec<u32>,
    /// Loose rounds (bandoleers, stripper clips).
    pub loose_rounds: u32,
    pub frag_grenades: u32,
    pub smoke_grenades: u32,
    pub field_dressings: u32,
}

impl Inventory {
    /// Rounds in spare magazines plus loose rounds (not counting the weapon).
    pub fn reserve_rounds(&self) -> u32 {
        self.magazines.iter().sum::<u32>() + self.loose_rounds
    }
}

// ------------------------------------------------------------------ units

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
