//! `Order::FireAt`: stay put and fire at a point until given another order.

use bevy_ecs::query::{QueryData, Without};
use bevy_ecs::system::{Query, Res, ResMut};

use crate::components::{
    Activity, FireControl, Inventory, PlayerControlled, Role, Soldier, Transform, WeaponState,
    swap_magazine, yaw_towards,
};
use crate::events::{SimEvent, SimEvents};
use crate::rng::SimRng;
use crate::sim::SimClock;

use super::apply::Recipient;
use super::queries::AiSoldiers;
use super::tuning;

/// Start a fire mission at `target` for every soldier in `units`.
pub fn assign(units: &[Recipient], target: [f32; 3], rng: &mut SimRng, soldiers: &mut AiSoldiers) {
    for m in units.iter().flat_map(|u| &u.members) {
        if let Ok(mut s) = soldiers.get_mut(*m) {
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
    soldier: &'static Soldier,
    transform: &'static mut Transform,
    fire: &'static mut FireControl,
    activity: &'static mut Activity,
    inventory: &'static mut Inventory,
}

/// Fire at ordered targets: aimed singles for riflemen, bursts for gunners,
/// magazine changes from the soldier's own pouches.
pub fn fire_control(
    clock: Res<SimClock>,
    mut rng: ResMut<SimRng>,
    mut events: ResMut<SimEvents>,
    mut shooters: Query<Shooter, Without<PlayerControlled>>,
    mut weapons: Query<&mut WeaponState>,
) {
    let dt = clock.dt as f32;
    for s in &mut shooters {
        let ShooterItem {
            soldier,
            mut transform,
            mut fire,
            mut activity,
            mut inventory,
        } = s;
        let Some(target) = fire.target else { continue };
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
        let shots = if gunner { tuning::GUNNER_BURST } else { 1 }.min(rounds);
        weapon.magazine = Some(rounds - shots);
        let spread = tuning::SPREAD_BASE + tuning::SPREAD_PER_METRE * range;
        let (r, a) = (
            rng.unit().sqrt() * spread,
            rng.unit() * std::f32::consts::TAU,
        );
        let impact = [target[0] + r * a.cos(), target[1], target[2] + r * a.sin()];
        events.push(SimEvent::ShotFired {
            soldier_id: soldier.id,
            target: impact,
            rounds: shots,
        });
        let (lo, hi) = if gunner {
            tuning::GUNNER_INTERVAL
        } else {
            tuning::RIFLE_INTERVAL
        };
        fire.cooldown = lo + (hi - lo) * rng.unit();
        *activity = Activity::Firing;
    }
}
