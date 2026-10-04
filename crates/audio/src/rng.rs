//! A seeded stream of draws. Every choice a piece makes comes from here,
//! so a seed names a piece exactly, and every voice draws from its own
//! fork so a change to one voice's choices does not reshuffle another's.

#[derive(Clone, Debug)]
pub struct Rng(u64);

fn mix(mut h: u64) -> u64 {
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^ (h >> 31)
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(mix(seed ^ 0x9E37_79B9_7F4A_7C15))
    }

    /// An independent stream for one voice or one purpose. Two forks of
    /// one state with different salts never overlap; forking is how a
    /// piece keeps its voices' choices apart.
    pub fn fork(&self, salt: u64) -> Self {
        Rng(mix(self.0 ^ salt.wrapping_mul(0xC2B2_AE3D_27D4_EB4F)))
    }

    /// splitmix64.
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.0)
    }

    /// Uniform in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Uniform in 0..n. Panics on 0.
    pub fn below(&mut self, n: usize) -> usize {
        assert!(n > 0, "a draw from nothing");
        (self.next() % n as u64) as usize
    }

    /// Uniform in lo..=hi.
    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + self.below((hi - lo + 1) as usize) as i32
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }

    pub fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }

    /// An index drawn in proportion to `weights`.
    pub fn weighted(&mut self, weights: &[f32]) -> usize {
        let total: f32 = weights.iter().sum();
        let mut x = self.f32() * total;
        for (i, w) in weights.iter().enumerate() {
            if x < *w {
                return i;
            }
            x -= w;
        }
        weights.len() - 1
    }
}
