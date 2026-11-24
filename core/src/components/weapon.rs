//! Weapons: static specs per weapon type, and the state of a weapon entity.
//! Weapons are their own entities so they can be dropped, picked up or handed over.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;

/// Static facts about a weapon type. Later this comes from data files.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeaponSpec {
    pub id: &'static str,
    pub caliber: &'static str,
    pub magazine_capacity: u32,
}

pub const M16A2: WeaponSpec = WeaponSpec {
    id: "m16a2",
    caliber: "5.56x45mm",
    magazine_capacity: 30,
};

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
