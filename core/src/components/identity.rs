//! Who a soldier is: id, name, rank and appointment (1985 Canadian Forces).

use bevy_ecs::component::Component;

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
        matches!(
            self,
            Self::SecondLieutenant | Self::Lieutenant | Self::Captain | Self::Major
        )
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
