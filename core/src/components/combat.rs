//! What a soldier is doing right now, and their firing state.

use bevy_ecs::component::Component;

use super::identity::Rank;

/// What the soldier is doing right now (for the client and AI).
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize,
)]
pub enum Activity {
    #[default]
    Idle,
    Moving,
    Firing,
    Reloading,
}

impl Activity {
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Moving => "moving",
            Self::Firing => "firing",
            Self::Reloading => "reloading",
        }
    }
}

/// How experienced a soldier is. Better soldiers aim better (their numbers
/// are in `core/data/veterancy.json`).
#[derive(
    Component,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum Veterancy {
    Green,
    #[default]
    Trained,
    Veteran,
    Elite,
}

impl Veterancy {
    pub const ALL: [Self; 4] = [Self::Green, Self::Trained, Self::Veteran, Self::Elite];

    pub fn name(self) -> &'static str {
        match self {
            Self::Green => "green",
            Self::Trained => "trained",
            Self::Veteran => "veteran",
            Self::Elite => "elite",
        }
    }

    /// The soldier's own aiming error (mrad, 1 standard deviation).
    pub fn aim_error_mrad(self) -> f32 {
        crate::data::VETERANCY[&self].aim_error_mrad
    }

    /// Starting veterancy by rank, until soldiers have their own records:
    /// privates and officers are trained, NCOs are veterans, warrant officers
    /// are elite.
    pub fn for_rank(rank: Rank) -> Self {
        match rank {
            Rank::Private
            | Rank::SecondLieutenant
            | Rank::Lieutenant
            | Rank::Captain
            | Rank::Major => Self::Trained,
            Rank::Corporal | Rank::MasterCorporal | Rank::Sergeant => Self::Veteran,
            Rank::WarrantOfficer | Rank::MasterWarrantOfficer | Rank::ChiefWarrantOfficer => {
                Self::Elite
            }
        }
    }
}

/// How suppressed a soldier is, 0 (calm) to 1 (pinned). Rounds passing close
/// raise it; it fades over time. Suppressed soldiers aim worse.
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize,
)]
pub struct Suppression(pub f32);

/// Firing state for an ordered fire mission.
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize,
)]
pub struct FireControl {
    /// Point the soldier is firing at (None = not firing).
    pub target: Option<[f32; 3]>,
    /// Seconds until the next shot or burst.
    pub cooldown: f32,
    /// Seconds left in a magazine change (0 = not reloading).
    pub reload_left: f32,
    pub out_of_ammo: bool,
    /// Rounds still to fire in this fire mission (one magazine's worth when
    /// the order is given). The soldier stops firing at 0.
    #[serde(default)]
    pub rounds_left: u32,
}
