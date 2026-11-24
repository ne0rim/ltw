//! Accuracy and bullet paths. Pure functions with no ECS, so the maths can be
//! tested and tuned on its own.
//!
//! # The accuracy model
//!
//! Every shot leaves with a small random angular error. Its spread (one
//! standard deviation, in milliradians) combines the weapon and the shooter:
//!
//! ```text
//! shooter_error = veterancy.aim_error × stance_factor × move_factor
//!                 × (1 + SUPPRESSION_PENALTY × suppression)
//! σ (mrad)      = √(weapon.dispersion² + shooter_error²)
//! ```
//!
//! An angular error turns into a miss distance that grows with range, which is
//! why accuracy rises the closer the target is. At distance `d` the spread is
//! `σ_d = σ / 1000 × d` metres. For a target `W` wide and `H` tall, the chance
//! one round hits is ([`hit_probability`]):
//!
//! ```text
//! P(hit) = erf(W / (2√2·σ_d)) × erf(H / (2√2·σ_d))
//! ```
//!
//! The game does not roll `P(hit)` directly. It draws each round's error from
//! a normal distribution with that σ ([`deviate`]) and traces the bullet
//! ([`trace`]), so misses fly on and can hit someone else. For a single target
//! the two give the same odds (a test checks this).
//!
//! Example, a Trained soldier (4 mrad) standing, M16A2 (1 mrad), at a standing
//! man (0.5 × 1.75 m): about 77% at 50 m, 44% at 100 m, 8% at 300 m.
//!
//! # Bullet drop
//!
//! [`GRAVITY`] and each weapon's muzzle velocity are stored for bullet drop.
//! It is off for now ([`BULLET_DROP`]); [`drop_at`] gives the drop when it is
//! switched on.

use crate::components::{Stance, Veterancy};
use crate::rng::SimRng;

/// Gravity for bullet drop (m/s²).
pub const GRAVITY: f32 = 9.81;
/// Whether bullets drop over distance. Off until aiming allows for it.
pub const BULLET_DROP: bool = false;

/// How much full suppression worsens aim: ×(1 + this) at suppression 1.
pub const SUPPRESSION_PENALTY: f32 = 2.0;
/// Firing while walking worsens aim by this factor.
pub const MOVING_PENALTY: f32 = 2.0;
/// A body is a vertical cylinder this wide (radius, m).
pub const BODY_RADIUS: f32 = 0.25;

/// How steady each stance is (multiplies the shooter's error).
pub fn stance_factor(stance: Stance) -> f32 {
    match stance {
        Stance::Standing => 1.0,
        Stance::Crouching => 0.85,
        Stance::Prone => 0.7,
    }
}

/// How tall a body is in each stance (m).
pub fn body_height(stance: Stance) -> f32 {
    match stance {
        Stance::Standing => 1.75,
        Stance::Crouching => 1.2,
        Stance::Prone => 0.45,
    }
}

/// Where a soldier's muzzle is above their feet in each stance (m).
pub fn muzzle_height(stance: Stance) -> f32 {
    match stance {
        Stance::Standing => 1.45,
        Stance::Crouching => 1.0,
        Stance::Prone => 0.3,
    }
}

/// The shooter's own aiming error (mrad, 1 σ).
pub fn shooter_error_mrad(
    veterancy: Veterancy,
    stance: Stance,
    moving: bool,
    suppression: f32,
) -> f32 {
    let move_factor = if moving { MOVING_PENALTY } else { 1.0 };
    veterancy.aim_error_mrad()
        * stance_factor(stance)
        * move_factor
        * (1.0 + SUPPRESSION_PENALTY * suppression.clamp(0.0, 1.0))
}

/// Total angular spread of a shot (mrad, 1 σ): weapon and shooter combined.
pub fn spread_mrad(weapon_dispersion_mrad: f32, shooter_error_mrad: f32) -> f32 {
    (weapon_dispersion_mrad.powi(2) + shooter_error_mrad.powi(2)).sqrt()
}

/// Chance that one round with spread `sigma_mrad`, aimed at the centre of a
/// `width` × `height` target `distance` metres away, hits it.
pub fn hit_probability(sigma_mrad: f32, distance: f32, width: f32, height: f32) -> f32 {
    let sigma_m = sigma_mrad / 1000.0 * distance.max(0.01);
    if sigma_m <= 0.0 {
        return 1.0;
    }
    let k = 2.0 * std::f32::consts::SQRT_2 * sigma_m;
    erf(width / k) * erf(height / k)
}

/// How far a bullet has dropped after `distance` metres (m). Zero while
/// [`BULLET_DROP`] is off.
pub fn drop_at(distance: f32, muzzle_velocity_mps: f32) -> f32 {
    if !BULLET_DROP || muzzle_velocity_mps <= 0.0 {
        return 0.0;
    }
    let t = distance / muzzle_velocity_mps;
    0.5 * GRAVITY * t * t
}

/// Error function (Abramowitz & Stegun 7.1.26, accurate to about 1e-7).
pub fn erf(x: f32) -> f32 {
    let sign = x.signum();
    let x = x.abs() as f64;
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let y = 1.0
        - (((((1.061_405_429 * t - 1.453_152_027) * t) + 1.421_413_741) * t - 0.284_496_736) * t
            + 0.254_829_592)
            * t
            * (-x * x).exp();
    sign * y as f32
}

/// A normally distributed number (mean 0, standard deviation 1).
pub fn gaussian(rng: &mut SimRng) -> f32 {
    // Box–Muller. Keep u1 away from 0 so ln() stays finite.
    let u1 = rng.unit().max(1e-7);
    let u2 = rng.unit();
    (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
}

/// `dir` (unit length) turned by a random angular error with spread
/// `sigma_mrad` (1 σ), sideways and up/down independently.
pub fn deviate(dir: [f32; 3], sigma_mrad: f32, rng: &mut SimRng) -> [f32; 3] {
    let sigma = sigma_mrad / 1000.0;
    let ex = gaussian(rng) * sigma;
    let ey = gaussian(rng) * sigma;
    // Right and up relative to the shot.
    let mut right = cross(dir, [0.0, 1.0, 0.0]);
    if length(right) < 1e-4 {
        right = [1.0, 0.0, 0.0]; // shooting straight up or down
    }
    let right = normalize(right);
    let up = cross(right, dir);
    normalize([
        dir[0] + right[0] * ex + up[0] * ey,
        dir[1] + right[1] * ex + up[1] * ey,
        dir[2] + right[2] * ex + up[2] * ey,
    ])
}

/// A body a bullet can hit: a vertical cylinder standing on `feet`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    pub feet: [f32; 3],
    pub height: f32,
}

/// Where a bullet from `origin` along `dir` (unit length) first hits one of
/// `bodies` within `max_distance`: (index into `bodies`, distance along the
/// path). Bodies `skip` says no to are passed through.
pub fn trace(
    origin: [f32; 3],
    dir: [f32; 3],
    max_distance: f32,
    bodies: &[Body],
    skip: impl Fn(usize) -> bool,
) -> Option<(usize, f32)> {
    let mut best: Option<(usize, f32)> = None;
    // Work on the ground plane: the path's horizontal direction and length.
    let flat = [dir[0], dir[2]];
    let flat_len = (flat[0] * flat[0] + flat[1] * flat[1]).sqrt();
    for (i, b) in bodies.iter().enumerate() {
        if skip(i) {
            continue;
        }
        let (t, d_flat) = if flat_len < 1e-5 {
            // Straight up or down: hits only if we start inside the column.
            let dx = b.feet[0] - origin[0];
            let dz = b.feet[2] - origin[2];
            (0.0, (dx * dx + dz * dz).sqrt())
        } else {
            // Closest approach to the body's axis, measured on the ground.
            let fx = flat[0] / flat_len;
            let fz = flat[1] / flat_len;
            let dx = b.feet[0] - origin[0];
            let dz = b.feet[2] - origin[2];
            let along = dx * fx + dz * fz; // ground distance to closest approach
            let across = (dx * fz - dz * fx).abs();
            (along / flat_len, across)
        };
        if d_flat > BODY_RADIUS || t <= 0.0 || t > max_distance {
            continue;
        }
        // Height of the bullet there, compared with the body's height.
        let y = origin[1] + dir[1] * t;
        if y < b.feet[1] || y > b.feet[1] + b.height {
            continue;
        }
        if best.is_none_or(|(_, bt)| t < bt) {
            best = Some((i, t));
        }
    }
    best
}

/// Shortest distance from `point` to the path from `origin` along `dir` for
/// `length` metres.
pub fn distance_to_path(point: [f32; 3], origin: [f32; 3], dir: [f32; 3], length: f32) -> f32 {
    let v = sub(point, origin);
    let t = dot(v, dir).clamp(0.0, length);
    let closest = [
        origin[0] + dir[0] * t,
        origin[1] + dir[1] * t,
        origin[2] + dir[2] * t,
    ];
    self::length(sub(point, closest))
}

pub fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

pub fn normalize(a: [f32; 3]) -> [f32; 3] {
    let l = length(a);
    if l < 1e-9 {
        [0.0, 0.0, -1.0]
    } else {
        [a[0] / l, a[1] / l, a[2] / l]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRAINED_STANDING: f32 = 4.0;
    const M16: f32 = 1.0;

    fn sigma() -> f32 {
        spread_mrad(M16, TRAINED_STANDING)
    }

    #[test]
    fn accuracy_falls_with_range() {
        let p = |d| hit_probability(sigma(), d, 0.5, 1.75);
        assert!(p(25.0) > p(50.0) && p(50.0) > p(100.0) && p(100.0) > p(300.0));
        assert!((p(50.0) - 0.77).abs() < 0.02, "50 m: {}", p(50.0));
        assert!((p(100.0) - 0.44).abs() < 0.02, "100 m: {}", p(100.0));
        assert!((p(300.0) - 0.08).abs() < 0.02, "300 m: {}", p(300.0));
    }

    #[test]
    fn veterancy_stance_and_suppression_matter() {
        let at = |v, s, m, sup| {
            hit_probability(
                spread_mrad(M16, shooter_error_mrad(v, s, m, sup)),
                150.0,
                0.5,
                1.75,
            )
        };
        let base = at(Veterancy::Trained, Stance::Standing, false, 0.0);
        assert!(at(Veterancy::Elite, Stance::Standing, false, 0.0) > base);
        assert!(at(Veterancy::Green, Stance::Standing, false, 0.0) < base);
        assert!(at(Veterancy::Trained, Stance::Prone, false, 0.0) > base);
        assert!(at(Veterancy::Trained, Stance::Standing, true, 0.0) < base);
        assert!(at(Veterancy::Trained, Stance::Standing, false, 1.0) < base);
    }

    #[test]
    fn traced_shots_match_the_formula() {
        // Fire many rounds at a lone standing body and count hits.
        let mut rng = SimRng::new(42);
        let target = Body {
            feet: [0.0, 0.0, -100.0],
            height: 1.75,
        };
        let origin = [0.0, 0.875, 0.0]; // aim at the body's middle
        let aim = normalize(sub([0.0, 0.875, -100.0], origin));
        let n = 20_000;
        let hits = (0..n)
            .filter(|_| {
                let d = deviate(aim, sigma(), &mut rng);
                trace(origin, d, 200.0, &[target], |_| false).is_some()
            })
            .count();
        let measured = hits as f32 / n as f32;
        let predicted = hit_probability(sigma(), 100.0, 0.5, 1.75);
        assert!(
            (measured - predicted).abs() < 0.02,
            "traced {measured:.3} vs formula {predicted:.3}"
        );
    }

    #[test]
    fn trace_finds_the_first_body_along_the_path() {
        let bodies = [
            Body {
                feet: [0.0, 0.0, -50.0],
                height: 1.75,
            },
            Body {
                feet: [0.1, 0.0, -20.0],
                height: 1.75,
            },
            Body {
                feet: [5.0, 0.0, -10.0], // off to the side
                height: 1.75,
            },
        ];
        let hit = trace([0.0, 1.0, 0.0], [0.0, 0.0, -1.0], 100.0, &bodies, |_| false);
        assert_eq!(hit.map(|h| h.0), Some(1));
        // Skipping the near one, the far one is hit.
        let hit = trace([0.0, 1.0, 0.0], [0.0, 0.0, -1.0], 100.0, &bodies, |i| {
            i == 1
        });
        assert_eq!(hit.map(|h| h.0), Some(0));
        // Over a prone body's head.
        let prone = [Body {
            feet: [0.0, 0.0, -20.0],
            height: 0.45,
        }];
        assert!(trace([0.0, 1.0, 0.0], [0.0, 0.0, -1.0], 100.0, &prone, |_| false).is_none());
    }

    #[test]
    fn bullet_drop_is_off_for_now() {
        assert_eq!(drop_at(500.0, 948.0), 0.0);
    }

    #[test]
    fn erf_matches_known_values() {
        assert!((erf(0.5) - 0.5205).abs() < 1e-3);
        assert!((erf(1.0) - 0.8427).abs() < 1e-3);
        assert!((erf(-1.0) + 0.8427).abs() < 1e-3);
    }
}
