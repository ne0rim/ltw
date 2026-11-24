//! The soldier entity: what a soldier is made of, and how to spawn one.

use bevy_ecs::bundle::Bundle;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::components::{
    Activity, FireControl, FormationSlot, Gait, GoalFacing, Health, Inventory, LifeState, MoveGoal,
    Role, Side, Soldier, SquadMember, Stance, Suppression, Transform, Velocity, Veterancy,
    WeaponSpec, weapon_spec,
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
    pub goal_facing: GoalFacing,
    pub activity: Activity,
    pub fire: FireControl,
    pub side: Side,
    pub veterancy: Veterancy,
    pub suppression: Suppression,
}

impl SoldierBundle {
    /// A healthy soldier with no orders.
    pub fn new(soldier: Soldier, member: SquadMember, slot: FormationSlot) -> Self {
        let veterancy = Veterancy::for_rank(soldier.rank);
        Self {
            soldier,
            member,
            slot,
            health: Health::default(),
            life: LifeState::Healthy,
            goal: MoveGoal::default(),
            goal_facing: GoalFacing::default(),
            activity: Activity::default(),
            fire: FireControl::default(),
            side: Side::default(),
            veterancy,
            suppression: Suppression::default(),
        }
    }
}

/// Weapon id (in `weapons.json`) everyone is issued for now.
const DEFAULT_RIFLE: &str = "m16a2";

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
            weapon: weapon_spec(DEFAULT_RIFLE).expect("default rifle missing from weapons.json"),
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
        magazines: vec![loadout.weapon.stats.magazine_capacity; loadout.spare_magazines as usize],
        loose_rounds: 0,
        frag_grenades: loadout.frag_grenades,
        smoke_grenades: loadout.smoke_grenades,
        field_dressings: loadout.field_dressings,
    });
}
