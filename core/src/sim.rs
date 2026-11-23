//! Fixed-timestep simulation ticker.
//!
//! The sim advances in whole ticks of a fixed length. Callers feed in real
//! frame time through [`Sim::update`]. Each update runs as many ticks as fit
//! in the accumulated time, so the sim stays deterministic however fast the
//! client renders.

use bevy_ecs::prelude::*;

use crate::components::{PlayerControlled, Transform};

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
    /// Latest transform reported by the client, applied at the next tick.
    pending_player: Option<Transform>,
}

impl Default for Sim {
    fn default() -> Self {
        Self::new(SimConfig::default())
    }
}

impl Sim {
    pub fn new(config: SimConfig) -> Self {
        let mut world = World::new();
        world.insert_resource(SimClock::default());
        let player = world.spawn((PlayerControlled, Transform::default())).id();
        Self {
            config,
            world,
            schedule: Schedule::default(),
            accumulator: 0.0,
            player,
            pending_player: None,
        }
    }

    /// Advance by `real_delta` seconds of wall time. Returns how many ticks ran.
    pub fn update(&mut self, real_delta: f64) -> u32 {
        let Some(tick_seconds) = self.config.tick_seconds() else { return 0 };
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
        self.config.tick_seconds().map_or(0.0, |t| (self.accumulator / t).clamp(0.0, 1.0))
    }

    /// Client reports where the player is now. Only the newest report counts.
    /// It reaches the world at the start of the next tick, so every system in
    /// a tick sees the same position.
    pub fn report_player_transform(&mut self, transform: Transform) {
        self.pending_player = Some(transform);
    }

    /// The player's transform as the sim currently knows it.
    pub fn player_transform(&self) -> Transform {
        *self.world.get::<Transform>(self.player).expect("player entity always exists")
    }

    pub fn player_entity(&self) -> Entity {
        self.player
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
        if let Some(t) = self.pending_player.take() {
            *self.world.get_mut::<Transform>(self.player).expect("player entity always exists") = t;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sim_at(hz: f64) -> Sim {
        Sim::new(SimConfig { tick_rate_hz: hz, ..Default::default() })
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
        let t = Transform { position: [1.0, 2.0, 3.0], yaw: 0.5 };
        sim.report_player_transform(t);
        assert_eq!(sim.player_transform(), Transform::default()); // not yet
        sim.update(0.1);
        assert_eq!(sim.player_transform(), t);
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
        sim.report_player_transform(Transform { position: [42.0, 0.0, 0.0], yaw: 0.0 });
        sim.update(0.1);
        assert_eq!(sim.world().resource::<SeenX>().0, 42.0);
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
        sim.schedule_mut().add_systems(|mut c: ResMut<Count>| c.0 += 1);
        sim.update(0.5);
        assert_eq!(sim.world().resource::<Count>().0, 5);
    }
}
