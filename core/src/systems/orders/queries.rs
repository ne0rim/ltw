//! Named queries shared by the order systems. `#[derive(QueryData)]` turns a
//! query into a struct, so code reads `soldier.goal` instead of counting its
//! position in a tuple.

use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QueryData, Without};
use bevy_ecs::system::Query;

use crate::components::{
    Activity, Controller, CurrentOrder, FireControl, FormationSlot, Inventory, Members, MoveGoal,
    PlayerControlled, Transform, Unit,
};

/// A unit (platoon or section) and the order it is carrying out.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct OrderedUnit {
    pub entity: Entity,
    pub unit: &'static Unit,
    pub members: &'static Members,
    pub current: &'static mut CurrentOrder,
    pub controller: &'static Controller,
}

/// What an order changes on a soldier.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct OrderedSoldier {
    pub transform: &'static Transform,
    pub slot: &'static FormationSlot,
    pub goal: &'static mut MoveGoal,
    pub activity: &'static mut Activity,
    pub fire: &'static mut FireControl,
    pub inventory: Option<&'static Inventory>,
}

/// Every AI soldier (orders never touch the player-controlled one).
pub type AiSoldiers<'w, 's> = Query<'w, 's, OrderedSoldier, Without<PlayerControlled>>;
