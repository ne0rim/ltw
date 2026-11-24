//! What a soldier carries, and magazine handling.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;

use super::weapon::WeaponState;

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

/// Swap the inserted magazine for the fullest spare. Partly used magazines go
/// back in the pouch; empty ones are dropped. Returns the rounds now loaded,
/// or None if no spare magazine has rounds.
pub fn swap_magazine(inv: &mut Inventory, weapon: &mut WeaponState) -> Option<u32> {
    let (idx, best) = inv
        .magazines
        .iter()
        .copied()
        .enumerate()
        .max_by_key(|(_, r)| *r)?;
    if best == 0 {
        return None;
    }
    inv.magazines.remove(idx);
    if let Some(r) = weapon.magazine.filter(|r| *r > 0) {
        inv.magazines.push(r);
    }
    weapon.magazine = Some(best);
    Some(best)
}
