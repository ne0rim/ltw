//! AI units without sub-units (sections, platoon HQ): carry out the task by
//! giving themselves orders.
//!
//!   New      → order the move the task needs                    → Moving
//!   Moving   → arrived (the move order cleared):
//!                SupportByFire             → order a fire mission → Firing
//!                Defend, Hold, Perimeter   →                      → InPosition
//!                Attack, MoveTo, Withdraw  →                      → Done
//!                Patrol                    → next waypoint        → New
//!   Firing   → fire mission finished → fire again while anyone has ammo
//!
//! "The move the task needs" is a formation move (`Order::MoveTo`) for most
//! tasks, and a perimeter (`Order::Perimeter`, everyone facing outward) for
//! Hold and Perimeter.

use std::f32::consts::TAU;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::QueryData;
use bevy_ecs::system::{Query, ResMut};

use crate::components::{
    Controller, CurrentOrder, FireControl, Issuer, Members, Order, Task, TaskKind, TaskStage,
    Transform, Unit, UnitKind,
};
use crate::systems::orders::{OrderRequest, PendingOrders};

use super::geometry::{centre, direction, facing};
use super::tuning;

/// An AI unit carrying out a task itself.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct TaskedUnit {
    entity: Entity,
    unit: &'static Unit,
    controller: &'static Controller,
    members: &'static Members,
    current: &'static CurrentOrder,
    task: &'static mut Task,
}

pub fn command_sections(
    mut pending: ResMut<PendingOrders>,
    mut units: Query<TaskedUnit>,
    soldiers: Query<&Transform>,
    fire: Query<&FireControl>,
) {
    let mut order: Vec<(u32, Entity)> = units
        .iter()
        .filter(|u| *u.controller == Controller::Ai && u.unit.kind != UnitKind::Platoon)
        .filter(|u| u.task.0.is_some())
        .map(|u| (u.unit.id, u.entity))
        .collect();
    order.sort_unstable();

    for (id, e) in order {
        let Ok(mut u) = units.get_mut(e) else {
            continue;
        };
        let Some((kind, stage)) = u.task.0 else {
            continue;
        };
        let here = centre(
            u.members
                .0
                .iter()
                .filter_map(|m| soldiers.get(*m).ok())
                .map(|t| t.position),
        );
        let mut issue = |o: Order| {
            pending.0.push(OrderRequest {
                units: vec![id],
                order: o,
                issuer: Issuer::Ai,
            })
        };

        match stage {
            TaskStage::New => {
                issue(move_order(kind, here));
                u.task.set_stage(TaskStage::Moving);
            }
            TaskStage::Moving if u.current.order.is_none() => match kind {
                TaskKind::SupportByFire { target, .. } => {
                    issue(Order::FireAt { target });
                    u.task.set_stage(TaskStage::Firing);
                }
                TaskKind::Defend { .. } | TaskKind::Hold { .. } | TaskKind::Perimeter { .. } => {
                    u.task.set_stage(TaskStage::InPosition)
                }
                TaskKind::Attack { .. } | TaskKind::MoveTo { .. } | TaskKind::Withdraw { .. } => {
                    u.task.set_stage(TaskStage::Done)
                }
                TaskKind::Patrol { route, leg } => {
                    // On to the next waypoint (looping), issued next tick.
                    let next = ((leg as usize + 1) % route.len().max(1)) as u8;
                    *u.task = Task::new(TaskKind::Patrol { route, leg: next });
                }
            },
            TaskStage::Firing if u.current.order.is_none() => {
                // Keep suppressing while anyone still has rounds.
                let any_ammo = u
                    .members
                    .0
                    .iter()
                    .any(|m| fire.get(*m).is_ok_and(|f| !f.out_of_ammo));
                if let (true, TaskKind::SupportByFire { target, .. }) = (any_ammo, kind) {
                    issue(Order::FireAt { target });
                } else {
                    u.task.set_stage(TaskStage::InPosition);
                }
            }
            _ => {}
        }
    }
}

/// The move that starts a task, given where the unit is now.
fn move_order(kind: TaskKind, here: Option<[f32; 3]>) -> Order {
    let walk = |target: [f32; 3], facing: f32| Order::MoveTo { target, facing };
    let toward = |to: [f32; 3]| facing(direction(here.unwrap_or(to), to));
    match kind {
        TaskKind::Attack { objective } => walk(objective, toward(objective)),
        TaskKind::SupportByFire { position, target } => {
            walk(position, facing(direction(position, target)))
        }
        TaskKind::Defend { position, facing }
        | TaskKind::MoveTo { position, facing }
        | TaskKind::Withdraw { position, facing } => walk(position, facing),
        TaskKind::Patrol { route, leg } => {
            let to = route.get(leg as usize);
            walk(to, toward(to))
        }
        TaskKind::Hold { position } => Order::Perimeter {
            centre: position,
            radius: tuning::SECTION_HOLD_RADIUS,
            start: 0.0,
            sweep: TAU,
        },
        TaskKind::Perimeter {
            centre,
            radius,
            start,
            sweep,
        } => Order::Perimeter {
            centre,
            radius,
            start,
            sweep,
        },
    }
}
