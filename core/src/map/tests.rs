//! Tests for maps: loading the hill map, terrain and objects stopping rounds,
//! and the enemy defending from its barricades.

use std::f32::consts::{FRAC_1_SQRT_2, PI};

use crate::components::{Controller, LifeState, Playbook, Side, Stance, TaskKind, TaskStage};
use crate::data;
use crate::events::SimEvent;
use crate::sim::{Sim, SimConfig};

use super::{Heightfield, MapObject, ObjectKind, yaw_from_heading};

fn run(sim: &mut Sim, seconds: f64) -> Vec<SimEvent> {
    let mut events = Vec::new();
    for _ in 0..(seconds * sim.config.tick_rate_hz) as u32 {
        sim.step();
        events.extend(sim.drain_events());
    }
    events
}

fn flat(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[test]
fn hill_map_rises_100_m_to_a_peak_in_the_north_east_corner() {
    let map = data::map("hill").expect("the hill map is embedded");
    assert_eq!(map.size_m, 1024.0);
    let h = map.heights.as_ref().expect("it has terrain");
    assert_eq!((h.samples, h.cell_m), (2049, 0.5));
    // Summit: north-east quarter (+x, -z), about 100 m up.
    let top = map.height_at(440.0, -440.0);
    assert!((top - 100.0).abs() < 5.0, "summit {top:.1} m");
    // The rise covers most of the map: the centre is already well up it...
    let middle = map.height_at(0.0, 0.0);
    assert!((15.0..60.0).contains(&middle), "centre {middle:.1} m");
    // ...and only the far south-west corner is flat, low ground.
    let low = map.height_at(-480.0, 480.0);
    assert!(low.abs() < 6.0, "south-west corner {low:.1} m");
    // Bumpy, not smooth: along most 20 m stretches the ground strays from a
    // straight line by more than 0.25 m somewhere.
    let bumpy = (0..40)
        .filter(|i| {
            let (x0, z) = (-400.0 + *i as f32 * 20.0, 300.0 - *i as f32 * 15.0);
            let (h0, h1) = (map.height_at(x0, z), map.height_at(x0 + 20.0, z));
            (0..=40).any(|k| {
                let t = k as f32 / 40.0;
                let line = h0 + (h1 - h0) * t;
                (map.height_at(x0 + 20.0 * t, z) - line).abs() > 0.25
            })
        })
        .count();
    assert!(bumpy >= 30, "only {bumpy}/40 stretches have bumps");
}

#[test]
fn forces_start_at_the_foot_and_on_the_top() {
    let map = data::map("hill").unwrap();
    let blue = map.forces.iter().find(|f| f.side == Side::Blue).unwrap();
    let red = map.forces.iter().find(|f| f.side == Side::Red).unwrap();
    assert!(blue.led_by_player && blue.controller == Controller::Player);
    assert_eq!(red.controller, Controller::Ai);
    assert!(matches!(red.task, Some(TaskKind::Defend { .. })));
    // Blue at the foot in the far corner, Red on the top: ~1 km apart (beyond
    // the 750 m sight limit) and ~100 m higher.
    let d = flat(blue.position, red.position);
    assert!((1000.0..1100.0).contains(&d), "{d:.0} m apart");
    assert!(red.position[1] - blue.position[1] > 90.0);
}

#[test]
fn soldiers_stand_on_the_ground() {
    let mut sim = Sim::new(SimConfig::default());
    sim.start_map("hill").unwrap();
    run(&mut sim, 5.0);
    let map = data::map("hill").unwrap();
    for s in sim.soldiers().iter().filter(|s| !s.is_player) {
        let ground = map.height_at(s.position[0], s.position[2]);
        assert!((s.position[1] - ground).abs() < 1e-3, "{} floats", s.name);
    }
}

#[test]
fn the_enemy_mans_its_barricades_crouched_and_holds_fire() {
    let mut sim = Sim::new(SimConfig::default());
    sim.start_map("hill").unwrap();
    let events = run(&mut sim, 120.0);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SimEvent::ShotFired { .. }))
    );

    let red = sim
        .units()
        .into_iter()
        .find(|u| u.side == Side::Red && u.parent_id.is_none())
        .unwrap();
    assert_eq!(red.playbook, Some(Playbook::PreparedDefence));
    assert_eq!(red.task_stage, Some(TaskStage::InPosition));

    // Every rifleman kneels just behind a barricade, facing over it.
    let map = data::map("hill").unwrap();
    let sections: Vec<u32> = sim
        .units()
        .into_iter()
        .filter(|u| u.parent_id == Some(red.id) && u.name != "Platoon HQ")
        .map(|u| u.id)
        .collect();
    for s in sim
        .soldiers()
        .iter()
        .filter(|s| sections.contains(&s.unit_id))
    {
        assert_eq!(s.stance, Stance::Crouching, "{} isn't crouched", s.name);
        let wall = map
            .objects
            .iter()
            .min_by(|a, b| flat(a.position, s.position).total_cmp(&flat(b.position, s.position)))
            .unwrap();
        let (from, to) = wall.firing_line(0.5);
        let mid = [(from[0] + to[0]) / 2.0, 0.0, (from[2] + to[2]) / 2.0];
        assert!(
            flat(mid, s.position) < 8.0,
            "{} is not at a barricade",
            s.name
        );
        let turn = (s.yaw - wall.defender_yaw()).rem_euclid(2.0 * PI);
        assert!(turn.min(2.0 * PI - turn) < 0.01, "{} faces away", s.name);
    }
}

#[test]
fn the_hill_stops_rounds_fired_into_it() {
    let map = data::map("hill").unwrap();
    // From halfway up, level toward the summit: into the slope.
    let from = [0.0, map.height_at(0.0, 0.0) + 1.5, 0.0];
    let dir = [FRAC_1_SQRT_2, 0.0, -FRAC_1_SQRT_2];
    let hit = map.obstruction(from, dir, 600.0).expect("into the hill");
    assert!(hit > 2.0 && hit < 300.0, "{hit:.0} m");
    // Straight up: nothing.
    assert!(map.obstruction(from, [0.0, 1.0, 0.0], 600.0).is_none());
}

#[test]
fn a_barricade_stops_rounds_below_its_top() {
    let wall = MapObject {
        kind: ObjectKind::Barricade,
        position: [0.0, 0.0, -10.0],
        facing_deg: 180.0, // faces south, toward a shooter at the origin
        length_m: 10.0,
        height_m: 1.0,
        thickness_m: 0.6,
        ..Default::default()
    };
    let north = [0.0, 0.0, -1.0];
    let low = wall.raycast([0.0, 0.5, 0.0], north, 100.0);
    assert!(low.is_some_and(|t| (t - 9.7).abs() < 0.01), "{low:?}");
    assert!(
        wall.raycast([0.0, 1.5, 0.0], north, 100.0).is_none(),
        "over the top"
    );
    assert!(
        wall.raycast([20.0, 0.5, 0.0], north, 100.0).is_none(),
        "beside it"
    );
    // Its defenders stand on the north side, facing south over it.
    let (from, to) = wall.firing_line(0.5);
    assert!(from[2] < -10.0 && to[2] < -10.0);
    let turn = (wall.defender_yaw() - yaw_from_heading(180.0)).rem_euclid(2.0 * PI);
    assert!(
        turn.min(2.0 * PI - turn) < 1e-4,
        "defenders face {}",
        wall.defender_yaw()
    );
}

#[test]
fn rounds_at_a_crouched_defenders_chest_hit_the_sandbags() {
    let mut sim = Sim::new(SimConfig::default());
    sim.start_map("hill").unwrap();
    run(&mut sim, 120.0);
    let map = data::map("hill").unwrap();
    // The defender nearest the middle barricade, and a shooter 30 m in front.
    let wall = &map.objects[1];
    let target = sim
        .soldiers()
        .into_iter()
        .filter(|s| s.side == Side::Red && s.stance == Stance::Crouching)
        .min_by(|a, b| flat(a.position, wall.position).total_cmp(&flat(b.position, wall.position)))
        .unwrap();
    let front = [-wall.yaw().sin(), -wall.yaw().cos()];
    let (sx, sz) = (
        wall.position[0] + front[0] * 30.0,
        wall.position[2] + front[1] * 30.0,
    );
    let eye = [sx, map.height_at(sx, sz) + 0.6, sz];
    // Aim at their middle (0.5 m up): below the 1 m wall.
    let aim = [
        target.position[0],
        target.position[1] + 0.5,
        target.position[2],
    ];
    let dir = [aim[0] - eye[0], aim[1] - eye[1], aim[2] - eye[2]];
    for _ in 0..5 {
        sim.player_fire(eye, dir);
        sim.step();
    }
    let events = run(&mut sim, 0.5);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SimEvent::SoldierKilled { .. }))
    );
    let after = sim
        .soldiers()
        .into_iter()
        .find(|s| s.id == target.id)
        .unwrap();
    assert_ne!(after.life, LifeState::Dead);
}

#[test]
fn a_force_with_no_task_is_idle() {
    let mut def = data::map("hill").unwrap();
    for f in &mut def.forces {
        f.task = None;
    }
    let mut sim = Sim::new(SimConfig::default());
    sim.start_map_def(def);
    let events = run(&mut sim, 30.0);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SimEvent::ShotFired { .. }))
    );
    let red = sim
        .units()
        .into_iter()
        .find(|u| u.side == Side::Red && u.parent_id.is_none())
        .unwrap();
    assert_eq!(red.playbook, Some(Playbook::Harbour));
}

#[test]
fn saves_remember_the_map_and_its_terrain() {
    let mut sim = Sim::new(SimConfig::default());
    sim.start_map("hill").unwrap();
    let save = sim.to_save();
    assert_eq!(save.map_id.as_deref(), Some("hill"));
    let loaded = Sim::from_save(SimConfig::default(), &save);
    let map = loaded.map().unwrap();
    assert_eq!(map.id, "hill");
    assert!(map.heights.is_some());
}

#[test]
fn unknown_map_is_an_error() {
    assert!(Sim::new(SimConfig::default()).start_map("nowhere").is_err());
}

#[test]
fn heights_survive_the_file_format() {
    let field = Heightfield::new(3, 2.0, vec![0.0, 1.0, 2.0, 3.0, 4.5, 5.0, 6.0, 7.0, 8.0]);
    let back = Heightfield::from_bytes(3, 2.0, &field.to_bytes()).unwrap();
    assert_eq!(back, field);
    assert_eq!(back.height_at(0.0, 0.0), 4.5);
    assert!(Heightfield::from_bytes(3, 2.0, &[0; 8]).is_err());
}

#[test]
fn heading_to_yaw() {
    assert_eq!(yaw_from_heading(0.0), 0.0);
    // East (90°) is yaw -π/2: looking down +X.
    assert!((yaw_from_heading(90.0) + std::f32::consts::FRAC_PI_2).abs() < 1e-6);
}

#[test]
fn the_map_has_woods_and_boulders_on_the_ground() {
    let map = data::map("hill").unwrap();
    let trees: Vec<&MapObject> = map
        .objects
        .iter()
        .filter(|o| o.kind == ObjectKind::Tree)
        .collect();
    let boulders: Vec<&MapObject> = map
        .objects
        .iter()
        .filter(|o| o.kind == ObjectKind::Boulder)
        .collect();
    assert!(trees.len() > 1000, "only {} trees", trees.len());
    assert!(boulders.len() > 200, "only {} boulders", boulders.len());
    for t in &trees {
        assert!((t.position[1] - map.height_at(t.position[0], t.position[2])).abs() < 1e-3);
        assert!(t.canopy_m > 0.0 && t.height_m > 5.0);
    }
    // Boulders are partly sunk in.
    for b in &boulders {
        assert!(b.position[1] < map.height_at(b.position[0], b.position[2]));
    }
}

#[test]
fn nothing_grows_on_the_starts_the_summit_or_the_barricades() {
    let map = data::map("hill").unwrap();
    let blue = map.forces.iter().find(|f| f.side == Side::Blue).unwrap();
    let summit = [440.0, 0.0, -440.0];
    for o in map
        .objects
        .iter()
        .filter(|o| matches!(o.kind, ObjectKind::Tree | ObjectKind::Boulder))
    {
        assert!(
            flat(o.position, summit) > 80.0,
            "{:?} on the summit",
            o.kind
        );
        assert!(
            flat(o.position, blue.position) > 35.0,
            "{:?} on Blue's start",
            o.kind
        );
        for wall in map
            .objects
            .iter()
            .filter(|w| w.kind == ObjectKind::Barricade)
        {
            assert!(flat(o.position, wall.position) > 10.0);
        }
    }
}

#[test]
fn tree_trunks_and_boulders_stop_rounds() {
    let map = data::map("hill").unwrap();
    for kind in [ObjectKind::Tree, ObjectKind::Boulder] {
        let o = map.objects.iter().find(|o| o.kind == kind).unwrap();
        // Level at its middle height from 10 m south, straight at it.
        let y = o.position[1] + o.height_m.min(2.0) * 0.5;
        let from = [o.position[0], y, o.position[2] + 10.0];
        let hit = map.obstruction(from, [0.0, 0.0, -1.0], 20.0);
        assert!(
            hit.is_some_and(|t| t < 10.0),
            "{kind:?} didn't stop the round ({hit:?})"
        );
    }
}
