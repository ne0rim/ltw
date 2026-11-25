//! The elevation grid: heights on a square grid of samples, `cell_m` metres
//! apart, covering the map from corner (-size/2, -size/2) to (+size/2,
//! +size/2). Stored in `heights.bin` as little-endian `f32` metres, row by row
//! along +Z (row 0 is the north edge), each row west to east.

use serde::{Deserialize, Serialize};

/// How the elevation grid is laid out (the `terrain` entry in `map.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainDef {
    /// Samples per side (cells + 1).
    pub samples: usize,
    /// Distance between samples (m).
    pub cell_m: f32,
    /// The heights file, relative to the map's folder.
    pub heights_file: String,
}

/// The elevation grid itself.
#[derive(Debug, Clone, PartialEq)]
pub struct Heightfield {
    pub samples: usize,
    pub cell_m: f32,
    /// World x and z of sample (0, 0) (the north-west corner).
    pub origin: [f32; 2],
    /// `samples * samples` heights in metres, row by row along +Z.
    pub heights: Vec<f32>,
}

impl Heightfield {
    /// A grid centred on the origin, `samples` per side, `cell_m` apart.
    pub fn new(samples: usize, cell_m: f32, heights: Vec<f32>) -> Self {
        assert_eq!(heights.len(), samples * samples, "height grid size");
        let half = (samples - 1) as f32 * cell_m * 0.5;
        Self {
            samples,
            cell_m,
            origin: [-half, -half],
            heights,
        }
    }

    /// Read heights from `heights.bin` bytes.
    pub fn from_bytes(samples: usize, cell_m: f32, bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != samples * samples * 4 {
            return Err(format!(
                "heights file has {} bytes, expected {} ({samples} x {samples} f32)",
                bytes.len(),
                samples * samples * 4
            ));
        }
        let (chunks, _) = bytes.as_chunks::<4>();
        let heights = chunks.iter().map(|b| f32::from_le_bytes(*b)).collect();
        Ok(Self::new(samples, cell_m, heights))
    }

    /// Heights as `heights.bin` bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.heights.iter().flat_map(|h| h.to_le_bytes()).collect()
    }

    /// Width (and depth) covered, in metres.
    pub fn size_m(&self) -> f32 {
        (self.samples - 1) as f32 * self.cell_m
    }

    pub fn at(&self, ix: usize, iz: usize) -> f32 {
        self.heights[iz * self.samples + ix]
    }

    /// Ground height at a world position (bilinear; clamped at the edges).
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let last = (self.samples - 1) as f32;
        let fx = ((x - self.origin[0]) / self.cell_m).clamp(0.0, last - 0.001);
        let fz = ((z - self.origin[1]) / self.cell_m).clamp(0.0, last - 0.001);
        let (ix, iz) = (fx as usize, fz as usize);
        let (tx, tz) = (fx - ix as f32, fz - iz as f32);
        let h00 = self.at(ix, iz);
        let h10 = self.at(ix + 1, iz);
        let h01 = self.at(ix, iz + 1);
        let h11 = self.at(ix + 1, iz + 1);
        let top = h00 + (h10 - h00) * tx;
        let bottom = h01 + (h11 - h01) * tx;
        top + (bottom - top) * tz
    }

    /// How far along `dir` (unit length) from `origin` the path first goes
    /// below the ground, within `max` metres.
    pub fn raycast(&self, origin: [f32; 3], dir: [f32; 3], max: f32) -> Option<f32> {
        let step = (self.cell_m * 0.5).clamp(0.25, 1.0);
        let above = |t: f32| {
            let p = [
                origin[0] + dir[0] * t,
                origin[1] + dir[1] * t,
                origin[2] + dir[2] * t,
            ];
            p[1] - self.height_at(p[0], p[2])
        };
        // Starting under the ground (a muzzle inside a slope): blocked at once.
        if above(0.0) < 0.0 {
            return Some(0.0);
        }
        let mut t = 0.0;
        while t < max {
            let next = (t + step).min(max);
            if above(next) < 0.0 {
                // Narrow down where it crossed.
                let (mut lo, mut hi) = (t, next);
                for _ in 0..12 {
                    let mid = (lo + hi) * 0.5;
                    if above(mid) < 0.0 {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                return Some(hi);
            }
            t = next;
        }
        None
    }
}
