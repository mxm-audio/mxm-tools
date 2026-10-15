//! The minimum-phase design, built on the collection's FFT (`mxm-fft`), and the one call every
//! transform here goes through.
//!
//! `mxm-fft`'s convention: forward `X[k] = Σ x[n]·e^(−i2πkn/N)`, inverse `x[n] = (1/N)·Σ
//! X[k]·e^(+i2πkn/N)`. The minimum-phase design depends on it: folding the real cepstrum yields a
//! causal (minimum-phase) response only when the forward transform uses the negative exponent.
//! Until the architecture audit's O6 (2026-10-15) this crate had its own radix-2 FFT.

use std::cell::RefCell;
use std::f64::consts::PI;

use mxm_fft::{Complex, Planner};

thread_local! {
    /// One planner per thread: it keeps every length's plan, and a thread's results do not depend on
    /// which other threads planned what.
    static PLANNER: RefCell<Planner<f64>> = RefCell::new(Planner::new());
}

/// The transform of `buf` in place: forward unnormalised, inverse scaled by 1/N.
pub(crate) fn fft_in_place(buf: &mut [Complex<f64>], inverse: bool) {
    let fft = PLANNER.with(|planner| planner.borrow_mut().complex(buf.len()));
    let mut scratch = fft.make_scratch();
    let done = if inverse {
        fft.inverse(buf, &mut scratch)
    } else {
        fft.forward(buf, &mut scratch)
    };
    done.expect("a plan of the buffer's own length, with its own scratch");
}

/// A minimum-phase FIR whose magnitude follows `magnitude(f)`, by folding the real cepstrum.
///
/// `grid` is the FFT length (a power of two) at `sample_rate`; `taps` is the output length,
/// `taps <= grid / 2`. The last quarter of the taps is faded with a half-Hann window, and the final
/// tap is exactly zero, so a rendered response ends in exact silence.
///
/// Method: Oppenheim & Schafer, *Discrete-Time Signal Processing*, 3rd ed., §13.5.3, eqs.
/// 13.41–13.42. The complex cepstrum of a minimum-phase sequence is the real cepstrum times
/// `2u[n] − δ[n]`.
pub(crate) fn minimum_phase_fir(
    magnitude: impl Fn(f64) -> f64,
    sample_rate: f64,
    grid: usize,
    taps: usize,
) -> Vec<f64> {
    assert!(grid.is_power_of_two() && taps >= 2 && taps <= grid / 2);
    const FLOOR: f64 = 1e-10;
    let mut buf = vec![Complex::<f64>::default(); grid];
    for (k, c) in buf.iter_mut().enumerate() {
        let bin = if k <= grid / 2 { k } else { grid - k };
        let f = bin as f64 * sample_rate / grid as f64;
        c.re = magnitude(f).max(FLOOR).ln();
    }
    // Real cepstrum: the inverse transform of the log magnitude.
    fft_in_place(&mut buf, true);
    for c in buf.iter_mut().take(grid / 2).skip(1) {
        c.re *= 2.0;
        c.im *= 2.0;
    }
    for c in buf.iter_mut().skip(grid / 2 + 1) {
        *c = Complex::default();
    }
    // Back to a complex log spectrum, exponentiate, and return to time.
    fft_in_place(&mut buf, false);
    for c in buf.iter_mut() {
        let m = c.re.exp();
        let (s, co) = c.im.sin_cos();
        *c = Complex {
            re: m * co,
            im: m * s,
        };
    }
    fft_in_place(&mut buf, true);
    let fade_start = taps - taps / 4;
    let mut out: Vec<f64> = buf.iter().take(taps).map(|c| c.re).collect();
    let fade_len = (taps - fade_start) as f64;
    for (i, v) in out.iter_mut().enumerate().skip(fade_start) {
        let x = (i - fade_start) as f64 / fade_len;
        *v *= 0.5 * (1.0 + (PI * x).cos());
    }
    out[taps - 1] = 0.0;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let n = 64;
        let mut buf: Vec<Complex<f64>> = (0..n)
            .map(|i| Complex {
                re: (i as f64 * 0.37).sin(),
                im: (i as f64 * 0.11).cos(),
            })
            .collect();
        let original = buf.clone();
        fft_in_place(&mut buf, false);
        fft_in_place(&mut buf, true);
        for (a, b) in buf.iter().zip(&original) {
            assert!((a.re - b.re).abs() < 1e-12 && (a.im - b.im).abs() < 1e-12);
        }
    }

    #[test]
    fn forward_convention_is_negative_exponent() {
        // A unit impulse at n = 1 has spectrum e^(−i2πk/N); at k = N/4 that is −i.
        let n = 16;
        let mut buf = vec![Complex::default(); n];
        buf[1].re = 1.0;
        fft_in_place(&mut buf, false);
        assert!(buf[4].re.abs() < 1e-12 && (buf[4].im + 1.0).abs() < 1e-12);
    }

    #[test]
    fn minimum_phase_matches_magnitude_and_is_front_loaded() {
        let fs = 48_000.0;
        let target = |f: f64| if f < 1000.0 { 1.0 } else { 0.25 };
        let smooth = |f: f64| {
            let x = ((f / 1000.0).log2()).clamp(-1.0, 1.0);
            let t = 0.5 * (1.0 - (PI * (x + 1.0) / 2.0).cos());
            (1.0 - t) * target(0.0) + t * target(20_000.0)
        };
        let h = minimum_phase_fir(smooth, fs, 8192, 2048);
        // Energy concentrated at the start: that is what minimum phase means.
        let total: f64 = h.iter().map(|v| v * v).sum();
        let early: f64 = h.iter().take(64).map(|v| v * v).sum();
        assert!(early / total > 0.99, "early fraction {}", early / total);
        // Magnitude matches the target within 0.2 dB at a few frequencies.
        let mut buf = vec![Complex::default(); 8192];
        for (i, v) in h.iter().enumerate() {
            buf[i].re = *v;
        }
        fft_in_place(&mut buf, false);
        for &f in &[100.0, 700.0, 1000.0, 1500.0, 5000.0] {
            let k = (f / fs * 8192.0).round() as usize;
            let m = (buf[k].re.powi(2) + buf[k].im.powi(2)).sqrt();
            let err_db = 20.0 * (m / smooth(k as f64 * fs / 8192.0)).log10();
            assert!(err_db.abs() < 0.2, "{f} Hz off by {err_db} dB");
        }
        assert_eq!(*h.last().unwrap(), 0.0);
    }
}
