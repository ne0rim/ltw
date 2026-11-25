//! Map generator (a developer tool). Writes a map folder the core loads:
//!
//! ```text
//! cargo run --release --bin mapgen
//! ```
//!
//! makes `core/data/maps/hill/` (`map.json` + `heights.bin`). Rebuild the core
//! afterwards so it embeds the new files. The output is deterministic: running
//! it again gives the same map. Hand edits to `map.json` (objects, forces) are
//! overwritten by a re-run, so re-run only when the generated ground changes.
//!
//! The "hill" map (the training map): 1024 m square at 0.5 m resolution. A
//! long, gentle rise covers most of the map: the summit (100 m up) sits in the
//! north-east corner and the slope runs out about 1000 m, so only the
//! south-west corner is flat. The ground is rough on a small scale (ripples
//! and hummocks up to about 1 m), flattened under objects so they sit level.
//! Red holds the summit behind sandbag barricades facing south-west; Blue (the
//! player's platoon) starts in the south-west corner, about 1050 m from the
//! summit, with an ammo box behind it. Woods, lone trees and boulders are
//! scattered over the map, kept off the starts, the barricades and the
//! summit's field of fire.

use std::f32::consts::PI;
use std::fs;
use std::path::PathBuf;

use ltw_core::components::{Controller, Side, TaskKind};
use ltw_core::map::{
    ForceDef, Heightfield, MapDef, MapObject, ObjectKind, TerrainDef, yaw_from_heading,
};
use ltw_core::rng::SimRng;

const SIZE_M: f32 = 1024.0;
const CELL_M: f32 = 0.5;
const SAMPLES: usize = (SIZE_M / CELL_M) as usize + 1;

/// The hill: summit position (x, z), height, flat top and foot radius.
const SUMMIT: [f32; 2] = [440.0, -440.0];
const HILL_HEIGHT_M: f32 = 100.0;
const SUMMIT_FLAT_M: f32 = 35.0;
const HILL_FOOT_M: f32 = 1000.0;
/// Rolling ground: (amplitude m, wavelength m) of each layer, broad to fine.
/// The fine layers are the small bumps you feel underfoot.
const ROLL: [(f32, f32); 5] = [
    (2.0, 220.0),
    (0.8, 70.0),
    (0.5, 10.0),
    (0.25, 4.0),
    (0.08, 1.5),
];
/// Hummocks: how many, their radius and height ranges (m).
const HUMMOCKS: usize = 6000;
const HUMMOCK_RADIUS: (f32, f32) = (1.5, 5.0);
const HUMMOCK_HEIGHT: (f32, f32) = (0.2, 1.0);
/// Objects stand on level ground: flat under them, blending back to the
/// natural ground over this distance (m).
const PAD_BLEND_M: f32 = 1.5;

/// Woods: a tree is tried every `TREE_SPACING_M`; where the woods noise
/// (wavelength `WOODS_WAVE_M`) is high, most attempts become trees.
const TREE_SPACING_M: f32 = 9.0;
const WOODS_WAVE_M: f32 = 170.0;
/// Woods noise above this starts to be wooded, fully wooded by the second.
const WOODS_EDGE: (f32, f32) = (0.55, 0.75);
/// Chance of a lone tree in open ground.
const LONE_TREE_CHANCE: f32 = 0.012;
const TREE_HEIGHT: (f32, f32) = (8.0, 18.0);
const TRUNK_DIAMETER: (f32, f32) = (0.3, 0.7);
const CANOPY_RADIUS: (f32, f32) = (2.5, 5.0);
/// Boulders: how many, and their size across (m).
const BOULDERS: usize = 350;
const BOULDER_SIZE: (f32, f32) = (0.6, 2.4);
/// Model variants per kind (the client picks model n, wrapping).
const VARIANTS: u8 = 4;
/// No trees or boulders this close to: the summit, Blue's start, and any
/// placed object (m).
const SUMMIT_CLEAR_M: f32 = 90.0;
const START_CLEAR_M: f32 = 40.0;
const OBJECT_CLEAR_M: f32 = 12.0;

/// Blue starts this far from the summit, toward the south-west.
const BLUE_DISTANCE_M: f32 = 1050.0;
/// Barricades on the summit: distance out from it and compass bearings.
const BARRICADE_RING_M: f32 = 48.0;
const FRONT_BEARINGS: [f32; 3] = [195.0, 225.0, 255.0];
const FLANK_BEARINGS: [f32; 2] = [150.0, 300.0];

fn main() {
    let mut heights = natural_ground();
    let raw = Heightfield::new(SAMPLES, CELL_M, heights.clone());
    let ground = |x: f32, z: f32| raw.height_at(x, z);

    // Directions from the summit: south-west (toward Blue).
    let sw = [-(0.5_f32).sqrt(), (0.5_f32).sqrt()];
    let from_summit = |d: f32, dir: [f32; 2]| [SUMMIT[0] + dir[0] * d, SUMMIT[1] + dir[1] * d];

    let barricade = |bearing: f32| {
        // Bearing (compass) from the summit: north = -Z, east = +X.
        let rad = bearing.to_radians();
        let p = from_summit(BARRICADE_RING_M, [rad.sin(), -rad.cos()]);
        MapObject {
            kind: ObjectKind::Barricade,
            position: [p[0], ground(p[0], p[1]), p[1]],
            facing_deg: bearing,
            length_m: 14.0,
            height_m: 1.0,
            thickness_m: 0.7,
            ..Default::default()
        }
    };
    let mut objects: Vec<MapObject> = FRONT_BEARINGS
        .iter()
        .chain(FLANK_BEARINGS.iter())
        .map(|b| barricade(*b))
        .collect();
    // An ammo box a few metres behind Blue's start.
    let ammo = from_summit(BLUE_DISTANCE_M + 8.0, sw);
    objects.push(MapObject {
        kind: ObjectKind::AmmoBox,
        position: [ammo[0] + 3.0, ground(ammo[0] + 3.0, ammo[1]), ammo[1]],
        facing_deg: 45.0,
        length_m: 0.9,
        height_m: 0.5,
        thickness_m: 0.55,
        ..Default::default()
    });

    // Level the ground under every object, then stand them on it.
    for o in &objects {
        flatten_under(&mut heights, o);
    }
    let field = Heightfield::new(SAMPLES, CELL_M, heights);
    let ground = |x: f32, z: f32| field.height_at(x, z);
    for o in &mut objects {
        o.position[1] = ground(o.position[0], o.position[2]);
    }

    // Trees and boulders, kept off the forces' starts, the placed objects
    // and the summit (so the defenders have a field of fire).
    let blue_start = from_summit(BLUE_DISTANCE_M, sw);
    let mut clear: Vec<([f32; 2], f32)> =
        vec![(SUMMIT, SUMMIT_CLEAR_M), (blue_start, START_CLEAR_M)];
    clear.extend(
        objects
            .iter()
            .map(|o| ([o.position[0], o.position[2]], OBJECT_CLEAR_M)),
    );
    objects.extend(trees(&field, &clear));
    objects.extend(boulders(&field, &clear));

    let on_ground = |p: [f32; 2]| [p[0], ground(p[0], p[1]), p[1]];
    let summit = on_ground(SUMMIT);
    let forces = vec![
        ForceDef {
            side: Side::Blue,
            controller: Controller::Player,
            led_by_player: true,
            name: "1 Platoon".into(),
            callsign: "11".into(),
            position: on_ground(from_summit(BLUE_DISTANCE_M, sw)),
            facing_deg: 45.0,
            task: None,
        },
        ForceDef {
            side: Side::Red,
            controller: Controller::Ai,
            led_by_player: false,
            name: "Enemy Platoon".into(),
            callsign: "E1".into(),
            // Just behind the summit; the defence plan moves them forward.
            position: on_ground(from_summit(12.0, [-sw[0], -sw[1]])),
            facing_deg: 225.0,
            task: Some(TaskKind::Defend {
                position: summit,
                facing: yaw_from_heading(225.0),
            }),
        },
    ];

    let map = MapDef {
        id: "hill".into(),
        name: "Hill 100".into(),
        size_m: SIZE_M,
        terrain: Some(TerrainDef {
            samples: SAMPLES,
            cell_m: CELL_M,
            heights_file: "heights.bin".into(),
        }),
        objects,
        forces,
        heights: None,
        index: None,
    };

    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/maps/hill");
    fs::create_dir_all(&dir).expect("create map folder");
    fs::write(dir.join("heights.bin"), field.to_bytes()).expect("write heights.bin");
    let json = serde_json::to_string_pretty(&map).expect("map to JSON");
    fs::write(dir.join("map.json"), json + "\n").expect("write map.json");

    let (lo, hi) = field
        .heights
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), h| (lo.min(*h), hi.max(*h)));
    println!(
        "wrote {}: {SAMPLES}x{SAMPLES} heights ({CELL_M} m), {lo:.1}..{hi:.1} m, {} objects, {} forces",
        dir.display(),
        map.objects.len(),
        map.forces.len()
    );
    println!("rebuild the core to embed it (cargo build)");
}

/// Is a spot inside any clear zone?
fn in_clear(x: f32, z: f32, clear: &[([f32; 2], f32)]) -> bool {
    clear
        .iter()
        .any(|(c, r)| (x - c[0]).powi(2) + (z - c[1]).powi(2) < r * r)
}

/// Woods and lone trees: one attempt per `TREE_SPACING_M` cell, jittered,
/// kept with a chance set by the woods noise.
fn trees(field: &Heightfield, clear: &[([f32; 2], f32)]) -> Vec<MapObject> {
    let mut rng = SimRng::new(2024);
    let range = |r: (f32, f32), rng: &mut SimRng| r.0 + (r.1 - r.0) * rng.unit();
    let half = SIZE_M * 0.5 - 4.0;
    let cells = (SIZE_M / TREE_SPACING_M) as i32;
    let mut out = Vec::new();
    for cz in 0..cells {
        for cx in 0..cells {
            // Draw every number every time, so a change in one place doesn't
            // reshuffle the whole map.
            let jitter = [rng.unit(), rng.unit()];
            let keep = rng.unit();
            let (height, trunk, canopy) = (
                range(TREE_HEIGHT, &mut rng),
                range(TRUNK_DIAMETER, &mut rng),
                range(CANOPY_RADIUS, &mut rng),
            );
            let (facing, variant) = (rng.unit() * 360.0, rng.below(VARIANTS as u32) as u8);
            let x = -half + (cx as f32 + jitter[0]) * TREE_SPACING_M;
            let z = -half + (cz as f32 + jitter[1]) * TREE_SPACING_M;
            if x.abs() > half || z.abs() > half || in_clear(x, z, clear) {
                continue;
            }
            let woods = value_noise(x / WOODS_WAVE_M, z / WOODS_WAVE_M, 10) * 0.5 + 0.5;
            let t = ((woods - WOODS_EDGE.0) / (WOODS_EDGE.1 - WOODS_EDGE.0)).clamp(0.0, 1.0);
            let chance = LONE_TREE_CHANCE + smooth(t) * 0.9;
            if keep >= chance {
                continue;
            }
            out.push(MapObject {
                kind: ObjectKind::Tree,
                position: [x, field.height_at(x, z), z],
                facing_deg: facing,
                length_m: trunk,
                height_m: height,
                thickness_m: trunk,
                variant,
                canopy_m: canopy,
            });
        }
    }
    out
}

/// Boulders scattered at random, a quarter sunk into the ground.
fn boulders(field: &Heightfield, clear: &[([f32; 2], f32)]) -> Vec<MapObject> {
    let mut rng = SimRng::new(77);
    let half = SIZE_M * 0.5 - 4.0;
    let mut out = Vec::new();
    for _ in 0..BOULDERS {
        let (x, z) = (
            -half + 2.0 * half * rng.unit(),
            -half + 2.0 * half * rng.unit(),
        );
        let size = BOULDER_SIZE.0 + (BOULDER_SIZE.1 - BOULDER_SIZE.0) * rng.unit();
        let (deep, tall) = (0.6 + 0.4 * rng.unit(), 0.5 + 0.3 * rng.unit());
        let (facing, variant) = (rng.unit() * 360.0, rng.below(VARIANTS as u32) as u8);
        if in_clear(x, z, clear) {
            continue;
        }
        let height = size * tall;
        out.push(MapObject {
            kind: ObjectKind::Boulder,
            position: [x, field.height_at(x, z) - height * 0.25, z],
            facing_deg: facing,
            length_m: size,
            height_m: height,
            thickness_m: size * deep,
            variant,
            canopy_m: 0.0,
        });
    }
    out
}

/// The ground before anything is placed: the hill, rolling layers, hummocks.
fn natural_ground() -> Vec<f32> {
    let half = SIZE_M * 0.5;
    let mut out = Vec::with_capacity(SAMPLES * SAMPLES);
    for iz in 0..SAMPLES {
        for ix in 0..SAMPLES {
            let x = ix as f32 * CELL_M - half;
            let z = iz as f32 * CELL_M - half;
            out.push(hill(x, z) + rolling(x, z));
        }
    }
    add_hummocks(&mut out);
    out
}

/// The hill: flat on top, falling off smoothly (a cosine) to its foot.
fn hill(x: f32, z: f32) -> f32 {
    let d = ((x - SUMMIT[0]).powi(2) + (z - SUMMIT[1]).powi(2)).sqrt();
    let t = ((d - SUMMIT_FLAT_M) / (HILL_FOOT_M - SUMMIT_FLAT_M)).clamp(0.0, 1.0);
    HILL_HEIGHT_M * 0.5 * (1.0 + (PI * t).cos())
}

/// Layers of smooth noise, broad to fine.
fn rolling(x: f32, z: f32) -> f32 {
    ROLL.iter()
        .enumerate()
        .map(|(i, (amp, wave))| amp * value_noise(x / wave, z / wave, i as u32))
        .sum()
}

/// Scattered mounds: round bumps of random size, enough to hide a prone man.
fn add_hummocks(heights: &mut [f32]) {
    let half = SIZE_M * 0.5;
    let mut rng = SimRng::new(1985);
    let mut range = |r: (f32, f32)| r.0 + (r.1 - r.0) * rng.unit();
    for _ in 0..HUMMOCKS {
        let (cx, cz) = (range((-half, half)), range((-half, half)));
        let radius = range(HUMMOCK_RADIUS);
        let height = range(HUMMOCK_HEIGHT);
        let reach = (radius / CELL_M).ceil() as i64;
        let (ix0, iz0) = (((cx + half) / CELL_M) as i64, ((cz + half) / CELL_M) as i64);
        for iz in (iz0 - reach).max(0)..=(iz0 + reach).min(SAMPLES as i64 - 1) {
            for ix in (ix0 - reach).max(0)..=(ix0 + reach).min(SAMPLES as i64 - 1) {
                let x = ix as f32 * CELL_M - half;
                let z = iz as f32 * CELL_M - half;
                let d = ((x - cx).powi(2) + (z - cz).powi(2)).sqrt() / radius;
                if d < 1.0 {
                    heights[iz as usize * SAMPLES + ix as usize] +=
                        height * 0.5 * (1.0 + (PI * d).cos());
                }
            }
        }
    }
}

/// Level the ground under an object at the height of its centre: flat under
/// its footprint, blending back to the natural ground over `PAD_BLEND_M`.
fn flatten_under(heights: &mut [f32], o: &MapObject) {
    let half = SIZE_M * 0.5;
    let target = {
        let f = Heightfield::new(SAMPLES, CELL_M, heights.to_vec());
        f.height_at(o.position[0], o.position[2])
    };
    let yaw = o.yaw();
    let (front, right) = ([-yaw.sin(), -yaw.cos()], [yaw.cos(), -yaw.sin()]);
    let (hl, ht) = (o.length_m * 0.5, o.thickness_m * 0.5);
    let reach = ((hl.max(ht) + PAD_BLEND_M) / CELL_M).ceil() as i64 + 1;
    let (ix0, iz0) = (
        ((o.position[0] + half) / CELL_M) as i64,
        ((o.position[2] + half) / CELL_M) as i64,
    );
    for iz in (iz0 - reach).max(0)..=(iz0 + reach).min(SAMPLES as i64 - 1) {
        for ix in (ix0 - reach).max(0)..=(ix0 + reach).min(SAMPLES as i64 - 1) {
            let x = ix as f32 * CELL_M - half - o.position[0];
            let z = iz as f32 * CELL_M - half - o.position[2];
            // How far outside the footprint (0 inside it).
            let across = (x * right[0] + z * right[1]).abs() - hl;
            let along = (x * front[0] + z * front[1]).abs() - ht;
            let out = across.max(0.0).hypot(along.max(0.0));
            let w = 1.0 - smooth((out / PAD_BLEND_M).clamp(0.0, 1.0));
            let h = &mut heights[iz as usize * SAMPLES + ix as usize];
            *h += (target - *h) * w;
        }
    }
}

/// Smooth noise in -1..1 on a unit grid (bilinear with smoothstep).
fn value_noise(x: f32, z: f32, seed: u32) -> f32 {
    let (x0, z0) = (x.floor(), z.floor());
    let (tx, tz) = (smooth(x - x0), smooth(z - z0));
    let c = |dx: f32, dz: f32| lattice((x0 + dx) as i32, (z0 + dz) as i32, seed);
    let top = c(0.0, 0.0) + (c(1.0, 0.0) - c(0.0, 0.0)) * tx;
    let bottom = c(0.0, 1.0) + (c(1.0, 1.0) - c(0.0, 1.0)) * tx;
    top + (bottom - top) * tz
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// A repeatable pseudo-random value in -1..1 for a grid point.
fn lattice(x: i32, z: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343)
        ^ (z as u32).wrapping_mul(0xd816_3841)
        ^ seed.wrapping_mul(0xcb1a_b31f)
        ^ 1985;
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h as f32 / u32::MAX as f32) * 2.0 - 1.0
}
