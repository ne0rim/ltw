//! Fixed-timestep simulation ticker.
//!
//! The sim advances in whole ticks of a fixed length. Callers feed in real
//! frame time through [`Sim::update`]. Each update runs as many ticks as fit
//! in the accumulated time, so the sim stays deterministic however fast the
//! client renders.

use std::collections::HashMap;
use std::path::Path;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, Without};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::world::World;

use crate::components::{
    Activity, CommissionedOfficer, CurrentOrder, FireControl, FireTeam, Gait, Health, Inventory,
    Leader, LifeState, Members, MoveGoal, Order, ParentUnit, PlayerControlled, PlayerReport, Rank,
    Role, Soldier, SquadMember, Stance, Transform, Unit, UnitKind, Velocity, WeaponState,
    swap_magazine,
};
use crate::entities::soldier::Loadout;
use crate::entities::{platoon, player};
use crate::events::{SimEvent, SimEvents};
use crate::rng::SimRng;
use crate::save::{self, SaveError, SaveGame};
use crate::systems::{self, orders::PendingOrders};

/// Default values for the ticker. Change these to change the starting
/// behaviour everywhere. At runtime, edit [`SimConfig`] instead.
pub mod defaults {
    /// Sim ticks per second of game time.
    pub const TICK_RATE_HZ: f64 = 30.0;
    /// Multiplier on real time. 0.5 is half speed, 2.0 is double speed.
    pub const TIME_SCALE: f64 = 1.0;
    /// Cap on ticks run in one update. This stops a "spiral of death" after
    /// a long hitch, at the cost of the sim falling behind real time.
    pub const MAX_TICKS_PER_UPDATE: u32 = 8;
}

/// Live, tweakable ticker settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimConfig {
    pub tick_rate_hz: f64,
    pub time_scale: f64,
    pub max_ticks_per_update: u32,
    pub paused: bool,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            tick_rate_hz: defaults::TICK_RATE_HZ,
            time_scale: defaults::TIME_SCALE,
            max_ticks_per_update: defaults::MAX_TICKS_PER_UPDATE,
            paused: false,
        }
    }
}

impl SimConfig {
    /// Length of one tick in seconds. Returns `None` if the tick rate isn't usable.
    pub fn tick_seconds(&self) -> Option<f64> {
        (self.tick_rate_hz.is_finite() && self.tick_rate_hz > 0.0).then(|| 1.0 / self.tick_rate_hz)
    }
}

/// Sim-wide clock resource that systems can read.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct SimClock {
    /// Number of ticks completed since the sim started.
    pub tick: u64,
    /// Length of the tick being run, in seconds (game time).
    pub dt: f64,
    /// Total game time elapsed, in seconds.
    pub elapsed: f64,
}

/// Owns the ECS world and the schedule that runs once per tick.
pub struct Sim {
    pub config: SimConfig,
    world: World,
    schedule: Schedule,
    accumulator: f64,
    player: Entity,
    /// Latest state reported by the client, applied at the next tick.
    pending_player: Option<PlayerReport>,
}

impl Default for Sim {
    fn default() -> Self {
        Self::new(SimConfig::default())
    }
}

impl Sim {
    pub fn new(config: SimConfig) -> Self {
        let (world, player) = Self::base_world();
        Self::with_world(config, world, player)
    }

    /// Rebuild a sim from a save file's contents.
    pub fn from_save(config: SimConfig, save: &SaveGame) -> Self {
        let (mut world, player) = Self::base_world();
        save::restore(&mut world, player, save);
        Self::with_world(config, world, player)
    }

    /// Snapshot everything needed to rebuild this sim (see [`save`]).
    pub fn to_save(&mut self) -> SaveGame {
        // A player report not yet applied would be lost; apply it to the snapshot.
        if let Some(r) = self.pending_player {
            self.world
                .entity_mut(self.player)
                .insert((r.transform, r.velocity, r.stance, r.gait));
        }
        save::capture(&mut self.world, self.player)
    }

    /// Save to a file (written safely: temp file, then rename).
    pub fn save_to_file(&mut self, path: &Path) -> Result<(), SaveError> {
        save::write_file(path, &self.to_save())
    }

    /// Load from a file. `Ok(None)` if there is no save yet.
    pub fn load_from_file(config: SimConfig, path: &Path) -> Result<Option<Self>, SaveError> {
        Ok(save::read_file(path)?.map(|s| Self::from_save(config, &s)))
    }

    /// A world with the sim's resources and a bare player entity.
    fn base_world() -> (World, Entity) {
        let mut world = World::new();
        world.insert_resource(SimClock::default());
        world.init_resource::<SimEvents>();
        world.init_resource::<SimRng>();
        world.init_resource::<PendingOrders>();
        let player = player::spawn_player(&mut world);
        (world, player)
    }

    fn with_world(config: SimConfig, world: World, player: Entity) -> Self {
        let mut schedule = Schedule::default();
        schedule.add_systems(
            (
                systems::orders::apply_orders,
                systems::orders::move_soldiers,
                systems::orders::fire_control,
                systems::orders::report_in_position,
                systems::player::watch_player_movement,
            )
                .chain(),
        );
        Self {
            config,
            world,
            schedule,
            accumulator: 0.0,
            player,
            pending_player: None,
        }
    }

    /// Advance by `real_delta` seconds of wall time. Returns how many ticks ran.
    pub fn update(&mut self, real_delta: f64) -> u32 {
        let Some(tick_seconds) = self.config.tick_seconds() else {
            return 0;
        };
        if self.config.paused || !real_delta.is_finite() || real_delta <= 0.0 {
            return 0;
        }

        self.accumulator += real_delta * self.config.time_scale.max(0.0);

        let mut ran = 0;
        while self.accumulator >= tick_seconds && ran < self.config.max_ticks_per_update {
            self.accumulator -= tick_seconds;
            self.run_tick(tick_seconds);
            ran += 1;
        }
        // If we hit the cap, drop the backlog instead of carrying it forward.
        if ran == self.config.max_ticks_per_update {
            self.accumulator = self.accumulator.min(tick_seconds);
        }
        ran
    }

    /// Run exactly one tick, even when paused. Useful for stepping while debugging.
    pub fn step(&mut self) {
        if let Some(tick_seconds) = self.config.tick_seconds() {
            self.run_tick(tick_seconds);
        }
    }

    /// How far we are into the next tick (0..1), for interpolating visuals.
    pub fn alpha(&self) -> f64 {
        self.config
            .tick_seconds()
            .map_or(0.0, |t| (self.accumulator / t).clamp(0.0, 1.0))
    }

    /// Client reports the player's current movement state. Only the newest
    /// report counts. It reaches the world at the start of the next tick, so
    /// every system in a tick sees the same state.
    pub fn report_player(&mut self, report: PlayerReport) {
        self.pending_player = Some(report);
    }

    /// The player's state as the sim currently knows it (updates once per tick).
    pub fn player_state(&self) -> PlayerReport {
        let e = self.world.entity(self.player);
        PlayerReport {
            transform: *e.get::<Transform>().expect("player has Transform"),
            velocity: *e.get::<Velocity>().expect("player has Velocity"),
            stance: *e.get::<Stance>().expect("player has Stance"),
            gait: *e.get::<Gait>().expect("player has Gait"),
        }
    }

    /// Take every event the sim has produced since the last call.
    pub fn drain_events(&mut self) -> Vec<SimEvent> {
        std::mem::take(&mut self.world.resource_mut::<SimEvents>().0)
    }

    pub fn player_entity(&self) -> Entity {
        self.player
    }

    // ---------------------------------------------------------------- units & soldiers

    /// Create a platoon (platoon HQ of 6 + 3 rifle sections of 10)
    /// around the player, who becomes its platoon commander. Returns the platoon's unit id.
    pub fn spawn_platoon(&mut self, origin: [f32; 3], yaw: f32) -> u32 {
        let platoon = platoon::spawn_platoon(&mut self.world, self.player, origin, yaw);
        let unit_id = self
            .world
            .get::<Unit>(platoon)
            .expect("platoon has Unit")
            .id;
        let squads = self
            .world
            .get::<Members>(platoon)
            .expect("platoon has Members")
            .0
            .clone();
        let soldiers = squads
            .iter()
            .map(|sq| self.world.get::<Members>(*sq).map_or(0, |m| m.0.len()))
            .sum::<usize>() as u32;
        self.world
            .resource_mut::<SimEvents>()
            .push(SimEvent::PlatoonSpawned { unit_id, soldiers });
        unit_id
    }

    /// Snapshot of every soldier, sorted by id.
    pub fn soldiers(&mut self) -> Vec<SoldierInfo> {
        let mut q = self.world.query::<(
            &Soldier,
            &SquadMember,
            &Transform,
            &Stance,
            &Health,
            &LifeState,
            &Inventory,
            Has<CommissionedOfficer>,
            Has<PlayerControlled>,
            Option<&Activity>,
            Option<&FireControl>,
        )>();
        let rows: Vec<_> = q
            .iter(&self.world)
            .map(|(s, m, t, st, h, l, inv, off, pl, act, fc)| {
                (
                    s.clone(),
                    *m,
                    *t,
                    *st,
                    *h,
                    *l,
                    inv.clone(),
                    off,
                    pl,
                    act.copied(),
                    fc.copied(),
                )
            })
            .collect();
        let mut out: Vec<SoldierInfo> = rows
            .into_iter()
            .map(
                |(s, m, t, stance, h, life, inv, is_officer, is_player, act, fc)| {
                    let unit = self.world.get::<Unit>(m.squad);
                    let weapon = inv.primary.and_then(|w| self.world.get::<WeaponState>(w));
                    SoldierInfo {
                        id: s.id,
                        name: s.name,
                        rank: s.rank,
                        role: s.role,
                        unit_id: unit.map_or(0, |u| u.id),
                        unit_name: unit.map_or_else(String::new, |u| u.name.clone()),
                        fire_team: m.fire_team,
                        slot: m.slot,
                        is_player,
                        is_officer,
                        position: t.position,
                        yaw: t.yaw,
                        stance,
                        health: h.current,
                        max_health: h.max,
                        life,
                        weapon_id: weapon.map_or_else(String::new, |w| w.weapon_id.clone()),
                        rounds_in_weapon: weapon.and_then(|w| w.magazine).unwrap_or(0),
                        magazines: inv.magazines.clone(),
                        reserve_rounds: inv.reserve_rounds(),
                        frag_grenades: inv.frag_grenades,
                        smoke_grenades: inv.smoke_grenades,
                        field_dressings: inv.field_dressings,
                        activity: act.unwrap_or_default(),
                        fire_target: fc.and_then(|f| f.target),
                        out_of_ammo: fc.is_some_and(|f| f.out_of_ammo),
                    }
                },
            )
            .collect();
        out.sort_by_key(|s| s.id);
        out
    }

    /// Snapshot of every unit (platoons and squads), sorted by id.
    pub fn units(&mut self) -> Vec<UnitInfo> {
        let mut q = self.world.query::<(
            &Unit,
            &Members,
            &Leader,
            Option<&ParentUnit>,
            Option<&CurrentOrder>,
        )>();
        let rows: Vec<_> = q
            .iter(&self.world)
            .map(|(u, m, l, p, o)| {
                (
                    u.clone(),
                    m.0.clone(),
                    l.0,
                    p.map(|p| p.0),
                    o.copied().unwrap_or_default(),
                )
            })
            .collect();
        let world = &self.world;
        let soldier_id = |e: Entity| world.get::<Soldier>(e).map(|s| s.id);
        let unit_id = |e: Entity| world.get::<Unit>(e).map(|u| u.id);
        let mut out: Vec<UnitInfo> = rows
            .into_iter()
            .map(|(u, members, leader, parent, current)| UnitInfo {
                id: u.id,
                kind: u.kind,
                name: u.name,
                callsign: u.callsign,
                parent_id: parent.and_then(unit_id),
                leader_soldier_id: leader.and_then(soldier_id),
                soldier_ids: members.iter().filter_map(|e| soldier_id(*e)).collect(),
                child_unit_ids: members.iter().filter_map(|e| unit_id(*e)).collect(),
                order: current.order,
                status: unit_status(world, &members, current.order),
                rounds: members.iter().map(|e| soldier_rounds(world, *e)).sum(),
            })
            .collect();
        out.sort_by_key(|u| u.id);
        out
    }

    /// The client knows the terrain; it tells the sim how high the ground is
    /// under an AI soldier. (Ignored for the player, whose position is reported.)
    pub fn set_soldier_height(&mut self, soldier_id: u32, y: f32) -> bool {
        let Some(e) = self.soldier_entity(soldier_id) else {
            return false;
        };
        if e == self.player {
            return false;
        }
        if let Some(mut t) = self.world.get_mut::<Transform>(e) {
            t.position[1] = y;
            return true;
        }
        false
    }

    pub fn soldier_entity(&mut self, soldier_id: u32) -> Option<Entity> {
        let mut q = self.world.query::<(Entity, &Soldier)>();
        q.iter(&self.world)
            .find(|(_, s)| s.id == soldier_id)
            .map(|(e, _)| e)
    }

    /// The client fires the player's weapon; it reports how many rounds are left
    /// in the inserted magazine.
    pub fn report_player_rounds(&mut self, rounds: u32) {
        let Some(w) = self
            .world
            .get::<Inventory>(self.player)
            .and_then(|i| i.primary)
        else {
            return;
        };
        if let Some(mut ws) = self.world.get_mut::<WeaponState>(w) {
            if ws.magazine.is_some() {
                ws.magazine = Some(rounds.min(ws.magazine_capacity));
            }
        }
    }

    /// Swap the player's magazine for the fullest spare. Returns the rounds in
    /// the new magazine, or None if there are no loaded spares.
    pub fn player_reload(&mut self) -> Option<u32> {
        self.reload_soldier(self.player)
    }

    /// Refill the player's pouches from an ammo box. Returns the number of
    /// full spare magazines they now carry.
    pub fn player_resupply(&mut self) -> Option<u32> {
        self.resupply_soldier(self.player)
    }

    /// Refill a soldier's pouches to their basic load: every spare magazine
    /// full (partly used ones are swapped for full ones). The magazine in the
    /// weapon is left alone. Clears "out of ammo" so AI soldiers fire again.
    pub fn resupply_soldier(&mut self, soldier: Entity) -> Option<u32> {
        let role = self.world.get::<Soldier>(soldier)?.role;
        let weapon = self.world.get::<Inventory>(soldier)?.primary?;
        let capacity = self.world.get::<WeaponState>(weapon)?.magazine_capacity;
        let count = Loadout::basic(role).spare_magazines;
        self.world.get_mut::<Inventory>(soldier)?.magazines = vec![capacity; count as usize];
        if let Some(mut fc) = self.world.get_mut::<FireControl>(soldier) {
            fc.out_of_ammo = false;
        }
        let soldier_id = self.world.get::<Soldier>(soldier).map_or(0, |s| s.id);
        self.world
            .resource_mut::<SimEvents>()
            .push(SimEvent::Resupplied {
                soldier_id,
                magazines: count,
            });
        Some(count)
    }

    /// Swap a soldier's magazine for their fullest spare. Partly used magazines
    /// go back into the pouch; empty ones are dropped.
    pub fn reload_soldier(&mut self, soldier: Entity) -> Option<u32> {
        let weapon = self.world.get::<Inventory>(soldier)?.primary?;
        let mut inv = self.world.get::<Inventory>(soldier)?.clone();
        let mut ws = self.world.get::<WeaponState>(weapon)?.clone();
        let best = swap_magazine(&mut inv, &mut ws)?;
        let left = inv.magazines.len() as u32;
        *self.world.get_mut::<Inventory>(soldier)? = inv;
        *self.world.get_mut::<WeaponState>(weapon)? = ws;
        let soldier_id = self.world.get::<Soldier>(soldier).map_or(0, |s| s.id);
        self.world
            .resource_mut::<SimEvents>()
            .push(SimEvent::Reloaded {
                soldier_id,
                rounds: best,
                magazines_left: left,
            });
        Some(best)
    }

    /// Order units to move to `target`, forming up facing `facing` (yaw).
    /// Several units keep their spacing relative to each other. Applied next tick.
    pub fn order_move(&mut self, unit_ids: &[u32], target: [f32; 3], facing: f32) {
        self.world
            .resource_mut::<PendingOrders>()
            .0
            .push((unit_ids.to_vec(), Order::MoveTo { target, facing }));
    }

    /// Order units to fire at `target` until given another order. Applied next tick.
    pub fn order_fire_at(&mut self, unit_ids: &[u32], target: [f32; 3]) {
        self.world
            .resource_mut::<PendingOrders>()
            .0
            .push((unit_ids.to_vec(), Order::FireAt { target }));
    }

    /// Ground heights for many soldiers at once (the client knows the terrain).
    pub fn set_soldier_heights(&mut self, ids: &[u32], heights: &[f32]) {
        let by_id: HashMap<u32, f32> = ids.iter().copied().zip(heights.iter().copied()).collect();
        let mut q = self
            .world
            .query_filtered::<(&Soldier, &mut Transform), Without<PlayerControlled>>();
        for (s, mut t) in q.iter_mut(&mut self.world) {
            if let Some(y) = by_id.get(&s.id) {
                t.position[1] = *y;
            }
        }
    }

    pub fn clock(&self) -> SimClock {
        *self.world.resource::<SimClock>()
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// Register gameplay systems that run once per tick.
    pub fn schedule_mut(&mut self) -> &mut Schedule {
        &mut self.schedule
    }

    fn run_tick(&mut self, tick_seconds: f64) {
        if let Some(r) = self.pending_player.take() {
            let mut e = self.world.entity_mut(self.player);
            e.insert((r.transform, r.velocity, r.stance, r.gait));
        }
        {
            let mut clock = self.world.resource_mut::<SimClock>();
            clock.dt = tick_seconds;
        }
        self.schedule.run(&mut self.world);
        let mut clock = self.world.resource_mut::<SimClock>();
        clock.tick += 1;
        clock.elapsed += tick_seconds;
    }
}

/// Flat snapshot of one soldier (for the client, UI and debugging).
#[derive(Debug, Clone, PartialEq)]
pub struct SoldierInfo {
    pub id: u32,
    pub name: String,
    pub rank: Rank,
    pub role: Role,
    pub unit_id: u32,
    pub unit_name: String,
    pub fire_team: Option<FireTeam>,
    pub slot: u8,
    pub is_player: bool,
    pub is_officer: bool,
    pub position: [f32; 3],
    pub yaw: f32,
    pub stance: Stance,
    pub health: f32,
    pub max_health: f32,
    pub life: LifeState,
    pub weapon_id: String,
    pub rounds_in_weapon: u32,
    pub magazines: Vec<u32>,
    pub reserve_rounds: u32,
    pub frag_grenades: u32,
    pub smoke_grenades: u32,
    pub field_dressings: u32,
    pub activity: Activity,
    pub fire_target: Option<[f32; 3]>,
    pub out_of_ammo: bool,
}

/// Flat snapshot of one unit.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitInfo {
    pub id: u32,
    pub kind: UnitKind,
    pub name: String,
    pub callsign: String,
    pub parent_id: Option<u32>,
    pub leader_soldier_id: Option<u32>,
    /// Soldiers directly in this unit (squads).
    pub soldier_ids: Vec<u32>,
    /// Sub-units (platoons).
    pub child_unit_ids: Vec<u32>,
    pub order: Option<Order>,
    /// "holding", "moving", "in position", "firing", "out of ammo".
    pub status: &'static str,
    /// Rounds carried by the unit's soldiers (loaded + spare).
    pub rounds: u32,
}

fn soldier_rounds(world: &World, soldier: Entity) -> u32 {
    let Some(inv) = world.get::<Inventory>(soldier) else {
        return 0;
    };
    let loaded = inv
        .primary
        .and_then(|w| world.get::<WeaponState>(w))
        .and_then(|w| w.magazine)
        .unwrap_or(0);
    loaded + inv.reserve_rounds()
}

fn unit_status(world: &World, members: &[Entity], order: Option<Order>) -> &'static str {
    // Only AI soldiers carry out orders (the player moves themselves).
    let ai: Vec<Entity> = members
        .iter()
        .copied()
        .filter(|e| world.get::<PlayerControlled>(*e).is_none())
        .collect();
    match order {
        None => "holding",
        Some(Order::MoveTo { .. }) => {
            if ai
                .iter()
                .any(|e| world.get::<MoveGoal>(*e).is_some_and(|g| g.0.is_some()))
            {
                "moving"
            } else {
                "in position"
            }
        }
        Some(Order::FireAt { .. }) => {
            if !ai.is_empty()
                && ai
                    .iter()
                    .all(|e| world.get::<FireControl>(*e).is_some_and(|f| f.out_of_ammo))
            {
                "out of ammo"
            } else {
                "firing"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy_ecs::query::With;
    use bevy_ecs::resource::Resource;
    use bevy_ecs::system::{Query, ResMut};

    use crate::components::{
        Activity, Gait, LifeState, PlayerControlled, PlayerReport, Rank, Role, Stance, Transform,
        UnitKind, Velocity,
    };
    use crate::events::SimEvent;

    use super::{Sim, SimConfig};

    fn sim_at(hz: f64) -> Sim {
        Sim::new(SimConfig {
            tick_rate_hz: hz,
            ..Default::default()
        })
    }

    #[test]
    fn runs_whole_ticks_and_carries_remainder() {
        let mut sim = sim_at(10.0);
        assert_eq!(sim.update(0.25), 2);
        assert_eq!(sim.update(0.04), 0); // 0.09 accumulated: not a full tick yet
        assert_eq!(sim.update(0.02), 1);
        assert_eq!(sim.clock().tick, 3);
        assert!((sim.clock().elapsed - 0.3).abs() < 1e-9);
    }

    #[test]
    fn paused_runs_nothing_but_step_still_works() {
        let mut sim = sim_at(10.0);
        sim.config.paused = true;
        assert_eq!(sim.update(1.0), 0);
        sim.step();
        assert_eq!(sim.clock().tick, 1);
    }

    #[test]
    fn caps_ticks_after_a_hitch() {
        let mut sim = sim_at(60.0);
        sim.config.max_ticks_per_update = 4;
        assert_eq!(sim.update(10.0), 4);
        assert!(sim.update(0.0) == 0 && sim.alpha() <= 1.0);
    }

    #[test]
    fn time_scale_speeds_up_sim() {
        let mut sim = sim_at(10.0);
        sim.config.time_scale = 2.0;
        assert_eq!(sim.update(0.1), 2);
    }

    #[test]
    fn player_report_applies_on_next_tick() {
        let mut sim = sim_at(10.0);
        let r = PlayerReport {
            transform: Transform {
                position: [1.0, 2.0, 3.0],
                yaw: 0.5,
            },
            velocity: Velocity([3.0, 0.0, 4.0]),
            stance: Stance::Crouching,
            gait: Gait::Walk,
        };
        sim.report_player(r);
        assert_eq!(sim.player_state(), PlayerReport::default()); // not yet
        sim.update(0.1);
        assert_eq!(sim.player_state(), r);
        assert_eq!(sim.player_state().velocity.horizontal_speed(), 5.0);
    }

    #[test]
    fn systems_can_read_player() {
        #[derive(Resource, Default)]
        struct SeenX(f32);
        let mut sim = sim_at(10.0);
        sim.world_mut().init_resource::<SeenX>();
        sim.schedule_mut().add_systems(
            |q: Query<&Transform, With<PlayerControlled>>, mut seen: ResMut<SeenX>| {
                seen.0 = q.single().unwrap().position[0];
            },
        );
        sim.report_player(PlayerReport {
            transform: Transform {
                position: [42.0, 0.0, 0.0],
                yaw: 0.0,
            },
            ..Default::default()
        });
        sim.update(0.1);
        assert_eq!(sim.world().resource::<SeenX>().0, 42.0);
    }

    #[test]
    fn movement_change_sends_event_back() {
        let mut sim = sim_at(10.0);
        sim.update(0.1);
        assert!(
            sim.drain_events().is_empty(),
            "no event for the initial state"
        );

        sim.report_player(PlayerReport {
            stance: Stance::Prone,
            gait: Gait::Walk,
            ..Default::default()
        });
        sim.update(0.1);
        let events = sim.drain_events();
        assert!(matches!(
            events.as_slice(),
            [SimEvent::PlayerMovementChanged {
                stance: Stance::Prone,
                gait: Gait::Walk,
                ..
            }]
        ));

        // Same state again: nothing new.
        sim.report_player(PlayerReport {
            stance: Stance::Prone,
            gait: Gait::Walk,
            ..Default::default()
        });
        sim.update(0.1);
        assert!(sim.drain_events().is_empty());
    }

    #[test]
    fn platoon_has_hq_of_six_and_three_sections_of_ten() {
        let mut sim = sim_at(10.0);
        sim.spawn_platoon([0.0, 0.0, 0.0], 0.0);
        let units = sim.units();
        let platoon = units.iter().find(|u| u.kind == UnitKind::Platoon).unwrap();
        assert_eq!(platoon.child_unit_ids.len(), 4);
        let squads: Vec<_> = units
            .iter()
            .filter(|u| u.parent_id == Some(platoon.id))
            .collect();
        assert_eq!(
            squads
                .iter()
                .filter(|u| u.kind == UnitKind::PlatoonHeadquarters)
                .count(),
            1
        );
        assert_eq!(
            squads
                .iter()
                .filter(|u| u.kind == UnitKind::RifleSection)
                .count(),
            3
        );
        for sq in &squads {
            let expected = if sq.kind == UnitKind::PlatoonHeadquarters {
                6
            } else {
                10
            };
            assert_eq!(sq.soldier_ids.len(), expected, "{}", sq.name);
            assert_eq!(sq.leader_soldier_id, Some(sq.soldier_ids[0]));
        }
        assert_eq!(sim.soldiers().len(), 36);
    }

    #[test]
    fn player_is_platoon_commander_and_only_officer() {
        let mut sim = sim_at(10.0);
        sim.spawn_platoon([0.0, 0.0, 0.0], 0.0);
        let soldiers = sim.soldiers();
        let players: Vec<_> = soldiers.iter().filter(|s| s.is_player).collect();
        assert_eq!(players.len(), 1);
        assert_eq!(players[0].role, Role::PlatoonCommander);
        assert_eq!(players[0].rank, Rank::Lieutenant);
        assert!(players[0].is_officer);
        assert_eq!(soldiers.iter().filter(|s| s.is_officer).count(), 1);
        let platoon = sim
            .units()
            .into_iter()
            .find(|u| u.kind == UnitKind::Platoon)
            .unwrap();
        assert_eq!(platoon.leader_soldier_id, Some(players[0].id));
    }

    #[test]
    fn everyone_has_an_m16a2_and_basic_load() {
        let mut sim = sim_at(10.0);
        sim.spawn_platoon([0.0, 0.0, 0.0], 0.0);
        for s in sim.soldiers() {
            assert_eq!(s.weapon_id, "m16a2");
            assert_eq!(s.rounds_in_weapon, 30);
            assert_eq!(s.magazines, vec![30; 6]);
            assert_eq!(s.reserve_rounds, 180);
            assert_eq!(s.life, LifeState::Healthy);
            assert_eq!(s.health, 100.0);
        }
    }

    #[test]
    fn formation_faces_yaw_and_spreads_out() {
        let mut sim = sim_at(10.0);
        sim.spawn_platoon([100.0, 5.0, 50.0], 0.0);
        let soldiers = sim.soldiers();
        // yaw 0 looks down -Z, so the rifle squads (ahead) have smaller z.
        let ahead = soldiers
            .iter()
            .filter(|s| s.unit_name.contains("Section"))
            .all(|s| s.position[2] < 50.0);
        assert!(ahead);
        let mut seen = std::collections::HashSet::new();
        for s in soldiers.iter().filter(|s| !s.is_player) {
            assert!(
                seen.insert(((s.position[0] * 10.0) as i32, (s.position[2] * 10.0) as i32)),
                "two soldiers share a spot"
            );
        }
    }

    #[test]
    fn resupply_refills_every_spare_magazine() {
        let mut sim = Sim::default();
        sim.spawn_platoon([0.0; 3], 0.0);
        for _ in 0..7 {
            sim.report_player_rounds(0);
            sim.player_reload();
        }
        let before = sim.soldiers().into_iter().find(|s| s.is_player).unwrap();
        assert!(before.magazines.is_empty(), "spent every spare first");

        assert_eq!(sim.player_resupply(), Some(6));
        let after = sim.soldiers().into_iter().find(|s| s.is_player).unwrap();
        assert_eq!(after.magazines, vec![30; 6]);
        assert_eq!(after.rounds_in_weapon, before.rounds_in_weapon);
        assert!(
            sim.drain_events()
                .iter()
                .any(|e| matches!(e, SimEvent::Resupplied { magazines: 6, .. }))
        );
    }

    #[test]
    fn reload_takes_fullest_spare_and_keeps_partial() {
        let mut sim = sim_at(10.0);
        sim.spawn_platoon([0.0, 0.0, 0.0], 0.0);
        sim.report_player_rounds(12);
        assert_eq!(sim.player_reload(), Some(30));
        let me = sim.soldiers().into_iter().find(|s| s.is_player).unwrap();
        assert_eq!(me.rounds_in_weapon, 30);
        assert_eq!(me.magazines.len(), 6); // 5 full + the partial 12
        assert!(me.magazines.contains(&12));
        // Empty magazines are dropped, and you run out eventually.
        for _ in 0..10 {
            sim.report_player_rounds(0);
            sim.player_reload();
        }
        assert_eq!(sim.player_reload(), None);
    }

    #[test]
    fn height_updates_ai_but_not_player() {
        let mut sim = sim_at(10.0);
        sim.spawn_platoon([0.0, 0.0, 0.0], 0.0);
        let soldiers = sim.soldiers();
        let ai = soldiers.iter().find(|s| !s.is_player).unwrap().id;
        let me = soldiers.iter().find(|s| s.is_player).unwrap().id;
        assert!(sim.set_soldier_height(ai, 12.5));
        assert!(!sim.set_soldier_height(me, 99.0));
        assert_eq!(
            sim.soldiers().iter().find(|s| s.id == ai).unwrap().position[1],
            12.5
        );
    }

    #[test]
    fn units_use_canadian_names_and_ranks() {
        let mut sim = sim_at(10.0);
        sim.spawn_platoon([0.0, 0.0, 0.0], 0.0);
        let names: Vec<String> = sim
            .units()
            .into_iter()
            .map(|u| format!("{} {}", u.name, u.callsign))
            .collect();
        assert_eq!(
            names,
            [
                "1 Platoon 11",
                "Platoon HQ 11",
                "1 Section 11A",
                "2 Section 11B",
                "3 Section 11C"
            ]
        );
        let soldiers = sim.soldiers();
        let sect_comds = soldiers
            .iter()
            .filter(|s| s.role == Role::SectionCommander && s.rank == Rank::Sergeant);
        assert_eq!(sect_comds.count(), 3);
        assert!(
            soldiers
                .iter()
                .any(|s| s.role == Role::PlatoonWarrant && s.rank == Rank::WarrantOfficer)
        );
    }

    fn unit_id(sim: &mut Sim, name: &str) -> u32 {
        sim.units().into_iter().find(|u| u.name == name).unwrap().id
    }

    fn run_seconds(sim: &mut Sim, s: f64) -> Vec<SimEvent> {
        let mut events = Vec::new();
        let ticks = (s * sim.config.tick_rate_hz).round() as u32;
        for _ in 0..ticks {
            sim.step();
            events.extend(sim.drain_events());
        }
        events
    }

    #[test]
    fn move_order_walks_section_into_formation_and_reports() {
        let mut sim = sim_at(30.0);
        sim.spawn_platoon([0.0, 0.0, 0.0], 0.0);
        let s1 = unit_id(&mut sim, "1 Section");
        sim.order_move(&[s1], [10.0, 0.0, -100.0], 0.0);
        let events = run_seconds(&mut sim, 1.0);
        assert!(events.iter().any(|e| matches!(e, SimEvent::OrderAcknowledged { unit_id, order: "move", .. } if *unit_id == s1)));
        assert_eq!(
            sim.units().iter().find(|u| u.id == s1).unwrap().status,
            "moving"
        );
        let events = run_seconds(&mut sim, 60.0);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SimEvent::UnitInPosition { unit_id, .. } if *unit_id == s1))
        );
        assert_eq!(
            sim.units().iter().find(|u| u.id == s1).unwrap().status,
            "in position"
        );
        // The section commander's slot is 2 m behind the anchor.
        let sc = sim
            .soldiers()
            .into_iter()
            .find(|s| s.unit_id == s1 && s.slot == 0)
            .unwrap();
        assert!(
            (sc.position[0] - 10.0).abs() < 0.01 && (sc.position[2] - -98.0).abs() < 0.01,
            "{:?}",
            sc.position
        );
        assert_eq!(sc.activity, Activity::Idle);
    }

    #[test]
    fn group_move_keeps_sections_apart() {
        let mut sim = sim_at(30.0);
        sim.spawn_platoon([0.0, 0.0, 0.0], 0.0);
        let ids: Vec<u32> = ["1 Section", "2 Section", "3 Section"]
            .iter()
            .map(|n| unit_id(&mut sim, n))
            .collect();
        sim.order_move(&ids, [0.0, 0.0, -150.0], 0.0);
        run_seconds(&mut sim, 80.0);
        let comds: Vec<[f32; 3]> = sim
            .soldiers()
            .into_iter()
            .filter(|s| s.role == Role::SectionCommander)
            .map(|s| s.position)
            .collect();
        // Started 35 m apart left/centre/right; still roughly 35 m apart.
        assert!(
            (comds[0][0] - -35.0).abs() < 2.0
                && comds[1][0].abs() < 2.0
                && (comds[2][0] - 35.0).abs() < 2.0,
            "{comds:?}"
        );
    }

    #[test]
    fn hq_order_never_moves_the_player() {
        let mut sim = sim_at(30.0);
        sim.spawn_platoon([0.0, 0.0, 0.0], 0.0);
        let hq = unit_id(&mut sim, "Platoon HQ");
        sim.order_move(&[hq], [50.0, 0.0, 50.0], 0.0);
        run_seconds(&mut sim, 30.0);
        let me = sim.soldiers().into_iter().find(|s| s.is_player).unwrap();
        assert_eq!(me.position, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn fire_order_spends_rounds_reloads_and_reports_shots() {
        let mut sim = sim_at(30.0);
        sim.spawn_platoon([0.0, 0.0, 0.0], 0.0);
        let s2 = unit_id(&mut sim, "2 Section");
        let before = sim.units().iter().find(|u| u.id == s2).unwrap().rounds;
        sim.order_fire_at(&[s2], [0.0, 0.0, -200.0]);
        let events = run_seconds(&mut sim, 90.0);
        let shots: u32 = events
            .iter()
            .filter_map(|e| match e {
                SimEvent::ShotFired { rounds, .. } => Some(*rounds),
                _ => None,
            })
            .sum();
        let after = sim.units().iter().find(|u| u.id == s2).unwrap().rounds;
        assert!(shots > 100, "only {shots} rounds fired");
        assert_eq!(
            before - after,
            shots,
            "rounds fired must come out of inventories"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SimEvent::Reloaded { .. })),
            "nobody reloaded"
        );
        assert_eq!(
            sim.units().iter().find(|u| u.id == s2).unwrap().status,
            "firing"
        );
        // Everyone faces the target (yaw 0 = looking down -Z).
        for s in sim.soldiers().into_iter().filter(|s| s.unit_id == s2) {
            assert!(s.yaw.abs() < 0.4, "{} faces {}", s.name, s.yaw);
        }
    }

    #[test]
    fn soldiers_run_dry_and_say_so() {
        let mut sim = sim_at(30.0);
        sim.spawn_platoon([0.0, 0.0, 0.0], 0.0);
        let s3 = unit_id(&mut sim, "3 Section");
        sim.order_fire_at(&[s3], [0.0, 0.0, -100.0]);
        let events = run_seconds(&mut sim, 900.0);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SimEvent::OutOfAmmo { .. }))
        );
        let u = sim.units().into_iter().find(|u| u.id == s3).unwrap();
        assert_eq!((u.rounds, u.status), (0, "out of ammo"));
    }

    #[test]
    fn new_order_replaces_fire_with_move() {
        let mut sim = sim_at(30.0);
        sim.spawn_platoon([0.0, 0.0, 0.0], 0.0);
        let s1 = unit_id(&mut sim, "1 Section");
        sim.order_fire_at(&[s1], [0.0, 0.0, -100.0]);
        run_seconds(&mut sim, 5.0);
        sim.order_move(&[s1], [0.0, 0.0, 40.0], std::f32::consts::PI);
        let events = run_seconds(&mut sim, 40.0);
        let late_shots = events
            .iter()
            .skip(5)
            .filter(|e| matches!(e, SimEvent::ShotFired { .. }))
            .count();
        assert_eq!(late_shots, 0);
        assert_eq!(
            sim.units().iter().find(|u| u.id == s1).unwrap().status,
            "in position"
        );
    }

    #[test]
    fn invalid_rate_is_ignored() {
        let mut sim = sim_at(0.0);
        assert_eq!(sim.update(1.0), 0);
    }

    #[test]
    fn systems_run_each_tick() {
        #[derive(Resource, Default)]
        struct Count(u32);
        let mut sim = sim_at(10.0);
        sim.world_mut().init_resource::<Count>();
        sim.schedule_mut()
            .add_systems(|mut c: ResMut<Count>| c.0 += 1);
        sim.update(0.5);
        assert_eq!(sim.world().resource::<Count>().0, 5);
    }
}
