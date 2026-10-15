//! ISO 3382 decay parameters of an impulse response, broadband and per octave.
//!
//! - **Decay curves and times are `mxm-classic-verb-fit`'s `decay_times`** (the architecture audit's
//!   O6, 2026-10-15): backward integration of the squared response to its end (Schroeder 1965; ISO
//!   3382-1:2009 §3.3, method A, right for a render with no noise floor) and least-squares fits over
//!   the ISO spans, EDT 0 to −10 dB, T20 −5 to −25 dB, T30 −5 to −35 dB (ISO 3382-1 §3.5). Its
//!   refusal of an unreservable curve makes the analysis absent.
//! - **Energy ratios:** C50 and C80 = `10·lg(early/late)`, D50 = early fraction at 50 ms, Ts =
//!   energy centroid. These follow the definitions as read in Masovic's lecture notes §6
//!   (`research:effects/room-acoustics-simulation.md` §8).
//!
//! **Chosen, and decided here:**
//! - **Time zero** is the first sample within 20 dB of the squared response's peak.
//! - **Band filters** are zero-phase, power-complementary octave weights applied in the frequency
//!   domain: flat within a third of an octave of each centre, crossing over with a raised-cosine
//!   shape across the third of an octave around each band edge. They are not IEC 61260 filters.
//!   Zero phase smears energy slightly before sharp onsets, which matters for C50 at 63 Hz and
//!   not for decay times.
//!
//! **The analyser is trusted only because it reproduces known decays.** The tests build noise and
//! narrowband decays with a known T and check every parameter against its analytic value.

use std::f64::consts::PI;

use crate::bands::{self, NUM_BANDS};
use crate::fft::fft_in_place;
use mxm_fft::Complex;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DecayMetrics {
    pub edt_s: Option<f64>,
    pub t20_s: Option<f64>,
    pub t30_s: Option<f64>,
    pub c50_db: Option<f64>,
    pub c80_db: Option<f64>,
    pub d50: Option<f64>,
    pub ts_s: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    pub sample_rate: f64,
    /// Sample taken as time zero.
    pub onset_sample: usize,
    pub broadband: DecayMetrics,
    pub bands: [DecayMetrics; NUM_BANDS],
}

/// Analyses a single-channel response. `None` if it is silent, or if a decay curve's buffer cannot
/// be reserved.
pub fn analyse(ir: &[f32], sample_rate: f64) -> Option<Analysis> {
    let ir: Vec<f64> = ir.iter().map(|&v| f64::from(v)).collect();
    analyse_f64(&ir, sample_rate)
}

/// As [`analyse`], for `f64` samples.
pub fn analyse_f64(ir: &[f64], sample_rate: f64) -> Option<Analysis> {
    let energy: Vec<f64> = ir.iter().map(|v| v * v).collect();
    let peak = energy.iter().copied().fold(0.0, f64::max);
    if peak.is_nan() || peak <= 0.0 || sample_rate.is_nan() || sample_rate <= 0.0 {
        return None;
    }
    let onset = energy.iter().position(|&e| e >= peak * 0.01)?;
    let broadband = metrics(&energy[onset..], sample_rate)?;
    let split = band_split(ir, sample_rate);
    let mut bands = [DecayMetrics::default(); NUM_BANDS];
    for (band, signal) in bands.iter_mut().zip(&split) {
        let e: Vec<f64> = signal[onset..].iter().map(|v| v * v).collect();
        *band = metrics(&e, sample_rate)?;
    }
    Some(Analysis {
        sample_rate,
        onset_sample: onset,
        broadband,
        bands,
    })
}

/// Weight of band `k` at `frequency_hz`; the squares of all bands' weights sum to one.
pub(crate) fn band_weight(k: usize, frequency_hz: f64) -> f64 {
    let first = bands::exact_centre_hz(0);
    let last = bands::exact_centre_hz(NUM_BANDS - 1);
    if frequency_hz.is_nan() || frequency_hz <= first {
        return if k == 0 { 1.0 } else { 0.0 };
    }
    if frequency_hz >= last {
        return if k == NUM_BANDS - 1 { 1.0 } else { 0.0 };
    }
    let x = (frequency_hz / first).log10() / 0.3;
    let j = (x.floor() as usize).min(NUM_BANDS - 2);
    let frac = x - j as f64;
    let half = 1.0 / 6.0;
    let u = ((frac - (0.5 - half)) / (2.0 * half)).clamp(0.0, 1.0);
    if k == j {
        (PI * u / 2.0).cos()
    } else if k == j + 1 {
        (PI * u / 2.0).sin()
    } else {
        0.0
    }
}

/// Energy of `signal` in each octave band, by the same power-complementary weights as
/// [`band_split`], so the bands' energies sum to the signal's.
pub fn band_energies(signal: &[f64], sample_rate: f64) -> [f64; NUM_BANDS] {
    let n = signal.len().max(2).next_power_of_two();
    let mut spectrum = vec![Complex::default(); n];
    for (c, &v) in spectrum.iter_mut().zip(signal) {
        c.re = v;
    }
    fft_in_place(&mut spectrum, false);
    let mut energy = [0.0; NUM_BANDS];
    for (i, c) in spectrum.iter().enumerate() {
        let bin = if i <= n / 2 { i } else { n - i };
        let frequency = bin as f64 * sample_rate / n as f64;
        let power = (c.re * c.re + c.im * c.im) / n as f64;
        for (k, e) in energy.iter_mut().enumerate() {
            let w = band_weight(k, frequency);
            *e += power * w * w;
        }
    }
    energy
}

/// Splits a response into octave bands, each the same length as the input.
pub fn band_split(ir: &[f64], sample_rate: f64) -> [Vec<f64>; NUM_BANDS] {
    let n = (ir.len().max(2)).next_power_of_two() * 2;
    let mut spectrum = vec![Complex::default(); n];
    for (c, &v) in spectrum.iter_mut().zip(ir) {
        c.re = v;
    }
    fft_in_place(&mut spectrum, false);
    std::array::from_fn(|k| {
        let mut band: Vec<Complex<f64>> = spectrum
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let bin = if i <= n / 2 { i } else { n - i };
                let w = band_weight(k, bin as f64 * sample_rate / n as f64);
                Complex {
                    re: c.re * w,
                    im: c.im * w,
                }
            })
            .collect();
        fft_in_place(&mut band, true);
        band.iter().take(ir.len()).map(|c| c.re).collect()
    })
}

/// `erfc(1/√2)`: the fraction of a Gaussian's samples beyond one standard deviation, the
/// normaliser of Abel & Huang's echo density (research page §7) [D].
const ERFC_INV_SQRT2: f64 = 0.317_310_507_862_914_1;

/// Normalised echo density (Abel & Huang, AES 121st Convention, paper 6985, 2006, eq. 1), every
/// `hop_s`, over a Hann window of `window_s`. Returns `(time_s, density)` pairs, time measured
/// from the first sample. A Gaussian tail gives 1; sparse discrete reflections give less.
pub fn echo_density(ir: &[f64], sample_rate: f64, window_s: f64, hop_s: f64) -> Vec<(f64, f64)> {
    let half = ((window_s * sample_rate / 2.0).round() as usize).max(1);
    let hop = ((hop_s * sample_rate).round() as usize).max(1);
    let weights: Vec<f64> = (0..=2 * half)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / (2 * half) as f64).cos())
        .collect();
    let total: f64 = weights.iter().sum();
    let mut out = Vec::new();
    let mut centre = half;
    while centre + half < ir.len() {
        let window = &ir[centre - half..=centre + half];
        let sigma = (window
            .iter()
            .zip(&weights)
            .map(|(h, w)| w * h * h)
            .sum::<f64>()
            / total)
            .sqrt();
        let beyond: f64 = window
            .iter()
            .zip(&weights)
            .filter(|(h, _)| h.abs() > sigma)
            .map(|(_, w)| w)
            .sum();
        out.push((centre as f64 / sample_rate, beyond / total / ERFC_INV_SQRT2));
        centre += hop;
    }
    out
}

/// Correlation coefficient of two responses in each octave band over samples `from..to`. Each
/// band signal is first divided by the pair's common smoothed envelope, so a decaying tail weighs
/// every part of the span equally.
pub fn band_correlation(
    a: &[f64],
    b: &[f64],
    sample_rate: f64,
    from: usize,
    to: usize,
) -> [f64; NUM_BANDS] {
    let (sa, sb) = (band_split(a, sample_rate), band_split(b, sample_rate));
    std::array::from_fn(|k| {
        let to = to.min(sa[k].len()).min(sb[k].len());
        let half = (0.025 * sample_rate) as usize;
        let mut prefix = vec![0.0; sa[k].len() + 1];
        for i in 0..sa[k].len() {
            prefix[i + 1] = prefix[i] + sa[k][i] * sa[k][i] + sb[k][i] * sb[k][i];
        }
        let (mut xy, mut xx, mut yy) = (0.0, 0.0, 0.0);
        for i in from..to {
            let lo = i.saturating_sub(half);
            let hi = (i + half + 1).min(sa[k].len());
            let env = (prefix[hi] - prefix[lo]) / (hi - lo) as f64;
            if env <= 0.0 {
                continue;
            }
            let (x, y) = (sa[k][i], sb[k][i]);
            xy += x * y / env;
            xx += x * x / env;
            yy += y * y / env;
        }
        if xx > 0.0 && yy > 0.0 {
            xy / (xx * yy).sqrt()
        } else {
            0.0
        }
    })
}

/// The correlation coefficient an octave band of an isotropic diffuse field gives two omnis
/// `spacing_m` apart: `sin(kd)/kd` (Cook et al. 1955, via Jacobsen & Roisin 2000), averaged over the
/// band with the analyser's band weight squared. Coherence is its square (Jacobsen & Roisin eq. 7).
pub fn diffuse_correlation(band: usize, spacing_m: f64, speed: f64, sample_rate: f64) -> f64 {
    let steps = 20_000;
    let (mut num, mut den) = (0.0, 0.0);
    for i in 1..steps {
        let f = i as f64 / steps as f64 * sample_rate / 2.0;
        let w = band_weight(band, f).powi(2);
        let kd = 2.0 * PI * f * spacing_m / speed;
        num += w * kd.sin() / kd;
        den += w;
    }
    num / den
}

/// The parameters of one energy signal from its time zero; `None` only if its decay curve's buffer
/// cannot be reserved.
fn metrics(energy: &[f64], sample_rate: f64) -> Option<DecayMetrics> {
    let total: f64 = energy.iter().sum();
    if total.is_nan() || total <= 0.0 {
        return Some(DecayMetrics::default());
    }
    let times = mxm_classic_verb_fit::decay_times(energy, sample_rate).ok()?;
    let ratio = |ms: f64| {
        let n = ((ms / 1000.0) * sample_rate).round() as usize;
        let early: f64 = energy.iter().take(n).sum();
        let late = total - early;
        (early, late)
    };
    let (e50, l50) = ratio(50.0);
    let (e80, l80) = ratio(80.0);
    let db =
        |early: f64, late: f64| (early > 0.0 && late > 0.0).then(|| 10.0 * (early / late).log10());
    let centroid: f64 = energy
        .iter()
        .enumerate()
        .map(|(i, e)| i as f64 / sample_rate * e)
        .sum::<f64>()
        / total;
    Some(DecayMetrics {
        edt_s: times.edt_s,
        t20_s: times.t20_s,
        t30_s: times.t30_s,
        c50_db: db(e50, l50),
        c80_db: db(e80, l80),
        d50: Some(e50 / total),
        ts_s: Some(centroid),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxm_rng::SplitMix64;

    const FS: f64 = 48_000.0;
    /// `ln(10^6)`: energy falls 60 dB in T seconds as `e^(−k·t)` with `k = LN_1E6 / T`.
    const LN_1E6: f64 = 13.815_510_557_964_274;

    fn noise_decay(t60: f64, seconds: f64, seed: u64) -> Vec<f64> {
        let mut rng = SplitMix64::new(seed);
        let k = LN_1E6 / t60;
        (0..(seconds * FS) as usize)
            .map(|i| crate::rng::normal(&mut rng) * (-k * i as f64 / FS / 2.0).exp())
            .collect()
    }

    fn within(value: Option<f64>, expected: f64, rel: f64) -> bool {
        value.is_some_and(|v| ((v - expected) / expected).abs() <= rel)
    }

    #[test]
    fn band_energies_sum_to_the_signal_and_find_a_tone() {
        let fs = 48_000.0;
        let n = 4096;
        let x: Vec<f64> = (0..n)
            .map(|i| {
                let t = i as f64;
                (2.0 * PI * 1000.0 * t / fs).sin() * (PI * t / n as f64).sin().powi(2)
            })
            .collect();
        let e = band_energies(&x, fs);
        let total: f64 = x.iter().map(|v| v * v).sum();
        assert!((e.iter().sum::<f64>() / total - 1.0).abs() < 1e-9);
        assert!(e[4] / total > 0.99);
    }

    #[test]
    fn band_weights_are_power_complementary_and_flat_near_centres() {
        for i in 0..2000 {
            let f = 20.0 * (1.0035f64).powi(i);
            let sum: f64 = (0..NUM_BANDS).map(|k| band_weight(k, f).powi(2)).sum();
            assert!((sum - 1.0).abs() < 1e-12, "{f} Hz: {sum}");
        }
        for k in 0..NUM_BANDS {
            let c = bands::exact_centre_hz(k);
            for off in [-0.3, 0.0, 0.3] {
                let f = c * 2f64.powf(off);
                assert!(
                    (band_weight(k, f) - 1.0).abs() < 1e-12,
                    "band {k} at {off} oct"
                );
            }
        }
    }

    #[test]
    fn broadband_exponential_decay_reproduces_every_parameter() {
        let t60 = 1.2;
        let ir = noise_decay(t60, 3.0, 11);
        let a = analyse_f64(&ir, FS).unwrap();
        let m = a.broadband;
        let k = LN_1E6 / t60;
        assert!(within(m.t20_s, t60, 0.03), "T20 {:?}", m.t20_s);
        assert!(within(m.t30_s, t60, 0.03), "T30 {:?}", m.t30_s);
        assert!(within(m.edt_s, t60, 0.05), "EDT {:?}", m.edt_s);
        let c80 = 10.0 * ((0.08 * k).exp() - 1.0).log10();
        assert!(
            (m.c80_db.unwrap() - c80).abs() < 0.3,
            "C80 {:?} vs {c80}",
            m.c80_db
        );
        let c50 = 10.0 * ((0.05 * k).exp() - 1.0).log10();
        assert!(
            (m.c50_db.unwrap() - c50).abs() < 0.3,
            "C50 {:?} vs {c50}",
            m.c50_db
        );
        let d50 = 1.0 - (-0.05 * k).exp();
        assert!(
            (m.d50.unwrap() - d50).abs() < 0.02,
            "D50 {:?} vs {d50}",
            m.d50
        );
        assert!(
            within(m.ts_s, 1.0 / k, 0.05),
            "Ts {:?} vs {}",
            m.ts_s,
            1.0 / k
        );
    }

    /// One exponentially decaying sinusoid per band, each band with its own T. The signal has no
    /// random envelope, so this checks band separation and the decay fit alone. Random-phase
    /// narrowband sums were tried first; their envelope fluctuation grows as bandwidth shrinks and
    /// made the 125 Hz band miss by 11 %. That was the signal's variance, which the broadband noise
    /// test already covers.
    #[test]
    fn per_band_decays_are_separated() {
        let seconds = 5.0;
        let len = (seconds * FS) as usize;
        let cases = [(1usize, 2.0), (3usize, 1.0), (5usize, 0.6), (7usize, 0.35)];
        let mut ir = vec![0.0; len];
        for &(band, t60) in &cases {
            let k = LN_1E6 / t60;
            let f = bands::exact_centre_hz(band);
            for (i, v) in ir.iter_mut().enumerate() {
                let t = i as f64 / FS;
                *v += (2.0 * PI * f * t + 0.3).sin() * (-k * t / 2.0).exp();
            }
        }
        let a = analyse_f64(&ir, FS).unwrap();
        for &(band, t60) in &cases {
            let m = a.bands[band];
            assert!(
                within(m.t30_s, t60, 0.02),
                "band {band}: T30 {:?} vs {t60}",
                m.t30_s
            );
            assert!(
                within(m.t20_s, t60, 0.02),
                "band {band}: T20 {:?} vs {t60}",
                m.t20_s
            );
            assert!(
                within(m.edt_s, t60, 0.02),
                "band {band}: EDT {:?} vs {t60}",
                m.edt_s
            );
        }
    }

    #[test]
    fn silence_and_short_decays() {
        assert!(analyse(&[0.0; 100], FS).is_none());
        // A single impulse falls to silence at once: no span to fit.
        let a = analyse_f64(&[1.0, 0.0, 0.0, 0.0], FS).unwrap();
        assert!(a.broadband.t30_s.is_none() && a.broadband.edt_s.is_none());
    }
}
