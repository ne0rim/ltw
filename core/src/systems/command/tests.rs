//! Tests for AI commanders: every playbook runs end to end.

use std::collections::HashSet;
use std::f32::consts::PI;

use crate::components::{Plan, PlanPhase, Playbook, Route, Side, TaskKind, TaskStage, Unit};
use crate::events::SimEvent;
use crate::rng::SimRng;
use crate::sim::{Sim, SimConfig, UnitInfo};

use super::tuning;

/// A Red AI platoon around (0, 0, -400) facing south, with seed `seed`.
fn platoon(seed: u64) -> (Sim, u32) {
    let mut sim = Sim::new(SimConfig::default());
    sim.world_mut().insert_resource(SimRng::new(seed));
    let id = sim.spawn_ai_platoon(Side::Red, "Enemy", "E1", [0.0, 0.0, -400.0], PI);
    (sim, id)
}

fn run(sim: &mut Sim, seconds: f64) -> Vec<SimEvent> {
    let mut events = Vec::new();
    for _ in 0..(seconds * sim.config.tick_rate_hz) as u32 {
        sim.step();
        events.extend(sim.drain_events());
    }
    events
}

fn unit(sim: &mut Sim, id: u32) -> UnitInfo {
    sim.units().into_iter().find(|u| u.id == id).unwrap()
}

/// The playbook a fresh platoon picks for `task` with `seed`.
fn playbook_for(seed: u64, task: TaskKind) -> Playbook {
    let (mut sim, id) = platoon(seed);
    sim.assign_task(id, task);
    sim.step();
    sim.step();
    unit(&mut sim, id).playbook.unwrap()
}

/// A seed whose platoon picks `want` for `task`.
fn seed_for(task: TaskKind, want: Playbook) -> u64 {
    (1..200)
        .find(|s| playbook_for(*s, task) == want)
        .unwrap_or_else(|| panic!("no seed picks {want:?}"))
}

fn flat_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[test]
fn idle_platoon_forms_an_all_round_harbour_and_holds_fire() {
    let (mut sim, id) = platoon(1);
    let events = run(&mut sim, 60.0);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SimEvent::ShotFired { .. }))
    );
    let p = unit(&mut sim, id);
    assert_eq!(p.playbook, Some(Playbook::Harbour));
    assert!(matches!(p.task, Some(TaskKind::Hold { .. })));
    assert_eq!(p.task_stage, Some(TaskStage::InPosition));

    // Riflemen stand on a circle round the harbour, each facing outward.
    let centre = p.task.unwrap().point();
    let hq = sim
        .units()
        .into_iter()
        .find(|u| u.parent_id == Some(id) && u.name == "Platoon HQ")
        .unwrap()
        .id;
    for s in sim.soldiers().into_iter().filter(|s| s.unit_id != hq) {
        let r = flat_distance(s.position, centre);
        assert!(
            (r - tuning::HARBOUR_RADIUS).abs() < 0.5,
            "{} is {r:.1} m out",
            s.name
        );
        let out = [
            (s.position[0] - centre[0]) / r,
            (s.position[2] - centre[2]) / r,
        ];
        let looking = [-s.yaw.sin(), -s.yaw.cos()];
        let dot = out[0] * looking[0] + out[1] * looking[1];
        assert!(dot > 0.99, "{} faces inward-ish ({dot:.2})", s.name);
    }
}

#[test]
fn defence_can_be_linear_or_all_round() {
    let task = TaskKind::Defend {
        position: [0.0, 0.0, -350.0],
        facing: PI,
    };
    let seen: HashSet<Playbook> = (1..40).map(|s| playbook_for(s, task)).collect();
    assert!(seen.contains(&Playbook::Defend) && seen.contains(&Playbook::AllRoundDefence));

    // An all-round defence gets everyone into place.
    let seed = seed_for(task, Playbook::AllRoundDefence);
    let (mut sim, id) = platoon(seed);
    sim.assign_task(id, task);
    run(&mut sim, 60.0);
    assert_eq!(unit(&mut sim, id).task_stage, Some(TaskStage::InPosition));
}

#[test]
fn long_attacks_use_every_attack_playbook() {
    let task = TaskKind::Attack {
        objective: [0.0, 0.0, 0.0],
    };
    let seen: HashSet<Playbook> = (1..40).map(|s| playbook_for(s, task)).collect();
    for p in [
        Playbook::FireBaseAssault,
        Playbook::TwoUp,
        Playbook::BoundingAdvance,
    ] {
        assert!(seen.contains(&p), "never picked {p:?}");
    }
}

#[test]
fn bounding_advance_leapfrogs_then_takes_the_objective() {
    let objective = [0.0, 0.0, 0.0];
    let task = TaskKind::Attack { objective };
    let seed = seed_for(task, Playbook::BoundingAdvance);
    let (mut sim, id) = platoon(seed);
    sim.assign_task(id, task);
    sim.step();
    sim.step();
    // The lead sections, from the commander's plan.
    let leads: Vec<u32> = {
        let mut q = sim.world_mut().query::<(&Unit, &Plan)>();
        let world = sim.world();
        q.iter(world)
            .find(|(u, _)| u.id == id)
            .map(|(_, p)| p.assault.clone())
            .unwrap()
    };
    assert_eq!(leads.len(), 2);
    let mut bounded = HashSet::new();
    for _ in 0..(600.0 * 30.0) as u32 {
        sim.step();
        let units = sim.units();
        let p = units.iter().find(|u| u.id == id).unwrap();
        if p.plan_phase != Some(PlanPhase::Prepare) {
            break;
        }
        // While bounding, never more than one lead section walks at once.
        let walking: Vec<u32> = units
            .iter()
            .filter(|u| leads.contains(&u.id))
            .filter(|u| u.task_stage == Some(TaskStage::Moving))
            .map(|u| u.id)
            .collect();
        assert!(walking.len() <= 1, "both lead sections moving at once");
        bounded.extend(walking);
    }
    assert_eq!(bounded.len(), 2, "both lead sections should have bounded");
    let events = run(&mut sim, 300.0);
    assert!(events.iter().any(
        |e| matches!(e, SimEvent::TaskComplete { unit_id, task: "attack", .. } if *unit_id == id)
    ));
    for s in sim.soldiers() {
        assert!(flat_distance(s.position, objective) < 70.0, "{}", s.name);
    }
}

#[test]
fn patrol_walks_the_route_again_and_again() {
    let (mut sim, id) = platoon(3);
    let route = Route::new(&[
        [0.0, 0.0, -300.0],
        [100.0, 0.0, -300.0],
        [100.0, 0.0, -400.0],
    ]);
    sim.assign_task(id, TaskKind::Patrol { route, leg: 0 });
    // Track how close the lead section gets to each waypoint over two laps.
    let mut closest = [f32::MAX; 3];
    let mut events = Vec::new();
    for _ in 0..(400.0 * 30.0) as u32 {
        sim.step();
        events.extend(sim.drain_events());
        let lead = sim
            .units()
            .into_iter()
            .find(|u| u.parent_id == Some(id) && u.name == "1 Section")
            .unwrap();
        let centre = sim
            .soldiers()
            .into_iter()
            .filter(|s| s.unit_id == lead.id)
            .fold([0.0; 3], |a, s| {
                [
                    a[0] + s.position[0] / 10.0,
                    0.0,
                    a[2] + s.position[2] / 10.0,
                ]
            });
        for (i, wp) in route.points().iter().enumerate() {
            closest[i] = closest[i].min(flat_distance(centre, *wp));
        }
    }
    assert_eq!(unit(&mut sim, id).playbook, Some(Playbook::Patrol));
    for (i, d) in closest.iter().enumerate() {
        assert!(*d < 10.0, "never reached waypoint {i} (closest {d:.0} m)");
    }
    // Patrols never finish.
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SimEvent::TaskComplete { .. }))
    );
}

#[test]
fn withdraw_bounds_back_then_defends_facing_the_threat() {
    let (mut sim, id) = platoon(5);
    let position = [0.0, 0.0, -600.0];
    // The threat is to the south (+Z), so the rear is north.
    sim.assign_task(
        id,
        TaskKind::Withdraw {
            position,
            facing: PI,
        },
    );
    let events = run(&mut sim, 400.0);
    assert!(
        events.iter().any(
            |e| matches!(e, SimEvent::TaskComplete { unit_id, task: "withdraw", .. } if *unit_id == id)
        ),
        "withdraw never finished"
    );
    for u in sim.units().into_iter().filter(|u| u.parent_id == Some(id)) {
        assert!(
            matches!(u.task, Some(TaskKind::Defend { .. })),
            "{}",
            u.name
        );
        assert_eq!(u.task_stage, Some(TaskStage::InPosition), "{}", u.name);
    }
    for s in sim.soldiers() {
        assert!(flat_distance(s.position, position) < 60.0, "{}", s.name);
        assert!(
            (s.yaw.abs() - PI).abs() < 0.01,
            "{} faces {}",
            s.name,
            s.yaw
        );
    }
}
