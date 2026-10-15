//! Room-ir's seeding rule and normal draw, on the collection's generator.
//!
//! The generator is `mxm_rng::SplitMix64` (Steele, Lea & Flood, *Fast splittable pseudorandom number
//! generators*, OOPSLA 2014). What stays here is room-ir's own: how a seeded run splits into
//! independent streams, and the Box–Muller normal. Both draw exactly what the private copy they
//! replaced drew (architecture audit O6; the test pins its draws), so a seed names the same result.

use mxm_rng::SplitMix64;

/// An independent stream for item `index` of a seeded run, so each ray's draws depend only on the
/// seed and its index, never on which thread traced it.
pub fn stream(seed: u64, index: u64) -> SplitMix64 {
    let mut mixer = SplitMix64::new(seed ^ index.wrapping_mul(0xD1B5_4A32_D192_ED03));
    SplitMix64::new(mixer.next_u64())
}

/// Standard normal, by the Box–Muller transform.
pub fn normal(rng: &mut SplitMix64) -> f64 {
    let u1 = 1.0 - rng.unit_f64();
    let u2 = rng.unit_f64();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The private generator's draws, recorded before it was replaced (2026-10-15).
    #[test]
    fn draws_what_the_private_generator_drew() {
        let mut s = stream(0x5EED, 7);
        let words: Vec<u64> = (0..3).map(|_| s.next_u64()).collect();
        assert_eq!(
            words,
            [
                0x72aa_fda3_6f8a_5b96,
                0xd095_30f2_73af_d6b5,
                0x3e1b_9211_4fe8_e476
            ]
        );
        assert_eq!(s.unit_f64().to_bits(), 0x3fe6_c776_3bfd_3037);
        // The normal goes through `ln` and `cos`, which each platform's maths library rounds its own
        // way: Windows' bits are pinned, elsewhere within rounding (the root contract).
        let pinned = [
            (normal(&mut s), 0xbff1_f6ed_2b5b_0946u64),
            (normal(&mut SplitMix64::new(9)), 0x3f7b_15ad_c1d1_2bb4),
        ];
        for (drawn, bits) in pinned {
            if cfg!(windows) {
                assert_eq!(drawn.to_bits(), bits);
            } else {
                let recorded = f64::from_bits(bits);
                assert!((drawn - recorded).abs() <= 1e-12 * recorded.abs().max(1.0));
            }
        }
    }

    #[test]
    fn roughly_normal() {
        let mut a = SplitMix64::new(7);
        let n = 200_000;
        let (mut sum, mut sq) = (0.0, 0.0);
        for _ in 0..n {
            let x = normal(&mut a);
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
