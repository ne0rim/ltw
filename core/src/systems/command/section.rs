//! AI units without sub-units (sections, platoon HQ): carry out the task by
//! giving themselves orders.
//!
//!   New      → order a move to the task's position          → Moving
//!   Moving   → arrived (the move order cleared):
//!                SupportByFire → order a fire mission        → Firing
//!                Defend        →                             → InPosition
//!                Attack/MoveTo →                             → Done
//!   Firing   → fire mission finished → fire again while anyone has ammo

use bevy_ecs::entity::Entity;
use bevy_ecs::query::QueryData;
use bevy_ecs::system::{Query, ResMut};

use crate::components::{
    Controller, CurrentOrder, FireControl, Issuer, Members, Order, Task, TaskKind, TaskStage,
    Transform, Unit, UnitKind,
};
use crate::systems::orders::{OrderRequest, PendingOrders};

use super::geometry::{centre, direction, facing};

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
                let (target, face) = move_for(kind, here);
                issue(Order::MoveTo {
                    target,
                    facing: face,
                });
                u.task.set_stage(TaskStage::Moving);
            }
            TaskStage::Moving if u.current.order.is_none() => {
                let next = match kind {
                    TaskKind::SupportByFire { target, .. } => {
                        issue(Order::FireAt { target });
                        TaskStage::Firing
                    }
                    TaskKind::Defend { .. } => TaskStage::InPosition,
                    TaskKind::Attack { .. } | TaskKind::MoveTo { .. } => TaskStage::Done,
                };
                u.task.set_stage(next);
            }
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

/// Where to move for a task, and which way to face on arrival.
fn move_for(kind: TaskKind, here: Option<[f32; 3]>) -> ([f32; 3], f32) {
    match kind {
        TaskKind::Attack { objective } => {
            let from = here.unwrap_or(objective);
            (objective, facing(direction(from, objective)))
        }
        TaskKind::SupportByFire { position, target } => {
            (position, facing(direction(position, target)))
        }
        TaskKind::Defend { position, facing } | TaskKind::MoveTo { position, facing } => {
            (position, facing)
        }
    }
}
