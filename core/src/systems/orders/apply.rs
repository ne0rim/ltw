//! Turning queued orders into work for each order type.

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, ResMut};

use crate::components::{Issuer, Order, UnitKind, WeaponState};
use crate::events::{SimEvent, SimEvents};
use crate::rng::SimRng;

use super::queries::{AiSoldiers, OrderedUnit};
use super::{fire_at, move_to};

/// An order for some units, and who gave it.
#[derive(Debug, Clone)]
pub struct OrderRequest {
    pub units: Vec<u32>,
    pub order: Order,
    pub issuer: Issuer,
}

/// Orders waiting to be applied (from the client, the AI or a script).
#[derive(Resource, Debug, Default)]
pub struct PendingOrders(pub Vec<OrderRequest>);

/// A unit that has been given an order.
pub struct Recipient {
    pub entity: Entity,
    pub id: u32,
    pub callsign: String,
    pub members: Vec<Entity>,
}

/// Hand each queued order to its order type, record it on the units, and
/// acknowledge it.
pub fn apply_orders(
    mut pending: ResMut<PendingOrders>,
    mut rng: ResMut<SimRng>,
    mut events: ResMut<SimEvents>,
    mut units: Query<OrderedUnit>,
    mut soldiers: AiSoldiers,
    weapons: Query<&WeaponState>,
) {
    for OrderRequest {
        units: unit_ids,
        order,
        issuer,
    } in std::mem::take(&mut pending.0)
    {
        // Orders go to sections and HQs (not the platoon itself), and only
        // from whoever controls the unit: the player can't command the enemy,
        // and the AI can't command the player's units.
        let recipients: Vec<Recipient> = units
            .iter()
            .filter(|u| {
                unit_ids.contains(&u.unit.id)
                    && u.unit.kind != UnitKind::Platoon
                    && issuer.may_command(*u.controller)
            })
            .map(|u| Recipient {
                entity: u.entity,
                id: u.unit.id,
                callsign: u.unit.callsign.clone(),
                members: u.members.0.clone(),
            })
            .collect();
        if recipients.is_empty() {
            continue;
        }

        match order {
            Order::MoveTo { target, facing } => {
                move_to::assign(&recipients, target, facing, &mut soldiers)
            }
            Order::FireAt { target } => {
                fire_at::assign(&recipients, target, &mut rng, &mut soldiers, &weapons)
            }
        }

        for r in &recipients {
            if let Ok(mut u) = units.get_mut(r.entity) {
                u.current.order = Some(order);
                u.current.arrived_reported = false;
            }
            events.push(SimEvent::OrderAcknowledged {
                unit_id: r.id,
                callsign: r.callsign.clone(),
                order: order.name(),
            });
        }
    }
}
