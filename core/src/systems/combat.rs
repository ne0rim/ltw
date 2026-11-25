//! Shots, hits and suppression. Every shot in the game (AI fire missions and
//! the player's rifle) is queued as a [`ShotRequest`] and resolved here with
//! the same maths (see `crate::ballistics`):
//!
//! 1. aim: at a point (AI) or along a direction (the player's camera)
//! 2. a random error drawn from the shot's spread
//! 3. trace the bullet; the first body on its path is hit
//! 4. one hit kills: the soldier drops where they stand and stops fighting
//! 5. everyone the bullet passed close to is suppressed a little
//!
//! AI rounds don't hit their own side within `FRIENDLY_SAFE_DISTANCE` of the
//! shooter (soldiers don't shoot through their mates in formation); beyond
//! that, friendly fire is possible. The player's rounds can hit anyone.
//!
//! The map's ground and objects (barricades) stop rounds: a body is only hit
//! if the round reaches it first.

use std::collections::HashSet;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::QueryData;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, Res, ResMut};

use crate::map::ActiveMap;

use crate::ballistics::{self, Body};
use crate::components::{
    Activity, FireControl, GoalFacing, Health, LifeState, MoveGoal, Side, Soldier, Stance,
    Suppression, Transform, Velocity,
};
use crate::events::{SimEvent, SimEvents};
use crate::rng::SimRng;
use crate::sim::SimClock;

/// Tuning for shots and suppression. They'll move into data files later.
pub mod tuning {
    /// AI rounds pass through their own side this close to the shooter (m).
    pub const FRIENDLY_SAFE_DISTANCE: f32 = 40.0;
    /// Rounds passing this close to someone suppress them (m).
    pub const SUPPRESS_RADIUS: f32 = 3.0;
    /// Suppression added by each close round (0..1 scale).
    pub const SUPPRESS_PER_ROUND: f32 = 0.15;
    /// Suppression lost per second without rounds coming close.
    pub const SUPPRESSION_RECOVERY: f32 = 0.2;
    /// AI aim this high above the ground at a point target (m): a standing
    /// man's chest.
    pub const AIM_HEIGHT: f32 = 1.0;
    /// A point target's ground height is taken from soldiers this close to it
    /// (the sim has no terrain yet) (m).
    pub const GROUND_SAMPLE_RADIUS: f32 = 30.0;
}

/// Where a shot is aimed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Aim {
    /// A point on the ground (AI fire missions). Its height is worked out from
    /// soldiers nearby, plus `AIM_HEIGHT`.
    Point([f32; 3]),
    /// Straight along a direction (the player's camera).
    Direction([f32; 3]),
}

/// One round to resolve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotRequest {
    pub shooter: Entity,
    /// Where the round leaves the muzzle.
    pub origin: [f32; 3],
    pub aim: Aim,
    /// Total angular spread, 1 σ in milliradians (`ballistics::spread_mrad`).
    pub sigma_mrad: f32,
    pub max_range: f32,
    /// AI rounds don't hit friends close to the shooter; the player's can.
    pub spare_close_friends: bool,
}

/// Rounds fired this tick, waiting to be resolved.
#[derive(Resource, Debug, Default)]
pub struct PendingShots(pub Vec<ShotRequest>);

/// A soldier a round can hit or suppress.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct Target {
    entity: Entity,
    soldier: &'static Soldier,
    side: &'static Side,
    transform: &'static Transform,
    stance: &'static Stance,
    life: &'static mut LifeState,
    health: &'static mut Health,
    suppression: &'static mut Suppression,
    goal: &'static mut MoveGoal,
    goal_facing: &'static mut GoalFacing,
    activity: &'static mut Activity,
    fire: &'static mut FireControl,
    velocity: &'static mut Velocity,
}

pub fn resolve_shots(
    map: Option<Res<ActiveMap>>,
    mut pending: ResMut<PendingShots>,
    mut rng: ResMut<SimRng>,
    mut events: ResMut<SimEvents>,
    mut targets: Query<Target>,
) {
    if pending.0.is_empty() {
        return;
    }
    // Everyone alive, as bodies to trace against.
    let people: Vec<(Entity, u32, Side, Body)> = targets
        .iter()
        .filter(|t| *t.life != LifeState::Dead)
        .map(|t| {
            (
                t.entity,
                t.soldier.id,
                *t.side,
                Body {
                    feet: t.transform.position,
                    height: ballistics::body_height(*t.stance),
                },
            )
        })
        .collect();
    let mut killed: HashSet<usize> = HashSet::new();

    for shot in std::mem::take(&mut pending.0) {
        // The shooter may have been killed earlier this tick.
        let Some(shooter_i) = people.iter().position(|p| p.0 == shot.shooter) else {
            continue;
        };
        if killed.contains(&shooter_i) {
            continue;
        }
        let (_, shooter_id, shooter_side, shooter_body) = people[shooter_i];

        // Where the round is aimed, and how far it is traced.
        let (aim_dir, aim_distance) = match shot.aim {
            Aim::Direction(d) => (ballistics::normalize(d), shot.max_range),
            Aim::Point(p) => {
                // The map's ground if it has terrain; otherwise guess from
                // soldiers near the point.
                let ground = match map.as_deref().filter(|m| m.0.heights.is_some()) {
                    Some(m) => m.0.height_at(p[0], p[2]),
                    None => ground_near(p, &people, &killed).unwrap_or(shooter_body.feet[1]),
                };
                let aim = [p[0], ground + tuning::AIM_HEIGHT, p[2]];
                let v = ballistics::sub(aim, shot.origin);
                (ballistics::normalize(v), ballistics::length(v))
            }
        };
        let dir = ballistics::deviate(aim_dir, shot.sigma_mrad, &mut rng);
        // Rounds carry on past a point they were aimed at.
        let reach = (aim_distance * 1.5).min(shot.max_range);
        // The ground and objects (barricades) stop rounds.
        let blocked = map
            .as_deref()
            .and_then(|m| m.0.obstruction(shot.origin, dir, reach));
        let reach = blocked.unwrap_or(reach);
        let close_friend = |i: usize| {
            shot.spare_close_friends
                && people[i].2 == shooter_side
                && ballistics::length(ballistics::sub(people[i].3.feet, shooter_body.feet))
                    < tuning::FRIENDLY_SAFE_DISTANCE
        };
        let bodies: Vec<Body> = people.iter().map(|p| p.3).collect();
        let hit = ballistics::trace(shot.origin, dir, reach, &bodies, |i| {
            i == shooter_i || killed.contains(&i) || close_friend(i)
        });

        // Where the tracer ends: the hit, or about where it was aimed.
        let end_distance = hit.map_or(blocked.unwrap_or(aim_distance.min(reach)), |(_, t)| t);
        let impact = [
            shot.origin[0] + dir[0] * end_distance,
            shot.origin[1] + dir[1] * end_distance,
            shot.origin[2] + dir[2] * end_distance,
        ];
        events.push(SimEvent::ShotFired {
            soldier_id: shooter_id,
            target: impact,
            rounds: 1,
        });

        // Near misses suppress whoever they pass.
        for (i, p) in people.iter().enumerate() {
            if i == shooter_i || killed.contains(&i) || close_friend(i) {
                continue;
            }
            if hit.is_some_and(|(h, _)| h == i) {
                continue;
            }
            let centre = [p.3.feet[0], p.3.feet[1] + p.3.height * 0.5, p.3.feet[2]];
            let d = ballistics::distance_to_path(centre, shot.origin, dir, end_distance);
            if d < tuning::SUPPRESS_RADIUS
                && let Ok(mut t) = targets.get_mut(p.0)
            {
                t.suppression.0 = (t.suppression.0 + tuning::SUPPRESS_PER_ROUND).min(1.0);
            }
        }

        // One hit kills.
        if let Some((i, _)) = hit {
            killed.insert(i);
            let (victim, victim_id, victim_side, _) = people[i];
            if let Ok(mut t) = targets.get_mut(victim) {
                kill(&mut t);
            }
            events.push(SimEvent::SoldierKilled {
                soldier_id: victim_id,
                side: victim_side,
                by_soldier_id: shooter_id,
            });
        }
    }
}

/// The soldier drops where they stand and stops doing anything.
fn kill(t: &mut TargetItem) {
    *t.life = LifeState::Dead;
    t.health.current = 0.0;
    t.goal.0 = None;
    t.goal_facing.0 = None;
    *t.activity = Activity::Idle;
    t.fire.target = None;
    t.velocity.0 = [0.0; 3];
    t.suppression.0 = 0.0;
}

/// Ground height near a point: from the nearest living soldier within
/// `GROUND_SAMPLE_RADIUS` (the sim has no terrain of its own yet).
fn ground_near(
    point: [f32; 3],
    people: &[(Entity, u32, Side, Body)],
    killed: &HashSet<usize>,
) -> Option<f32> {
    people
        .iter()
        .enumerate()
        .filter(|(i, _)| !killed.contains(i))
        .map(|(_, p)| {
            let dx = p.3.feet[0] - point[0];
            let dz = p.3.feet[2] - point[2];
            ((dx * dx + dz * dz).sqrt(), p.3.feet[1])
        })
        .filter(|(d, _)| *d < tuning::GROUND_SAMPLE_RADIUS)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, y)| y)
}

/// Suppression wears off when rounds stop coming close.
pub fn recover_from_suppression(clock: Res<SimClock>, mut q: Query<&mut Suppression>) {
    let step = tuning::SUPPRESSION_RECOVERY * clock.dt as f32;
    for mut s in &mut q {
        if s.0 > 0.0 {
            s.0 = (s.0 - step).max(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use crate::components::{
        Activity, Inventory, Issuer, LifeState, Order, Side, Suppression, WeaponState,
    };
    use crate::events::SimEvent;
    use crate::sim::{Sim, SimConfig, SoldierInfo};

    fn run(sim: &mut Sim, seconds: f64) -> Vec<SimEvent> {
        let mut events = Vec::new();
        for _ in 0..(seconds * sim.config.tick_rate_hz) as u32 {
            sim.step();
            events.extend(sim.drain_events());
        }
        events
    }

    fn soldier(sim: &mut Sim, id: u32) -> SoldierInfo {
        sim.soldiers().into_iter().find(|s| s.id == id).unwrap()
    }

    /// The player's platoon at the origin and a Red platoon 60 m north,
    /// facing them. Red is AI-commanded but given nothing to do yet.
    fn two_platoons() -> (Sim, u32) {
        let mut sim = Sim::new(SimConfig::default());
        sim.spawn_platoon([0.0, 0.0, 0.0], 0.0);
        let red = sim.spawn_ai_platoon(Side::Red, "Enemy", "E1", [0.0, 0.0, -110.0], PI);
        (sim, red)
    }

    #[test]
    fn the_players_shot_kills_the_soldier_it_is_aimed_at() {
        let (mut sim, _) = two_platoons();
        let target = sim
            .soldiers()
            .into_iter()
            .filter(|s| s.side == Side::Red)
            .min_by(|a, b| a.position[2].total_cmp(&b.position[2]).reverse())
            .unwrap();
        // Aim at their chest from the player's eye.
        let eye = [0.0, 1.6, 0.0];
        let chest = [target.position[0], 1.2, target.position[2]];
        let dir = [chest[0] - eye[0], chest[1] - eye[1], chest[2] - eye[2]];
        assert!(sim.player_fire(eye, dir));
        let events = run(&mut sim, 0.1);
        assert!(events.iter().any(
            |e| matches!(e, SimEvent::SoldierKilled { soldier_id, .. } if *soldier_id == target.id)
        ));
        // Dead: down where they stood, and they stay there.
        let dead = soldier(&mut sim, target.id);
        assert_eq!(dead.life, LifeState::Dead);
        run(&mut sim, 10.0);
        assert_eq!(soldier(&mut sim, target.id).position, dead.position);
        // Nobody else was hit by that one round.
        let dead_count = sim
            .soldiers()
            .iter()
            .filter(|s| s.life == LifeState::Dead)
            .count();
        assert_eq!(dead_count, 1);
    }

    #[test]
    fn ai_fire_kills_and_suppresses_and_the_dead_stop_fighting() {
        let (mut sim, red) = two_platoons();
        sim.step(); // the idle enemy plans its harbour
        let red_section = sim
            .units()
            .into_iter()
            .find(|u| u.parent_id == Some(red) && u.name == "1 Section")
            .unwrap()
            .id;
        // Red's 1 Section fires at Blue's 2 Section (about 140 m away).
        let blue_2 = sim
            .soldiers()
            .into_iter()
            .filter(|s| s.side == Side::Blue && s.unit_name == "2 Section")
            .map(|s| s.position)
            .fold([0.0; 3], |a, p| {
                [a[0] + p[0] / 10.0, 0.0, a[2] + p[2] / 10.0]
            });
        sim.queue_order(&[red_section], Order::FireAt { target: blue_2 }, Issuer::Ai);
        let mut events = Vec::new();
        let mut max_suppression = 0.0_f32;
        for _ in 0..(60.0 * 30.0) as u32 {
            sim.step();
            events.extend(sim.drain_events());
            let mut q = sim.world_mut().query::<&Suppression>();
            for s in q.iter(sim.world()) {
                max_suppression = max_suppression.max(s.0);
            }
        }
        let killed: Vec<u32> = events
            .iter()
            .filter_map(|e| match e {
                SimEvent::SoldierKilled {
                    soldier_id,
                    side: Side::Blue,
                    ..
                } => Some(*soldier_id),
                _ => None,
            })
            .collect();
        assert!(
            !killed.is_empty(),
            "a section's fire mission at 140 m killed nobody"
        );
        assert!(
            max_suppression > 0.3,
            "nobody was suppressed ({max_suppression})"
        );
        for id in killed {
            let s = soldier(&mut sim, id);
            assert_eq!((s.life, s.activity), (LifeState::Dead, Activity::Idle));
        }
        // Suppression wears off once the firing stops.
        run(&mut sim, 10.0);
        let mut q = sim.world_mut().query::<&Suppression>();
        assert!(q.iter(sim.world()).all(|s| s.0 == 0.0));
    }

    #[test]
    fn ai_rounds_never_hit_friends_close_to_the_shooter() {
        let mut sim = Sim::new(SimConfig::default());
        let red = sim.spawn_ai_platoon(Side::Red, "Enemy", "E1", [0.0, 0.0, 0.0], 0.0);
        sim.step();
        // The HQ fires straight through the rifle sections in front of it.
        let hq = sim
            .units()
            .into_iter()
            .find(|u| u.parent_id == Some(red) && u.name == "Platoon HQ")
            .unwrap()
            .id;
        sim.queue_order(
            &[hq],
            Order::FireAt {
                target: [0.0, 0.0, -25.0],
            },
            Issuer::Ai,
        );
        let events = run(&mut sim, 60.0);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SimEvent::ShotFired { .. }))
        );
        // Any friend killed was beyond the safe distance (real friendly
        // fire); nobody close to a shooter was hit.
        let all = sim.soldiers();
        for e in &events {
            if let SimEvent::SoldierKilled {
                soldier_id,
                by_soldier_id,
                ..
            } = e
            {
                let v = all.iter().find(|s| s.id == *soldier_id).unwrap();
                let k = all.iter().find(|s| s.id == *by_soldier_id).unwrap();
                let d = ((v.position[0] - k.position[0]).powi(2)
                    + (v.position[2] - k.position[2]).powi(2))
                .sqrt();
                assert!(
                    d >= super::tuning::FRIENDLY_SAFE_DISTANCE,
                    "{} killed {} only {d:.1} m away",
                    k.name,
                    v.name
                );
            }
        }
    }

    #[test]
    fn a_dead_player_cannot_fire_until_revived() {
        let (mut sim, _) = two_platoons();
        let me = sim.player_entity();
        *sim.world_mut().get_mut::<LifeState>(me).unwrap() = LifeState::Dead;
        assert!(!sim.player_fire([0.0, 1.6, 0.0], [0.0, 0.0, -1.0]));
        sim.dev_revive_player();
        assert!(sim.player_fire([0.0, 1.6, 0.0], [0.0, 0.0, -1.0]));
    }

    #[test]
    fn veterancy_suppression_and_weapon_stats_are_saved() {
        let (mut sim, _) = two_platoons();
        let me = sim.player_entity();
        sim.world_mut().get_mut::<Suppression>(me).unwrap().0 = 0.5;
        let weapon = sim.world().get::<Inventory>(me).unwrap().primary.unwrap();
        sim.world_mut()
            .get_mut::<WeaponState>(weapon)
            .unwrap()
            .stats
            .dispersion_mrad = 9.0;
        let save = sim.to_save();
        let mut loaded = Sim::from_save(SimConfig::default(), &save);
        let me = loaded.player_entity();
        assert_eq!(loaded.world().get::<Suppression>(me).unwrap().0, 0.5);
        let weapon = loaded
            .world()
            .get::<Inventory>(me)
            .unwrap()
            .primary
            .unwrap();
        assert_eq!(
            loaded
                .world()
                .get::<WeaponState>(weapon)
                .unwrap()
                .stats
                .dispersion_mrad,
            9.0
        );
        assert_eq!(loaded.to_save(), save);

        // Saves from before stats existed fall back to the data file.
        let json = serde_json::to_string(&save).unwrap();
        let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
        for s in v["soldiers"].as_array_mut().unwrap() {
            s["weapon"].as_object_mut().unwrap().remove("stats");
            s.as_object_mut().unwrap().remove("veterancy");
        }
        let old: crate::save::SaveGame = serde_json::from_value(v).unwrap();
        let mut loaded = Sim::from_save(SimConfig::default(), &old);
        let me = loaded.player_entity();
        let weapon = loaded
            .world()
            .get::<Inventory>(me)
            .unwrap()
            .primary
            .unwrap();
        assert_eq!(
            loaded
                .world()
                .get::<WeaponState>(weapon)
                .unwrap()
                .stats
                .dispersion_mrad,
            1.0
        );
        let _ = loaded.soldiers();
    }
}
