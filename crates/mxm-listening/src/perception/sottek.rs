//! Tonality and loudness after the Sottek hearing model: ECMA-418-2 Clause 6 (the tonal and noise
//! parts of each band's specific loudness, separated by the band's autocorrelation) and Clause 8 (the
//! loudness that weights them). Written from the standard's text; the hearing model is `hearing`.
//!
//! Everything is on the common time base of 187.5 Hz: index `l` is the block ending `l / 187.5` s
//! after the sound's first sample (Formulae (18)–(19) align every band's blocks at their end).

use super::hearing::{self, BANDS, RATE};
use mxm_measure::spectrum::{fft, ifft};

/// The common time base, Hz (§6.2.6).
pub const RATE_TIME: f64 = RATE / 256.0;
/// The largest block and hop (Table 4).
const SB_MAX: usize = 8192;
const SH_MAX: usize = 2048;
/// The DFT of the windowed ACF (§6.2.5).
const DFT_TONAL: usize = 16384;
const EPS: f64 = 1e-12;
/// Noise reduction (Tables 7 and 9) and the calibration of tonality (Formula (51)).
const NR_ALPHA: f64 = 20.0;
const NR_BETA: f64 = 0.07;
const Q_A: f64 = 35.0;
const Q_B: f64 = 0.003;
/// c_T: a 1 kHz tone at 40 dB SPL is 1 tu_HMS.
pub const C_T: f64 = 2.875_861_5;
/// Loudness's power average (Table 12).
const W_N: f64 = 0.5331;
const E_A: f64 = 0.2918;
const E_B: f64 = 0.5459;
/// Blocks discarded from a single value: about the first 300 ms, the filters' transient (§6.2.9).
pub const SETTLE: usize = 57;

/// Block size by band (Table 4).
fn block(i: usize) -> usize {
    let z = hearing::z(i);
    if z <= 1.5 {
        8192
    } else if z <= 8.0 {
        4096
    } else if z <= 12.5 {
        2048
    } else {
        1024
    }
}

/// Bands averaged each side (Table 5).
fn neighbours(s: usize) -> usize {
    match s {
        8192 | 4096 => 2,
        2048 => 1,
        _ => 0,
    }
}

/// g(z) of Formula (46), from Table 8.
fn g(i: usize) -> f64 {
    let (c, d) = match block(i) {
        8192 => (18.21, 0.36),
        4096 => (12.14, 0.36),
        2048 => (417.54, 0.71),
        _ => (962.68, 0.69),
    };
    c / hearing::centre_hz(hearing::z(i)).powf(d)
}

/// The tonal and noise loudness of a sound, time by time and band by band.
#[derive(Clone, Debug, Default)]
pub struct Sottek {
    /// Per time index: specific tonal loudness N′_tonal (Formula (47)), sone_HMS/Bark_HMS.
    pub tonal: Vec<[f64; BANDS]>,
    /// Specific noise loudness N′_noise (Formula (48)).
    pub noise: Vec<[f64; BANDS]>,
    /// Specific loudness N′ (Formula (113)).
    pub loudness: Vec<[f64; BANDS]>,
    /// Specific tonality T′ (Formula (51)), tu_HMS.
    pub tonality: Vec<[f64; BANDS]>,
    /// The tonal component's frequency (Formula (39)), Hz.
    pub frequency: Vec<[f64; BANDS]>,
    /// Specific basis loudness N′_basis (Formula (25)), each band from its own blocks: the hearing
    /// model before the tonal weighting and its 3.5 Hz smoothing, so it keeps a hit's timing.
    pub basis: Vec<[f64; BANDS]>,
}

impl Sottek {
    /// Time-dependent loudness N(l), sone_HMS (Formula (116)).
    #[must_use]
    pub fn total_loudness(&self) -> Vec<f64> {
        self.loudness
            .iter()
            .map(|b| b.iter().sum::<f64>() * hearing::DZ)
            .collect()
    }

    /// Total basis loudness N_basis(l), sone_HMS (Formula (26)).
    #[must_use]
    pub fn total_basis(&self) -> Vec<f64> {
        self.basis
            .iter()
            .map(|b| b.iter().sum::<f64>() * hearing::DZ)
            .collect()
    }

    /// Time-dependent tonality T(l), tu_HMS, and its frequency (Formulae (61)–(62)).
    #[must_use]
    pub fn total_tonality(&self) -> Vec<(f64, f64)> {
        self.tonality
            .iter()
            .zip(&self.frequency)
            .map(|(t, f)| {
                let (i, v) =
                    t.iter().enumerate().fold(
                        (0, 0.0f64),
                        |(bi, bv), (i, &v)| if v > bv { (i, v) } else { (bi, bv) },
                    );
                (v, f[i])
            })
            .collect()
    }

    /// The single loudness value (Formula (117)): a power mean of N(l) after the settling blocks.
    #[must_use]
    pub fn single_loudness(&self) -> Option<f64> {
        let n = self.total_loudness();
        let tail = n.get(SETTLE..).filter(|t| !t.is_empty())?;
        let e = 1.0 / 2f64.log10();
        Some((tail.iter().map(|v| v.powf(e)).sum::<f64>() / tail.len() as f64).powf(1.0 / e))
    }

    /// The single tonality value (Formulae (63)–(64)): the mean of T(l) above 0.02 tu_HMS after the
    /// settling blocks.
    #[must_use]
    pub fn single_tonality(&self) -> Option<f64> {
        let t: Vec<f64> = self
            .total_tonality()
            .into_iter()
            .skip(SETTLE)
            .map(|(v, _)| v)
            .filter(|v| *v > 0.02)
            .collect();
        Some(t.iter().sum::<f64>() / (t.len() as f64 + EPS))
    }
}

/// The analysis of pressure `p` (Pa, 48 kHz, already faded in); `None` for an empty signal.
#[must_use]
pub fn analyse(p: &[f64]) -> Option<Sottek> {
    let n_samples = p.len();
    if n_samples == 0 {
        return None;
    }
    // Zero padding (§5.1.2.1), then the ear.
    let n_new = SH_MAX * ((n_samples + SH_MAX + SB_MAX).div_ceil(SH_MAX) - 1);
    let mut padded = vec![0.0; SB_MAX];
    padded.extend_from_slice(p);
    padded.resize(SB_MAX + n_new, 0.0);
    hearing::ear(&mut padded);
    let l_end = ((n_samples as f64 / RATE) * RATE_TIME).ceil() as usize;
    let fine = l_end + 1;

    let signals: Vec<Vec<f64>> = (0..BANDS).map(|j| hearing::band(&padded, j)).collect();

    // Per band, on the fine time base: N′_signal, N̂′_tonal and the tonal frequency.
    let mut n_signal = vec![[0.0; BANDS]; fine];
    let mut n_tonal_hat = vec![[0.0; BANDS]; fine];
    let mut f_tonal = vec![[0.0; BANDS]; fine];
    let mut basis = vec![[0.0; BANDS]; fine];
    for i in 0..BANDS {
        let s = block(i);
        let h = s / 4;
        let factor = s / 1024;
        let nb = neighbours(s);
        let group: Vec<usize> = if i == 0 {
            vec![0, 1]
        } else {
            let e = nb.min(i).min(BANDS - 1 - i);
            (i - e..=i + e).collect()
        };
        let df = hearing::bandwidth_hz(hearing::z(i));
        let tau_start = (0.5 / df).max(0.002);
        let tau_end = (4.0 / df).max(tau_start + 0.001);
        let m_start = ((tau_start * RATE).ceil() as usize).saturating_sub(1);
        let m_end = ((tau_end * RATE).floor() as usize)
            .saturating_sub(1)
            .min(3 * s / 4 - 1);
        let lags = m_end + 1;
        let blocks = (fine.div_ceil(factor) + 2).min((n_new + h).div_ceil(h));
        // The scaled ACFs averaged over the band's group.
        let mut acfs = vec![vec![0.0; lags]; blocks];
        let mut own = vec![0.0; blocks];
        for &j in &group {
            let x = &signals[j];
            for (lc, acc) in acfs.iter_mut().enumerate() {
                let start = lc * h + (SB_MAX - s);
                let mut rect: Vec<f64> = (0..s)
                    .map(|k| x.get(start + k).copied().unwrap_or(0.0).max(0.0))
                    .collect();
                let nb_loud = hearing::basis_loudness(j, hearing::rectified_rms(&rect));
                if j == i {
                    own[lc] = nb_loud;
                }
                if nb_loud <= 0.0 {
                    continue;
                }
                let Some(phi) = acf(&mut rect, lags) else {
                    continue;
                };
                let w = nb_loud / group.len() as f64;
                for (a, v) in acc.iter_mut().zip(&phi) {
                    *a += w * v;
                }
            }
        }
        // Over neighbouring blocks, for the two largest block sizes, except the first and last.
        if s >= 4096 && blocks >= 3 {
            let original = acfs.clone();
            for lc in 1..blocks - 1 {
                for m in 0..lags {
                    acfs[lc][m] =
                        (original[lc - 1][m] + original[lc][m] + original[lc + 1][m]) / 3.0;
                }
            }
        }
        // The lag window, and the tonal loudness from its spectrum's peak.
        let big_m = m_end - m_start + 1;
        let mut coarse = Vec::with_capacity(blocks);
        for a in &acfs {
            let signal0 = a[0];
            if signal0 <= 0.0 {
                coarse.push((0.0, 0.0, 0.0));
                continue;
            }
            let window = &a[m_start..=m_end];
            let mean = window.iter().sum::<f64>() / big_m as f64;
            let mut re = vec![0.0; DFT_TONAL];
            let mut im = vec![0.0; DFT_TONAL];
            for (r, v) in re.iter_mut().zip(window) {
                *r = v - mean;
            }
            if fft(&mut re, &mut im).is_none() {
                coarse.push((signal0, 0.0, 0.0));
                continue;
            }
            let (k_max, peak) = (0..DFT_TONAL / 2).map(|k| (k, re[k].hypot(im[k]))).fold(
                (0, 0.0f64),
                |(bk, bv), (k, v)| if v > bv { (k, v) } else { (bk, bv) },
            );
            let tonal = (2.0 * peak / (big_m as f64 / 2.0)).min(signal0);
            coarse.push((signal0, tonal, k_max as f64 * RATE / DFT_TONAL as f64));
        }
        // Onto the fine time base by linear interpolation (§6.2.6).
        for l in 0..fine {
            let pos = l as f64 / factor as f64;
            let lc = pos.floor() as usize;
            let u = pos - lc as f64;
            let at = |k: usize| coarse.get(k).copied().unwrap_or((0.0, 0.0, 0.0));
            let (a, b) = (at(lc), at(lc + 1));
            n_signal[l][i] = a.0 + u * (b.0 - a.0);
            let (p0, p1) = (
                own.get(lc).copied().unwrap_or(0.0),
                own.get(lc + 1).copied().unwrap_or(0.0),
            );
            basis[l][i] = p0 + u * (p1 - p0);
            n_tonal_hat[l][i] = a.1 + u * (b.1 - a.1);
            // The frequency is taken from the nearer block rather than interpolated: a mean of two
            // blocks' peaks is a frequency neither held.
            f_tonal[l][i] = if u < 0.5 || b.2 == 0.0 { a.2 } else { b.2 };
        }
    }

    // Noise reduction (§6.2.7), specific tonality (§6.2.8) and loudness (§8.1.1).
    let lp = LowPass::new();
    let mut out = Sottek {
        tonal: vec![[0.0; BANDS]; fine],
        noise: vec![[0.0; BANDS]; fine],
        loudness: vec![[0.0; BANDS]; fine],
        tonality: vec![[0.0; BANDS]; fine],
        frequency: f_tonal,
        basis,
    };
    for i in 0..BANDS {
        let tonal: Vec<f64> = (0..fine).map(|l| n_tonal_hat[l][i]).collect();
        let snr: Vec<f64> = (0..fine)
            .map(|l| n_tonal_hat[l][i] / (n_signal[l][i] - n_tonal_hat[l][i] + EPS))
            .collect();
        let whole: Vec<f64> = (0..fine).map(|l| n_signal[l][i]).collect();
        let (tonal, snr, whole) = (lp.run(&tonal), lp.run(&snr), lp.run(&whole));
        let gi = g(i);
        for l in 0..fine {
            let x = (-NR_ALPHA * (snr[l] / gi - NR_BETA)).exp();
            let nr = if x < 1.0 { 1.0 - x } else { 0.0 };
            let t = nr * tonal[l];
            out.tonal[l][i] = t;
            out.noise[l][i] = (whole[l] - t).max(0.0);
        }
    }
    for l in 0..fine {
        let noise_sum: f64 = out.noise[l].iter().sum();
        let tonal_max = out.tonal[l].iter().copied().fold(0.0f64, f64::max);
        let snr = tonal_max / (EPS + noise_sum);
        let x = (-Q_A * (snr - Q_B)).exp();
        let q = if x < 1.0 { 1.0 - x } else { 0.0 };
        let peak = (0..BANDS)
            .map(|i| out.tonal[l][i] + out.noise[l][i])
            .fold(0.0f64, f64::max);
        let e = E_A / (peak + EPS) + E_B;
        for i in 0..BANDS {
            let (t, n) = (out.tonal[l][i], out.noise[l][i]);
            out.tonality[l][i] = C_T * q * t;
            out.loudness[l][i] = if t + n > 0.0 {
                (t.powf(e) + (W_N * n).powf(e)).powf(1.0 / e)
            } else {
                0.0
            };
        }
    }
    Some(out)
}

/// The unbiased, energy-normalised ACF of a rectified block for lags `0..lags` (Formulae (27)–(29));
/// `None` for a block the FFT cannot take.
fn acf(block: &mut [f64], lags: usize) -> Option<Vec<f64>> {
    let s = block.len();
    let n = 2 * s;
    let mut prefix = vec![0.0; s + 1];
    for k in 0..s {
        prefix[k + 1] = prefix[k] + block[k] * block[k];
    }
    let mut re = vec![0.0; n];
    let mut im = vec![0.0; n];
    re[..s].copy_from_slice(block);
    fft(&mut re, &mut im)?;
    for k in 0..n {
        re[k] = re[k] * re[k] + im[k] * im[k];
        im[k] = 0.0;
    }
    ifft(&mut re, &mut im)?;
    let limit = (3 * s / 4).min(lags);
    Some(
        (0..lags)
            .map(|m| {
                if m >= limit {
                    return 0.0;
                }
                let e1 = prefix[s - m];
                let e2 = prefix[s] - prefix[m];
                re[m] / ((e1 * e2).sqrt() + EPS)
            })
            .collect(),
    )
}

/// The order-3 low-pass of §6.2.7 on the 187.5 Hz time base: cut-off 3.5 Hz, so τ = (6/32)/(7 Hz).
struct LowPass {
    b: [f64; 3],
    a: [f64; 3],
}

impl LowPass {
    fn new() -> Self {
        let tau = 6.0 / 32.0 / 7.0;
        let d = (-1.0 / (RATE_TIME * tau)).exp();
        let norm = d + d * d;
        let gain = (1.0 - d).powi(3) / norm;
        Self {
            b: [0.0, gain * d, gain * d * d],
            a: [-3.0 * d, 3.0 * d * d, -d * d * d],
        }
    }

    fn run(&self, x: &[f64]) -> Vec<f64> {
        let mut y = vec![0.0; x.len()];
        for n in 0..x.len() {
            let mut v = 0.0;
            for m in 0..3 {
                if n >= m {
                    v += self.b[m] * x[n - m];
                }
            }
            for m in 1..=3 {
                if n >= m {
                    v -= self.a[m - 1] * y[n - m];
                }
            }
            y[n] = v;
        }
        y
    }
}
