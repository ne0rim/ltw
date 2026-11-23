//! Orders to units, and the per-tick systems that carry them out.
//!
//! The client queues orders (`Sim::order_move`, `Sim::order_fire_at`); they
//! are applied at the start of the next tick so every system sees the same
//! state. Units acknowledge orders and report back through `SimEvent`s.
//!
//! The player-controlled soldier is never moved or fired by these systems.

use std::collections::HashMap;

use bevy_ecs::prelude::*;

use crate::components::{Gait, PlayerControlled, Transform, Velocity};
use crate::events::{SimEvent, SimEvents};
use crate::org::place;
use crate::rng::SimRng;
use crate::sim::SimClock;
use crate::soldier::*;

/// Tuning values for orders. They'll move into data files later.
pub mod tuning {
    /// Walking pace for ordered moves (m/s).
    pub const MOVE_SPEED: f32 = 2.5;
    /// Close enough to a move goal to count as arrived (m).
    pub const ARRIVE_DISTANCE: f32 = 0.35;
    /// Time to change magazines (s).
    pub const RELOAD_SECONDS: f32 = 3.0;
    /// Seconds between aimed single shots for riflemen (min, max).
    pub const RIFLE_INTERVAL: (f32, f32) = (1.1, 2.4);
    /// Seconds between bursts for machine gunners (min, max).
    pub const GUNNER_INTERVAL: (f32, f32) = (0.9, 1.7);
    pub const GUNNER_BURST: u32 = 3;
    /// Impact scatter around the target: base radius + per metre of range.
    pub const SPREAD_BASE: f32 = 0.8;
    pub const SPREAD_PER_METRE: f32 = 0.03;
}

// ------------------------------------------------------------------ data

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Order {
    /// Walk to `target` and form up facing `facing` (yaw, radians).
    MoveTo { target: [f32; 3], facing: f32 },
    /// Stay put and fire at `target` until given another order.
    FireAt { target: [f32; 3] },
}

impl Order {
    pub fn name(&self) -> &'static str {
        match self {
            Self::MoveTo { .. } => "move",
            Self::FireAt { .. } => "fire",
        }
    }
    pub fn target(&self) -> [f32; 3] {
        match *self {
            Self::MoveTo { target, .. } | Self::FireAt { target } => target,
        }
    }
}

/// The order a unit is carrying out (on platoon/section entities).
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct CurrentOrder {
    pub order: Option<Order>,
    /// Set once the unit has reported "in position" for its current move.
    pub arrived_reported: bool,
}

/// Orders from the client waiting for the next tick: (unit ids, order).
#[derive(Resource, Debug, Default)]
pub struct PendingOrders(pub Vec<(Vec<u32>, Order)>);

// ------------------------------------------------------------------ systems

/// Turn queued orders into per-soldier goals, and acknowledge them.
pub fn apply_orders(
    mut pending: ResMut<PendingOrders>,
    mut rng: ResMut<SimRng>,
    mut events: ResMut<SimEvents>,
    mut units: Query<(Entity, &Unit, &Members, &mut CurrentOrder)>,
    mut soldiers: Query<
        (&Transform, &FormationSlot, &mut MoveGoal, &mut Activity, &mut FireControl),
        Without<PlayerControlled>,
    >,
) {
    for (unit_ids, order) in std::mem::take(&mut pending.0) {
        let picked: Vec<(Entity, u32, String, Vec<Entity>)> = units
            .iter()
            .filter(|(_, u, _, _)| unit_ids.contains(&u.id) && u.kind != UnitKind::Platoon)
            .map(|(e, u, m, _)| (e, u.id, u.callsign.clone(), m.0.clone()))
            .collect();
        if picked.is_empty() {
            continue;
        }

        // For a group move, keep each unit's place relative to the others.
        let centroid = |members: &[Entity]| -> Option<[f32; 2]> {
            let pts: Vec<[f32; 2]> = members
                .iter()
                .filter_map(|m| soldiers.get(*m).ok())
                .map(|(t, ..)| [t.position[0], t.position[2]])
                .collect();
            (!pts.is_empty()).then(|| {
                let n = pts.len() as f32;
                [pts.iter().map(|p| p[0]).sum::<f32>() / n, pts.iter().map(|p| p[1]).sum::<f32>() / n]
            })
        };
        let unit_centres: HashMap<Entity, [f32; 2]> =
            picked.iter().filter_map(|(e, _, _, m)| centroid(m).map(|c| (*e, c))).collect();
        let n = unit_centres.len().max(1) as f32;
        let group = [
            unit_centres.values().map(|c| c[0]).sum::<f32>() / n,
            unit_centres.values().map(|c| c[1]).sum::<f32>() / n,
        ];

        for (unit_e, unit_id, callsign, members) in &picked {
            match order {
                Order::MoveTo { target, facing } => {
                    let c = unit_centres.get(unit_e).copied().unwrap_or(group);
                    let anchor = [target[0] + (c[0] - group[0]), target[1], target[2] + (c[1] - group[1])];
                    for m in members {
                        if let Ok((_, slot, mut goal, mut act, mut fc)) = soldiers.get_mut(*m) {
                            goal.0 = Some(place(anchor, facing, slot.right, slot.forward));
                            *act = Activity::Moving;
                            fc.target = None;
                        }
                    }
                }
                Order::FireAt { target } => {
                    for m in members {
                        if let Ok((_, _, mut goal, _, mut fc)) = soldiers.get_mut(*m) {
                            goal.0 = None;
                            fc.target = Some(target);
                            fc.out_of_ammo = false;
                            // Stagger the first shots so the section doesn't fire in unison.
                            fc.cooldown = rng.unit() * 1.2;
                        }
                    }
                }
            }
            if let Ok((_, _, _, mut current)) = units.get_mut(*unit_e) {
                current.order = Some(order);
                current.arrived_reported = false;
            }
            events.push(SimEvent::OrderAcknowledged {
                unit_id: *unit_id,
                callsign: callsign.clone(),
                order: order.name(),
            });
        }
    }
}

/// Walk soldiers toward their move goals.
pub fn move_soldiers(
    clock: Res<SimClock>,
    orders: Query<&CurrentOrder>,
    mut q: Query<
        (&SquadMember, &mut Transform, &mut Velocity, &mut Gait, &mut Activity, &mut MoveGoal),
        Without<PlayerControlled>,
    >,
) {
    let dt = clock.dt as f32;
    for (member, mut t, mut vel, mut gait, mut act, mut goal) in &mut q {
        let Some(g) = goal.0 else { continue };
        let dx = g[0] - t.position[0];
        let dz = g[2] - t.position[2];
        let dist = (dx * dx + dz * dz).sqrt();
        if dist <= tuning::ARRIVE_DISTANCE {
            t.position[0] = g[0];
            t.position[2] = g[2];
            if let Ok(CurrentOrder { order: Some(Order::MoveTo { facing, .. }), .. }) = orders.get(member.squad) {
                t.yaw = *facing;
            }
            goal.0 = None;
            vel.0 = [0.0; 3];
            *gait = Gait::Still;
            *act = Activity::Idle;
            continue;
        }
        let step = (tuning::MOVE_SPEED * dt).min(dist);
        let (ux, uz) = (dx / dist, dz / dist);
        t.position[0] += ux * step;
        t.position[2] += uz * step;
        t.yaw = yaw_towards(ux, uz);
        vel.0 = [ux * tuning::MOVE_SPEED, 0.0, uz * tuning::MOVE_SPEED];
        *gait = Gait::Walk;
        *act = Activity::Moving;
    }
}

/// Fire at ordered targets: aimed singles for riflemen, bursts for gunners,
/// magazine changes from the soldier's own pouches.
pub fn fire_control(
    clock: Res<SimClock>,
    mut rng: ResMut<SimRng>,
    mut events: ResMut<SimEvents>,
    mut shooters: Query<
        (&Soldier, &mut Transform, &mut FireControl, &mut Activity, &mut Inventory),
        Without<PlayerControlled>,
    >,
    mut weapons: Query<&mut WeaponState>,
) {
    let dt = clock.dt as f32;
    for (soldier, mut t, mut fc, mut act, mut inv) in &mut shooters {
        let Some(target) = fc.target else { continue };
        let dx = target[0] - t.position[0];
        let dz = target[2] - t.position[2];
        let range = (dx * dx + dz * dz).sqrt();
        if range > 0.01 {
            t.yaw = yaw_towards(dx / range, dz / range);
        }
        if fc.out_of_ammo {
            *act = Activity::Idle;
            continue;
        }
        let Some(weapon_e) = inv.primary else { continue };
        let Ok(mut weapon) = weapons.get_mut(weapon_e) else { continue };

        // Changing magazines.
        if fc.reload_left > 0.0 {
            fc.reload_left -= dt;
            *act = Activity::Reloading;
            if fc.reload_left <= 0.0 {
                match swap_magazine(&mut inv, &mut weapon) {
                    Some(rounds) => events.push(SimEvent::Reloaded {
                        soldier_id: soldier.id,
                        rounds,
                        magazines_left: inv.magazines.len() as u32,
                    }),
                    None => fc.out_of_ammo = true,
                }
            }
            continue;
        }

        fc.cooldown -= dt;
        if fc.cooldown > 0.0 {
            continue;
        }
        let rounds = weapon.magazine.unwrap_or(0);
        if rounds == 0 {
            if inv.magazines.iter().any(|r| *r > 0) {
                fc.reload_left = tuning::RELOAD_SECONDS;
                *act = Activity::Reloading;
            } else {
                fc.out_of_ammo = true;
                *act = Activity::Idle;
                events.push(SimEvent::OutOfAmmo { soldier_id: soldier.id });
            }
            continue;
        }

        let gunner = matches!(soldier.role, Role::LightMachineGunner | Role::MachineGunner);
        let shots = if gunner { tuning::GUNNER_BURST } else { 1 }.min(rounds);
        weapon.magazine = Some(rounds - shots);
        let spread = tuning::SPREAD_BASE + tuning::SPREAD_PER_METRE * range;
        let (r, a) = (rng.unit().sqrt() * spread, rng.unit() * std::f32::consts::TAU);
        let impact = [target[0] + r * a.cos(), target[1], target[2] + r * a.sin()];
        events.push(SimEvent::ShotFired { soldier_id: soldier.id, target: impact, rounds: shots });
        let (lo, hi) = if gunner { tuning::GUNNER_INTERVAL } else { tuning::RIFLE_INTERVAL };
        fc.cooldown = lo + (hi - lo) * rng.unit();
        *act = Activity::Firing;
    }
}

/// Tell the client once when every soldier of a moving unit has arrived.
pub fn report_in_position(
    mut events: ResMut<SimEvents>,
    mut units: Query<(&Unit, &Members, &mut CurrentOrder)>,
    goals: Query<&MoveGoal, Without<PlayerControlled>>,
) {
    for (unit, members, mut current) in &mut units {
        if current.arrived_reported || !matches!(current.order, Some(Order::MoveTo { .. })) {
            continue;
        }
        let still_moving = members.0.iter().any(|m| goals.get(*m).is_ok_and(|g| g.0.is_some()));
        if !still_moving {
            current.arrived_reported = true;
            events.push(SimEvent::UnitInPosition { unit_id: unit.id, callsign: unit.callsign.clone() });
        }
    }
}

/// Client yaw convention: yaw 0 looks down -Z.
pub fn yaw_towards(dir_x: f32, dir_z: f32) -> f32 {
    (-dir_x).atan2(-dir_z)
}
