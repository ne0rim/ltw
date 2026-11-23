//! Unit organisation: spawning a platoon and laying out its formation.
//!
//! Canadian rifle platoon (1985): platoon HQ of 6 + 3 rifle sections of 10
//! (36 total). The player is the platoon commander in the HQ. No size limits
//! are enforced yet; the templates below are just the default build.

use bevy_ecs::prelude::*;

use crate::components::{Gait, Stance, Transform, Velocity};
use crate::rng::SimRng;
use crate::soldier::*;

/// Hands out stable ids for soldiers and units.
#[derive(Resource, Debug, Clone, Copy)]
pub struct NextIds {
    pub soldier: u32,
    pub unit: u32,
}

impl Default for NextIds {
    fn default() -> Self {
        Self { soldier: 1, unit: 1 }
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
    Slot { role, rank, team, offset }
}

use FireTeam::{Alpha, Bravo};
use Rank::*;
use Role::*;

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
    "Tremblay", "MacDonald", "Gagnon", "Campbell", "Roy", "Fraser", "Cote", "Stewart", "Bouchard",
    "MacLeod", "Gauthier", "Wilson", "Morin", "Murray", "Lavoie", "Robertson", "Fortin", "Thompson",
    "Gagne", "Mackenzie", "Ouellet", "Anderson", "Pelletier", "Taylor", "Belanger", "Martin",
    "Levesque", "Smith", "Bergeron", "Brown", "Leblanc", "Johnston", "Paquette", "Graham", "Girard",
    "Ross", "Simard", "Morrison", "Boucher", "Reid", "Caron", "Kennedy", "Beaulieu", "Hamilton",
    "Cloutier", "Scott", "Dube", "Grant", "Poirier", "Ferguson", "Fournier", "Bell", "Lapointe",
    "Clarke", "Leclerc", "Doucet", "Hebert", "Sinclair", "Lefebvre", "Cameron", "Gaudreault",
    "Mercier", "MacKay", "Theriault",
];

/// Spawn a platoon around `leader` (the player's entity), which becomes the
/// platoon commander. Returns the platoon entity.
pub fn spawn_platoon(world: &mut World, leader: Entity, origin: [f32; 3], yaw: f32) -> Entity {
    let mut ids = *world.get_resource_or_insert_with(NextIds::default);
    let mut names = shuffled_names(world);

    let platoon = world
        .spawn((
            Unit {
                id: next(&mut ids.unit),
                kind: UnitKind::Platoon,
                name: "1 Platoon".into(),
                callsign: "11".into(),
            },
            Members::default(),
            Leader(Some(leader)),
        ))
        .id();

    let mut squads = Vec::new();

    // Platoon headquarters (around the commander).
    let hq = spawn_squad(
        world, &mut ids, &mut names, platoon, UnitKind::PlatoonHeadquarters, "Platoon HQ", "11",
        &PLATOON_HQ, origin, (0.0, 0.0), yaw, Some(leader),
    );
    squads.push(hq);

    // Rifle sections: 1, 2 and 3 Section, callsigns 11A, 11B, 11C.
    for (i, anchor) in SECTION_ANCHORS.iter().enumerate() {
        let name = format!("{} Section", i + 1);
        let callsign = format!("11{}", (b'A' + i as u8) as char);
        squads.push(spawn_squad(
            world, &mut ids, &mut names, platoon, UnitKind::RifleSection, &name, &callsign,
            &RIFLE_SECTION, origin, *anchor, yaw, None,
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
            Unit { id: next(&mut ids.unit), kind, name: name.into(), callsign: callsign.into() },
            Members::default(),
            Leader(None),
            ParentUnit(platoon),
        ))
        .id();

    let mut members = Vec::new();
    for (i, s) in template.iter().enumerate() {
        let (right, fwd) = (anchor.0 + s.offset.0, anchor.1 + s.offset.1);
        let position = place(origin, yaw, right, fwd);
        let soldier = Soldier {
            id: next(&mut ids.soldier),
            name: names.pop().unwrap_or("Smith").to_string(),
            rank: s.rank,
            role: s.role,
        };
        let member = SquadMember { squad, fire_team: s.team, slot: i as u8 };

        // The player's entity already exists: give it soldier components but
        // keep its client-reported transform.
        let e = match (i, existing_leader) {
            (0, Some(player)) => {
                world.entity_mut(player).insert((soldier, member));
                player
            }
            _ => world
                .spawn((
                    soldier,
                    member,
                    Transform { position, yaw },
                    Velocity::default(),
                    Stance::default(),
                    Gait::default(),
                ))
                .id(),
        };
        world.entity_mut(e).insert((Health::default(), LifeState::Healthy));
        if s.rank.is_officer() && matches!(s.role, Role::PlatoonCommander) {
            world.entity_mut(e).insert(CommissionedOfficer);
        }
        give_loadout(world, e, s.role);
        members.push(e);
    }

    world.get_mut::<Leader>(squad).unwrap().0 = members.first().copied();
    world.get_mut::<Members>(squad).unwrap().0 = members;
    squad
}

/// Basic load: rifle with a full magazine + 6 spares (210 rounds), two frags,
/// a smoke, a field dressing. Medical assistants carry extra dressings.
fn give_loadout(world: &mut World, soldier: Entity, role: Role) {
    let spec = M16A2;
    let weapon = world
        .spawn((
            WeaponState {
                weapon_id: spec.id.to_string(),
                magazine_capacity: spec.magazine_capacity,
                magazine: Some(spec.magazine_capacity),
            },
            Holder(Some(soldier)),
        ))
        .id();
    let inv = Inventory {
        primary: Some(weapon),
        magazines: vec![spec.magazine_capacity; 6],
        loose_rounds: 0,
        frag_grenades: 2,
        smoke_grenades: if matches!(role, Role::SectionCommander | Role::PlatoonWarrant) { 2 } else { 1 },
        field_dressings: if matches!(role, Role::MedicalAssistant) { 12 } else { 1 },
    };
    world.entity_mut(soldier).insert(inv);
}

/// World position for a formation offset. Uses the client's axes: +Y up,
/// facing `yaw` (radians), where yaw 0 looks down -Z.
pub fn place(origin: [f32; 3], yaw: f32, right: f32, forward: f32) -> [f32; 3] {
    let (s, c) = yaw.sin_cos();
    let fwd = (-s, -c);
    let rgt = (c, -s);
    [origin[0] + rgt.0 * right + fwd.0 * forward, origin[1], origin[2] + rgt.1 * right + fwd.1 * forward]
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
