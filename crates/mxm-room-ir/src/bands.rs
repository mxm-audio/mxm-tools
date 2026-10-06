//! Octave bands, and per-band quantities.
//!
//! **Chosen:** nine octaves, 63 Hz–16 kHz. Their exact centres are base-10: `1000·10^(0.3·k)`.
//! The published material data read for R0 covers only 125 Hz–4 kHz
//! (`research:effects/room-acoustics-simulation.md` §8.1). [`from_125_to_4k`] extends it by
//! holding the 125 Hz value down to 63 Hz and the 4 kHz value up through 8 and 16 kHz. That is an
//! assumption, not data, and it is the first thing better material data should replace.

/// Number of octave bands.
pub const NUM_BANDS: usize = 9;

/// A value per octave band, lowest band first.
pub type Bands = [f64; NUM_BANDS];

/// Nominal octave centres, for labels.
pub const NOMINAL_CENTRES_HZ: [f64; NUM_BANDS] = [
    63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

/// Index of the 1 kHz band.
const BAND_1K: usize = 4;

/// Exact base-10 octave centre of band `k`.
pub fn exact_centre_hz(k: usize) -> f64 {
    1000.0 * 10f64.powf(0.3 * (k as f64 - BAND_1K as f64))
}

/// A band vector with the same value everywhere.
pub const fn uniform(value: f64) -> Bands {
    [value; NUM_BANDS]
}

/// Extends six octave values (125 Hz–4 kHz) to all nine bands, holding the edge values.
pub fn from_125_to_4k(v: [f64; 6]) -> Bands {
    [v[0], v[0], v[1], v[2], v[3], v[4], v[5], v[5], v[5]]
}

/// Element-wise product.
pub fn mul(a: &Bands, b: &Bands) -> Bands {
    let mut out = [0.0; NUM_BANDS];
    for k in 0..NUM_BANDS {
        out[k] = a[k] * b[k];
    }
    out
}

/// The gain at an arbitrary frequency, interpolated in dB over log frequency between band centres.
/// The value is held constant below the lowest centre and above the highest.
pub fn gain_at(gains: &Bands, frequency_hz: f64) -> f64 {
    const FLOOR: f64 = 1e-10;
    let db = |g: f64| 20.0 * g.max(FLOOR).log10();
    let first = exact_centre_hz(0);
    let last = exact_centre_hz(NUM_BANDS - 1);
    if frequency_hz.is_nan() || frequency_hz <= first {
        return gains[0].max(FLOOR);
    }
    if frequency_hz >= last {
        return gains[NUM_BANDS - 1].max(FLOOR);
    }
    let x = (frequency_hz / first).log10() / 0.3;
    let k = (x.floor() as usize).min(NUM_BANDS - 2);
    let frac = x - k as f64;
    let value = (1.0 - frac) * db(gains[k]) + frac * db(gains[k + 1]);
    10f64.powf(value / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centres_are_base_ten_octaves() {
        assert!((exact_centre_hz(4) - 1000.0).abs() < 1e-9);
        assert!((exact_centre_hz(5) / exact_centre_hz(4) - 10f64.powf(0.3)).abs() < 1e-12);
        for (k, nominal) in NOMINAL_CENTRES_HZ.iter().enumerate() {
            let ratio = exact_centre_hz(k) / nominal;
            assert!((0.97..1.03).contains(&ratio), "band {k}");
        }
    }

    #[test]
    fn interpolation_hits_centres_and_holds_edges() {
        let g = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9];
        for (k, &expected) in g.iter().enumerate() {
            assert!(
                (gain_at(&g, exact_centre_hz(k)) - expected).abs() < 1e-9,
                "band {k}"
            );
        }
        assert_eq!(gain_at(&g, 1.0), 0.1);
        assert_eq!(gain_at(&g, 0.0), 0.1);
        assert_eq!(gain_at(&g, 30_000.0), 0.9);
    }

    #[test]
    fn extension_holds_edge_values() {
        let b = from_125_to_4k([1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(b, [1.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 6.0, 6.0]);
    }
}
