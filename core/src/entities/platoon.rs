//! The platoon entity tree: templates and spawning.
//!
//! Canadian rifle platoon (1985): platoon HQ of 6 + 3 rifle sections of 10
//! (36 total). Both sides use the same build for now. On the player's side the
//! player is the platoon commander in the HQ. No size limits are enforced yet;
//! the templates below are just the default build.

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;

use crate::components::Rank::{
    Corporal, Lieutenant, MasterCorporal, Private, Sergeant, WarrantOfficer,
};
use crate::components::Role::{
    AntiArmourGunner, GroupLeader, LightMachineGunner, MachineGunner, MedicalAssistant,
    PlatoonCommander, PlatoonWarrant, Rifleman, Section2IC, SectionCommander, Signaller,
};
use crate::components::{
    CommissionedOfficer, Controller, FireTeam, FormationSlot, Leader, Members, ParentUnit, Rank,
    Role, Side, Soldier, SquadMember, Transform, UnitKind, place,
};
use crate::rng::SimRng;

use super::soldier::{Loadout, SoldierBundle, make_soldier, spawn_soldier};
use super::unit::UnitBundle;

use crate::components::FireTeam::{Alpha, Bravo};

/// Hands out stable ids for soldiers and units.
#[derive(Resource, Debug, Clone, Copy)]
pub struct NextIds {
    pub soldier: u32,
    pub unit: u32,
}

impl Default for NextIds {
    fn default() -> Self {
        Self {
            soldier: 1,
            unit: 1,
        }
    }
}

/// One slot in a section template.
struct Slot {
    role: Role,
    rank: Rank,
    team: Option<FireTeam>,
    /// Formation offset in metres: (right, forward) relative to the section anchor.
    offset: (f32, f32),
}

const fn slot(role: Role, rank: Rank, team: Option<FireTeam>, offset: (f32, f32)) -> Slot {
    Slot {
        role,
        rank,
        team,
        offset,
    }
}

/// Rifle section: section commander with the Carl G gunner, and two assault
/// groups (Alpha led by the 2IC, Bravo by a corporal), each with a C2 gunner.
const RIFLE_SECTION: [Slot; 10] = [
    slot(SectionCommander, Sergeant, None, (0.0, -2.0)),
    slot(Section2IC, MasterCorporal, Some(Alpha), (-7.0, 3.0)),
    slot(LightMachineGunner, Corporal, Some(Alpha), (-11.0, 0.0)),
    slot(Rifleman, Private, Some(Alpha), (-3.0, 0.0)),
    slot(Rifleman, Private, Some(Alpha), (-7.0, -3.0)),
    slot(GroupLeader, Corporal, Some(Bravo), (7.0, 3.0)),
    slot(LightMachineGunner, Corporal, Some(Bravo), (11.0, 0.0)),
    slot(Rifleman, Private, Some(Bravo), (3.0, 0.0)),
    slot(Rifleman, Private, Some(Bravo), (7.0, -3.0)),
    slot(AntiArmourGunner, Corporal, None, (0.0, -6.0)),
];

/// Platoon headquarters (6). Slot 0 is the platoon commander (the player).
/// The C5 gunner's partner is listed as a rifleman (the "No. 2").
const PLATOON_HQ: [Slot; 6] = [
    slot(PlatoonCommander, Lieutenant, None, (0.0, 0.0)),
    slot(PlatoonWarrant, WarrantOfficer, None, (-6.0, -8.0)),
    slot(Signaller, Corporal, None, (2.0, -2.5)),
    slot(MedicalAssistant, Corporal, None, (-3.0, -6.0)),
    slot(MachineGunner, Corporal, None, (8.0, -4.0)),
    slot(Rifleman, Private, None, (10.0, -6.0)),
];

/// Where each section sits relative to the platoon commander: (right, forward) metres.
const SECTION_ANCHORS: [(f32, f32); 3] = [(-35.0, 25.0), (0.0, 30.0), (35.0, 25.0)];

/// What kind of platoon to spawn, and for whom.
#[derive(Debug, Clone, Copy)]
pub struct PlatoonSpec<'a> {
    pub side: Side,
    /// Who commands the platoon and its sections.
    pub controller: Controller,
    pub name: &'a str,
    /// Platoon callsign; sections get A, B, C appended.
    pub callsign: &'a str,
    /// An existing entity that becomes the platoon commander (the player's
    /// body). None spawns a new soldier in that slot.
    pub commander: Option<Entity>,
}

/// Spawn a platoon: platoon HQ around `origin`, rifle sections ahead of it,
/// everyone facing `yaw`. Returns the platoon entity.
pub fn spawn_platoon(world: &mut World, spec: PlatoonSpec, origin: [f32; 3], yaw: f32) -> Entity {
    let mut ids = *world.get_resource_or_insert_with(NextIds::default);
    let mut names = shuffled_names(world);

    let mut bundle = UnitBundle::new(
        next(&mut ids.unit),
        UnitKind::Platoon,
        spec.name,
        spec.callsign,
    );
    bundle.side = spec.side;
    bundle.controller = spec.controller;
    let platoon = world.spawn(bundle).id();

    let mut ctx = SquadCtx {
        ids: &mut ids,
        names: &mut names,
        platoon,
        spec,
        origin,
        yaw,
    };
    let mut squads = Vec::new();

    // Platoon headquarters (around the commander).
    squads.push(spawn_squad(
        world,
        &mut ctx,
        UnitKind::PlatoonHeadquarters,
        "Platoon HQ",
        spec.callsign,
        &PLATOON_HQ,
        (0.0, 0.0),
        spec.commander,
    ));

    // Rifle sections: 1, 2 and 3 Section, callsigns <platoon>A, B, C.
    for (i, anchor) in SECTION_ANCHORS.iter().enumerate() {
        let name = format!("{} Section", i + 1);
        let callsign = format!("{}{}", spec.callsign, (b'A' + i as u8) as char);
        squads.push(spawn_squad(
            world,
            &mut ctx,
            UnitKind::RifleSection,
            &name,
            &callsign,
            &RIFLE_SECTION,
            *anchor,
            None,
        ));
    }

    // The platoon commander (first in the HQ) leads the platoon.
    let commander = world.get::<Leader>(squads[0]).and_then(|l| l.0);
    world.get_mut::<Leader>(platoon).unwrap().0 = commander;
    world.get_mut::<Members>(platoon).unwrap().0 = squads;
    world.insert_resource(ids);
    platoon
}

/// Shared state while spawning one platoon's squads.
struct SquadCtx<'a, 'b> {
    ids: &'a mut NextIds,
    names: &'a mut Vec<&'static str>,
    platoon: Entity,
    spec: PlatoonSpec<'b>,
    origin: [f32; 3],
    yaw: f32,
}

#[allow(clippy::too_many_arguments)]
fn spawn_squad(
    world: &mut World,
    ctx: &mut SquadCtx,
    kind: UnitKind,
    name: &str,
    callsign: &str,
    template: &[Slot],
    anchor: (f32, f32),
    existing_leader: Option<Entity>,
) -> Entity {
    let mut unit = UnitBundle::new(next(&mut ctx.ids.unit), kind, name, callsign);
    unit.side = ctx.spec.side;
    unit.controller = ctx.spec.controller;
    let squad = world.spawn((unit, ParentUnit(ctx.platoon))).id();

    let mut members = Vec::new();
    for (i, s) in template.iter().enumerate() {
        let position = place(
            ctx.origin,
            ctx.yaw,
            anchor.0 + s.offset.0,
            anchor.1 + s.offset.1,
        );
        let mut bundle = SoldierBundle::new(
            Soldier {
                id: next(&mut ctx.ids.soldier),
                name: ctx.names.pop().unwrap_or("Smith").to_string(),
                rank: s.rank,
                role: s.role,
            },
            SquadMember {
                squad,
                fire_team: s.team,
                slot: i as u8,
            },
            FormationSlot {
                right: s.offset.0,
                forward: s.offset.1,
            },
        );
        bundle.side = ctx.spec.side;
        let loadout = Loadout::basic(s.role);

        // The player's entity already exists: make it a soldier but keep its
        // client-reported body.
        let e = match (i, existing_leader) {
            (0, Some(player)) => {
                make_soldier(world, player, bundle, &loadout);
                player
            }
            _ => spawn_soldier(
                world,
                bundle,
                Transform {
                    position,
                    yaw: ctx.yaw,
                },
                &loadout,
            ),
        };
        if s.rank.is_officer() && matches!(s.role, Role::PlatoonCommander) {
            world.entity_mut(e).insert(CommissionedOfficer);
        }
        members.push(e);
    }

    world.get_mut::<Leader>(squad).unwrap().0 = members.first().copied();
    world.get_mut::<Members>(squad).unwrap().0 = members;
    squad
}

fn next(counter: &mut u32) -> u32 {
    let v = *counter;
    *counter += 1;
    v
}

fn shuffled_names(world: &mut World) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = crate::data::SURNAMES.clone();
    let mut rng = world.get_resource_or_insert_with(SimRng::default);
    for i in (1..names.len()).rev() {
        let j = rng.below(i as u32 + 1) as usize;
        names.swap(i, j);
    }
    names
}
