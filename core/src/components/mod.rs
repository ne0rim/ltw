//! ECS components: the data attached to entities. One file per area.
//!
//! Every component is re-exported here, so code can import from
//! `crate::components` without knowing which file it lives in.

pub mod combat;
pub mod health;
pub mod identity;
pub mod inventory;
pub mod movement;
pub mod orders;
pub mod organisation;
pub mod player;
pub mod transform;
pub mod weapon;

pub use combat::{Activity, FireControl};
pub use health::{Health, LifeState};
pub use identity::{CommissionedOfficer, Rank, Role, Soldier};
pub use inventory::{Inventory, swap_magazine};
pub use movement::{Gait, MoveGoal, Stance};
pub use orders::{CurrentOrder, Order};
pub use organisation::{
    FireTeam, FormationSlot, Leader, Members, ParentUnit, SquadMember, Unit, UnitKind,
};
pub use player::{PlayerControlled, PlayerReport};
pub use transform::{Transform, Velocity, place, yaw_towards};
pub use weapon::{Holder, M16A2, WeaponSpec, WeaponState, weapon_spec};
