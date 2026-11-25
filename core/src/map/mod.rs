//! Maps: the ground, what stands on it, and who is there when the mission
//! starts. The core owns all of it; the Godot client draws what the core
//! loaded (one map file, one truth).
//!
//! A map is a folder in `core/data/maps/<id>/`:
//!
//! - `map.json`: name, size, the terrain grid's layout, objects (barricades)
//!   and forces. See [`MapDef`].
//! - `heights.bin`: the elevation grid, row by row along +Z, as little-endian
//!   `f32` metres (see [`terrain`]).
//!
//! Maps are made by the generator (`cargo run --bin mapgen`) and can be edited
//! by hand afterwards. They are embedded into the core when it is built; add a
//! new one in `data::MAP_FILES`.
//!
//! Positions are world metres with +Y up, centred on the origin. `facing_deg`
//! is a compass heading: 0 = north (-Z), 90 = east, clockwise.
//!
//! A force's `task` in the JSON is one of (positions in metres, facings in
//! radians as yaw: 0 = north, 3.1416 = south):
//!
//! ```text
//! null                                                   idle: all-round harbour
//! {"Hold":     {"position": [x, y, z]}}                  harbour at a point
//! {"Defend":   {"position": [x, y, z], "facing": 3.1416}} prepared (barricades), linear or all-round
//! {"Attack":   {"objective": [x, y, z]}}                 fire base, two up or bounding
//! {"Patrol":   {"route": [[x, y, z], [x, y, z], ...]}}   loop through waypoints
//! {"Withdraw": {"position": [x, y, z], "facing": 3.1416}} bound back, then defend
//! ```
//!
//! The AI commander picks the playbook for the task (see `systems::command`).

pub mod index;
pub mod objects;
pub mod terrain;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use bevy_ecs::resource::Resource;
use serde::{Deserialize, Serialize};

use crate::components::{Controller, Side, TaskKind};

pub use index::ObjectIndex;
pub use objects::{MapObject, ObjectKind};
pub use terrain::{Heightfield, TerrainDef};

/// A map file: its size, ground, objects and the forces that start on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapDef {
    pub id: String,
    pub name: String,
    /// Width and depth of the (square) map, in metres, centred on the origin.
    pub size_m: f32,
    /// Layout of the elevation grid. None = flat ground at height 0.
    #[serde(default)]
    pub terrain: Option<TerrainDef>,
    /// Things placed on the map: barricades for now.
    #[serde(default)]
    pub objects: Vec<MapObject>,
    pub forces: Vec<ForceDef>,
    /// The elevation grid, loaded from `heights.bin` (not part of the JSON).
    #[serde(skip)]
    pub heights: Option<Arc<Heightfield>>,
    /// Where each object is (built by [`MapDef::prepare`], not saved).
    #[serde(skip)]
    pub index: Option<Arc<ObjectIndex>>,
}

impl MapDef {
    /// Build what is worked out from the file rather than stored in it (the
    /// object index). Done when a map is loaded; safe to call again.
    pub fn prepare(&mut self) {
        self.index = Some(Arc::new(ObjectIndex::build(
            &self.objects,
            index::DEFAULT_CELL_M,
        )));
    }

    /// Objects whose position is within `radius` of `centre` (on the ground
    /// plane), with their ids (positions in `objects`), in id order.
    pub fn objects_within(&self, centre: [f32; 3], radius: f32) -> Vec<(u32, &MapObject)> {
        let ids: Vec<u32> = match &self.index {
            // Footprints can stick out past their position; widen the search.
            Some(index) => index.near(centre, radius + index.cell_m()),
            None => (0..self.objects.len() as u32).collect(),
        };
        ids.into_iter()
            .map(|id| (id, &self.objects[id as usize]))
            .filter(|(_, o)| {
                let (dx, dz) = (o.position[0] - centre[0], o.position[2] - centre[2]);
                dx * dx + dz * dz <= radius * radius
            })
            .collect()
    }

    /// Ground height at a world position (0 where there is no terrain).
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        self.heights.as_ref().map_or(0.0, |h| h.height_at(x, z))
    }

    /// How far a bullet from `origin` along `dir` (unit length) gets before
    /// the ground or an object stops it (None = nothing within `max`).
    pub fn obstruction(&self, origin: [f32; 3], dir: [f32; 3], max: f32) -> Option<f32> {
        let ground = self
            .heights
            .as_ref()
            .and_then(|h| h.raycast(origin, dir, max));
        // Only the objects in the cells the round passes through.
        let candidates: Vec<u32> = match &self.index {
            Some(index) => index.along_ray(origin, dir, max),
            None => (0..self.objects.len() as u32).collect(),
        };
        let objects = candidates
            .into_iter()
            .filter_map(|id| self.objects[id as usize].raycast(origin, dir, max))
            .min_by(f32::total_cmp);
        match (ground, objects) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
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
    /// Where the platoon commander starts (the height is taken from the
    /// ground).
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
