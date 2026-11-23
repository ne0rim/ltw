//! `SimBridge` node: owns the Rust sim and drives its ticker from Godot's frame loop.
//!
//! Every ticker setting is an exported property. You can change it in the
//! Inspector, including while the game runs (Remote scene tree).

use godot::prelude::*;
use ltw_core::{Gait, PlayerReport, SimEvent, Stance, Transform, Velocity};
use ltw_core::sim::{Sim, SimConfig, defaults};

#[derive(GodotClass)]
#[class(init, base = Node)]
pub struct SimBridge {
    /// Sim ticks per second.
    #[export(range = (1.0, 240.0, 1.0, or_greater))]
    #[init(val = defaults::TICK_RATE_HZ)]
    tick_rate_hz: f64,

    /// Multiplier on real time (0.5 = half speed, 2 = double).
    #[export(range = (0.0, 8.0, 0.05, or_greater))]
    #[init(val = defaults::TIME_SCALE)]
    time_scale: f64,

    /// Most ticks allowed in one frame, so a long hitch can't snowball.
    #[export(range = (1.0, 64.0, 1.0))]
    #[init(val = defaults::MAX_TICKS_PER_UPDATE as i64)]
    max_ticks_per_frame: i64,

    /// Freeze the sim. `step()` still advances one tick.
    #[export]
    paused: bool,

    sim: Sim,
    base: Base<Node>,
}

#[godot_api]
impl INode for SimBridge {
    fn process(&mut self, delta: f64) {
        self.sync_config();
        let ran = self.sim.update(delta);
        if ran > 0 {
            let tick = self.sim.clock().tick as i64;
            self.signals().ticked().emit(tick, ran as i64);
            self.flush_events();
        }
    }
}

#[godot_api]
impl SimBridge {
    /// Emitted after each frame in which at least one tick ran.
    #[signal]
    fn ticked(tick: i64, ticks_this_frame: i64);

    /// A message from the sim (order acks, AI reports, ...). `kind` names the
    /// event; `data` holds its fields.
    #[signal]
    fn sim_event(kind: GString, data: VarDictionary);

    /// Ticks completed since start.
    #[func]
    fn get_tick(&self) -> i64 {
        self.sim.clock().tick as i64
    }

    /// Game seconds elapsed in the sim.
    #[func]
    fn get_sim_time(&self) -> f64 {
        self.sim.clock().elapsed
    }

    /// 0..1 progress toward the next tick, for smoothing visuals.
    #[func]
    fn get_alpha(&self) -> f64 {
        self.sim.alpha()
    }

    /// Godot owns player movement. Call this every physics frame to tell the
    /// sim the player's state. It's applied at the start of the next tick.
    /// `stance` and `gait` are the `Player.Stance` / `Player.Gait` enum values.
    #[func]
    fn report_player(&mut self, position: Vector3, yaw: f32, velocity: Vector3, stance: i64, gait: i64) {
        let (Some(stance), Some(gait)) = (Stance::from_index(stance), Gait::from_index(gait)) else {
            godot_error!("report_player: bad stance {stance} or gait {gait}");
            return;
        };
        self.sim.report_player(PlayerReport {
            transform: Transform { position: [position.x, position.y, position.z], yaw },
            velocity: Velocity([velocity.x, velocity.y, velocity.z]),
            stance,
            gait,
        });
    }

    /// The player's state as the sim knows it (updates once per tick):
    /// position, yaw, velocity, speed, stance, gait.
    #[func]
    fn get_player_state(&self) -> VarDictionary {
        let s = self.sim.player_state();
        let [px, py, pz] = s.transform.position;
        let [vx, vy, vz] = s.velocity.0;
        vdict! {
            "position" => &Vector3::new(px, py, pz).to_variant(),
            "yaw" => &s.transform.yaw.to_variant(),
            "velocity" => &Vector3::new(vx, vy, vz).to_variant(),
            "speed" => &s.velocity.horizontal_speed().to_variant(),
            "stance" => &(s.stance as i64).to_variant(),
            "stance_name" => &s.stance.name().to_variant(),
            "gait" => &(s.gait as i64).to_variant(),
            "gait_name" => &s.gait.name().to_variant(),
        }
    }

    /// Advance exactly one tick (works while paused).
    #[func]
    fn step(&mut self) {
        self.sync_config();
        self.sim.step();
        let tick = self.sim.clock().tick as i64;
        self.signals().ticked().emit(tick, 1);
        self.flush_events();
    }

    /// Emit a `sim_event` signal for everything the sim produced.
    fn flush_events(&mut self) {
        for event in self.sim.drain_events() {
            let (kind, data) = match event {
                SimEvent::PlayerMovementChanged { tick, stance, gait, speed } => (
                    "player_movement_changed",
                    vdict! {
                        "tick" => &(tick as i64).to_variant(),
                        "stance" => &stance.name().to_variant(),
                        "gait" => &gait.name().to_variant(),
                        "speed" => &speed.to_variant(),
                    },
                ),
            };
            self.signals().sim_event().emit(&GString::from(kind), &data);
        }
    }

    fn sync_config(&mut self) {
        self.sim.config = SimConfig {
            tick_rate_hz: self.tick_rate_hz,
            time_scale: self.time_scale,
            max_ticks_per_update: self.max_ticks_per_frame.clamp(1, 1024) as u32,
            paused: self.paused,
        };
    }
}
