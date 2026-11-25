//! Terrain meshes for one square chunk of the map, built in Rust (fast) for
//! the Godot client to draw.
//!
//! A chunk covers `cells` × `cells` height cells starting at sample (ix0,
//! iz0). `step` picks every n-th sample: 1 = full detail, 4 = a quarter of the
//! samples each way (for chunks further from the camera). Normals always come
//! from the full-detail samples, so coarse chunks still show the shading of
//! small bumps.
//!
//! Neighbouring chunks drawn at different steps don't meet exactly, so every
//! chunk has a "skirt": a strip hanging down from its edges that hides the
//! gaps.

use ltw_core::map::Heightfield;

/// One chunk's mesh, ready to hand to Godot.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ChunkMesh {
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Grey shade per vertex (r = g = b), alpha 1.
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<i32>,
}

/// Which chunk to build, and how.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChunkSpec {
    /// First sample (x, z) of the chunk.
    pub ix0: usize,
    pub iz0: usize,
    /// Cells per side.
    pub cells: usize,
    /// Use every n-th sample (1 = full detail).
    pub step: usize,
    /// How far the edge skirt hangs down (m).
    pub skirt_m: f32,
}

/// Build a chunk's mesh. `range` is the whole map's (lowest, highest) height,
/// for shading.
pub fn chunk_mesh(field: &Heightfield, spec: ChunkSpec, range: (f32, f32)) -> ChunkMesh {
    let ChunkSpec {
        ix0,
        iz0,
        cells,
        step,
        skirt_m,
    } = spec;
    let (lo, hi) = range;
    let last = field.samples - 1;
    let step = step.max(1);
    // Sample columns/rows in this chunk (always including its far edge).
    let mut xs: Vec<usize> = (ix0..=(ix0 + cells).min(last)).step_by(step).collect();
    if *xs.last().unwrap() != (ix0 + cells).min(last) {
        xs.push((ix0 + cells).min(last));
    }
    let mut zs: Vec<usize> = (iz0..=(iz0 + cells).min(last)).step_by(step).collect();
    if *zs.last().unwrap() != (iz0 + cells).min(last) {
        zs.push((iz0 + cells).min(last));
    }
    let (nx, nz) = (xs.len(), zs.len());
    let span = (hi - lo).max(0.001);
    let mut m = ChunkMesh::default();

    let world = |ix: usize, iz: usize| {
        [
            field.origin[0] + ix as f32 * field.cell_m,
            field.at(ix, iz),
            field.origin[1] + iz as f32 * field.cell_m,
        ]
    };
    for &iz in &zs {
        for &ix in &xs {
            let p = world(ix, iz);
            let n = normal(field, ix, iz);
            m.vertices.push(p);
            m.normals.push(n);
            // Higher is lighter, steeper is darker (as the editor preview).
            let height01 = (p[1] - lo) / span;
            let steep = 1.0 - n[1];
            let g = (0.22 + height01 * 0.6 - steep * 1.4).clamp(0.08, 0.95);
            m.colors.push([g, g, g, 1.0]);
        }
    }
    for z in 0..nz - 1 {
        for x in 0..nx - 1 {
            let a = (z * nx + x) as i32;
            let b = a + 1;
            let c = a + nx as i32;
            let d = c + 1;
            m.indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }

    // Skirts along the four edges: a copy of each edge vertex, lowered, and
    // quads joining them (both windings, so they show from either side).
    let edges: [Vec<usize>; 4] = [
        (0..nx).collect(),                            // north row
        (0..nx).map(|x| (nz - 1) * nx + x).collect(), // south row
        (0..nz).map(|z| z * nx).collect(),            // west column
        (0..nz).map(|z| z * nx + nx - 1).collect(),   // east column
    ];
    for edge in edges {
        let base = m.vertices.len() as i32;
        for &i in &edge {
            let p = m.vertices[i];
            m.vertices.push([p[0], p[1] - skirt_m, p[2]]);
            m.normals.push(m.normals[i]);
            m.colors.push(m.colors[i]);
        }
        for k in 0..edge.len() - 1 {
            let (top_a, top_b) = (edge[k] as i32, edge[k + 1] as i32);
            let (low_a, low_b) = (base + k as i32, base + k as i32 + 1);
            m.indices
                .extend_from_slice(&[top_a, top_b, low_a, top_b, low_b, low_a]);
            m.indices
                .extend_from_slice(&[top_a, low_a, top_b, top_b, low_a, low_b]);
        }
    }
    m
}

/// Surface normal at a sample, from its full-detail neighbours.
fn normal(field: &Heightfield, ix: usize, iz: usize) -> [f32; 3] {
    let last = field.samples - 1;
    let hl = field.at(ix.saturating_sub(1), iz);
    let hr = field.at((ix + 1).min(last), iz);
    let hn = field.at(ix, iz.saturating_sub(1));
    let hs = field.at(ix, (iz + 1).min(last));
    let n = [hl - hr, 2.0 * field.cell_m, hn - hs];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    [n[0] / len, n[1] / len, n[2] / len]
}

#[cfg(test)]
mod tests {
    use ltw_core::map::Heightfield;

    use super::{ChunkSpec, chunk_mesh};

    fn spec(ix0: usize, cells: usize, step: usize) -> ChunkSpec {
        ChunkSpec {
            ix0,
            iz0: 0,
            cells,
            step,
            skirt_m: 1.0,
        }
    }

    /// A 9 × 9 sample slope (8 cells of 0.5 m), rising to the east.
    fn slope() -> Heightfield {
        let heights = (0..81).map(|i| (i % 9) as f32 * 0.25).collect();
        Heightfield::new(9, 0.5, heights)
    }

    #[test]
    fn full_and_coarse_chunks_share_their_corners() {
        let f = slope();
        let fine = chunk_mesh(&f, spec(0, 8, 1), (0.0, 2.0));
        let coarse = chunk_mesh(&f, spec(0, 8, 4), (0.0, 2.0));
        // 9 × 9 and 3 × 3 surface vertices (plus skirts).
        assert_eq!(fine.vertices.len(), 81 + 2 * 9 + 2 * 9);
        assert_eq!(coarse.vertices.len(), 9 + 2 * 3 + 2 * 3);
        // Same corners: the chunk covers the same ground at either detail.
        assert_eq!(fine.vertices[0], coarse.vertices[0]);
        assert_eq!(fine.vertices[80], coarse.vertices[8]);
        // Every index points at a vertex.
        for m in [&fine, &coarse] {
            assert!(m.indices.iter().all(|i| (*i as usize) < m.vertices.len()));
            assert_eq!(m.indices.len() % 3, 0);
        }
    }

    #[test]
    fn heights_and_positions_match_the_field() {
        let f = slope();
        let m = chunk_mesh(&f, spec(4, 4, 1), (0.0, 2.0));
        let p = m.vertices[0]; // sample (4, 0)
        assert_eq!(p, [f.origin[0] + 2.0, f.at(4, 0), f.origin[1]]);
        assert!((p[1] - f.height_at(p[0], p[2])).abs() < 1e-5);
    }
}
