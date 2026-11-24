//! Weapons: static specs per weapon type, and the state of a weapon entity.
//! Weapons are their own entities so they can be dropped, picked up or handed over.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;

/// Static facts about a weapon type, loaded from `core/data/weapons.json`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeaponSpec {
    pub id: &'static str,
    /// Display name, e.g. "M16A2".
    pub name: &'static str,
    pub caliber: &'static str,
    pub magazine_capacity: u32,
}

/// Look up a weapon type by id (a key in `weapons.json`).
pub fn weapon_spec(id: &str) -> Option<WeaponSpec> {
    crate::data::WEAPONS.get(id).copied()
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
