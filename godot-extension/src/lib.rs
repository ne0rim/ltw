//! Rust ↔ Godot bridge for Lead the Way.

use godot::init::{ExtensionLibrary, gdextension};

mod map_dicts;
mod map_file;
mod sim_bridge;
mod terrain_mesh;

struct LtwExtension;

#[gdextension]
unsafe impl ExtensionLibrary for LtwExtension {}
