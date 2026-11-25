//! A spatial index for map objects: which objects are in which part of the
//! map, so questions like "what's near here?" and "what does this bullet pass
//! through?" only look at nearby objects, however large the map.
//!
//! The map is divided into square cells (`cell_m` metres). Each object is
//! listed in every cell its footprint touches. Objects themselves stay in the
//! map's list; the index stores their positions in that list (ids).
//!
//! Every query returns ids sorted and without duplicates, so results are the
//! same every run.

use std::collections::{BTreeSet, HashMap};

use super::objects::MapObject;

/// Default cell size (m): a few barricades or a building per cell.
pub const DEFAULT_CELL_M: f32 = 32.0;

/// A grid cell's coordinates (x and z cell numbers).
pub type Cell = (i32, i32);

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectIndex {
    cell_m: f32,
    cells: HashMap<Cell, Vec<u32>>,
}

impl ObjectIndex {
    /// Index `objects` (ids are their positions in the slice).
    pub fn build(objects: &[MapObject], cell_m: f32) -> Self {
        let mut index = Self {
            cell_m,
            cells: HashMap::new(),
        };
        for (id, o) in objects.iter().enumerate() {
            index.insert(id as u32, o);
        }
        index
    }

    pub fn cell_m(&self) -> f32 {
        self.cell_m
    }

    /// Add an object (for objects placed after the map loads).
    pub fn insert(&mut self, id: u32, object: &MapObject) {
        for cell in self.cells_touching(object) {
            let ids = self.cells.entry(cell).or_default();
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }

    /// Remove an object (destroyed, picked up).
    pub fn remove(&mut self, id: u32, object: &MapObject) {
        for cell in self.cells_touching(object) {
            if let Some(ids) = self.cells.get_mut(&cell) {
                ids.retain(|i| *i != id);
                if ids.is_empty() {
                    self.cells.remove(&cell);
                }
            }
        }
    }

    /// Ids of objects that may lie within `radius` of `centre` (on the
    /// ground plane). Callers check exact distances themselves.
    pub fn near(&self, centre: [f32; 3], radius: f32) -> Vec<u32> {
        let lo = self.cell_of(centre[0] - radius, centre[2] - radius);
        let hi = self.cell_of(centre[0] + radius, centre[2] + radius);
        let mut out = BTreeSet::new();
        for cx in lo.0..=hi.0 {
            for cz in lo.1..=hi.1 {
                if let Some(ids) = self.cells.get(&(cx, cz)) {
                    out.extend(ids.iter().copied());
                }
            }
        }
        out.into_iter().collect()
    }

    /// Ids of objects in the cells a ray passes through, from `origin` along
    /// `dir` (unit length) for `max` metres. Callers test the objects
    /// themselves; this only rules out everything elsewhere on the map.
    pub fn along_ray(&self, origin: [f32; 3], dir: [f32; 3], max: f32) -> Vec<u32> {
        let mut out = BTreeSet::new();
        for cell in self.cells_on_ray(origin, dir, max) {
            if let Some(ids) = self.cells.get(&cell) {
                out.extend(ids.iter().copied());
            }
        }
        out.into_iter().collect()
    }

    fn cell_of(&self, x: f32, z: f32) -> Cell {
        (
            (x / self.cell_m).floor() as i32,
            (z / self.cell_m).floor() as i32,
        )
    }

    /// Every cell an object's footprint (its rotated box) overlaps. Uses the
    /// box's bounding square, so long or turned walls are listed in every cell
    /// they could touch.
    fn cells_touching(&self, o: &MapObject) -> Vec<Cell> {
        let (min, max) = o.footprint_bounds();
        let lo = self.cell_of(min[0], min[1]);
        let hi = self.cell_of(max[0], max[1]);
        let mut out = Vec::new();
        for cx in lo.0..=hi.0 {
            for cz in lo.1..=hi.1 {
                out.push((cx, cz));
            }
        }
        out
    }

    /// The cells a ray crosses on the ground plane, in order (a grid walk:
    /// Amanatides & Woo). A straight-down ray is just its starting cell.
    fn cells_on_ray(&self, origin: [f32; 3], dir: [f32; 3], max: f32) -> Vec<Cell> {
        let (dx, dz) = (dir[0], dir[2]);
        let flat = (dx * dx + dz * dz).sqrt();
        let mut cell = self.cell_of(origin[0], origin[2]);
        let mut out = vec![cell];
        if flat < 1e-6 {
            return out;
        }
        // Distance along the ray (in its own units) to cross one cell, per axis.
        let step_x = if dx > 0.0 { 1 } else { -1 };
        let step_z = if dz > 0.0 { 1 } else { -1 };
        let next_edge = |pos: f32, c: i32, step: i32| {
            let edge = if step > 0 { c + 1 } else { c } as f32 * self.cell_m;
            edge - pos
        };
        let mut t_x = if dx.abs() < 1e-9 {
            f32::INFINITY
        } else {
            next_edge(origin[0], cell.0, step_x) / dx
        };
        let mut t_z = if dz.abs() < 1e-9 {
            f32::INFINITY
        } else {
            next_edge(origin[2], cell.1, step_z) / dz
        };
        let dt_x = if dx.abs() < 1e-9 {
            f32::INFINITY
        } else {
            self.cell_m / dx.abs()
        };
        let dt_z = if dz.abs() < 1e-9 {
            f32::INFINITY
        } else {
            self.cell_m / dz.abs()
        };
        while t_x.min(t_z) <= max {
            if (t_x - t_z).abs() < 1e-5 {
                // Exactly through a corner: list both cells beside it too.
                out.push((cell.0 + step_x, cell.1));
                out.push((cell.0, cell.1 + step_z));
                cell.0 += step_x;
                cell.1 += step_z;
                t_x += dt_x;
                t_z += dt_z;
            } else if t_x < t_z {
                cell.0 += step_x;
                t_x += dt_x;
            } else {
                cell.1 += step_z;
                t_z += dt_z;
            }
            out.push(cell);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use crate::map::{MapDef, MapObject, ObjectKind};
    use crate::rng::SimRng;

    use super::{DEFAULT_CELL_M, ObjectIndex};

    /// `n` random walls of all sizes and turns across a 1 km square.
    fn random_walls(n: usize, rng: &mut SimRng) -> Vec<MapObject> {
        let mut at = |lo: f32, hi: f32| lo + (hi - lo) * rng.unit();
        (0..n)
            .map(|_| MapObject {
                kind: ObjectKind::Barricade,
                position: [at(-500.0, 500.0), 0.0, at(-500.0, 500.0)],
                facing_deg: at(0.0, 360.0),
                length_m: at(1.0, 40.0),
                height_m: at(0.5, 3.0),
                thickness_m: at(0.3, 4.0),
                ..Default::default()
            })
            .collect()
    }

    fn map_with(objects: Vec<MapObject>, indexed: bool) -> MapDef {
        let mut map = MapDef {
            id: "test".into(),
            name: "test".into(),
            size_m: 1024.0,
            terrain: None,
            objects,
            forces: Vec::new(),
            heights: None,
            index: None,
        };
        if indexed {
            map.prepare();
        }
        map
    }

    #[test]
    fn near_finds_exactly_what_a_full_search_finds() {
        let mut rng = SimRng::new(7);
        let walls = random_walls(400, &mut rng);
        let indexed = map_with(walls.clone(), true);
        let plain = map_with(walls, false);
        for _ in 0..300 {
            let centre = [
                -550.0 + 1100.0 * rng.unit(),
                0.0,
                -550.0 + 1100.0 * rng.unit(),
            ];
            let radius = 1.0 + 200.0 * rng.unit();
            let fast: Vec<u32> = indexed
                .objects_within(centre, radius)
                .iter()
                .map(|(id, _)| *id)
                .collect();
            let slow: Vec<u32> = plain
                .objects_within(centre, radius)
                .iter()
                .map(|(id, _)| *id)
                .collect();
            assert_eq!(fast, slow, "near {centre:?} r {radius}");
        }
    }

    #[test]
    fn rays_hit_exactly_what_a_full_search_hits() {
        let mut rng = SimRng::new(11);
        let walls = random_walls(400, &mut rng);
        let index = ObjectIndex::build(&walls, DEFAULT_CELL_M);
        let indexed = map_with(walls.clone(), true);
        let plain = map_with(walls.clone(), false);
        for i in 0..2000 {
            let origin = [
                -500.0 + 1000.0 * rng.unit(),
                0.2 + 2.5 * rng.unit(),
                -500.0 + 1000.0 * rng.unit(),
            ];
            let a = std::f32::consts::TAU * rng.unit();
            let pitch = -0.05 + 0.1 * rng.unit();
            // Some rays run exactly along an axis or through cell corners.
            let (x, z) = match i % 10 {
                0 => (1.0, 0.0),
                1 => (0.0, -1.0),
                2 => (
                    std::f32::consts::FRAC_1_SQRT_2,
                    std::f32::consts::FRAC_1_SQRT_2,
                ),
                _ => (a.cos(), a.sin()),
            };
            let len = (x * x + pitch * pitch + z * z).sqrt();
            let dir = [x / len, pitch / len, z / len];
            let max = 20.0 + 600.0 * rng.unit();
            // Every wall the round really hits is a candidate...
            let candidates = index.along_ray(origin, dir, max);
            for (id, w) in walls.iter().enumerate() {
                if w.raycast(origin, dir, max).is_some() {
                    assert!(
                        candidates.contains(&(id as u32)),
                        "ray {i} hits wall {id} but the index missed it"
                    );
                }
            }
            // ...so the first thing it hits is the same either way.
            assert_eq!(
                indexed.obstruction(origin, dir, max),
                plain.obstruction(origin, dir, max),
                "ray {i}"
            );
        }
    }

    #[test]
    fn objects_can_be_added_and_removed() {
        let mut rng = SimRng::new(3);
        let walls = random_walls(2, &mut rng);
        let mut index = ObjectIndex::build(&walls[..1], DEFAULT_CELL_M);
        let at = walls[1].position;
        assert!(!index.near(at, 1.0).contains(&1));
        index.insert(1, &walls[1]);
        assert!(index.near(at, 1.0).contains(&1));
        index.remove(1, &walls[1]);
        assert!(!index.near(at, 1.0).contains(&1));
    }
}
