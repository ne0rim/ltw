//! A map's data as Godot values (Dictionaries and packed arrays). Shared by
//! `SimBridge` (the running game) and `MapFile` (the editor), so both hand
//! GDScript exactly the same shapes.

use godot::builtin::{
    Color, PackedColorArray, PackedFloat32Array, PackedInt32Array, PackedVector3Array, VarArray,
    VarDictionary, Vector3, vdict,
};
use godot::meta::ToGodot;

use ltw_core::map::{Heightfield, MapDef, MapObject};

use crate::terrain_mesh::{ChunkSpec, chunk_mesh};

/// The elevation grid: samples (per side), cell_m, size_m, heights (row by
/// row along +Z from the north-west corner), collision_heights (divided by
/// cell_m, for a HeightMapShape3D scaled by cell_m), and min/max height.
pub fn terrain(h: &Heightfield) -> VarDictionary {
    let heights: PackedFloat32Array = h.heights.iter().copied().collect();
    // Godot's HeightMapShape3D spaces samples 1 unit apart, so its node is
    // scaled by cell_m and its heights are divided by it.
    let collision: PackedFloat32Array = h.heights.iter().map(|y| y / h.cell_m).collect();
    let (lo, hi) = h
        .heights
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), y| (lo.min(*y), hi.max(*y)));
    vdict! {
        "samples" => &(h.samples as i64).to_variant(),
        "cell_m" => &h.cell_m.to_variant(),
        "size_m" => &h.size_m().to_variant(),
        "heights" => &heights.to_variant(),
        "collision_heights" => &collision.to_variant(),
        "min" => &lo.to_variant(),
        "max" => &hi.to_variant(),
    }
}

/// One terrain chunk's mesh: vertices, normals, colors, indices.
pub fn terrain_chunk(h: &Heightfield, spec: ChunkSpec, range: (f32, f32)) -> VarDictionary {
    let m = chunk_mesh(h, spec, range);
    let vertices: PackedVector3Array = m.vertices.iter().map(|p| v3(*p)).collect();
    let normals: PackedVector3Array = m.normals.iter().map(|n| v3(*n)).collect();
    let colors: PackedColorArray = m
        .colors
        .iter()
        .map(|c| Color::from_rgba(c[0], c[1], c[2], c[3]))
        .collect();
    let indices: PackedInt32Array = m.indices.iter().copied().collect();
    vdict! {
        "vertices" => &vertices.to_variant(),
        "normals" => &normals.to_variant(),
        "colors" => &colors.to_variant(),
        "indices" => &indices.to_variant(),
    }
}

/// Chunk arguments from GDScript (clamped to sensible values).
pub fn chunk_spec(ix0: i64, iz0: i64, cells: i64, step: i64, skirt_m: f32) -> ChunkSpec {
    ChunkSpec {
        ix0: ix0.max(0) as usize,
        iz0: iz0.max(0) as usize,
        cells: cells.max(1) as usize,
        step: step.max(1) as usize,
        skirt_m,
    }
}

/// Every object on the map.
pub fn objects(map: &MapDef) -> VarArray {
    let mut out = VarArray::new();
    for (id, o) in map.objects.iter().enumerate() {
        out.push(&object(id as u32, o).to_variant());
    }
    out
}

/// The objects within `radius` of `centre` (through the spatial index).
pub fn objects_near(map: &MapDef, centre: Vector3, radius: f32) -> VarArray {
    let mut out = VarArray::new();
    for (id, o) in map.objects_within([centre.x, centre.y, centre.z], radius) {
        out.push(&object(id, o).to_variant());
    }
    out
}

/// One map object: id, kind, position (centre of its base), yaw, size (length
/// across its front, height, thickness front to back), variant (which model
/// to show) and canopy (trees: canopy radius).
pub fn object(id: u32, o: &MapObject) -> VarDictionary {
    vdict! {
        "id" => &(id as i64).to_variant(),
        "kind" => &o.kind.name().to_variant(),
        "position" => &v3(o.position).to_variant(),
        "yaw" => &o.yaw().to_variant(),
        "length" => &o.length_m.to_variant(),
        "height" => &o.height_m.to_variant(),
        "thickness" => &o.thickness_m.to_variant(),
        "variant" => &(o.variant as i64).to_variant(),
        "canopy" => &o.canopy_m.to_variant(),
    }
}

/// The map's name, size and where the player's platoon starts.
pub fn info(map: &MapDef) -> VarDictionary {
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

fn v3(p: [f32; 3]) -> Vector3 {
    Vector3::new(p[0], p[1], p[2])
}
