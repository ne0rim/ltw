//! `SimBridge` node: owns the Rust sim and drives its ticker from Godot's frame loop.
//!
//! Every ticker setting is an exported property. You can change it in the
//! Inspector, including while the game runs (Remote scene tree).

use std::path::Path;

use godot::builtin::{
    GString, PackedFloat32Array, PackedInt32Array, VarArray, VarDictionary, Vector3, vdict,
};
use godot::classes::{INode, Node};
use godot::global::{godot_error, godot_print};
use godot::meta::ToGodot;
use godot::obj::{Base, WithUserSignals};
use godot::register::{GodotClass, godot_api};

use ltw_core::sim::{Sim, SimConfig, defaults};
use ltw_core::{Gait, PlayerReport, SimEvent, Stance, Transform, Velocity};

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
    fn report_player(
        &mut self,
        position: Vector3,
        yaw: f32,
        velocity: Vector3,
        stance: i64,
        gait: i64,
    ) {
        let (Some(stance), Some(gait)) = (Stance::from_index(stance), Gait::from_index(gait))
        else {
            godot_error!("report_player: bad stance {stance} or gait {gait}");
            return;
        };
        self.sim.report_player(PlayerReport {
            transform: Transform {
                position: [position.x, position.y, position.z],
                yaw,
            },
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

    // ------------------------------------------------------------ save files

    /// Replace the sim with the save at `path` (an absolute OS path; use
    /// `ProjectSettings.globalize_path("user://...")`). Returns true if a save
    /// was loaded; false if there is none or it couldn't be read (the error is
    /// logged and the current sim is kept).
    #[func]
    fn load_game(&mut self, path: GString) -> bool {
        self.sync_config();
        let path = path.to_string();
        match Sim::load_from_file(self.sim.config, Path::new(&path)) {
            Ok(Some(sim)) => {
                self.sim = sim;
                godot_print!("[save] loaded {path}");
                true
            }
            Ok(None) => false,
            Err(e) => {
                godot_error!("[save] could not load {path}: {e}");
                false
            }
        }
    }

    /// Write the sim to `path` (an absolute OS path). Returns true on success.
    #[func]
    fn save_game(&mut self, path: GString) -> bool {
        let path = path.to_string();
        match self.sim.save_to_file(Path::new(&path)) {
            Ok(()) => {
                godot_print!("[save] saved {path}");
                true
            }
            Err(e) => {
                godot_error!("[save] could not save {path}: {e}");
                false
            }
        }
    }

    // ------------------------------------------------------------ platoon

    /// Start a map from the core's map files (e.g. "test_area"): spawns every
    /// force on it, both sides. Returns false for an unknown map.
    #[func]
    fn start_map(&mut self, id: GString) -> bool {
        match self.sim.start_map(&id.to_string()) {
            Ok(()) => true,
            Err(e) => {
                godot_error!("start_map: {e}");
                false
            }
        }
    }

    /// The map being played: id, name, size_m, and where the player's platoon
    /// starts (player_start, player_yaw). Empty if no map is active.
    #[func]
    fn get_map(&self) -> VarDictionary {
        let Some(map) = self.sim.map() else {
            return VarDictionary::new();
        };
        let start = map.forces.iter().find(|f| f.led_by_player);
        vdict! {
            "id" => &map.id.to_variant(),
            "name" => &map.name.to_variant(),
            "size_m" => &map.size_m.to_variant(),
            "has_player_start" => &start.is_some().to_variant(),
            "player_start" => &v3(start.map_or([0.0; 3], |f| f.position)).to_variant(),
            "player_yaw" => &start.map_or(0.0, |f| f.yaw()).to_variant(),
        }
    }

    /// "blue" or "red": the side the player fights for.
    #[func]
    fn get_player_side(&self) -> GString {
        GString::from(self.sim.player_side().name())
    }

    /// Every soldier as a Dictionary (id, name, rank, role, unit, position,
    /// health, life, weapon, rounds, magazines, ...), sorted by id.
    #[func]
    fn get_soldiers(&mut self) -> VarArray {
        let mut out = VarArray::new();
        for s in self.sim.soldiers() {
            let [px, py, pz] = s.position;
            let mags: PackedInt32Array = s.magazines.iter().map(|m| *m as i32).collect();
            let d = vdict! {
                "id" => &(s.id as i64).to_variant(),
                "side" => &s.side.name().to_variant(),
                "name" => &s.name.to_variant(),
                "rank" => &s.rank.abbrev().to_variant(),
                "role" => &s.role.name().to_variant(),
                "role_abbrev" => &s.role.abbrev().to_variant(),
                "unit_id" => &(s.unit_id as i64).to_variant(),
                "unit_name" => &s.unit_name.to_variant(),
                "fire_team" => &s.fire_team.map_or("", |t| t.name()).to_variant(),
                "slot" => &(s.slot as i64).to_variant(),
                "is_player" => &s.is_player.to_variant(),
                "is_officer" => &s.is_officer.to_variant(),
                "position" => &Vector3::new(px, py, pz).to_variant(),
                "yaw" => &s.yaw.to_variant(),
                "stance" => &s.stance.name().to_variant(),
                "health" => &s.health.to_variant(),
                "max_health" => &s.max_health.to_variant(),
                "life" => &s.life.name().to_variant(),
                "veterancy" => &s.veterancy.name().to_variant(),
                "suppression" => &s.suppression.to_variant(),
                "weapon_id" => &s.weapon_id.to_variant(),
                "rounds_in_weapon" => &(s.rounds_in_weapon as i64).to_variant(),
                "magazines" => &mags.to_variant(),
                "reserve_rounds" => &(s.reserve_rounds as i64).to_variant(),
                "frag_grenades" => &(s.frag_grenades as i64).to_variant(),
                "smoke_grenades" => &(s.smoke_grenades as i64).to_variant(),
                "field_dressings" => &(s.field_dressings as i64).to_variant(),
                "activity" => &s.activity.name().to_variant(),
                "has_fire_target" => &s.fire_target.is_some().to_variant(),
                "fire_target" => &v3(s.fire_target.unwrap_or_default()).to_variant(),
                "out_of_ammo" => &s.out_of_ammo.to_variant(),
            };
            out.push(&d.to_variant());
        }
        out
    }

    /// Every unit (platoon, squads) as a Dictionary, sorted by id.
    #[func]
    fn get_units(&mut self) -> VarArray {
        let mut out = VarArray::new();
        for u in self.sim.units() {
            let soldiers: PackedInt32Array = u.soldier_ids.iter().map(|i| *i as i32).collect();
            let children: PackedInt32Array = u.child_unit_ids.iter().map(|i| *i as i32).collect();
            let kind = u.kind.key();
            let d = vdict! {
                "id" => &(u.id as i64).to_variant(),
                "kind" => &kind.to_variant(),
                "name" => &u.name.to_variant(),
                "callsign" => &u.callsign.to_variant(),
                "parent_id" => &u.parent_id.map_or(-1, |p| p as i64).to_variant(),
                "leader_soldier_id" => &u.leader_soldier_id.map_or(-1, |p| p as i64).to_variant(),
                "soldier_ids" => &soldiers.to_variant(),
                "child_unit_ids" => &children.to_variant(),
                "order" => &u.order.map_or("none", |o| o.name()).to_variant(),
                "order_target" => &v3(u.order.map_or([0.0; 3], |o| o.target())).to_variant(),
                "status" => &u.status.to_variant(),
                "rounds" => &(u.rounds as i64).to_variant(),
                "side" => &u.side.name().to_variant(),
                "controller" => &u.controller.name().to_variant(),
                "task" => &u.task.map_or("none", |t| t.name()).to_variant(),
                "task_point" => &v3(u.task.map_or([0.0; 3], |t| t.point())).to_variant(),
                "task_stage" => &u.task_stage.map_or("", |t| t.name()).to_variant(),
                "playbook" => &u.playbook.map_or("", |p| p.name()).to_variant(),
            };
            out.push(&d.to_variant());
        }
        out
    }

    /// Order units (by unit id) to move to `target` and face `facing` (yaw).
    /// Applied on the next tick; each unit answers with an `order_acknowledged` event.
    #[func]
    fn order_move(&mut self, unit_ids: PackedInt32Array, target: Vector3, facing: f32) {
        let ids: Vec<u32> = unit_ids.as_slice().iter().map(|i| *i as u32).collect();
        self.sim
            .order_move(&ids, [target.x, target.y, target.z], facing);
    }

    /// Order units (by unit id) to fire at `target` until given another order.
    #[func]
    fn order_fire_at(&mut self, unit_ids: PackedInt32Array, target: Vector3) {
        let ids: Vec<u32> = unit_ids.as_slice().iter().map(|i| *i as u32).collect();
        self.sim.order_fire_at(&ids, [target.x, target.y, target.z]);
    }

    /// Ground heights for many AI soldiers at once.
    #[func]
    fn set_soldier_heights(&mut self, soldier_ids: PackedInt32Array, heights: PackedFloat32Array) {
        let ids: Vec<u32> = soldier_ids.as_slice().iter().map(|i| *i as u32).collect();
        self.sim.set_soldier_heights(&ids, heights.as_slice());
    }

    /// Tell the sim the ground height under an AI soldier.
    #[func]
    fn set_soldier_height(&mut self, soldier_id: i64, y: f32) -> bool {
        self.sim.set_soldier_height(soldier_id as u32, y)
    }

    /// The player fired one round from `origin` along `direction` (use the
    /// camera: its position and -basis.z, so rounds go where the crosshair
    /// points). The sim traces it and applies any hit next tick.
    #[func]
    fn report_player_shot(&mut self, origin: Vector3, direction: Vector3) -> bool {
        self.sim.player_fire(
            [origin.x, origin.y, origin.z],
            [direction.x, direction.y, direction.z],
        )
    }

    /// Developer tool: bring a dead player back to life.
    #[func]
    fn dev_revive_player(&mut self) {
        self.sim.dev_revive_player();
    }

    /// The player fired: rounds left in the inserted magazine.
    #[func]
    fn report_player_rounds(&mut self, rounds: i64) {
        self.sim.report_player_rounds(rounds.max(0) as u32);
    }

    /// Swap the player's magazine. Returns rounds in the new magazine, or -1
    /// if there are no loaded spare magazines left.
    #[func]
    fn player_reload(&mut self) -> i64 {
        self.sim.player_reload().map_or(-1, |r| r as i64)
    }

    /// Refill the player's spare magazines from an ammo box. Returns how many
    /// full spare magazines they now carry, or -1 if they can't be resupplied.
    #[func]
    fn player_resupply(&mut self) -> i64 {
        self.sim.player_resupply().map_or(-1, |m| m as i64)
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
                SimEvent::PlayerMovementChanged {
                    tick,
                    stance,
                    gait,
                    speed,
                } => (
                    "player_movement_changed",
                    vdict! {
                        "tick" => &(tick as i64).to_variant(),
                        "stance" => &stance.name().to_variant(),
                        "gait" => &gait.name().to_variant(),
                        "speed" => &speed.to_variant(),
                    },
                ),
                SimEvent::PlatoonSpawned {
                    unit_id,
                    soldiers,
                    side,
                } => (
                    "platoon_spawned",
                    vdict! {
                        "unit_id" => &(unit_id as i64).to_variant(),
                        "soldiers" => &(soldiers as i64).to_variant(),
                        "side" => &side.name().to_variant(),
                    },
                ),
                SimEvent::TaskAssigned {
                    unit_id,
                    callsign,
                    task,
                } => (
                    "task_assigned",
                    vdict! {
                        "unit_id" => &(unit_id as i64).to_variant(),
                        "callsign" => &callsign.to_variant(),
                        "task" => &task.to_variant(),
                    },
                ),
                SimEvent::TaskComplete {
                    unit_id,
                    callsign,
                    task,
                } => (
                    "task_complete",
                    vdict! {
                        "unit_id" => &(unit_id as i64).to_variant(),
                        "callsign" => &callsign.to_variant(),
                        "task" => &task.to_variant(),
                    },
                ),
                SimEvent::PlanMade {
                    unit_id,
                    callsign,
                    playbook,
                } => (
                    "plan_made",
                    vdict! {
                        "unit_id" => &(unit_id as i64).to_variant(),
                        "callsign" => &callsign.to_variant(),
                        "playbook" => &playbook.to_variant(),
                    },
                ),
                SimEvent::Reloaded {
                    soldier_id,
                    rounds,
                    magazines_left,
                } => (
                    "reloaded",
                    vdict! {
                        "soldier_id" => &(soldier_id as i64).to_variant(),
                        "rounds" => &(rounds as i64).to_variant(),
                        "magazines_left" => &(magazines_left as i64).to_variant(),
                    },
                ),
                SimEvent::Resupplied {
                    soldier_id,
                    magazines,
                } => (
                    "resupplied",
                    vdict! {
                        "soldier_id" => &(soldier_id as i64).to_variant(),
                        "magazines" => &(magazines as i64).to_variant(),
                    },
                ),
                SimEvent::OrderAcknowledged {
                    unit_id,
                    callsign,
                    order,
                } => (
                    "order_acknowledged",
                    vdict! {
                        "unit_id" => &(unit_id as i64).to_variant(),
                        "callsign" => &callsign.to_variant(),
                        "order" => &order.to_variant(),
                    },
                ),
                SimEvent::UnitInPosition { unit_id, callsign } => (
                    "unit_in_position",
                    vdict! {
                        "unit_id" => &(unit_id as i64).to_variant(),
                        "callsign" => &callsign.to_variant(),
                    },
                ),
                SimEvent::FireMissionComplete { unit_id, callsign } => (
                    "fire_mission_complete",
                    vdict! {
                        "unit_id" => &(unit_id as i64).to_variant(),
                        "callsign" => &callsign.to_variant(),
                    },
                ),
                SimEvent::ShotFired {
                    soldier_id,
                    target,
                    rounds,
                } => (
                    "shot_fired",
                    vdict! {
                        "soldier_id" => &(soldier_id as i64).to_variant(),
                        "target" => &v3(target).to_variant(),
                        "rounds" => &(rounds as i64).to_variant(),
                    },
                ),
                SimEvent::SoldierKilled {
                    soldier_id,
                    side,
                    by_soldier_id,
                } => (
                    "soldier_killed",
                    vdict! {
                        "soldier_id" => &(soldier_id as i64).to_variant(),
                        "side" => &side.name().to_variant(),
                        "by_soldier_id" => &(by_soldier_id as i64).to_variant(),
                    },
                ),
                SimEvent::OutOfAmmo { soldier_id } => (
                    "out_of_ammo",
                    vdict! { "soldier_id" => &(soldier_id as i64).to_variant() },
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

fn v3(p: [f32; 3]) -> Vector3 {
    Vector3::new(p[0], p[1], p[2])
}
