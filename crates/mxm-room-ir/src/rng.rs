//! A small deterministic generator, so a seed names a result on every platform.
//!
//! SplitMix64 (Steele, Lea & Flood, *Fast splittable pseudorandom number generators*, OOPSLA 2014)
//! with Box–Muller for normals. No statistical claim beyond what the analyser's controls need: it
//! feeds test decays at R1 and late-tail synthesis later.

#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// An independent stream for item `index` of a seeded run, so each ray's draws depend only on
    /// the seed and its index, never on which thread traced it.
    pub fn stream(seed: u64, index: u64) -> Self {
        let mut mixer = Self::new(seed ^ index.wrapping_mul(0xD1B5_4A32_D192_ED03));
        Self::new(mixer.next_u64())
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Standard normal, by the Box–Muller transform.
    pub fn next_normal(&mut self) -> f64 {
        let u1 = 1.0 - self.next_f64();
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_roughly_normal() {
        let mut a = SplitMix64::new(7);
        let mut b = SplitMix64::new(7);
        assert_eq!(a.next_u64(), b.next_u64());
        let n = 200_000;
        let (mut sum, mut sq) = (0.0, 0.0);
        for _ in 0..n {
            let x = a.next_normal();
            sum += x;
            sq += x * x;
        }
        let mean = sum / n as f64;
        let var = sq / n as f64 - mean * mean;
        assert!(
            mean.abs() < 0.01 && (var - 1.0).abs() < 0.02,
            "mean {mean} var {var}"
        );
    }
}
