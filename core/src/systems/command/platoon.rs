//! AI commanders of units with sub-units (platoons): choose a playbook for the
//! task, give each sub-unit its part, and move the plan through its phases as
//! the sub-units report progress.
//!
//! Attack, "fire base + assault":
//!   Prepare     fire base → SupportByFire from FIRE_BASE_RANGE back;
//!               assault sections → MoveTo a form-up point on one flank;
//!               HQ → MoveTo behind the fire base
//!   Assault     when the fire base is firing and the assault has formed up:
//!               assault sections → Attack the objective
//!   Consolidate when the assault is on the objective: everyone → Defend
//!               around it. The platoon's task is Done.
//!
//! Attack, "two up": two sections → Attack abreast, the third follows in
//!   reserve, then Consolidate as above.
//!
//! Defend: sections across a frontage facing the threat, HQ behind.
//!
//! Which attack playbook, which section is the fire base and which flank to
//! use are weighted random choices, so the same mission plays out differently.

use std::collections::HashMap;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QueryData, With};
use bevy_ecs::system::{Commands, Query, ResMut};

use crate::components::{
    Controller, Members, Plan, PlanPhase, Playbook, Soldier, Task, TaskKind, TaskStage, Transform,
    Unit, UnitKind,
};
use crate::events::{SimEvent, SimEvents};
use crate::rng::SimRng;

use super::geometry::{centre, direction, distance, facing, offset};
use super::tuning;

/// Any unit, as a commander or a sub-unit.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct CommandUnit {
    entity: Entity,
    unit: &'static Unit,
    controller: &'static Controller,
    members: &'static Members,
    task: &'static mut Task,
    plan: Option<&'static mut Plan>,
}

pub fn command_platoons(
    mut commands: Commands,
    mut rng: ResMut<SimRng>,
    mut events: ResMut<SimEvents>,
    mut units: Query<CommandUnit>,
    soldiers: Query<&Transform, With<Soldier>>,
) {
    let by_id: HashMap<u32, Entity> = units.iter().map(|u| (u.unit.id, u.entity)).collect();
    let mut platoons: Vec<(u32, Entity)> = units
        .iter()
        .filter(|u| *u.controller == Controller::Ai && u.unit.kind == UnitKind::Platoon)
        .filter(|u| u.task.0.is_some())
        .map(|u| (u.unit.id, u.entity))
        .collect();
    platoons.sort_unstable();

    for (_, e) in platoons {
        let Ok(p) = units.get(e) else { continue };
        let Some((kind, stage)) = p.task.0 else {
            continue;
        };
        if stage == TaskStage::Done {
            continue;
        }
        let callsign = p.unit.callsign.clone();
        let unit_id = p.unit.id;
        let subs = sub_units(&units, &p.members.0);
        let plan = p.plan.cloned();

        // Where each sub-unit is now (centre of its soldiers).
        let pos_of = |id: u32| -> Option<[f32; 3]> {
            let ue = by_id.get(&id)?;
            let m = units.get(*ue).ok()?;
            centre(
                m.members
                    .0
                    .iter()
                    .filter_map(|s| soldiers.get(*s).ok())
                    .map(|t| t.position),
            )
        };
        let start = centre(subs.all().filter_map(pos_of)).unwrap_or(kind.point());

        let Some(plan) = plan else {
            // No plan yet: choose one and hand out the first tasks.
            let (plan, tasks) = make_plan(kind, start, &subs, &pos_of, &mut rng);
            for (id, task) in tasks {
                give(&mut units, &by_id, id, task);
            }
            events.push(SimEvent::PlanMade {
                unit_id,
                callsign,
                playbook: plan.playbook.name(),
            });
            commands.entity(e).insert(plan);
            if let Ok(mut p) = units.get_mut(e) {
                p.task.set_stage(TaskStage::Moving);
            }
            continue;
        };

        // A plan is running: advance it when the sub-units are ready.
        let stage_of = |id: u32| -> Option<TaskStage> {
            by_id
                .get(&id)
                .and_then(|e| units.get(*e).ok())
                .and_then(|u| u.task.stage())
        };
        // Missing sub-units (none yet, later: destroyed) don't hold the plan up.
        let all_done = |ids: &[u32]| {
            ids.iter()
                .all(|id| matches!(stage_of(*id), Some(TaskStage::Done) | None))
        };

        let mut next = plan.clone();
        let mut tasks: Vec<(u32, TaskKind)> = Vec::new();
        let mut finished = false;
        match (kind, plan.phase) {
            (TaskKind::Attack { objective }, PlanPhase::Prepare) => {
                let fire_base_ready = plan
                    .fire_base
                    .is_none_or(|id| matches!(stage_of(id), Some(TaskStage::Firing) | None));
                if fire_base_ready && all_done(&plan.assault) {
                    let dir = plan.axis;
                    for (i, id) in plan.assault.iter().enumerate() {
                        let spread = spread(i, plan.assault.len());
                        tasks.push((
                            *id,
                            TaskKind::Attack {
                                objective: offset(objective, dir, 0.0, spread),
                            },
                        ));
                    }
                    next.phase = PlanPhase::Assault;
                }
            }
            (TaskKind::Attack { objective }, PlanPhase::Assault) => {
                if all_done(&plan.assault) {
                    tasks.extend(consolidate(objective, plan.axis, &subs));
                    next.phase = PlanPhase::Consolidate;
                    finished = true;
                }
            }
            (TaskKind::Defend { .. }, _) => {
                let ids: Vec<u32> = subs.all().collect();
                let ready = ids
                    .iter()
                    .all(|id| matches!(stage_of(*id), Some(TaskStage::InPosition) | None));
                if ready && let Ok(mut p) = units.get_mut(e) {
                    p.task.set_stage(TaskStage::InPosition);
                }
            }
            _ => {}
        }

        for (id, task) in tasks {
            give(&mut units, &by_id, id, task);
        }
        if let Ok(mut p) = units.get_mut(e) {
            if let Some(mut current) = p.plan {
                *current = next;
            }
            if finished {
                p.task.set_stage(TaskStage::Done);
                events.push(SimEvent::TaskComplete {
                    unit_id,
                    callsign,
                    task: kind.name(),
                });
            }
        }
    }
}

/// A commander's sub-units by type, as unit ids in order.
struct SubUnits {
    sections: Vec<u32>,
    hq: Option<u32>,
}

impl SubUnits {
    fn all(&self) -> impl Iterator<Item = u32> + '_ {
        self.sections.iter().copied().chain(self.hq)
    }
}

fn sub_units(units: &Query<CommandUnit>, members: &[Entity]) -> SubUnits {
    let mut sections = Vec::new();
    let mut hq = None;
    for m in members {
        if let Ok(u) = units.get(*m) {
            match u.unit.kind {
                UnitKind::RifleSection => sections.push(u.unit.id),
                UnitKind::PlatoonHeadquarters => hq = Some(u.unit.id),
                UnitKind::Platoon => {}
            }
        }
    }
    sections.sort_unstable();
    SubUnits { sections, hq }
}

/// Choose a playbook for `kind` and the sub-units' first tasks.
fn make_plan(
    kind: TaskKind,
    start: [f32; 3],
    subs: &SubUnits,
    pos_of: &dyn Fn(u32) -> Option<[f32; 3]>,
    rng: &mut SimRng,
) -> (Plan, Vec<(u32, TaskKind)>) {
    let mut tasks = Vec::new();
    match kind {
        TaskKind::Attack { objective } => {
            let dir = direction(start, objective);
            let face = facing(dir);
            let long = distance(start, objective) > tuning::LONG_ATTACK;
            // Weighted choice: a fire base is favoured for long attacks, a
            // direct assault for short ones, but either can happen.
            let fire_base_weight = if long { 3 } else { 1 };
            let use_fire_base = subs.sections.len() >= 2 && rng.below(4) < fire_base_weight;

            if use_fire_base {
                let support = offset(objective, dir, tuning::FIRE_BASE_RANGE, 0.0);
                // The fire base is the section nearest its position, give or
                // take up to 25 m, so it isn't always the same one.
                let fire_base = subs
                    .sections
                    .iter()
                    .map(|id| {
                        let d = pos_of(*id).map_or(f32::MAX, |p| distance(p, support));
                        (d + rng.unit() * 25.0, *id)
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, id)| id)
                    .expect("at least two sections");
                let assault: Vec<u32> = subs
                    .sections
                    .iter()
                    .copied()
                    .filter(|id| *id != fire_base)
                    .collect();
                let flank = if rng.below(2) == 0 { -1.0 } else { 1.0 };
                tasks.push((
                    fire_base,
                    TaskKind::SupportByFire {
                        position: support,
                        target: objective,
                    },
                ));
                for (i, id) in assault.iter().enumerate() {
                    let along = spread(i, assault.len());
                    tasks.push((
                        *id,
                        TaskKind::MoveTo {
                            position: offset(
                                objective,
                                dir,
                                tuning::FORM_UP_BACK + along * 0.5,
                                flank * tuning::FORM_UP_FLANK + along,
                            ),
                            facing: face,
                        },
                    ));
                }
                if let Some(hq) = subs.hq {
                    tasks.push((
                        hq,
                        TaskKind::MoveTo {
                            position: offset(support, dir, 30.0, 0.0),
                            facing: face,
                        },
                    ));
                }
                let plan = Plan {
                    playbook: Playbook::FireBaseAssault,
                    phase: PlanPhase::Prepare,
                    fire_base: Some(fire_base),
                    assault,
                    reserve: None,
                    hq: subs.hq,
                    axis: dir,
                };
                (plan, tasks)
            } else {
                // Two up, one back: the two sections nearest the objective lead.
                let mut by_range = subs.sections.clone();
                by_range.sort_by(|a, b| {
                    let d = |id: &u32| pos_of(*id).map_or(f32::MAX, |p| distance(p, objective));
                    d(a).total_cmp(&d(b))
                });
                let lead: Vec<u32> = by_range.iter().take(2).copied().collect();
                let reserve = by_range.get(2).copied();
                for (i, id) in lead.iter().enumerate() {
                    tasks.push((
                        *id,
                        TaskKind::Attack {
                            objective: offset(objective, dir, 0.0, spread(i, lead.len())),
                        },
                    ));
                }
                let behind = offset(objective, dir, tuning::RESERVE_BACK, 0.0);
                if let Some(r) = reserve {
                    tasks.push((
                        r,
                        TaskKind::MoveTo {
                            position: behind,
                            facing: face,
                        },
                    ));
                }
                if let Some(hq) = subs.hq {
                    tasks.push((
                        hq,
                        TaskKind::MoveTo {
                            position: offset(behind, dir, 20.0, 0.0),
                            facing: face,
                        },
                    ));
                }
                let plan = Plan {
                    playbook: Playbook::TwoUp,
                    phase: PlanPhase::Assault,
                    fire_base: None,
                    assault: lead,
                    reserve,
                    hq: subs.hq,
                    axis: dir,
                };
                (plan, tasks)
            }
        }
        TaskKind::Defend { position, facing } => {
            let dir = [-facing.sin(), -facing.cos()];
            let rights = [-tuning::DEFENCE_FRONTAGE, 0.0, tuning::DEFENCE_FRONTAGE];
            for (i, id) in subs.sections.iter().enumerate() {
                let right = rights.get(i).copied().unwrap_or(0.0);
                // The centre section sits a little forward.
                let back = if right == 0.0 { -10.0 } else { 0.0 };
                tasks.push((
                    *id,
                    TaskKind::Defend {
                        position: offset(position, dir, back, right),
                        facing,
                    },
                ));
            }
            if let Some(hq) = subs.hq {
                tasks.push((
                    hq,
                    TaskKind::Defend {
                        position: offset(position, dir, 30.0, 0.0),
                        facing,
                    },
                ));
            }
            let plan = Plan {
                playbook: Playbook::Defend,
                phase: PlanPhase::Consolidate,
                fire_base: None,
                assault: Vec::new(),
                reserve: None,
                hq: subs.hq,
                axis: dir,
            };
            (plan, tasks)
        }
        // A platoon given a move or fire task just passes it to every sub-unit.
        TaskKind::MoveTo { .. } | TaskKind::SupportByFire { .. } => {
            for id in subs.all() {
                tasks.push((id, kind));
            }
            let plan = Plan {
                playbook: Playbook::Defend,
                phase: PlanPhase::Consolidate,
                fire_base: None,
                assault: Vec::new(),
                reserve: None,
                hq: subs.hq,
                axis: [0.0, -1.0],
            };
            (plan, tasks)
        }
    }
}

/// Objective taken: sections hold around it facing onward, HQ just behind.
fn consolidate(objective: [f32; 3], dir: [f32; 2], subs: &SubUnits) -> Vec<(u32, TaskKind)> {
    let face = facing(dir);
    let mut tasks = Vec::new();
    let rights = [-tuning::DEFENCE_FRONTAGE, 0.0, tuning::DEFENCE_FRONTAGE];
    for (i, id) in subs.sections.iter().enumerate() {
        let right = rights.get(i).copied().unwrap_or(0.0);
        tasks.push((
            *id,
            TaskKind::Defend {
                position: offset(objective, dir, -10.0, right),
                facing: face,
            },
        ));
    }
    if let Some(hq) = subs.hq {
        tasks.push((
            hq,
            TaskKind::Defend {
                position: offset(objective, dir, 25.0, 0.0),
                facing: face,
            },
        ));
    }
    tasks
}

/// Lateral offset for the i-th of n units side by side, centred.
fn spread(i: usize, n: usize) -> f32 {
    (i as f32 - (n as f32 - 1.0) * 0.5) * tuning::SECTION_SPACING
}

/// Give a sub-unit a new task (replacing what it was doing).
fn give(units: &mut Query<CommandUnit>, by_id: &HashMap<u32, Entity>, id: u32, task: TaskKind) {
    if let Some(Ok(mut u)) = by_id.get(&id).map(|e| units.get_mut(*e)) {
        *u.task = Task::new(task);
    }
}
