//! Save files: turn the sim's world into plain data and back.
//!
//! Entities refer to each other with `Entity` handles, which are only valid
//! while the game is running. In the file, every link is stored as a stable id
//! instead (soldier ids and unit ids). Loading happens in two passes: first
//! spawn every unit and soldier, then reconnect the links by id. A link whose
//! id no longer exists is dropped rather than crashing the load.
//!
//! Weapons are saved with the soldier holding them and store only their
//! weapon id and rounds loaded. Stats such as magazine capacity come from
//! `core/data/weapons.json` when loading, so rebalancing a weapon also changes
//! it in old saves.
//!
//! Godot owns the player's movement. After loading, the client reads
//! `Sim::player_state()` and moves the player body to that position.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::Has;
use bevy_ecs::world::World;
use serde::{Deserialize, Serialize};

use crate::components::{
    Activity, CommissionedOfficer, Controller, CurrentOrder, FireControl, FireTeam, FormationSlot,
    Gait, Health, Holder, Inventory, Leader, LifeState, Members, MoveGoal, Order, ParentUnit, Plan,
    PlayerControlled, Rank, Role, Side, Soldier, SquadMember, Stance, Task, Transform, Unit,
    UnitKind, Velocity, WeaponState, weapon_spec,
};
use crate::entities::platoon::NextIds;
use crate::entities::soldier::{BodyBundle, SoldierBundle};
use crate::entities::unit::UnitBundle;
use crate::entities::weapon::WeaponBundle;
use crate::rng::SimRng;
use crate::sim::SimClock;

/// Bump this when the file format changes in a way old files can't be read.
pub const SAVE_VERSION: u32 = 1;

/// Everything needed to rebuild the sim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveGame {
    pub version: u32,
    pub tick: u64,
    pub elapsed: f64,
    pub rng_state: u64,
    pub next_soldier_id: u32,
    pub next_unit_id: u32,
    /// The player's body. Also present as a soldier below once a platoon exists.
    pub player: PlayerSave,
    /// Sorted by id.
    pub units: Vec<UnitSave>,
    /// Sorted by id.
    pub soldiers: Vec<SoldierSave>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlayerSave {
    pub transform: Transform,
    pub stance: Stance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnitSave {
    pub id: u32,
    pub kind: UnitKind,
    pub name: String,
    pub callsign: String,
    pub parent: Option<u32>,
    pub leader: Option<u32>,
    /// Sub-unit ids for a platoon.
    pub child_units: Vec<u32>,
    /// Soldier ids for a section or platoon HQ.
    pub soldiers: Vec<u32>,
    pub order: Option<Order>,
    pub arrived_reported: bool,
    // Added after version 1 shipped: defaults keep older saves loading.
    #[serde(default)]
    pub side: Side,
    #[serde(default)]
    pub controller: Controller,
    #[serde(default)]
    pub task: Task,
    #[serde(default)]
    pub plan: Option<Plan>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoldierSave {
    pub id: u32,
    #[serde(default)]
    pub side: Side,
    pub name: String,
    pub rank: Rank,
    pub role: Role,
    pub is_player: bool,
    pub is_officer: bool,
    pub squad: u32,
    pub fire_team: Option<FireTeam>,
    pub slot: u8,
    /// Formation offset (right, forward) in metres.
    pub formation: [f32; 2],
    pub transform: Transform,
    pub stance: Stance,
    pub health: Health,
    pub life: LifeState,
    pub move_goal: Option<[f32; 3]>,
    pub activity: Activity,
    pub fire: FireControl,
    pub weapon: Option<WeaponSave>,
    pub magazines: Vec<u32>,
    pub loose_rounds: u32,
    pub frag_grenades: u32,
    pub smoke_grenades: u32,
    pub field_dressings: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeaponSave {
    pub weapon_id: String,
    /// Rounds in the inserted magazine (None = no magazine).
    pub magazine: Option<u32>,
}

#[derive(Debug)]
pub enum SaveError {
    Io(io::Error),
    Format(serde_json::Error),
    /// The file was written by a version of the game this build can't read.
    Version {
        found: u32,
        expected: u32,
    },
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Format(e) => write!(f, "bad save file: {e}"),
            Self::Version { found, expected } => {
                write!(f, "save version {found}, this build reads {expected}")
            }
        }
    }
}

impl std::error::Error for SaveError {}

impl From<io::Error> for SaveError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for SaveError {
    fn from(e: serde_json::Error) -> Self {
        Self::Format(e)
    }
}

// ------------------------------------------------------------------ files

/// Write `save` to `path`. Writes a temporary file first and renames it over
/// the old save, so a crash mid-write never leaves a half-written save.
pub fn write_file(path: &Path, save: &SaveGame) -> Result<(), SaveError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(save)?)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Read a save from `path`. `Ok(None)` if there is no save file yet.
pub fn read_file(path: &Path) -> Result<Option<SaveGame>, SaveError> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let save: SaveGame = serde_json::from_str(&text)?;
    if save.version != SAVE_VERSION {
        return Err(SaveError::Version {
            found: save.version,
            expected: SAVE_VERSION,
        });
    }
    Ok(Some(save))
}

// ------------------------------------------------------------------ world -> save

/// Snapshot the world. `player` is the player-controlled entity.
pub fn capture(world: &mut World, player: Entity) -> SaveGame {
    let clock = *world.resource::<SimClock>();
    let rng_state = world.resource::<SimRng>().state();
    let ids = world.get_resource::<NextIds>().copied().unwrap_or_default();

    let soldier_id = |w: &World, e: Entity| w.get::<Soldier>(e).map(|s| s.id);
    let unit_id = |w: &World, e: Entity| w.get::<Unit>(e).map(|u| u.id);

    let mut uq = world.query::<(
        &Unit,
        &Members,
        &Leader,
        Option<&ParentUnit>,
        Option<&CurrentOrder>,
        (
            Option<&Side>,
            Option<&Controller>,
            Option<&Task>,
            Option<&Plan>,
        ),
    )>();
    let mut units: Vec<UnitSave> = uq
        .iter(world)
        .map(|(u, m, l, p, o, (side, controller, task, plan))| {
            let o = o.copied().unwrap_or_default();
            UnitSave {
                id: u.id,
                kind: u.kind,
                name: u.name.clone(),
                callsign: u.callsign.clone(),
                parent: p.and_then(|p| unit_id(world, p.0)),
                leader: l.0.and_then(|e| soldier_id(world, e)),
                child_units: m.0.iter().filter_map(|e| unit_id(world, *e)).collect(),
                soldiers: m.0.iter().filter_map(|e| soldier_id(world, *e)).collect(),
                order: o.order,
                arrived_reported: o.arrived_reported,
                side: side.copied().unwrap_or_default(),
                controller: controller.copied().unwrap_or_default(),
                task: task.copied().unwrap_or_default(),
                plan: plan.cloned(),
            }
        })
        .collect();
    units.sort_by_key(|u| u.id);

    let mut sq = world.query::<(
        &Soldier,
        &SquadMember,
        &FormationSlot,
        &Transform,
        &Stance,
        &Health,
        &LifeState,
        Option<&MoveGoal>,
        Option<&Activity>,
        Option<&FireControl>,
        Option<&Inventory>,
        Has<PlayerControlled>,
        Has<CommissionedOfficer>,
        Option<&Side>,
    )>();
    let mut soldiers: Vec<SoldierSave> = sq
        .iter(world)
        .map(
            |(s, m, f, t, st, h, l, goal, act, fire, inv, is_player, is_officer, side)| {
                let inv = inv.cloned().unwrap_or_default();
                let weapon = inv
                    .primary
                    .and_then(|w| world.get::<WeaponState>(w))
                    .map(|w| WeaponSave {
                        weapon_id: w.weapon_id.clone(),
                        magazine: w.magazine,
                    });
                SoldierSave {
                    id: s.id,
                    side: side.copied().unwrap_or_default(),
                    name: s.name.clone(),
                    rank: s.rank,
                    role: s.role,
                    is_player,
                    is_officer,
                    squad: unit_id(world, m.squad).unwrap_or(0),
                    fire_team: m.fire_team,
                    slot: m.slot,
                    formation: [f.right, f.forward],
                    transform: *t,
                    stance: *st,
                    health: *h,
                    life: *l,
                    move_goal: goal.and_then(|g| g.0),
                    activity: act.copied().unwrap_or_default(),
                    fire: fire.copied().unwrap_or_default(),
                    weapon,
                    magazines: inv.magazines,
                    loose_rounds: inv.loose_rounds,
                    frag_grenades: inv.frag_grenades,
                    smoke_grenades: inv.smoke_grenades,
                    field_dressings: inv.field_dressings,
                }
            },
        )
        .collect();
    soldiers.sort_by_key(|s| s.id);

    let p = world.entity(player);
    SaveGame {
        version: SAVE_VERSION,
        tick: clock.tick,
        elapsed: clock.elapsed,
        rng_state,
        next_soldier_id: ids.soldier,
        next_unit_id: ids.unit,
        player: PlayerSave {
            transform: p.get::<Transform>().copied().unwrap_or_default(),
            stance: p.get::<Stance>().copied().unwrap_or_default(),
        },
        units,
        soldiers,
    }
}

// ------------------------------------------------------------------ save -> world

/// Rebuild a save into `world`, which must already hold the sim's resources
/// and the bare `player` entity (see `Sim::from_save`).
pub fn restore(world: &mut World, player: Entity, save: &SaveGame) {
    world.insert_resource(SimClock {
        tick: save.tick,
        dt: 0.0,
        elapsed: save.elapsed,
    });
    world.insert_resource(SimRng::new(save.rng_state));
    world.insert_resource(NextIds {
        soldier: save.next_soldier_id,
        unit: save.next_unit_id,
    });
    world.entity_mut(player).insert((
        save.player.transform,
        Velocity::default(),
        save.player.stance,
        Gait::Still,
    ));

    // Pass 1: spawn every unit and soldier, with links left empty.
    let mut units: HashMap<u32, Entity> = HashMap::new();
    for u in &save.units {
        let mut bundle = UnitBundle::new(u.id, u.kind, &u.name, &u.callsign);
        bundle.order = CurrentOrder {
            order: u.order,
            arrived_reported: u.arrived_reported,
        };
        bundle.side = u.side;
        bundle.controller = u.controller;
        bundle.task = u.task;
        let e = world.spawn(bundle).id();
        if let Some(plan) = &u.plan {
            world.entity_mut(e).insert(plan.clone());
        }
        units.insert(u.id, e);
    }

    let mut soldiers: HashMap<u32, Entity> = HashMap::new();
    for s in &save.soldiers {
        // A soldier must belong to a unit; skip any whose unit is missing.
        let Some(&squad) = units.get(&s.squad) else {
            continue;
        };
        let mut bundle = SoldierBundle::new(
            Soldier {
                id: s.id,
                name: s.name.clone(),
                rank: s.rank,
                role: s.role,
            },
            SquadMember {
                squad,
                fire_team: s.fire_team,
                slot: s.slot,
            },
            FormationSlot {
                right: s.formation[0],
                forward: s.formation[1],
            },
        );
        bundle.health = s.health;
        bundle.life = s.life;
        bundle.goal = MoveGoal(s.move_goal);
        bundle.activity = s.activity;
        bundle.fire = s.fire;
        bundle.side = s.side;

        let e = if s.is_player {
            // The player keeps the body the client reports.
            world.entity_mut(player).insert(bundle);
            player
        } else {
            world
                .spawn((
                    bundle,
                    BodyBundle {
                        transform: s.transform,
                        stance: s.stance,
                        ..Default::default()
                    },
                ))
                .id()
        };
        if s.is_officer {
            world.entity_mut(e).insert(CommissionedOfficer);
        }

        // Weapons of a type no longer in weapons.json are dropped.
        let primary = s.weapon.as_ref().and_then(|w| {
            let spec = weapon_spec(&w.weapon_id)?;
            Some(
                world
                    .spawn(WeaponBundle {
                        state: WeaponState {
                            weapon_id: w.weapon_id.clone(),
                            magazine_capacity: spec.magazine_capacity,
                            magazine: w.magazine.map(|r| r.min(spec.magazine_capacity)),
                        },
                        holder: Holder(Some(e)),
                    })
                    .id(),
            )
        });
        world.entity_mut(e).insert(Inventory {
            primary,
            magazines: s.magazines.clone(),
            loose_rounds: s.loose_rounds,
            frag_grenades: s.frag_grenades,
            smoke_grenades: s.smoke_grenades,
            field_dressings: s.field_dressings,
        });
        soldiers.insert(s.id, e);
    }

    // Pass 2: reconnect links by id. Ids that don't exist are dropped.
    for u in &save.units {
        let e = units[&u.id];
        let members: Vec<Entity> = u
            .child_units
            .iter()
            .filter_map(|id| units.get(id))
            .chain(u.soldiers.iter().filter_map(|id| soldiers.get(id)))
            .copied()
            .collect();
        let mut entity = world.entity_mut(e);
        entity.insert((
            Members(members),
            Leader(u.leader.and_then(|id| soldiers.get(&id).copied())),
        ));
        if let Some(parent) = u.parent.and_then(|id| units.get(&id)) {
            entity.insert(ParentUnit(*parent));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use crate::components::TaskKind;
    use crate::sim::{Sim, SimConfig};

    use super::*;

    /// A sim with a platoon in the middle of a fire mission and a move.
    fn busy_sim() -> Sim {
        let mut sim = Sim::new(SimConfig::default());
        sim.spawn_platoon([0.0, 0.0, 0.0], 0.0);
        let units = sim.units();
        let id = |name: &str| units.iter().find(|u| u.name == name).unwrap().id;
        sim.order_fire_at(&[id("1 Section")], [0.0, 0.0, -80.0]);
        sim.order_move(&[id("2 Section")], [20.0, 0.0, -30.0], 0.3);
        // An enemy platoon part-way through an AI attack.
        let enemy = sim.spawn_ai_platoon(Side::Red, "Enemy", "E1", [0.0, 0.0, -300.0], PI);
        sim.assign_task(
            enemy,
            TaskKind::Attack {
                objective: [0.0, 0.0, -20.0],
            },
        );
        for _ in 0..90 {
            sim.step();
        }
        sim
    }

    #[test]
    fn save_load_save_is_identical() {
        let mut sim = busy_sim();
        let first = sim.to_save();
        let mut loaded = Sim::from_save(SimConfig::default(), &first);
        assert_eq!(loaded.to_save(), first);
    }

    #[test]
    fn loaded_game_plays_on_exactly_like_the_original() {
        let mut original = busy_sim();
        let mut loaded = Sim::from_save(SimConfig::default(), &original.to_save());
        for _ in 0..300 {
            original.step();
            loaded.step();
        }
        assert_eq!(loaded.to_save(), original.to_save());
        assert_eq!(loaded.soldiers(), original.soldiers());
    }

    #[test]
    fn file_round_trip_and_missing_file() {
        let dir = std::env::temp_dir().join(format!("ltw-save-test-{}", std::process::id()));
        let path = dir.join("save.json");
        assert!(read_file(&path).unwrap().is_none());

        let mut sim = busy_sim();
        let save = sim.to_save();
        write_file(&path, &save).unwrap();
        assert_eq!(read_file(&path).unwrap(), Some(save));
        assert!(!path.with_extension("json.tmp").exists());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn wrong_version_is_refused() {
        let dir = std::env::temp_dir().join(format!("ltw-save-ver-{}", std::process::id()));
        let path = dir.join("save.json");
        let mut save = Sim::new(SimConfig::default()).to_save();
        save.version = SAVE_VERSION + 1;
        write_file(&path, &save).unwrap();
        assert!(matches!(read_file(&path), Err(SaveError::Version { .. })));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn dangling_links_are_dropped_not_fatal() {
        let mut save = busy_sim().to_save();
        let gone = save.soldiers.pop().unwrap().id; // a leader or member vanishes
        let mut sim = Sim::from_save(SimConfig::default(), &save);
        assert!(sim.soldiers().iter().all(|s| s.id != gone));
        assert!(sim.units().iter().all(|u| !u.soldier_ids.contains(&gone)));
    }
}
