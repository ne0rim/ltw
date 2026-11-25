//! `MapFile`: a map from the core's map files, without a running sim. Works in
//! the editor (it's a tool class), so the Terrain and MapObjects nodes can
//! show the real map while you edit the scene.
//!
//! It answers the same questions as `SimBridge` (`get_map`, `get_terrain`,
//! `get_terrain_chunk`, `get_map_objects`, `get_map_objects_near`) with the
//! same shapes, so code that draws a map works with either.

use godot::builtin::{GString, VarArray, VarDictionary, Vector3};
use godot::classes::{IRefCounted, RefCounted};
use godot::global::godot_error;
use godot::obj::Base;
use godot::register::{GodotClass, godot_api};

use ltw_core::data;
use ltw_core::map::MapDef;

use crate::map_dicts;

#[derive(GodotClass)]
#[class(tool, base = RefCounted)]
pub struct MapFile {
    map: Option<MapDef>,
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for MapFile {
    fn init(base: Base<RefCounted>) -> Self {
        Self { map: None, base }
    }
}

#[godot_api]
impl MapFile {
    /// Load a map by id (a folder in core/data/maps/). Returns false if there
    /// is no such map, or its heights haven't been generated yet.
    #[func]
    fn open(&mut self, id: GString) -> bool {
        let id = id.to_string();
        // An ungenerated map panics in the core with instructions; report it
        // here instead of taking the editor down.
        match std::panic::catch_unwind(|| data::map(&id)) {
            Ok(Some(map)) => {
                self.map = Some(map);
                true
            }
            Ok(None) => {
                godot_error!("MapFile: no map called {id:?}");
                false
            }
            Err(_) => {
                godot_error!(
                    "MapFile: map {id:?} couldn't be loaded (has it been generated? \
                     cargo run --release --bin mapgen in core/)"
                );
                false
            }
        }
    }

    /// Name, size and the player's start (see `SimBridge.get_map`).
    #[func]
    fn get_map(&self) -> VarDictionary {
        self.map.as_ref().map(map_dicts::info).unwrap_or_default()
    }

    /// The elevation grid (see `SimBridge.get_terrain`).
    #[func]
    fn get_terrain(&self) -> VarDictionary {
        self.map
            .as_ref()
            .and_then(|m| m.heights.as_deref())
            .map(map_dicts::terrain)
            .unwrap_or_default()
    }

    /// One terrain chunk's mesh (see `SimBridge.get_terrain_chunk`).
    #[func]
    #[allow(clippy::too_many_arguments)]
    fn get_terrain_chunk(
        &self,
        ix0: i64,
        iz0: i64,
        cells: i64,
        step: i64,
        lo: f32,
        hi: f32,
        skirt_m: f32,
    ) -> VarDictionary {
        let spec = map_dicts::chunk_spec(ix0, iz0, cells, step, skirt_m);
        self.map
            .as_ref()
            .and_then(|m| m.heights.as_deref())
            .map(|h| map_dicts::terrain_chunk(h, spec, (lo, hi)))
            .unwrap_or_default()
    }

    /// Every object on the map (see `SimBridge.get_map_objects`).
    #[func]
    fn get_map_objects(&self) -> VarArray {
        self.map
            .as_ref()
            .map(map_dicts::objects)
            .unwrap_or_default()
    }

    /// Objects near a point (see `SimBridge.get_map_objects_near`).
    #[func]
    fn get_map_objects_near(&self, centre: Vector3, radius: f32) -> VarArray {
        self.map
            .as_ref()
            .map(|m| map_dicts::objects_near(m, centre, radius))
            .unwrap_or_default()
    }
}
