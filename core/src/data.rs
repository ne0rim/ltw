//! Game content loaded from the JSON files in `core/data/`.
//!
//! The files are embedded into the binary at compile time with `include_str!`
//! (so exported builds never need to find them on disk) and parsed once, the
//! first time they are used. Edit the JSON and rebuild to change them.

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::components::WeaponSpec;

const SURNAMES_JSON: &str = include_str!("../data/surnames.json");
const WEAPONS_JSON: &str = include_str!("../data/weapons.json");

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
                    magazine_capacity: w.magazine_capacity,
                },
            )
        })
        .collect()
});

/// One entry in `weapons.json` (the id is the key, not a field).
#[derive(serde::Deserialize)]
struct WeaponFile {
    name: &'static str,
    caliber: &'static str,
    magazine_capacity: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_files_parse() {
        assert!(!SURNAMES.is_empty());
        assert!(!WEAPONS.is_empty());
    }
}
