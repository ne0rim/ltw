//! The platoon entity tree: templates and spawning.
//!
//! Canadian rifle platoon (1985): platoon HQ of 6 + 3 rifle sections of 10
//! (36 total). The player is the platoon commander in the HQ. No size limits
//! are enforced yet; the templates below are just the default build.

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
    CommissionedOfficer, FireTeam, FormationSlot, Leader, Members, ParentUnit, Rank, Role, Soldier,
    SquadMember, Transform, UnitKind, place,
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

/// English and French Canadian surnames.
const SURNAMES: [&str; 64] = [
    "Tremblay",
    "MacDonald",
    "Gagnon",
    "Campbell",
    "Roy",
    "Fraser",
    "Cote",
    "Stewart",
    "Bouchard",
    "MacLeod",
    "Gauthier",
    "Wilson",
    "Morin",
    "Murray",
    "Lavoie",
    "Robertson",
    "Fortin",
    "Thompson",
    "Gagne",
    "Mackenzie",
    "Ouellet",
    "Anderson",
    "Pelletier",
    "Taylor",
    "Belanger",
    "Martin",
    "Levesque",
    "Smith",
    "Bergeron",
    "Brown",
    "Leblanc",
    "Johnston",
    "Paquette",
    "Graham",
    "Girard",
    "Ross",
    "Simard",
    "Morrison",
    "Boucher",
    "Reid",
    "Caron",
    "Kennedy",
    "Beaulieu",
    "Hamilton",
    "Cloutier",
    "Scott",
    "Dube",
    "Grant",
    "Poirier",
    "Ferguson",
    "Fournier",
    "Bell",
    "Lapointe",
    "Clarke",
    "Leclerc",
    "Doucet",
    "Hebert",
    "Sinclair",
    "Lefebvre",
    "Cameron",
    "Gaudreault",
    "Mercier",
    "MacKay",
    "Theriault",
];

/// Spawn a platoon around `leader` (the player's entity), which becomes the
/// platoon commander. Returns the platoon entity.
pub fn spawn_platoon(world: &mut World, leader: Entity, origin: [f32; 3], yaw: f32) -> Entity {
    let mut ids = *world.get_resource_or_insert_with(NextIds::default);
    let mut names = shuffled_names(world);

    let mut bundle = UnitBundle::new(next(&mut ids.unit), UnitKind::Platoon, "1 Platoon", "11");
    bundle.leader = Leader(Some(leader));
    let platoon = world.spawn(bundle).id();

    let mut squads = Vec::new();

    // Platoon headquarters (around the commander).
    let hq = spawn_squad(
        world,
        &mut ids,
        &mut names,
        platoon,
        UnitKind::PlatoonHeadquarters,
        "Platoon HQ",
        "11",
        &PLATOON_HQ,
        origin,
        (0.0, 0.0),
        yaw,
        Some(leader),
    );
    squads.push(hq);

    // Rifle sections: 1, 2 and 3 Section, callsigns 11A, 11B, 11C.
    for (i, anchor) in SECTION_ANCHORS.iter().enumerate() {
        let name = format!("{} Section", i + 1);
        let callsign = format!("11{}", (b'A' + i as u8) as char);
        squads.push(spawn_squad(
            world,
            &mut ids,
            &mut names,
            platoon,
            UnitKind::RifleSection,
            &name,
            &callsign,
            &RIFLE_SECTION,
            origin,
            *anchor,
            yaw,
            None,
        ));
    }

    world.get_mut::<Members>(platoon).unwrap().0 = squads;
    world.insert_resource(ids);
    platoon
}

#[allow(clippy::too_many_arguments)]
fn spawn_squad(
    world: &mut World,
    ids: &mut NextIds,
    names: &mut Vec<&'static str>,
    platoon: Entity,
    kind: UnitKind,
    name: &str,
    callsign: &str,
    template: &[Slot],
    origin: [f32; 3],
    anchor: (f32, f32),
    yaw: f32,
    existing_leader: Option<Entity>,
) -> Entity {
    let squad = world
        .spawn((
            UnitBundle::new(next(&mut ids.unit), kind, name, callsign),
            ParentUnit(platoon),
        ))
        .id();

    let mut members = Vec::new();
    for (i, s) in template.iter().enumerate() {
        let position = place(origin, yaw, anchor.0 + s.offset.0, anchor.1 + s.offset.1);
        let bundle = SoldierBundle::new(
            Soldier {
                id: next(&mut ids.soldier),
                name: names.pop().unwrap_or("Smith").to_string(),
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
        let loadout = Loadout::basic(s.role);

        // The player's entity already exists: make it a soldier but keep its
        // client-reported body.
        let e = match (i, existing_leader) {
            (0, Some(player)) => {
                make_soldier(world, player, bundle, &loadout);
                player
            }
            _ => spawn_soldier(world, bundle, Transform { position, yaw }, &loadout),
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
    let mut names: Vec<&'static str> = SURNAMES.to_vec();
    let mut rng = world.get_resource_or_insert_with(SimRng::default);
    for i in (1..names.len()).rev() {
        let j = rng.below(i as u32 + 1) as usize;
        names.swap(i, j);
    }
    names
}
