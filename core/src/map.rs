//! Maps: the battlefield and who is on it when the mission starts.
//!
//! A map is a data file in `core/data/maps/` (see `data::map`). It lists the
//! forces on each side: where they start, which way they face, who controls
//! them, and an optional starting task. A force with no task is **idle**: it
//! stays where it is and holds its fire.
//!
//! `Sim::start_map` spawns everything from the definition and keeps an
//! [`ActiveMap`] resource in the world, so the core (not the client) knows what
//! is being played. Saves store the map's id.
//!
//! Positions are world metres; heights come from the client's terrain.
//! `facing_deg` is a compass heading: 0 = north (-Z), 90 = east, clockwise.
//!
//! A force's `task` in the JSON is one of (positions in metres, facings in
//! radians as yaw: 0 = north, 3.1416 = south):
//!
//! ```text
//! null                                                   idle: all-round harbour
//! {"Hold":     {"position": [x, y, z]}}                  harbour at a point
//! {"Defend":   {"position": [x, y, z], "facing": 3.1416}} linear or all-round
//! {"Attack":   {"objective": [x, y, z]}}                 fire base, two up or bounding
//! {"Patrol":   {"route": [[x, y, z], [x, y, z], ...]}}   loop through waypoints
//! {"Withdraw": {"position": [x, y, z], "facing": 3.1416}} bound back, then defend
//! ```
//!
//! The AI commander picks the playbook for the task (see `systems::command`).

use bevy_ecs::resource::Resource;
use serde::{Deserialize, Serialize};

use crate::components::{Controller, Side, TaskKind};

/// A map file: its size and the forces that start on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapDef {
    pub id: String,
    pub name: String,
    /// Width and depth of the (square) map, in metres, centred on the origin.
    pub size_m: f32,
    pub forces: Vec<ForceDef>,
}

/// One force on a map. For now every force is a platoon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForceDef {
    pub side: Side,
    /// Who commands it: the player, the AI or a script.
    pub controller: Controller,
    /// The player's body becomes this platoon's commander. At most one force
    /// on a map should set this.
    #[serde(default)]
    pub led_by_player: bool,
    pub name: String,
    pub callsign: String,
    /// Where the platoon commander starts.
    pub position: [f32; 3],
    /// Compass heading the force faces (0 = north, clockwise).
    pub facing_deg: f32,
    /// What it starts doing. None = idle.
    #[serde(default)]
    pub task: Option<TaskKind>,
}

impl ForceDef {
    /// Facing as a yaw (radians, yaw 0 looks down -Z = north).
    pub fn yaw(&self) -> f32 {
        yaw_from_heading(self.facing_deg)
    }
}

/// Compass heading (degrees, clockwise from north) to yaw (radians).
pub fn yaw_from_heading(deg: f32) -> f32 {
    -deg.to_radians()
}

/// The map being played (a resource in the sim's world).
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct ActiveMap(pub MapDef);

impl ActiveMap {
    /// The force the player commands, if any.
    pub fn player_force(&self) -> Option<&ForceDef> {
        self.0.forces.iter().find(|f| f.led_by_player)
    }
}

#[cfg(test)]
mod tests {
    use crate::components::{Controller, Side, TaskKind};
    use crate::events::SimEvent;
    use crate::sim::{Sim, SimConfig};

    use super::*;

    fn run(sim: &mut Sim, seconds: f64) -> Vec<SimEvent> {
        let mut events = Vec::new();
        for _ in 0..(seconds * sim.config.tick_rate_hz) as u32 {
            sim.step();
            events.extend(sim.drain_events());
        }
        events
    }

    #[test]
    fn test_area_spawns_both_platoons_at_opposite_ends() {
        let mut sim = Sim::new(SimConfig::default());
        sim.start_map("test_area").unwrap();
        assert_eq!(sim.map().unwrap().id, "test_area");
        assert_eq!(sim.map().unwrap().size_m, 1024.0);

        let soldiers = sim.soldiers();
        let me = soldiers
            .iter()
            .find(|s| s.is_player)
            .expect("player leads a platoon");
        assert_eq!(me.side, Side::Blue);
        let red: Vec<_> = soldiers.iter().filter(|s| s.side == Side::Red).collect();
        let blue: Vec<_> = soldiers.iter().filter(|s| s.side == Side::Blue).collect();
        assert_eq!((red.len(), blue.len()), (36, 36));
        // Red starts at the north end (-Z), facing south (yaw ±π).
        assert!(red.iter().all(|s| s.position[2] < -300.0));
        assert!(
            red.iter()
                .all(|s| (s.yaw.abs() - std::f32::consts::PI).abs() < 1e-4)
        );
        // Units carry the map's controllers.
        for u in sim.units() {
            let want = if u.side == Side::Red {
                Controller::Ai
            } else {
                Controller::Player
            };
            assert_eq!(u.controller, want, "{}", u.name);
        }
    }

    #[test]
    fn idle_enemy_holds_fire_near_its_start() {
        let mut sim = Sim::new(SimConfig::default());
        sim.start_map("test_area").unwrap();
        let events = run(&mut sim, 60.0);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, SimEvent::ShotFired { .. }))
        );
        // Idle means a harbour round where they started, not an advance.
        for s in sim.soldiers().iter().filter(|s| s.side == Side::Red) {
            assert!(
                s.position[2] < -330.0,
                "{} wandered to {:?}",
                s.name,
                s.position
            );
        }
    }

    #[test]
    fn a_force_can_start_with_a_task() {
        let mut def = crate::data::map("test_area").unwrap();
        def.forces[1].task = Some(TaskKind::Defend {
            position: [0.0, 0.0, -350.0],
            facing: std::f32::consts::PI,
        });
        let mut sim = Sim::new(SimConfig::default());
        sim.start_map_def(def);
        let events = run(&mut sim, 1.0);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SimEvent::PlanMade { .. }))
        );
    }

    #[test]
    fn saves_remember_the_map() {
        let mut sim = Sim::new(SimConfig::default());
        sim.start_map("test_area").unwrap();
        let save = sim.to_save();
        assert_eq!(save.map_id.as_deref(), Some("test_area"));
        let loaded = Sim::from_save(SimConfig::default(), &save);
        assert_eq!(loaded.map().map(|m| m.id.as_str()), Some("test_area"));
    }

    #[test]
    fn unknown_map_is_an_error() {
        assert!(Sim::new(SimConfig::default()).start_map("nowhere").is_err());
    }

    #[test]
    fn heading_to_yaw() {
        assert_eq!(yaw_from_heading(0.0), 0.0);
        // East (90°) is yaw -π/2: looking down +X.
        assert!((yaw_from_heading(90.0) + std::f32::consts::FRAC_PI_2).abs() < 1e-6);
    }
}
