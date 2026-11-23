//! `SimBridge` node: owns the Rust sim and drives its ticker from Godot's frame loop.
//!
//! Every ticker setting is an exported property. You can change it in the
//! Inspector, including while the game runs (Remote scene tree).

use godot::prelude::*;
use ltw_core::Transform;
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
        }
    }
}

#[godot_api]
impl SimBridge {
    /// Emitted after each frame in which at least one tick ran.
    #[signal]
    fn ticked(tick: i64, ticks_this_frame: i64);

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

    /// Godot owns player movement. Call this every physics frame to tell
    /// the sim where the player is. It's applied at the start of the next tick.
    #[func]
    fn report_player_transform(&mut self, position: Vector3, yaw: f32) {
        self.sim.report_player_transform(Transform {
            position: [position.x, position.y, position.z],
            yaw,
        });
    }

    /// Where the sim currently thinks the player is (updates once per tick).
    #[func]
    fn get_player_position(&self) -> Vector3 {
        let [x, y, z] = self.sim.player_transform().position;
        Vector3::new(x, y, z)
    }

    /// Advance exactly one tick (works while paused).
    #[func]
    fn step(&mut self) {
        self.sync_config();
        self.sim.step();
        let tick = self.sim.clock().tick as i64;
        self.signals().ticked().emit(tick, 1);
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
