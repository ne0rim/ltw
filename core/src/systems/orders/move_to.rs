//! Orders that move soldiers: `Order::MoveTo` (walk to a point and form up)
//! and `Order::Perimeter` (spread round an arc facing outward).

use std::collections::HashMap;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QueryData, Without};
use bevy_ecs::system::{Query, Res, ResMut};

use crate::components::{
    Activity, CurrentOrder, Gait, GoalFacing, MoveGoal, Order, PlayerControlled, SquadMember,
    Transform, Velocity, place, yaw_towards,
};
use crate::events::{SimEvent, SimEvents};
use crate::sim::SimClock;

use super::apply::Recipient;
use super::queries::{AiSoldiers, OrderedUnit};
use super::tuning;

/// Give every soldier their place in the formation at `target`, facing `facing`.
/// When several units move together, each keeps its place relative to the others.
pub fn assign(units: &[Recipient], target: [f32; 3], facing: f32, soldiers: &mut AiSoldiers) {
    let centres: HashMap<Entity, [f32; 2]> = units
        .iter()
        .filter_map(|u| centroid(&u.members, soldiers).map(|c| (u.entity, c)))
        .collect();
    // Sum in unit order (not HashMap order) so the result is the same every run.
    let group = average(units.iter().filter_map(|u| centres.get(&u.entity).copied()));

    for u in units {
        let c = centres.get(&u.entity).copied().unwrap_or(group);
        let anchor = [
            target[0] + (c[0] - group[0]),
            target[1],
            target[2] + (c[1] - group[1]),
        ];
        for m in &u.members {
            if let Ok(mut s) = soldiers.get_mut(*m) {
                s.goal.0 = Some(place(anchor, facing, s.slot.right, s.slot.forward));
                s.goal_facing.0 = Some(facing);
                *s.activity = Activity::Moving;
                s.fire.target = None;
            }
        }
    }
}

/// Spread the units' soldiers evenly along an arc around `centre`, each one
/// facing straight out. Several units share the arc, one after another.
pub fn assign_perimeter(
    units: &[Recipient],
    centre: [f32; 3],
    radius: f32,
    start: f32,
    sweep: f32,
    soldiers: &mut AiSoldiers,
) {
    let members: Vec<Entity> = units
        .iter()
        .flat_map(|u| u.members.iter().copied())
        .filter(|m| soldiers.contains(*m))
        .collect();
    let n = members.len().max(1) as f32;
    for (i, m) in members.iter().enumerate() {
        // Clockwise from `start` (yaw decreases clockwise), centred in each
        // soldier's share of the arc.
        let yaw = start - sweep * (i as f32 + 0.5) / n;
        if let Ok(mut s) = soldiers.get_mut(*m) {
            s.goal.0 = Some(place(centre, yaw, 0.0, radius));
            s.goal_facing.0 = Some(yaw);
            *s.activity = Activity::Moving;
            s.fire.target = None;
        }
    }
}

/// Average ground position (x, z) of a unit's AI soldiers.
fn centroid(members: &[Entity], soldiers: &AiSoldiers) -> Option<[f32; 2]> {
    let points: Vec<[f32; 2]> = members
        .iter()
        .filter_map(|m| soldiers.get(*m).ok())
        .map(|s| [s.transform.position[0], s.transform.position[2]])
        .collect();
    (!points.is_empty()).then(|| average(points.into_iter()))
}

/// Mean of some (x, z) points; [0, 0] if there are none.
fn average(points: impl Iterator<Item = [f32; 2]>) -> [f32; 2] {
    let (mut sum, mut n) = ([0.0_f32, 0.0], 0.0_f32);
    for p in points {
        sum[0] += p[0];
        sum[1] += p[1];
        n += 1.0;
    }
    let n = n.max(1.0);
    [sum[0] / n, sum[1] / n]
}

/// A soldier walking to a move goal.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct Walker {
    member: &'static SquadMember,
    goal_facing: &'static GoalFacing,
    transform: &'static mut Transform,
    velocity: &'static mut Velocity,
    gait: &'static mut Gait,
    activity: &'static mut Activity,
    goal: &'static mut MoveGoal,
}

/// Walk soldiers toward their move goals.
pub fn move_soldiers(
    clock: Res<SimClock>,
    orders: Query<&CurrentOrder>,
    mut walkers: Query<Walker, Without<PlayerControlled>>,
) {
    let dt = clock.dt as f32;
    for mut w in &mut walkers {
        let Some(g) = w.goal.0 else { continue };
        let dx = g[0] - w.transform.position[0];
        let dz = g[2] - w.transform.position[2];
        let dist = (dx * dx + dz * dz).sqrt();
        if dist <= tuning::ARRIVE_DISTANCE {
            w.transform.position[0] = g[0];
            w.transform.position[2] = g[2];
            if let Some(face) = w.goal_facing.0 {
                w.transform.yaw = face;
            } else if let Ok(CurrentOrder {
                order: Some(Order::MoveTo { facing, .. }),
                ..
            }) = orders.get(w.member.squad)
            {
                // Saves from before per-soldier facing existed.
                w.transform.yaw = *facing;
            }
            w.goal.0 = None;
            w.velocity.0 = [0.0; 3];
            *w.gait = Gait::Still;
            *w.activity = Activity::Idle;
            continue;
        }
        let step = (tuning::MOVE_SPEED * dt).min(dist);
        let (ux, uz) = (dx / dist, dz / dist);
        w.transform.position[0] += ux * step;
        w.transform.position[2] += uz * step;
        w.transform.yaw = yaw_towards(ux, uz);
        w.velocity.0 = [ux * tuning::MOVE_SPEED, 0.0, uz * tuning::MOVE_SPEED];
        *w.gait = Gait::Walk;
        *w.activity = Activity::Moving;
    }
}

/// When every soldier of a moving unit has arrived, report "in position" and
/// clear the order: the unit is idle again.
pub fn report_in_position(
    mut events: ResMut<SimEvents>,
    mut units: Query<OrderedUnit>,
    goals: Query<&MoveGoal, Without<PlayerControlled>>,
) {
    for mut u in &mut units {
        if !u.current.order.is_some_and(|o| o.is_move()) {
            continue;
        }
        let still_moving = u
            .members
            .0
            .iter()
            .any(|m| goals.get(*m).is_ok_and(|g| g.0.is_some()));
        if !still_moving {
            u.current.arrived_reported = true;
            u.current.order = None;
            events.push(SimEvent::UnitInPosition {
                unit_id: u.unit.id,
                callsign: u.unit.callsign.clone(),
            });
        }
    }
}
