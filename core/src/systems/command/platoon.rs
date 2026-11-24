//! AI commanders of units with sub-units (platoons): pick a playbook for the
//! task, give each sub-unit its part, and move the plan through its phases as
//! the sub-units report progress.
//!
//! | Task     | Playbooks (weighted random)                                  |
//! |----------|--------------------------------------------------------------|
//! | none     | Harbour: sections share a perimeter where they stand         |
//! | Hold     | Harbour                                                      |
//! | Defend   | Linear (frontage facing the threat) or All-round (perimeter) |
//! | Attack   | Fire base + assault, Two up, Bounding advance                |
//! | Patrol   | Patrol: the platoon walks the route in a wedge, looping      |
//! | Withdraw | Withdraw: sections bound back in turn, then a linear defence |
//! | other    | Direct: every sub-unit gets the same task                    |
//!
//! Attack, fire base + assault:
//!   Prepare     fire base → SupportByFire; assault → form-up on one flank
//!   Assault     once the fire base is firing and the assault has formed up
//!   Consolidate on the objective: everyone defends round it. Task Done.
//! Attack, two up: two sections assault abreast, one follows. Consolidate.
//! Attack, bounding advance: the two lead sections take turns to bound
//!   forward while the other halts and watches; the third follows. Within
//!   FINAL_ASSAULT_RANGE both lead sections assault. Consolidate.
//!
//! An AI platoon with no task is given `Hold` where it stands, so "idle" is a
//! plan too: all-round security, nobody firing.

use std::collections::HashMap;
use std::f32::consts::TAU;

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
        .map(|u| (u.unit.id, u.entity))
        .collect();
    platoons.sort_unstable();

    for (unit_id, e) in platoons {
        let Ok(p) = units.get(e) else { continue };
        let callsign = p.unit.callsign.clone();
        let subs = sub_units(&units, &p.members.0);
        let plan = p.plan.cloned();
        let task = *p.task;

        // Where each sub-unit is now (centre of its soldiers), and its stage.
        let pos_of = |id: u32| -> Option<[f32; 3]> {
            let m = units.get(*by_id.get(&id)?).ok()?;
            centre(
                m.members
                    .0
                    .iter()
                    .filter_map(|s| soldiers.get(*s).ok())
                    .map(|t| t.position),
            )
        };
        let stage_of = |id: u32| -> Option<TaskStage> {
            units
                .get(*by_id.get(&id)?)
                .ok()
                .and_then(|u| u.task.stage())
        };
        let here = centre(subs.all().filter_map(pos_of));

        // Idle: hold where we stand.
        let (kind, stage) = match task.0 {
            Some(t) => t,
            None => match here {
                Some(position) => (TaskKind::Hold { position }, TaskStage::New),
                None => continue,
            },
        };
        if stage == TaskStage::Done {
            continue;
        }
        let ctx = Ctx {
            subs: &subs,
            here: here.unwrap_or(kind.point()),
            pos_of: &pos_of,
            stage_of: &stage_of,
        };

        // Decide (reading only), then apply.
        let (new_plan, step) = match plan {
            None => {
                let (plan, tasks) = make_plan(kind, &ctx, &mut rng);
                let step = Step {
                    tasks,
                    ..Step::default()
                };
                (plan, step)
            }
            Some(plan) => advance(kind, plan, &ctx),
        };
        let fresh = p.plan.is_none();

        for (id, t) in &step.tasks {
            give(&mut units, &by_id, *id, *t);
        }
        let Ok(mut p) = units.get_mut(e) else {
            continue;
        };
        if task.0.is_none() {
            *p.task = Task::new(kind);
        }
        if fresh {
            events.push(SimEvent::PlanMade {
                unit_id,
                callsign: callsign.clone(),
                playbook: new_plan.playbook.name(),
            });
            p.task.set_stage(TaskStage::Moving);
            commands.entity(e).insert(new_plan);
        } else if let Some(mut current) = p.plan {
            *current = new_plan;
        }
        if step.holding {
            p.task.set_stage(TaskStage::InPosition);
        }
        if step.finished {
            p.task.set_stage(TaskStage::Done);
            events.push(SimEvent::TaskComplete {
                unit_id,
                callsign,
                task: kind.name(),
            });
        }
    }
}

/// What a commander knows while deciding.
struct Ctx<'a> {
    subs: &'a SubUnits,
    /// Centre of the whole unit now.
    here: [f32; 3],
    pos_of: &'a dyn Fn(u32) -> Option<[f32; 3]>,
    stage_of: &'a dyn Fn(u32) -> Option<TaskStage>,
}

impl Ctx<'_> {
    /// Every listed sub-unit has stopped: arrived, holding, firing, finished
    /// (or no longer exists).
    fn settled(&self, ids: &[u32]) -> bool {
        ids.iter().all(|id| {
            matches!(
                (self.stage_of)(*id),
                None | Some(TaskStage::Done | TaskStage::InPosition | TaskStage::Firing)
            )
        })
    }
    fn all_done(&self, ids: &[u32]) -> bool {
        ids.iter()
            .all(|id| matches!((self.stage_of)(*id), Some(TaskStage::Done) | None))
    }
    fn all_in_position(&self, ids: &[u32]) -> bool {
        ids.iter()
            .all(|id| matches!((self.stage_of)(*id), Some(TaskStage::InPosition) | None))
    }
}

/// What a commander decided this tick.
#[derive(Default)]
struct Step {
    tasks: Vec<(u32, TaskKind)>,
    /// Every sub-unit is in place (defences, harbour).
    holding: bool,
    /// The task is achieved.
    finished: bool,
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
    fn all_ids(&self) -> Vec<u32> {
        self.all().collect()
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

// ---------------------------------------------------------------- planning

/// Choose a playbook for `kind` and the sub-units' first tasks.
fn make_plan(kind: TaskKind, ctx: &Ctx, rng: &mut SimRng) -> (Plan, Vec<(u32, TaskKind)>) {
    let subs = ctx.subs;
    match kind {
        TaskKind::Hold { position } => {
            // Face the way the sections already are from the HQ.
            let front = subs
                .hq
                .and_then(ctx.pos_of)
                .map_or([0.0, -1.0], |hq| direction(hq, ctx.here));
            let plan = Plan::new(Playbook::Harbour, PlanPhase::Consolidate, subs.hq, front);
            (
                plan,
                perimeter(position, tuning::HARBOUR_RADIUS, front, ctx),
            )
        }
        TaskKind::Defend { position, facing } => {
            let front = forward(facing);
            let playbook = pick(
                rng,
                &[(Playbook::Defend, 2), (Playbook::AllRoundDefence, 1)],
            );
            let tasks = if playbook == Playbook::Defend {
                frontage(position, front, subs)
            } else {
                perimeter(position, tuning::ALL_ROUND_RADIUS, front, ctx)
            };
            (
                Plan::new(playbook, PlanPhase::Consolidate, subs.hq, front),
                tasks,
            )
        }
        TaskKind::Attack { objective } => plan_attack(objective, ctx, rng),
        TaskKind::Patrol { route, .. } => {
            let mut plan = Plan::new(Playbook::Patrol, PlanPhase::Prepare, subs.hq, [0.0, -1.0]);
            plan.leg = 0;
            let to = route.get(0);
            plan.axis = direction(ctx.here, to);
            let tasks = wedge(to, plan.axis, subs);
            (plan, tasks)
        }
        TaskKind::Withdraw { position, facing } => {
            let axis = direction(ctx.here, position);
            let mut plan = Plan::new(Playbook::Withdraw, PlanPhase::Prepare, subs.hq, axis);
            plan.start = ctx.here;
            plan.assault = subs.sections.clone();
            let mut tasks = Vec::new();
            if let Some(t) = bound(&mut plan, position, facing, 0.0) {
                tasks.push(t);
            }
            // The HQ goes straight back.
            if let Some(hq) = subs.hq {
                tasks.push((
                    hq,
                    TaskKind::MoveTo {
                        position: offset(position, forward(facing), 30.0, 0.0),
                        facing,
                    },
                ));
            }
            (plan, tasks)
        }
        TaskKind::MoveTo { .. } | TaskKind::SupportByFire { .. } | TaskKind::Perimeter { .. } => {
            let tasks = subs.all().map(|id| (id, kind)).collect();
            (
                Plan::new(
                    Playbook::Direct,
                    PlanPhase::Consolidate,
                    subs.hq,
                    [0.0, -1.0],
                ),
                tasks,
            )
        }
    }
}

fn plan_attack(objective: [f32; 3], ctx: &Ctx, rng: &mut SimRng) -> (Plan, Vec<(u32, TaskKind)>) {
    let subs = ctx.subs;
    let dir = direction(ctx.here, objective);
    let face = facing(dir);
    let range = distance(ctx.here, objective);
    let mut tasks = Vec::new();

    // Weighted choice: fire bases and bounding suit long approaches, a direct
    // two-up assault suits short ones. Any of them can happen.
    let options: &[(Playbook, u32)] = if subs.sections.len() < 2 {
        &[(Playbook::TwoUp, 1)]
    } else if range > tuning::VERY_LONG_ATTACK {
        &[
            (Playbook::FireBaseAssault, 3),
            (Playbook::BoundingAdvance, 3),
            (Playbook::TwoUp, 1),
        ]
    } else if range > tuning::LONG_ATTACK {
        &[
            (Playbook::FireBaseAssault, 3),
            (Playbook::BoundingAdvance, 2),
            (Playbook::TwoUp, 1),
        ]
    } else {
        &[(Playbook::FireBaseAssault, 1), (Playbook::TwoUp, 3)]
    };
    let mut plan = Plan::new(pick(rng, options), PlanPhase::Prepare, subs.hq, dir);
    plan.start = ctx.here;

    match plan.playbook {
        Playbook::FireBaseAssault => {
            let support = offset(objective, dir, tuning::FIRE_BASE_RANGE, 0.0);
            // The section nearest the fire base position, give or take 25 m.
            let fire_base = subs
                .sections
                .iter()
                .map(|id| {
                    let d = (ctx.pos_of)(*id).map_or(f32::MAX, |p| distance(p, support));
                    (d + rng.unit() * 25.0, *id)
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, id)| id)
                .expect("at least two sections");
            plan.fire_base = Some(fire_base);
            plan.assault = subs
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
            for (i, id) in plan.assault.iter().enumerate() {
                let along = spread(i, plan.assault.len());
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
                tasks.push((hq, move_to(offset(support, dir, 30.0, 0.0), face)));
            }
        }
        Playbook::BoundingAdvance => {
            let (lead, reserve) = lead_and_reserve(objective, ctx);
            plan.assault = lead;
            plan.reserve = reserve;
            if let Some(t) = bound(&mut plan, objective, face, tuning::FINAL_ASSAULT_RANGE) {
                tasks.push(t);
                tasks.extend(follow(&plan, subs));
            }
        }
        _ => {
            // Two up, one back.
            plan.phase = PlanPhase::Assault;
            let (lead, reserve) = lead_and_reserve(objective, ctx);
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
                tasks.push((r, move_to(behind, face)));
            }
            if let Some(hq) = subs.hq {
                tasks.push((hq, move_to(offset(behind, dir, 20.0, 0.0), face)));
            }
            plan.assault = lead;
            plan.reserve = reserve;
        }
    }
    (plan, tasks)
}

// ---------------------------------------------------------------- progress

/// Move a running plan on when its sub-units are ready.
fn advance(kind: TaskKind, mut plan: Plan, ctx: &Ctx) -> (Plan, Step) {
    let mut step = Step::default();
    let subs = ctx.subs;
    match (plan.playbook, plan.phase, kind) {
        // Fire base + assault: go once the fire base is firing and the assault
        // has formed up.
        (Playbook::FireBaseAssault, PlanPhase::Prepare, TaskKind::Attack { objective }) => {
            let fire_base_ready = plan
                .fire_base
                .is_none_or(|id| matches!((ctx.stage_of)(id), Some(TaskStage::Firing) | None));
            if fire_base_ready && ctx.all_done(&plan.assault) {
                step.tasks = assault(objective, &plan);
                // Shift fire beyond the objective so the assault isn't hit.
                if let Some(fb) = plan.fire_base {
                    step.tasks.push((
                        fb,
                        TaskKind::SupportByFire {
                            position: offset(objective, plan.axis, tuning::FIRE_BASE_RANGE, 0.0),
                            target: offset(objective, plan.axis, -tuning::SHIFT_FIRE_BEYOND, 0.0),
                        },
                    ));
                }
                plan.phase = PlanPhase::Assault;
            }
        }
        // Bounding: when the bounder has stopped, the next one goes; near the
        // objective, everyone in the lead assaults.
        (Playbook::BoundingAdvance, PlanPhase::Prepare, TaskKind::Attack { objective }) => {
            if ctx.settled(&plan.assault) {
                let left = distance(plan.start, objective) - plan.progress;
                if left <= tuning::FINAL_ASSAULT_RANGE {
                    step.tasks = assault(objective, &plan);
                    plan.phase = PlanPhase::Assault;
                } else if let face = facing(plan.axis)
                    && let Some(t) = bound(&mut plan, objective, face, tuning::FINAL_ASSAULT_RANGE)
                {
                    step.tasks.push(t);
                    step.tasks.extend(follow(&plan, subs));
                }
            }
        }
        // Any attack: once the assault is on the objective, hold round it.
        (_, PlanPhase::Assault, TaskKind::Attack { objective }) => {
            if ctx.all_done(&plan.assault) {
                step.tasks = consolidate(objective, plan.axis, subs);
                plan.phase = PlanPhase::Consolidate;
                step.finished = true;
            }
        }
        (Playbook::Patrol, _, TaskKind::Patrol { route, .. }) => {
            if ctx.all_done(&subs.all_ids()) && !route.is_empty() {
                let from = route.get(plan.leg as usize);
                plan.leg = ((plan.leg as usize + 1) % route.len()) as u32;
                let to = route.get(plan.leg as usize);
                plan.axis = direction(from, to);
                step.tasks = wedge(to, plan.axis, subs);
            }
        }
        (Playbook::Withdraw, PlanPhase::Prepare, TaskKind::Withdraw { position, facing }) => {
            if ctx.settled(&plan.assault) {
                let left = distance(plan.start, position) - plan.progress;
                if left <= tuning::BOUND {
                    step.tasks = frontage(position, forward(facing), subs);
                    plan.phase = PlanPhase::Consolidate;
                } else if let Some(t) = bound(&mut plan, position, facing, 0.0) {
                    step.tasks.push(t);
                }
            }
        }
        (Playbook::Withdraw, PlanPhase::Consolidate, _) => {
            if ctx.all_in_position(&subs.all_ids()) {
                step.finished = true;
            }
        }
        // Defences and harbours: report when everyone is in place.
        (Playbook::Defend | Playbook::AllRoundDefence | Playbook::Harbour, _, _) => {
            step.holding = ctx.all_in_position(&subs.all_ids());
        }
        _ => {}
    }
    (plan, step)
}

// ---------------------------------------------------------------- building blocks

/// The next bound of a leapfrog toward `target`. The lead sections take turns
/// (in `plan.assault` order); each bound puts the mover `tuning::BOUND` metres
/// past the furthest line so far, stopping `stop_short` metres from the target.
/// Halted sections face `halt_facing`. The reserve and HQ follow behind.
fn bound(
    plan: &mut Plan,
    target: [f32; 3],
    halt_facing: f32,
    stop_short: f32,
) -> Option<(u32, TaskKind)> {
    let leads = &plan.assault;
    if leads.is_empty() {
        return None;
    }
    let i = plan
        .last_bounder
        .and_then(|b| leads.iter().position(|x| *x == b))
        .map_or(0, |i| (i + 1) % leads.len());
    let total = distance(plan.start, target);
    plan.progress = (plan.progress + tuning::BOUND).min((total - stop_short).max(0.0));
    let line = offset(
        plan.start,
        plan.axis,
        -plan.progress,
        spread(i, leads.len()),
    );
    plan.last_bounder = Some(leads[i]);
    Some((
        leads[i],
        TaskKind::MoveTo {
            position: line,
            facing: halt_facing,
        },
    ))
}

/// The reserve and HQ keep `RESERVE_BACK` metres behind the furthest bound.
fn follow(plan: &Plan, subs: &SubUnits) -> Vec<(u32, TaskKind)> {
    let face = facing(plan.axis);
    let line = |back: f32| offset(plan.start, plan.axis, -(plan.progress - back).max(0.0), 0.0);
    let mut tasks = Vec::new();
    if let Some(r) = plan.reserve {
        tasks.push((r, move_to(line(tuning::RESERVE_BACK), face)));
    }
    if let Some(hq) = subs.hq {
        tasks.push((hq, move_to(line(tuning::RESERVE_BACK + 20.0), face)));
    }
    tasks
}

/// The lead sections assault the objective side by side.
fn assault(objective: [f32; 3], plan: &Plan) -> Vec<(u32, TaskKind)> {
    plan.assault
        .iter()
        .enumerate()
        .map(|(i, id)| {
            (
                *id,
                TaskKind::Attack {
                    objective: offset(objective, plan.axis, 0.0, spread(i, plan.assault.len())),
                },
            )
        })
        .collect()
}

/// The two sections nearest the objective lead; the third is the reserve.
fn lead_and_reserve(objective: [f32; 3], ctx: &Ctx) -> (Vec<u32>, Option<u32>) {
    let mut by_range = ctx.subs.sections.clone();
    by_range.sort_by(|a, b| {
        let d = |id: &u32| (ctx.pos_of)(*id).map_or(f32::MAX, |p| distance(p, objective));
        d(a).total_cmp(&d(b))
    });
    let lead = by_range.iter().take(2).copied().collect();
    (lead, by_range.get(2).copied())
}

/// Sections across a frontage at `position` facing along `front`, the centre
/// one a little forward, the HQ behind.
fn frontage(position: [f32; 3], front: [f32; 2], subs: &SubUnits) -> Vec<(u32, TaskKind)> {
    let face = facing(front);
    let rights = [-tuning::DEFENCE_FRONTAGE, 0.0, tuning::DEFENCE_FRONTAGE];
    let mut tasks: Vec<(u32, TaskKind)> = subs
        .sections
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let right = rights.get(i).copied().unwrap_or(0.0);
            let back = if right == 0.0 { -10.0 } else { 0.0 };
            (
                *id,
                TaskKind::Defend {
                    position: offset(position, front, back, right),
                    facing: face,
                },
            )
        })
        .collect();
    if let Some(hq) = subs.hq {
        tasks.push((
            hq,
            TaskKind::Defend {
                position: offset(position, front, 30.0, 0.0),
                facing: face,
            },
        ));
    }
    tasks
}

/// All-round: the sections share a circle of `radius` round `centre`, each
/// taking the arc nearest where it is now (so nobody crosses the middle), the
/// first arc centred on `front`. The HQ holds a small circle in the middle.
fn perimeter(
    centre_point: [f32; 3],
    radius: f32,
    front: [f32; 2],
    ctx: &Ctx,
) -> Vec<(u32, TaskKind)> {
    let subs = ctx.subs;
    let n = subs.sections.len().max(1);
    let share = TAU / n as f32;
    let front_yaw = facing(front);
    // Clockwise angle of each section from the front, to hand out arcs in order.
    let mut order: Vec<(f32, u32)> = subs
        .sections
        .iter()
        .map(|id| {
            let bearing =
                (ctx.pos_of)(*id).map_or(front_yaw, |p| facing(direction(centre_point, p)));
            ((front_yaw - bearing + share * 0.5).rem_euclid(TAU), *id)
        })
        .collect();
    order.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut tasks: Vec<(u32, TaskKind)> = order
        .iter()
        .enumerate()
        .map(|(i, (_, id))| {
            let arc_centre = front_yaw - share * i as f32;
            (
                *id,
                TaskKind::Perimeter {
                    centre: centre_point,
                    radius,
                    start: arc_centre + share * 0.5,
                    sweep: share,
                },
            )
        })
        .collect();
    if let Some(hq) = subs.hq {
        tasks.push((
            hq,
            TaskKind::Perimeter {
                centre: centre_point,
                radius: tuning::HQ_HOLD_RADIUS,
                start: front_yaw,
                sweep: TAU,
            },
        ));
    }
    tasks
}

/// Patrol formation: the first section leads at the waypoint, the others
/// follow left and right behind it, the HQ in the middle.
fn wedge(waypoint: [f32; 3], dir: [f32; 2], subs: &SubUnits) -> Vec<(u32, TaskKind)> {
    let face = facing(dir);
    let gap = tuning::PATROL_SPACING;
    let slots = [(0.0, 0.0), (gap, -gap), (gap, gap)];
    let mut tasks: Vec<(u32, TaskKind)> = subs
        .sections
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let (back, right) = slots.get(i).copied().unwrap_or((gap * 2.0, 0.0));
            (*id, move_to(offset(waypoint, dir, back, right), face))
        })
        .collect();
    if let Some(hq) = subs.hq {
        tasks.push((hq, move_to(offset(waypoint, dir, gap, 0.0), face)));
    }
    tasks
}

/// Objective taken: sections hold round it facing onward, HQ just behind.
fn consolidate(objective: [f32; 3], dir: [f32; 2], subs: &SubUnits) -> Vec<(u32, TaskKind)> {
    frontage(objective, dir, subs)
}

fn move_to(position: [f32; 3], facing: f32) -> TaskKind {
    TaskKind::MoveTo { position, facing }
}

/// Direction (x, z) a yaw looks along.
fn forward(yaw: f32) -> [f32; 2] {
    [-yaw.sin(), -yaw.cos()]
}

/// Lateral offset for the i-th of n units side by side, centred.
fn spread(i: usize, n: usize) -> f32 {
    (i as f32 - (n as f32 - 1.0) * 0.5) * tuning::SECTION_SPACING
}

/// Weighted random choice.
fn pick<T: Copy>(rng: &mut SimRng, options: &[(T, u32)]) -> T {
    let total: u32 = options.iter().map(|(_, w)| w).sum();
    let mut r = rng.below(total.max(1));
    for (t, w) in options {
        if r < *w {
            return *t;
        }
        r -= w;
    }
    options[0].0
}

/// Give a sub-unit a new task (replacing what it was doing).
fn give(units: &mut Query<CommandUnit>, by_id: &HashMap<u32, Entity>, id: u32, task: TaskKind) {
    if let Some(Ok(mut u)) = by_id.get(&id).map(|e| units.get_mut(*e)) {
        *u.task = Task::new(task);
    }
}
