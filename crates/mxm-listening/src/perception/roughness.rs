//! Roughness after the Sottek hearing model, ECMA-418-2 Clause 7: each band's envelope spectrum,
//! scaled by its loudness, cleaned of noise by the correlation across bands, its peaks weighted by how
//! rough their modulation rate sounds, and a harmonic complex of modulation found; then a nonlinear
//! transform that treats broadband and narrowband roughness differently. Written from the standard's
//! text; the optional entropy weighting (§7.1.6), which needs a rotational-speed signal, is not used.

use super::hearing::{self, BANDS, RATE};
use mxm_measure::spectrum::fft;

/// Block and hop at 48 kHz (§7.1.1), and after the envelope's decimation by 32.
const SB: usize = 16384;
const SH: usize = 4096;
const DECIMATE: usize = 32;
const SB_E: usize = SB / DECIMATE;
/// The envelope spectrum's resolution, Hz: 1500 Hz / 512.
const DF: f64 = RATE / DECIMATE as f64 / SB_E as f64;
/// The output rate, Hz (§7.1.7).
pub const RATE_OUT: f64 = 50.0;
/// Values before this index (about 300 ms) are the filters' transient (§7.1.8).
pub const SETTLE: usize = 16;
const C_R: f64 = 0.018_068_5;
const EPS: f64 = 1e-12;

/// Table 10: the Hann window's quadratic-fit bias, Hz, for θ = 0 … 33.
const BIAS: [f64; 34] = [
    0.0000, 0.0457, 0.0907, 0.1346, 0.1765, 0.2157, 0.2515, 0.2828, 0.3084, 0.3269, 0.3364, 0.3348,
    0.3188, 0.2844, 0.2259, 0.1351, 0.0000, -0.1351, -0.2259, -0.2844, -0.3188, -0.3348, -0.3364,
    -0.3269, -0.3084, -0.2828, -0.2515, -0.2157, -0.1765, -0.1346, -0.0907, -0.0457, 0.0000,
    0.0000,
];

/// Time-dependent specific roughness R′(l₅₀, z), asper per Bark_HMS, at [`RATE_OUT`].
#[derive(Clone, Debug, Default)]
pub struct Roughness {
    pub specific: Vec<[f64; BANDS]>,
}

impl Roughness {
    /// Time-dependent roughness R(l₅₀), asper (Formula (111)).
    #[must_use]
    pub fn total(&self) -> Vec<f64> {
        self.specific
            .iter()
            .map(|b| b.iter().sum::<f64>() * hearing::DZ)
            .collect()
    }

    /// The single value: the 90th percentile after the settling values (§7.1.10).
    #[must_use]
    pub fn single(&self) -> Option<f64> {
        let mut r: Vec<f64> = self.total().into_iter().skip(SETTLE).collect();
        if r.is_empty() {
            return None;
        }
        r.sort_by(f64::total_cmp);
        let rank = 0.9 * (r.len() - 1) as f64;
        let (lo, u) = (rank.floor() as usize, rank - rank.floor());
        Some(r[lo] + u * (r[(lo + 1).min(r.len() - 1)] - r[lo]))
    }
}

/// Roughness of pressure `p` (Pa, 48 kHz, faded in); `None` for an empty signal.
#[must_use]
pub fn analyse(p: &[f64]) -> Option<Roughness> {
    let n_samples = p.len();
    if n_samples == 0 {
        return None;
    }
    let mut padded = vec![0.0; SB];
    padded.extend_from_slice(p);
    hearing::ear(&mut padded);
    let total = padded.len();
    // Blocks (§5.1.5.2): l = 0 … l_last − 1 every hop, and the last one holding the last SB samples.
    let l_last = (n_samples + SB).div_ceil(SH);
    let mut starts: Vec<usize> = (0..l_last).map(|l| l * SH).collect();
    starts.push(total.saturating_sub(SB));
    let mut times: Vec<f64> = (0..l_last).map(|l| (l * SH) as f64 / RATE).collect();
    times.push(n_samples as f64 / RATE);
    let blocks = starts.len();

    // Per block and band: the scaled envelope power spectrum (Formula (66)), k = 0 … 256.
    let window: Vec<f64> = (0..SB_E)
        .map(|n| {
            (0.5 - 0.5 * (std::f64::consts::TAU * n as f64 / SB_E as f64).cos()) / 0.375f64.sqrt()
        })
        .collect();
    let mut spectra = vec![vec![[0.0; SB_E / 2 + 1]; BANDS]; blocks];
    let mut loud = vec![[0.0; BANDS]; blocks];
    let mut phi0 = vec![[0.0; BANDS]; blocks];
    for z in 0..BANDS {
        let x = hearing::band(&padded, z);
        for (l, &start) in starts.iter().enumerate() {
            let block: Vec<f64> = (0..SB)
                .map(|n| x.get(start + n).copied().unwrap_or(0.0))
                .collect();
            loud[l][z] = hearing::basis_loudness(z, hearing::rectified_rms(&block));
            if loud[l][z] <= 0.0 {
                continue;
            }
            let Some((re, im)) = crate::repr::fir::analytic(&block) else {
                continue;
            };
            let mut er: Vec<f64> = (0..SB_E)
                .map(|n| re[n * DECIMATE].hypot(im[n * DECIMATE]) * window[n])
                .collect();
            phi0[l][z] = er.iter().map(|v| v * v).sum();
            let mut ei = vec![0.0; SB_E];
            if fft(&mut er, &mut ei).is_none() {
                continue;
            }
            for k in 0..=SB_E / 2 {
                spectra[l][z][k] = er[k] * er[k] + ei[k] * ei[k];
            }
        }
    }
    let mut amplitude = vec![[0.0; BANDS]; blocks];
    for l in 0..blocks {
        let n_max = loud[l].iter().copied().fold(0.0f64, f64::max);
        let mut scaled = vec![[0.0; SB_E / 2 + 1]; BANDS];
        for z in 0..BANDS {
            let d = n_max * phi0[l][z];
            if d > 0.0 {
                for k in 0..=SB_E / 2 {
                    scaled[z][k] = loud[l][z] * loud[l][z] / d * spectra[l][z][k];
                }
            }
        }
        // Averaged over three bands, the first and last kept (§7.1.4).
        let mut avg = scaled.clone();
        for z in 1..BANDS - 1 {
            for k in 0..=SB_E / 2 {
                avg[z][k] = (scaled[z - 1][k] + scaled[z][k] + scaled[z + 1][k]) / 3.0;
            }
        }
        let weights = noise_weights(&avg);
        for z in 0..BANDS {
            let spectrum: Vec<f64> = (0..=SB_E / 2).map(|k| avg[z][k] * weights[k]).collect();
            amplitude[l][z] = band_amplitude(&spectrum, hearing::centre_hz(hearing::z(z)));
        }
    }

    // Onto 50 Hz (§7.1.7), then the nonlinear transform and the smoother.
    let last = (times[blocks - 1] * RATE_OUT).floor() as usize;
    let mut est = vec![[0.0; BANDS]; last + 1];
    for z in 0..BANDS {
        let y: Vec<f64> = (0..blocks).map(|l| amplitude[l][z]).collect();
        for (l50, row) in est.iter_mut().enumerate() {
            row[z] = pchip(&times, &y, l50 as f64 / RATE_OUT).max(0.0);
        }
    }
    let mut specific = vec![[0.0; BANDS]; last + 1];
    for l50 in 0..=last {
        let quadratic = (est[l50].iter().map(|v| v * v).sum::<f64>() / BANDS as f64).sqrt();
        let linear = est[l50].iter().sum::<f64>() / BANDS as f64;
        let b = if linear != 0.0 {
            quadratic / linear
        } else {
            0.0
        };
        let e = 0.37106 * ((1.6407 * (b - 2.5804)).tanh() + 1.0) * 0.5 + 0.58449;
        for z in 0..BANDS {
            let target = C_R * est[l50][z].powf(e);
            specific[l50][z] = if l50 == 0 {
                target
            } else {
                let prev = specific[l50 - 1][z];
                let tau = if target >= prev { 0.0625 } else { 0.5 };
                let a = (-1.0 / (RATE_OUT * tau)).exp();
                target * (1.0 - a) + prev * a
            };
        }
    }
    Some(Roughness { specific })
}

/// The noise-suppression weights w(k), k = 0 … 256 (Formulae (68)–(71)).
fn noise_weights(avg: &[[f64; SB_E / 2 + 1]]) -> Vec<f64> {
    let s: Vec<f64> = (0..=SB_E / 2)
        .map(|k| avg.iter().map(|b| b[k]).sum())
        .collect();
    let mut mid: Vec<f64> = s[2..=255].to_vec();
    mid.sort_by(f64::total_cmp);
    let median = if mid.len() % 2 == 1 {
        mid[mid.len() / 2]
    } else {
        0.5 * (mid[mid.len() / 2 - 1] + mid[mid.len() / 2])
    };
    let raw: Vec<f64> = (0..=SB_E / 2)
        .map(|k| {
            0.0856 * s[k] / (median + 1e-10) * (0.1891 * (0.0120 * k as f64).exp()).clamp(0.0, 1.0)
        })
        .collect();
    let top = raw[2..=255].iter().copied().fold(0.0f64, f64::max);
    raw.iter()
        .map(|&w| {
            if w >= 0.05 * top {
                (w - 0.1407).clamp(0.0, 1.0)
            } else {
                0.0
            }
        })
        .collect()
}

/// A band's roughness amplitude A(l, z) from its weighted envelope spectrum (§7.1.5).
fn band_amplitude(spectrum: &[f64], centre_hz: f64) -> f64 {
    let peaks = peaks(spectrum);
    if peaks.is_empty() {
        return 0.0;
    }
    let f_max = 72.6937 * (1.0 - 1.1739 * (-5.4583 * centre_hz / 1000.0).exp());
    let octave = (centre_hz / 1000.0).log2();
    let (r1, r2) = if centre_hz < 1000.0 {
        (0.3560, 0.8049)
    } else {
        (0.8024, 0.9333)
    };
    let r_max = 1.0 / (1.0 + r1 * octave.abs().powf(r2));
    let q2_high = if centre_hz / 1000.0 < 2f64.powf(-3.4253) {
        0.2471
    } else {
        0.2471 + 0.0129 * (octave + 3.4253).powi(2)
    };
    let weight = |f: f64, q1: f64, q2: f64| -> f64 {
        1.0 / (1.0 + ((f / f_max - f_max / f) * q1).powi(2)).powf(q2)
    };
    // (rate, weighted amplitude) per peak (Formula (83)).
    let weighted: Vec<(f64, f64)> = peaks
        .iter()
        .map(|&(f, a)| {
            let w = if f <= DF {
                0.0
            } else if f <= f_max {
                a * r_max
            } else {
                weight(f, 1.2822, q2_high) * a * r_max
            };
            (f, w)
        })
        .collect();
    // The dominant harmonic complex of modulation (§7.1.5.3).
    let mut best: Option<(f64, Vec<usize>, usize)> = None;
    for i0 in 0..weighted.len() {
        let f0 = weighted[i0].0;
        if f0 <= 0.0 {
            continue;
        }
        let mut chosen: Vec<(i64, usize, f64)> = Vec::new();
        for (i, &(f, _)) in weighted.iter().enumerate() {
            let r = (f / f0).round() as i64;
            if r < 1 {
                continue;
            }
            let err = (f / (r as f64 * f0) - 1.0).abs();
            match chosen.iter_mut().find(|c| c.0 == r) {
                Some(c) if err < c.2 => *c = (r, i, err),
                Some(_) => {}
                None => chosen.push((r, i, err)),
            }
        }
        let set: Vec<usize> = chosen.iter().filter(|c| c.2 < 0.04).map(|c| c.1).collect();
        let energy: f64 = set.iter().map(|&i| weighted[i].1).sum();
        if best.as_ref().is_none_or(|b| energy > b.0) {
            best = Some((energy, set, i0));
        }
    }
    let Some((_, set, i_max)) = best else {
        return 0.0;
    };
    let sum: f64 = set.iter().map(|&i| weighted[i].1).sum();
    let centroid = set
        .iter()
        .map(|&i| weighted[i].0 * weighted[i].1)
        .sum::<f64>()
        / (sum + EPS);
    let i_peak = set
        .iter()
        .copied()
        .max_by(|&a, &b| weighted[a].1.total_cmp(&weighted[b].1))
        .unwrap_or(i_max);
    let w_peak = 1.0 + 0.1 * (centroid - weighted[i_peak].0).abs().powf(0.749);
    let f_fund = weighted[i_max].0;
    let total: f64 = set.iter().map(|&i| w_peak * weighted[i].1).sum();
    let a = if f_fund <= DF {
        0.0
    } else if f_fund <= f_max {
        weight(f_fund, 0.7066, 1.0967 - 0.0640 * octave) * total
    } else {
        total
    };
    if a < 0.074376 { 0.0 } else { a }
}

/// The spectrum's peaks as (refined rate in Hz, amplitude): the ten most prominent local maxima over
/// k = 3 … 254 above 5 % of the largest, each placed by a quadratic fit with the Hann window's bias
/// removed (Formulae (72)–(82)).
fn peaks(s: &[f64]) -> Vec<(f64, f64)> {
    let mut found: Vec<(usize, f64)> = Vec::new();
    for k in 3..=254 {
        if s[k] > s[k - 1] && s[k] >= s[k + 1] && s[k] > 0.0 {
            found.push((k, prominence(s, k)));
        }
    }
    found.sort_by(|a, b| b.1.total_cmp(&a.1));
    found.truncate(10);
    let top = found.iter().map(|&(k, _)| s[k]).fold(0.0f64, f64::max);
    let mut kept: Vec<usize> = found
        .iter()
        .map(|&(k, _)| k)
        .filter(|&k| s[k] > 0.05 * top)
        .collect();
    kept.sort_unstable();
    kept.into_iter()
        .map(|k| {
            let (y0, y1, y2) = (s[k - 1], s[k], s[k + 1]);
            // The parabola through (k−1, k, k+1): its vertex, in bins.
            let denom = y0 - 2.0 * y1 + y2;
            let vertex = if denom.abs() > 0.0 {
                k as f64 + 0.5 * (y0 - y2) / denom
            } else {
                k as f64
            };
            let rough = vertex * DF;
            (rough + bias(rough), y0 + y1 + y2)
        })
        .collect()
}

/// A peak's topographic prominence: its height over the higher of the lowest valleys either side,
/// each side running to a higher sample or the end.
fn prominence(s: &[f64], k: usize) -> f64 {
    let h = s[k];
    let mut left_min = h;
    for j in (2..k).rev() {
        if s[j] > h {
            break;
        }
        left_min = left_min.min(s[j]);
    }
    let mut right_min = h;
    for &v in s.iter().take(256).skip(k + 1) {
        if v > h {
            break;
        }
        right_min = right_min.min(v);
    }
    h - left_min.max(right_min)
}

/// The bias correction ρ of Formulae (77)–(81) for a quadratic-fit rate `f`, Hz.
fn bias(f: f64) -> f64 {
    let base = (f / DF).floor();
    let beta = |t: usize| (base + t as f64 / 32.0) * DF - (f + BIAS[t]);
    let theta_min = (0..=32)
        .min_by(|&a, &b| beta(a).abs().total_cmp(&beta(b).abs()))
        .unwrap_or(0);
    let theta = if theta_min > 0 && beta(theta_min) * beta(theta_min - 1) < 0.0 {
        theta_min
    } else {
        theta_min + 1
    };
    let (b0, b1) = (beta(theta - 1), beta(theta));
    if (b1 - b0).abs() < 1e-15 {
        return BIAS[theta - 1];
    }
    BIAS[theta - 1] - (BIAS[theta] - BIAS[theta - 1]) * b0 / (b1 - b0)
}

/// Shape-preserving piecewise cubic Hermite interpolation of (x, y) at `t` (Fritsch & Carlson's
/// monotone slopes), held flat outside the data.
pub(super) fn pchip(x: &[f64], y: &[f64], t: f64) -> f64 {
    let n = x.len();
    if n == 0 {
        return 0.0;
    }
    if n == 1 || t <= x[0] {
        return y[0];
    }
    if t >= x[n - 1] {
        return y[n - 1];
    }
    let h: Vec<f64> = (0..n - 1).map(|k| (x[k + 1] - x[k]).max(1e-12)).collect();
    let delta: Vec<f64> = (0..n - 1).map(|k| (y[k + 1] - y[k]) / h[k]).collect();
    let slope = |k: usize| -> f64 {
        if k == 0 || k == n - 1 {
            // The end interval and the one next to it.
            let (e, f) = if k == 0 {
                (0, 1.min(n - 2))
            } else {
                (n - 2, (n - 2).saturating_sub(1))
            };
            let (d0, d1, h0, h1) = (delta[e], delta[f], h[e], h[f]);
            let d = ((2.0 * h0 + h1) * d0 - h0 * d1) / (h0 + h1);
            if d.signum() != d0.signum() {
                0.0
            } else if d0.signum() != d1.signum() && d.abs() > 3.0 * d0.abs() {
                3.0 * d0
            } else {
                d
            }
        } else {
            let (d0, d1) = (delta[k - 1], delta[k]);
            if d0 * d1 <= 0.0 {
                0.0
            } else {
                let (w1, w2) = (2.0 * h[k] + h[k - 1], h[k] + 2.0 * h[k - 1]);
                (w1 + w2) / (w1 / d0 + w2 / d1)
            }
        }
    };
    let k = x.partition_point(|&v| v <= t).saturating_sub(1).min(n - 2);
    let (x0, x1, y0, y1) = (x[k], x[k + 1], y[k], y[k + 1]);
    let (m0, m1) = (slope(k), slope(k + 1));
    let hk = x1 - x0;
    let s = (t - x0) / hk;
    let (s2, s3) = (s * s, s * s * s);
    (2.0 * s3 - 3.0 * s2 + 1.0) * y0
        + (s3 - 2.0 * s2 + s) * hk * m0
        + (-2.0 * s3 + 3.0 * s2) * y1
        + (s3 - s2) * hk * m1
}
