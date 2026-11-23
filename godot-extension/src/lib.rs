//! Rust ↔ Godot bridge for Lead the Way.

use godot::prelude::*;

mod sim_bridge;

struct LtwExtension;

#[gdextension]
unsafe impl ExtensionLibrary for LtwExtension {}
