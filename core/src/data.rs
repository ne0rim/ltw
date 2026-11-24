//! Game content loaded from the JSON files in `core/data/`.
//!
//! The files are embedded into the binary at compile time with `include_str!`
//! (so exported builds never need to find them on disk) and parsed once, the
//! first time they are used. Edit the JSON and rebuild to change them.

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::components::{Veterancy, WeaponSpec, WeaponStats};
use crate::map::MapDef;

const SURNAMES_JSON: &str = include_str!("../data/surnames.json");
const WEAPONS_JSON: &str = include_str!("../data/weapons.json");
const VETERANCY_JSON: &str = include_str!("../data/veterancy.json");

/// Every map file, by id. Add a new map by adding its file here.
const MAP_FILES: &[(&str, &str)] = &[("test_area", include_str!("../data/maps/test_area.json"))];

/// English and French Canadian surnames, handed out to soldiers at spawn.
pub static SURNAMES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    serde_json::from_str(SURNAMES_JSON).expect("core/data/surnames.json is invalid")
});

/// Weapon specs by id (the keys of `weapons.json`).
pub static WEAPONS: LazyLock<HashMap<&'static str, WeaponSpec>> = LazyLock::new(|| {
    let raw: HashMap<&'static str, WeaponFile> =
        serde_json::from_str(WEAPONS_JSON).expect("core/data/weapons.json is invalid");
    raw.into_iter()
        .map(|(id, w)| {
            (
                id,
                WeaponSpec {
                    id,
                    name: w.name,
                    caliber: w.caliber,
                    stats: w.stats,
                },
            )
        })
        .collect()
});

/// Map definitions by id (from `core/data/maps/`).
pub static MAPS: LazyLock<HashMap<&'static str, MapDef>> = LazyLock::new(|| {
    MAP_FILES
        .iter()
        .map(|(id, json)| {
            let def: MapDef = serde_json::from_str(json)
                .unwrap_or_else(|e| panic!("core/data/maps/{id}.json is invalid: {e}"));
            assert_eq!(def.id, *id, "map file {id}.json has id {}", def.id);
            (*id, def)
        })
        .collect()
});

/// A map definition by id.
pub fn map(id: &str) -> Option<MapDef> {
    MAPS.get(id).cloned()
}

/// One entry in `weapons.json` (the id is the key, not a field).
#[derive(serde::Deserialize)]
struct WeaponFile {
    name: &'static str,
    caliber: &'static str,
    stats: WeaponStats,
}

/// Numbers for each veterancy level (from `veterancy.json`).
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
pub struct VeterancyStats {
    /// The shooter's own aiming error, 1 standard deviation in milliradians.
    pub aim_error_mrad: f32,
}

pub static VETERANCY: LazyLock<HashMap<Veterancy, VeterancyStats>> = LazyLock::new(|| {
    let table: HashMap<Veterancy, VeterancyStats> =
        serde_json::from_str(VETERANCY_JSON).expect("core/data/veterancy.json is invalid");
    for level in Veterancy::ALL {
        assert!(
            table.contains_key(&level),
            "veterancy.json has no {level:?}"
        );
    }
    table
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_files_parse() {
        assert!(!SURNAMES.is_empty());
        assert!(!WEAPONS.is_empty());
        assert!(!MAPS.is_empty());
        assert_eq!(VETERANCY.len(), Veterancy::ALL.len());
    }
}
