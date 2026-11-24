//! Weapons: static specs per weapon type, and the state of a weapon entity.
//! Weapons are their own entities so they can be dropped, picked up or handed over.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use serde::{Deserialize, Serialize};

/// The numbers that decide how a weapon performs. Each weapon entity carries
/// its own copy (saved with it), starting from its type's values in
/// `core/data/weapons.json`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WeaponStats {
    pub magazine_capacity: u32,
    /// Spread of the weapon and ammunition alone, 1 standard deviation in
    /// milliradians (1 mrad = 10 cm at 100 m). The shooter adds their own.
    pub dispersion_mrad: f32,
    /// For bullet drop and time of flight (see `ballistics`).
    pub muzzle_velocity_mps: f32,
    /// Bullets are traced no further than this (m).
    pub max_range_m: f32,
}

/// Static facts about a weapon type, loaded from `core/data/weapons.json`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeaponSpec {
    pub id: &'static str,
    /// Display name, e.g. "M16A2".
    pub name: &'static str,
    pub caliber: &'static str,
    pub stats: WeaponStats,
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
    pub stats: WeaponStats,
    /// Rounds in the inserted magazine (None = no magazine inserted).
    pub magazine: Option<u32>,
}

/// Which soldier holds a weapon (None if dropped).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Holder(pub Option<Entity>);
