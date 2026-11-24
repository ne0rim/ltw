//! Points around an objective, in the client's axes (+Y up, yaw 0 looks
//! down -Z). Everything is on the XZ plane; heights come from the client.

use crate::components::yaw_towards;

/// Flat direction from `from` to `to` (unit length), or "north" if they coincide.
pub fn direction(from: [f32; 3], to: [f32; 3]) -> [f32; 2] {
    let (dx, dz) = (to[0] - from[0], to[2] - from[2]);
    let len = (dx * dx + dz * dz).sqrt();
    if len < 0.001 {
        [0.0, -1.0]
    } else {
        [dx / len, dz / len]
    }
}

pub fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (dx, dz) = (b[0] - a[0], b[2] - a[2]);
    (dx * dx + dz * dz).sqrt()
}

/// Yaw that looks along `dir`.
pub fn facing(dir: [f32; 2]) -> f32 {
    yaw_towards(dir[0], dir[1])
}

/// `origin` moved `back` metres against `forward` and `right` metres to its
/// right (looking along `forward`).
pub fn offset(origin: [f32; 3], forward: [f32; 2], back: f32, right: f32) -> [f32; 3] {
    // Right of a direction (x, z) when +Y is up and we look along it.
    let r = [-forward[1], forward[0]];
    [
        origin[0] - forward[0] * back + r[0] * right,
        origin[1],
        origin[2] - forward[1] * back + r[1] * right,
    ]
}

/// Average position of some points (None if there are none).
pub fn centre(points: impl Iterator<Item = [f32; 3]>) -> Option<[f32; 3]> {
    let (mut sum, mut n) = ([0.0_f32; 3], 0.0_f32);
    for p in points {
        sum[0] += p[0];
        sum[1] += p[1];
        sum[2] += p[2];
        n += 1.0;
    }
    (n > 0.0).then(|| [sum[0] / n, sum[1] / n, sum[2] / n])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_back_and_right_of_north() {
        // Facing north (-Z): back is +Z, right is +X.
        let p = offset([0.0, 0.0, 0.0], [0.0, -1.0], 10.0, 5.0);
        assert!(
            (p[0] - 5.0).abs() < 1e-5 && (p[2] - 10.0).abs() < 1e-5,
            "{p:?}"
        );
    }

    #[test]
    fn facing_north_is_zero() {
        assert!(facing([0.0, -1.0]).abs() < 1e-5);
    }
}
