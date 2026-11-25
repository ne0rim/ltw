//! Build script: make sure every map's `heights.bin` exists before the core
//! is compiled (it embeds them with `include_bytes!`).
//!
//! The heights files are generated and not kept in git. On a fresh clone they
//! are missing, which would stop the core (and the map generator, which is in
//! the same crate) from compiling. So an empty placeholder is written in their
//! place. The game then says clearly that the map needs generating:
//!
//! ```text
//! cd core && cargo run --release --bin mapgen
//! ```

use std::fs;
use std::path::Path;

fn main() {
    let maps = Path::new("data/maps");
    println!("cargo:rerun-if-changed=data/maps");
    let Ok(entries) = fs::read_dir(maps) else {
        return;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let heights = dir.join("heights.bin");
        println!("cargo:rerun-if-changed={}", heights.display());
        if !heights.exists() {
            fs::write(&heights, []).expect("write placeholder heights.bin");
            println!(
                "cargo:warning={} was missing; wrote an empty placeholder. \
                 Generate the map with: cargo run --release --bin mapgen",
                heights.display()
            );
        }
    }
}
