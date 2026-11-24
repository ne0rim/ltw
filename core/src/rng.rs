//! Seeded random numbers for the sim, so runs are reproducible.

use bevy_ecs::resource::Resource;

/// SplitMix64: tiny, fast, good enough for gameplay randomness.
#[derive(Resource, Debug, Clone)]
pub struct SimRng {
    state: u64,
}

impl Default for SimRng {
    fn default() -> Self {
        Self::new(1985)
    }
}

impl SimRng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform integer in 0..n (n > 0).
    pub fn below(&mut self, n: u32) -> u32 {
        (self.next_u64() % n as u64) as u32
    }

    /// Uniform float in 0..1.
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
}
