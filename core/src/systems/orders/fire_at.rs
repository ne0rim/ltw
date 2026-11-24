//! `Order::FireAt`: stay put and fire one magazine's worth at a point.
//!
//! Each soldier gets a budget of one magazine (their weapon's capacity) when
//! the order is given. A partly used magazine is swapped mid-mission, so they
//! still fire a full magazine's worth. When every soldier in the unit is done
//! (or out of ammo), the unit reports and its order clears.
//!
//! Each round becomes a `ShotRequest` with the shooter's spread (veterancy,
//! stance, movement, suppression and the weapon's own dispersion); the combat
//! system traces it and applies any hit (see `systems::combat`).

use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QueryData, Without};
use bevy_ecs::system::{Query, Res, ResMut};

use crate::ballistics;
use crate::components::{
    Activity, FireControl, Inventory, LifeState, Order, PlayerControlled, Role, Soldier, Stance,
    Suppression, Transform, Velocity, Veterancy, WeaponState, swap_magazine, yaw_towards,
};
use crate::events::{SimEvent, SimEvents};
use crate::rng::SimRng;
use crate::sim::SimClock;
use crate::systems::combat::{Aim, PendingShots, ShotRequest};

use super::apply::Recipient;
use super::queries::{AiSoldiers, OrderedUnit};
use super::tuning;

/// Start a fire mission at `target` for every soldier in `units`: one
/// magazine's worth of rounds each.
pub fn assign(
    units: &[Recipient],
    target: [f32; 3],
    rng: &mut SimRng,
    soldiers: &mut AiSoldiers,
    weapons: &Query<&WeaponState>,
) {
    for m in units.iter().flat_map(|u| &u.members) {
        if let Ok(mut s) = soldiers.get_mut(*m) {
            if *s.life == LifeState::Dead {
                continue;
            }
            s.fire.rounds_left = s
                .inventory
                .and_then(|i| i.primary)
                .and_then(|w| weapons.get(w).ok())
                .map_or(0, |w| w.stats.magazine_capacity);
            s.goal.0 = None;
            s.fire.target = Some(target);
            s.fire.out_of_ammo = false;
            // Stagger the first shots so the unit doesn't fire in unison.
            s.fire.cooldown = rng.unit() * tuning::FIRST_SHOT_STAGGER;
        }
    }
}

/// A soldier carrying out a fire mission.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct Shooter {
    entity: Entity,
    soldier: &'static Soldier,
    transform: &'static mut Transform,
    fire: &'static mut FireControl,
    activity: &'static mut Activity,
    inventory: &'static mut Inventory,
    stance: &'static Stance,
    veterancy: &'static Veterancy,
    suppression: &'static Suppression,
    velocity: &'static Velocity,
}

/// Fire at ordered targets: aimed singles for riflemen, bursts for gunners,
/// magazine changes from the soldier's own pouches.
pub fn fire_control(
    clock: Res<SimClock>,
    mut rng: ResMut<SimRng>,
    mut events: ResMut<SimEvents>,
    mut pending_shots: ResMut<PendingShots>,
    mut shooters: Query<Shooter, Without<PlayerControlled>>,
    mut weapons: Query<&mut WeaponState>,
) {
    let dt = clock.dt as f32;
    for s in &mut shooters {
        let ShooterItem {
            entity,
            soldier,
            mut transform,
            mut fire,
            mut activity,
            mut inventory,
            stance,
            veterancy,
            suppression,
            velocity,
        } = s;
        let Some(target) = fire.target else { continue };
        if fire.rounds_left == 0 {
            // Fired their magazine's worth: cease fire.
            fire.target = None;
            *activity = Activity::Idle;
            continue;
        }
        let dx = target[0] - transform.position[0];
        let dz = target[2] - transform.position[2];
        let range = (dx * dx + dz * dz).sqrt();
        if range > 0.01 {
            transform.yaw = yaw_towards(dx / range, dz / range);
        }
        if fire.out_of_ammo {
            *activity = Activity::Idle;
            continue;
        }
        let Some(weapon_e) = inventory.primary else {
            continue;
        };
        let Ok(mut weapon) = weapons.get_mut(weapon_e) else {
            continue;
        };

        // Changing magazines.
        if fire.reload_left > 0.0 {
            fire.reload_left -= dt;
            *activity = Activity::Reloading;
            if fire.reload_left <= 0.0 {
                match swap_magazine(&mut inventory, &mut weapon) {
                    Some(rounds) => events.push(SimEvent::Reloaded {
                        soldier_id: soldier.id,
                        rounds,
                        magazines_left: inventory.magazines.len() as u32,
                    }),
                    None => fire.out_of_ammo = true,
                }
            }
            continue;
        }

        fire.cooldown -= dt;
        if fire.cooldown > 0.0 {
            continue;
        }
        let rounds = weapon.magazine.unwrap_or(0);
        if rounds == 0 {
            if inventory.magazines.iter().any(|r| *r > 0) {
                fire.reload_left = tuning::RELOAD_SECONDS;
                *activity = Activity::Reloading;
            } else {
                fire.out_of_ammo = true;
                *activity = Activity::Idle;
                events.push(SimEvent::OutOfAmmo {
                    soldier_id: soldier.id,
                });
            }
            continue;
        }

        let gunner = matches!(soldier.role, Role::LightMachineGunner | Role::MachineGunner);
        let shots = if gunner { tuning::GUNNER_BURST } else { 1 }
            .min(rounds)
            .min(fire.rounds_left);
        weapon.magazine = Some(rounds - shots);
        fire.rounds_left -= shots;

        // The accuracy formula: the weapon's dispersion and the shooter's own
        // error (veterancy, stance, moving, suppression). See `ballistics`.
        let moving = velocity.horizontal_speed() > 0.3;
        let shooter_error =
            ballistics::shooter_error_mrad(*veterancy, *stance, moving, suppression.0);
        let sigma = ballistics::spread_mrad(weapon.stats.dispersion_mrad, shooter_error);
        let p = transform.position;
        let origin = [p[0], p[1] + ballistics::muzzle_height(*stance), p[2]];
        for _ in 0..shots {
            shots_out(
                &mut pending_shots,
                entity,
                origin,
                target,
                sigma,
                weapon.stats.max_range_m,
            );
        }
        let (lo, hi) = if gunner {
            tuning::GUNNER_INTERVAL
        } else {
            tuning::RIFLE_INTERVAL
        };
        fire.cooldown = lo + (hi - lo) * rng.unit();
        *activity = Activity::Firing;
        if fire.rounds_left == 0 {
            fire.target = None;
            *activity = Activity::Idle;
        }
    }
}

/// Queue one AI round at a point (resolved by `systems::combat`).
fn shots_out(
    shots: &mut PendingShots,
    shooter: Entity,
    origin: [f32; 3],
    target: [f32; 3],
    sigma_mrad: f32,
    max_range: f32,
) {
    shots.0.push(ShotRequest {
        shooter,
        origin,
        aim: Aim::Point(target),
        sigma_mrad,
        max_range,
        spare_close_friends: true,
    });
}

/// When every soldier of a firing unit has fired their magazine's worth (or
/// run out of ammo), report it and clear the order: the unit is idle again.
pub fn report_fire_complete(
    mut events: ResMut<SimEvents>,
    mut units: Query<OrderedUnit>,
    mut fire: Query<&mut FireControl, Without<PlayerControlled>>,
) {
    for mut u in &mut units {
        if !matches!(u.current.order, Some(Order::FireAt { .. })) {
            continue;
        }
        let still_firing = u.members.0.iter().any(|m| {
            fire.get(*m)
                .is_ok_and(|f| f.target.is_some() && !f.out_of_ammo)
        });
        if still_firing {
            continue;
        }
        for m in &u.members.0 {
            if let Ok(mut f) = fire.get_mut(*m) {
                f.target = None;
            }
        }
        u.current.order = None;
        events.push(SimEvent::FireMissionComplete {
            unit_id: u.unit.id,
            callsign: u.unit.callsign.clone(),
        });
    }
}
