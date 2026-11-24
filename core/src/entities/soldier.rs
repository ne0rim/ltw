//! The soldier entity: what a soldier is made of, and how to spawn one.

use bevy_ecs::bundle::Bundle;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::components::{
    Activity, FireControl, FormationSlot, Gait, Health, Inventory, LifeState, M16A2, MoveGoal,
    Role, Soldier, SquadMember, Stance, Transform, Velocity, WeaponSpec,
};

use super::weapon::spawn_weapon;

/// A physical body in the world (soldiers and the player have one).
#[derive(Bundle, Default)]
pub struct BodyBundle {
    pub transform: Transform,
    pub velocity: Velocity,
    pub stance: Stance,
    pub gait: Gait,
}

/// Everything that makes an entity a soldier, apart from its body and kit:
/// identity, place in the unit, health, and order/combat state.
#[derive(Bundle)]
pub struct SoldierBundle {
    pub soldier: Soldier,
    pub member: SquadMember,
    pub slot: FormationSlot,
    pub health: Health,
    pub life: LifeState,
    pub goal: MoveGoal,
    pub activity: Activity,
    pub fire: FireControl,
}

impl SoldierBundle {
    /// A healthy soldier with no orders.
    pub fn new(soldier: Soldier, member: SquadMember, slot: FormationSlot) -> Self {
        Self {
            soldier,
            member,
            slot,
            health: Health::default(),
            life: LifeState::Healthy,
            goal: MoveGoal::default(),
            activity: Activity::default(),
            fire: FireControl::default(),
        }
    }
}

/// What a soldier is issued when spawned.
#[derive(Debug, Clone, PartialEq)]
pub struct Loadout {
    pub weapon: WeaponSpec,
    /// Spare magazines (all full), not counting the one in the weapon.
    pub spare_magazines: u32,
    pub frag_grenades: u32,
    pub smoke_grenades: u32,
    pub field_dressings: u32,
}

impl Loadout {
    /// Basic load: rifle with a full magazine + 6 spares (210 rounds), two
    /// frags, a smoke, a field dressing. Leaders carry an extra smoke and
    /// medical assistants carry extra dressings.
    pub fn basic(role: Role) -> Self {
        Self {
            weapon: M16A2,
            spare_magazines: 6,
            frag_grenades: 2,
            smoke_grenades: if matches!(role, Role::SectionCommander | Role::PlatoonWarrant) {
                2
            } else {
                1
            },
            field_dressings: if matches!(role, Role::MedicalAssistant) {
                12
            } else {
                1
            },
        }
    }
}

/// Spawn a new (AI) soldier with a body at `transform`, and issue their kit.
pub fn spawn_soldier(
    world: &mut World,
    bundle: SoldierBundle,
    transform: Transform,
    loadout: &Loadout,
) -> Entity {
    let e = world
        .spawn((
            bundle,
            BodyBundle {
                transform,
                ..Default::default()
            },
        ))
        .id();
    issue_kit(world, e, loadout);
    e
}

/// Turn an existing entity (e.g. the player) into a soldier, keeping its body.
pub fn make_soldier(world: &mut World, entity: Entity, bundle: SoldierBundle, loadout: &Loadout) {
    world.entity_mut(entity).insert(bundle);
    issue_kit(world, entity, loadout);
}

/// Spawn the soldier's weapon and give them their inventory.
fn issue_kit(world: &mut World, soldier: Entity, loadout: &Loadout) {
    let weapon = spawn_weapon(world, loadout.weapon, Some(soldier));
    world.entity_mut(soldier).insert(Inventory {
        primary: Some(weapon),
        magazines: vec![loadout.weapon.magazine_capacity; loadout.spare_magazines as usize],
        loose_rounds: 0,
        frag_grenades: loadout.frag_grenades,
        smoke_grenades: loadout.smoke_grenades,
        field_dressings: loadout.field_dressings,
    });
}
