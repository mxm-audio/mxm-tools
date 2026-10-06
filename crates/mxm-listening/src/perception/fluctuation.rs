//! Fluctuation strength after the Sottek hearing model, ECMA-418-2 Clause 9: each band's slow envelope
//! over 1.37 s blocks, read within an envelope-dependent window by a high-resolution spectral analysis
//! (HSA: constant and spectral line pairs at any modulation rate, fitted by least squares through the
//! window's own kernel), the prominent lines weighted by how strongly their rate fluctuates (the
//! maximum at 4.87 Hz), a harmonic complex of modulation found, scaled by the HSA-based loudness; then
//! the nonlinear transform that treats broadband and narrowband fluctuation differently, and a 0.75 s
//! smoother. Written from the standard's text.
//!
//! Two readings of the extracted text are made where it is not self-consistent and are stated where
//! used: the window kernel is the rectangular window's exact Dirichlet kernel (the extracted exponent
//! lost a factor ½), and the fine tuning's step bounds are read in normalised frequency (in hertz they
//! would move a rate by at most 2 mHz in forty steps).

use super::hearing::{self, BANDS, RATE};
use crate::numeric::linalg::{Matrix, least_squares};

/// Block and hop at 48 kHz (§9.1.1), and after the envelope's decimation by 32 (§9.1.2).
const SB: usize = 65_536;
const SH: usize = 16_384;
const DECIMATE: usize = 32;
const SB_E: usize = SB / DECIMATE;
/// The envelope's rate, Hz, and its DFT resolution.
const RATE_E: f64 = RATE / DECIMATE as f64;
const DF: f64 = RATE_E / SB_E as f64;
/// The output rate, Hz (§9.1.11).
pub const RATE_OUT: f64 = 50.0;
/// Values up to this index (about 683 ms) are the filters' transient (§9.1.12).
pub const SETTLE: usize = 36;
const C_F: f64 = 0.003_840_572;
const EPS: f64 = 1e-12;
/// Default zeros at each end of the analysis window (§9.1.3.1).
const EDGE: usize = SB_E / 32;
/// The threshold under which a whole block is quiet, Pa (§9.1.3.2).
const QUIET_PA: f64 = 5e-6;
/// Local maxima of the envelope power spectrum count when above this (§9.1.5).
const PHI_MIN: f64 = 0.15;
/// The weighting's maximum and its shape (§9.1.6, Formula (148)).
const F_MAX: f64 = 4.8659;
const Q1_L: f64 = 0.33048;
const Q2_L: f64 = 0.85902;
const Q1_H: f64 = 0.21792;
const Q2_H: f64 = 4.6728;
/// A(l, z) under this is no fluctuation (§9.1.10).
const A_MIN: f64 = 5.2519;

/// Time-dependent specific fluctuation strength F′(l₅₀, z), vacil per Bark_HMS, at [`RATE_OUT`].
#[derive(Clone, Debug, Default)]
pub struct Fluctuation {
    pub specific: Vec<[f64; BANDS]>,
}

impl Fluctuation {
    /// Time-dependent fluctuation strength F(l₅₀), vacil (Formula (169)).
    #[must_use]
    pub fn total(&self) -> Vec<f64> {
        self.specific
            .iter()
            .map(|b| b.iter().sum::<f64>() * hearing::DZ)
            .collect()
    }

    /// The single value: the 90th percentile after the settling values (§9.1.14).
    #[must_use]
    pub fn single(&self) -> Option<f64> {
        let mut f: Vec<f64> = self.total().into_iter().skip(SETTLE).collect();
        if f.is_empty() {
            return None;
        }
        f.sort_by(f64::total_cmp);
        let rank = 0.9 * (f.len() - 1) as f64;
        let (lo, u) = (rank.floor() as usize, rank - rank.floor());
        Some(f[lo] + u * (f[(lo + 1).min(f.len() - 1)] - f[lo]))
    }
}

/// Fluctuation strength of pressure `p` (Pa, 48 kHz, faded in); `None` for an empty signal.
#[must_use]
pub fn analyse(p: &[f64]) -> Option<Fluctuation> {
    let n_samples = p.len();
    if n_samples == 0 {
        return None;
    }
    let mut padded = vec![0.0; SB];
    padded.extend_from_slice(p);
    hearing::ear(&mut padded);
    let total = padded.len();
    let l_last = (n_samples + SB).div_ceil(SH);
    let mut starts: Vec<usize> = (0..l_last).map(|l| l * SH).collect();
    starts.push(total.saturating_sub(SB));
    let mut times: Vec<f64> = (0..l_last).map(|l| (l * SH) as f64 / RATE).collect();
    times.push(n_samples as f64 / RATE);
    let blocks = starts.len();

    // Per block and band: the harmonic complex's weighted sum Â, its power and its loudness.
    let mut complex = vec![[None::<Complex>; BANDS]; blocks];
    #[allow(clippy::needless_range_loop)]
    // z is the band index the filter bank and the output share
    for z in 0..BANDS {
        let x = hearing::band(&padded, z);
        let carrier = hearing::centre_hz(hearing::z(z));
        for (l, &start) in starts.iter().enumerate() {
            let block: Vec<f64> = (0..SB)
                .map(|n| x.get(start + n).copied().unwrap_or(0.0))
                .collect();
            let Some((re, im)) = crate::repr::fir::analytic(&block) else {
                continue;
            };
            let envelope: Vec<f64> = (0..SB_E)
                .map(|n| re[n * DECIMATE].hypot(im[n * DECIMATE]))
                .collect();
            complex[l][z] = band_complex(&envelope, carrier);
        }
    }
    // The HSA-based loudness and the scaling of Formula (159), block by block.
    let mut amplitude = vec![[0.0; BANDS]; blocks];
    for l in 0..blocks {
        let loud: Vec<f64> = (0..BANDS)
            .map(|z| {
                complex[l][z]
                    .as_ref()
                    .map_or(0.0, |c| hearing::basis_loudness(z, (0.5 * c.power).sqrt()))
            })
            .collect();
        let n_max = loud.iter().copied().fold(0.0f64, f64::max);
        for z in 0..BANDS {
            if let Some(c) = &complex[l][z] {
                let a =
                    loud[z] * loud[z] / (n_max + EPS) / (c.power + EPS) * SB_E as f64 * c.weighted;
                amplitude[l][z] = if a < A_MIN { 0.0 } else { a };
            }
        }
    }

    // Onto 50 Hz (§9.1.11), then the nonlinear transform and the smoother.
    let last = (times[blocks - 1] * RATE_OUT).floor() as usize;
    let mut est = vec![[0.0; BANDS]; last + 1];
    for z in 0..BANDS {
        let y: Vec<f64> = (0..blocks).map(|l| amplitude[l][z]).collect();
        for (l50, row) in est.iter_mut().enumerate() {
            row[z] = super::roughness::pchip(&times, &y, l50 as f64 / RATE_OUT).max(0.0);
        }
    }
    let b_raw: Vec<f64> = est
        .iter()
        .map(|row| {
            let quadratic = (row.iter().map(|v| v * v).sum::<f64>() / BANDS as f64).sqrt();
            let linear = row.iter().sum::<f64>() / BANDS as f64;
            quadratic / (linear + EPS)
        })
        .collect();
    let b = moving_median(&b_raw, 71);
    let a_tau = (-1.0 / (RATE_OUT * 0.75)).exp();
    let mut specific = vec![[0.0; BANDS]; last + 1];
    for l50 in 0..=last {
        let e = 0.37106 * ((1.6407 * (b[l50] - 2.5804)).tanh() + 1.0) * 0.5 + 0.58449;
        for z in 0..BANDS {
            let target = C_F * est[l50][z].powf(e);
            specific[l50][z] = if l50 == 0 {
                target
            } else {
                target * (1.0 - a_tau) + specific[l50 - 1][z] * a_tau
            };
        }
    }
    Some(Fluctuation { specific })
}

/// A band's harmonic complex of modulation in one block: Â (Formula (157)) and the power
/// p̂₀² + 2·ΣAᵢ it is scaled by (Formula (159)).
#[derive(Clone, Copy, Debug)]
struct Complex {
    weighted: f64,
    power: f64,
}

/// The moving median of length `n`, truncated at the ends (§9.1.3.2, footnote 37).
fn moving_median(x: &[f64], n: usize) -> Vec<f64> {
    let half = n / 2;
    (0..x.len())
        .map(|i| {
            let (a, b) = (i.saturating_sub(half), (i + half + 1).min(x.len()));
            let mut w: Vec<f64> = x[a..b].to_vec();
            w.sort_by(f64::total_cmp);
            let m = w.len();
            if m % 2 == 1 {
                w[m / 2]
            } else {
                0.5 * (w[m / 2 - 1] + w[m / 2])
            }
        })
        .collect()
}

fn round8(v: f64) -> f64 {
    (v * 1e8).round() / 1e8
}

/// The analysis window of §9.1.3 as its first and last sample `(n₁, n₂)`; `None` for a quiet block.
fn analysis_window(envelope: &[f64]) -> Option<(usize, usize)> {
    let (mut nzb, mut nze) = (EDGE, EDGE);
    let smooth: Vec<f64> = moving_median(envelope, SB_E / 64 + 1)
        .into_iter()
        .enumerate()
        .map(|(n, v)| {
            if n >= nzb && n < SB_E - nze {
                round8(v)
            } else {
                0.0
            }
        })
        .collect();
    let top = smooth.iter().copied().fold(0.0f64, f64::max);
    if top <= QUIET_PA {
        return None;
    }
    let threshold = round8(0.01 * top);
    let first = smooth.iter().position(|&v| v >= threshold)?;
    let last = smooth.iter().rposition(|&v| v >= threshold)?;
    nzb = first;
    nze = SB_E - 1 - last;
    // The longest quieter period inside, longer than 5/32 of a block.
    let (mut best, mut run_start) = (None::<(usize, usize)>, None::<usize>);
    for (n, &v) in smooth.iter().enumerate().take(SB_E - nze).skip(nzb) {
        if v < threshold {
            run_start.get_or_insert(n);
        } else if let Some(s) = run_start.take() {
            let len = n - s;
            if len > SB_E * 5 / 32 && best.is_none_or(|b| len > b.1 - b.0 + 1) {
                best = Some((s, n - 1));
            }
        }
    }
    if let Some((qb, qe)) = best {
        if qb as f64 - (nzb + EDGE) as f64 > (SB_E - 1 - nze - EDGE) as f64 - qe as f64 {
            nze = SB_E - 1 - qb + EDGE;
        } else {
            nzb = qe + EDGE;
        }
    }
    let (n1, n2) = (nzb, (SB_E - 1).checked_sub(nze)?);
    if n2 < n1 || n2 - n1 + 1 < SB_E * 5 / 32 || n2 < SB_E / 4 - 1 {
        return None;
    }
    // Enough variation to read: the relative standard deviation about a line, at least 0.1 %.
    let (a, b) = (n1 + 10, n2.saturating_sub(10));
    if b <= a + 2 {
        return None;
    }
    let pts: Vec<(f64, f64)> = (a..=b).map(|n| (n as f64, envelope[n])).collect();
    let m = pts.len() as f64;
    let (mx, my) = (
        pts.iter().map(|p| p.0).sum::<f64>() / m,
        pts.iter().map(|p| p.1).sum::<f64>() / m,
    );
    let sxx: f64 = pts.iter().map(|p| (p.0 - mx).powi(2)).sum();
    let slope = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f64>() / sxx;
    let sd = (pts
        .iter()
        .map(|p| (p.1 - my - slope * (p.0 - mx)).powi(2))
        .sum::<f64>()
        / m)
        .sqrt();
    (my > 0.0 && sd / my >= 1e-3).then_some((n1, n2))
}

/// The DFT over bins 0 … `bins` − 1 of a rectangular window from `n1` to `n2` holding
/// `e^(i·2π·f·n / RATE_E)`: its exact Dirichlet kernel, `e^(−iπ·φ·(n₁+n₂))·sin(π·φ·L)/sin(π·φ)` with
/// φ = k/SB_E − f/RATE_E and L = n₂ − n₁ + 1.
fn kernel(f: f64, n1: usize, n2: usize, bins: usize) -> Vec<(f64, f64)> {
    let length = (n2 - n1 + 1) as f64;
    let centre = (n1 + n2) as f64;
    (0..bins)
        .map(|k| {
            let phi = k as f64 / SB_E as f64 - f / RATE_E + f64::EPSILON;
            let den = (std::f64::consts::PI * phi).sin();
            let mag = if den.abs() < 1e-15 {
                length
            } else {
                (std::f64::consts::PI * phi * length).sin() / den
            };
            let arg = -std::f64::consts::PI * phi * centre;
            (mag * arg.cos(), mag * arg.sin())
        })
        .collect()
}

/// The HSA's result: the constant, each rate's positive-frequency line (re, im), and the fit's
/// squared error.
type Hsa = (f64, Vec<(f64, f64)>, f64);

/// The HSA of Formulae (123)–(135): the constant `p̂₀` and each rate's positive-frequency line `cᵢ`
/// (half its peak swing), the lines fitted together through the window's kernel over the first `K_L`
/// bins of `spectrum`, and the fit's squared error. `None` when the rates cannot be told apart.
fn hsa(spectrum: &[(f64, f64)], rates: &[f64], n1: usize, n2: usize) -> Option<Hsa> {
    let top = rates.iter().copied().fold(0.0, f64::max);
    let bins = ((top / DF).round() as usize + 8).clamp(17, 49);
    // Columns: the constant, then per rate its real part (W(f) + W(−f)) and its imaginary part
    // (i·(W(f) − W(−f))), each as the stacked real and imaginary parts over the bins.
    let w0 = kernel(0.0, n1, n2, bins);
    let mut columns: Vec<Vec<f64>> = vec![
        w0.iter()
            .map(|v| v.0)
            .chain(w0.iter().map(|v| v.1))
            .collect(),
    ];
    for &f in rates {
        let (plus, minus) = (kernel(f, n1, n2, bins), kernel(-f, n1, n2, bins));
        let re: Vec<(f64, f64)> = plus
            .iter()
            .zip(&minus)
            .map(|(a, b)| (a.0 + b.0, a.1 + b.1))
            .collect();
        // i·(a − b) = (−(a.im − b.im), a.re − b.re).
        let im: Vec<(f64, f64)> = plus
            .iter()
            .zip(&minus)
            .map(|(a, b)| (-(a.1 - b.1), a.0 - b.0))
            .collect();
        columns.push(
            re.iter()
                .map(|v| v.0)
                .chain(re.iter().map(|v| v.1))
                .collect(),
        );
        columns.push(
            im.iter()
                .map(|v| v.0)
                .chain(im.iter().map(|v| v.1))
                .collect(),
        );
    }
    let target: Vec<f64> = spectrum[..bins]
        .iter()
        .map(|v| v.0)
        .chain(spectrum[..bins].iter().map(|v| v.1))
        .collect();
    let rows: Vec<Vec<f64>> = (0..2 * bins)
        .map(|r| columns.iter().map(|c| c[r]).collect())
        .collect();
    let a = Matrix::from_rows(&rows)?;
    let x = least_squares(&a, &target)?;
    let error: f64 = (0..2 * bins)
        .map(|r| {
            let fit: f64 = columns.iter().zip(&x).map(|(c, v)| c[r] * v).sum();
            (fit - target[r]).powi(2)
        })
        .sum();
    let lines = (0..rates.len())
        .map(|i| (x[1 + 2 * i], x[2 + 2 * i]))
        .collect();
    Some((x[0], lines, error))
}

/// The weighting of Formula (148) at rate `f` for a band centred on `carrier` Hz.
fn weight(f: f64, carrier: f64) -> f64 {
    if f <= 0.0 {
        return 0.0;
    }
    let r = f / F_MAX - F_MAX / f;
    if f <= F_MAX {
        1.0 / (1.0 + (r * Q1_L).powi(2)).powf(Q2_L)
    } else {
        let c = 1.0 + 0.092623 * (carrier / 1000.0).log2().abs().powf(1.24);
        1.0 / c / (1.0 + (r * Q1_H).powi(2)).powf(Q2_H)
    }
}

/// A band's harmonic complex in one block (§9.1.3–§9.1.9), from its downsampled envelope; `None` when
/// the block is quiet or holds no modulation.
fn band_complex(envelope: &[f64], carrier: f64) -> Option<Complex> {
    let (n1, n2) = analysis_window(envelope)?;
    let mut re: Vec<f64> = (0..SB_E)
        .map(|n| if n >= n1 && n <= n2 { envelope[n] } else { 0.0 })
        .collect();
    let mut im = vec![0.0; SB_E];
    mxm_measure::spectrum::fft(&mut re, &mut im)?;
    let spectrum: Vec<(f64, f64)> = (0..49).map(|k| (re[k], im[k])).collect();
    let phi: Vec<f64> = spectrum.iter().map(|v| v.0 * v.0 + v.1 * v.1).collect();
    // §9.1.5: local maxima of the power spectrum, their rates by the centroid of three bins.
    let floor = (0.001 * phi[0]).max(PHI_MIN);
    let maxima: Vec<f64> = (1..48)
        .filter(|&k| phi[k] > phi[k - 1] && phi[k] >= phi[k + 1] && phi[k] >= floor)
        .take(24)
        .map(|k| {
            let s: f64 = (k - 1..=k + 1).map(|j| phi[j]).sum();
            let c: f64 = (k - 1..=k + 1).map(|j| j as f64 * phi[j]).sum();
            c / s * DF
        })
        .collect();
    // …and the single pair's error at rates 0.25·2^((i−2)/3) Hz, its least local minimum.
    let grid: Vec<f64> = (1..=16)
        .map(|i| 0.25 * 2f64.powf((f64::from(i) - 2.0) / 3.0))
        .collect();
    let errors: Vec<Option<f64>> = grid
        .iter()
        .map(|&f| hsa(&spectrum, &[f], n1, n2).map(|h| h.2))
        .collect();
    let f_min = (1..grid.len() - 1)
        .filter_map(|i| {
            let (a, b, c) = (errors[i - 1]?, errors[i]?, errors[i + 1]?);
            (b < a && b < c).then_some((grid[i], b))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|m| m.0);
    if maxima.is_empty() && f_min.is_none() {
        return None;
    }
    let mut candidates: Vec<Vec<f64>> = Vec::new();
    match f_min {
        None => candidates.push(maxima.clone()),
        Some(m) => {
            let duplicates: Vec<usize> = (0..maxima.len())
                .filter(|&i| (m - maxima[i]).abs() < 1.25 * DF)
                .collect();
            let mut with: Vec<f64> = maxima
                .iter()
                .enumerate()
                .filter(|(i, _)| !duplicates.contains(i))
                .map(|(_, &f)| f)
                .collect();
            with.push(m);
            candidates.push(with);
            if !duplicates.is_empty() {
                candidates.push(maxima.clone());
            }
        }
    }
    let (mut rates, p0, lines) = candidates
        .into_iter()
        .filter(|c| !c.is_empty())
        .filter_map(|mut c| {
            c.sort_by(f64::total_cmp);
            let (p0, lines, error) = hsa(&spectrum, &c, n1, n2)?;
            Some((c, p0, lines, error))
        })
        .min_by(|a, b| a.3.total_cmp(&b.3))
        .map(|(c, p0, lines, _)| (c, p0, lines))?;
    let _ = p0;
    // Keep lines above 5 % of the largest (Formula (146)).
    let powers: Vec<f64> = lines.iter().map(|l| l.0 * l.0 + l.1 * l.1).collect();
    let top = powers.iter().copied().fold(0.0, f64::max);
    let kept: Vec<usize> = (0..rates.len())
        .filter(|&i| powers[i] > 0.05 * top)
        .collect();
    rates = kept.iter().map(|&i| rates[i]).collect();
    let powers: Vec<f64> = kept.iter().map(|&i| powers[i]).collect();
    let weighted: Vec<f64> = rates
        .iter()
        .zip(&powers)
        .map(|(&f, &a)| a * weight(f, carrier))
        .collect();
    let i_max = (0..rates.len()).max_by(|&a, &b| weighted[a].total_cmp(&weighted[b]))?;
    // §9.1.7: the maximum's rate fine-tuned on the single pair's error, in normalised frequency.
    let error_at = |x: f64| hsa(&spectrum, &[x * RATE_E], n1, n2).map(|h| h.2);
    let start = rates[i_max] / RATE_E;
    let mut x = start;
    for _ in 0..40 {
        let d = 1e-5;
        let (Some(e0), Some(ep), Some(em)) = (error_at(x), error_at(x + d), error_at(x - d)) else {
            break;
        };
        let first = (ep - em) / (2.0 * d);
        let second = (ep - 2.0 * e0 + em) / (d * d);
        let step = 0.25 * first.signum() * (first.abs() / (second.abs() + f64::EPSILON)).min(2e-4);
        x -= step;
        if step.abs() <= 1e-7 {
            break;
        }
    }
    let f_opt = if ((x - start) * RATE_E).abs() > 1.25 * DF {
        rates[i_max]
    } else {
        x * RATE_E
    };
    if f_opt < 0.125 {
        return None;
    }
    rates[i_max] = f_opt;
    // §9.1.8: the harmonic complex, the maximum its first, second or third order.
    let (_, members, f1) = (1..=3)
        .map(|o| {
            let f1 = f_opt / f64::from(o);
            let members: Vec<(usize, usize)> = rates
                .iter()
                .enumerate()
                .filter_map(|(i, &f)| {
                    let r = (f / f1).round() as usize;
                    ((1..=5).contains(&r) && (f / (r as f64 * f1) - 1.0).abs() < 0.04)
                        .then_some((i, r))
                })
                .collect();
            let energy: f64 = members.iter().map(|&(i, _)| weighted[i]).sum();
            (energy, members, f1)
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))?;
    if members.is_empty() {
        return None;
    }
    // Each member refitted alone at its order's rate, the constant the mean of the fits.
    let mut constants = Vec::new();
    let mut parts: Vec<(f64, f64)> = Vec::new();
    for &(_, order) in &members {
        let f = order as f64 * f1;
        if let Some((c, lines, _)) = hsa(&spectrum, &[f], n1, n2) {
            constants.push(c);
            let a = lines[0].0 * lines[0].0 + lines[0].1 * lines[0].1;
            parts.push((f, a));
        }
    }
    if parts.is_empty() {
        return None;
    }
    let p0 = constants.iter().sum::<f64>() / constants.len() as f64;
    let weighted: Vec<(f64, f64)> = parts
        .iter()
        .map(|&(f, a)| (f, a * weight(f, carrier)))
        .collect();
    // §9.1.9: weighted by how far the complex's centre of gravity sits from the tuned maximum.
    let sum: f64 = weighted.iter().map(|w| w.1).sum();
    let centroid = weighted.iter().map(|w| w.0 * w.1).sum::<f64>() / (sum + f64::EPSILON);
    let w_bw = 1.0 + 0.79577 * (centroid - f_opt).abs().powf(0.43461);
    let power = p0 * p0 + 2.0 * parts.iter().map(|p| p.1).sum::<f64>();
    Some(Complex {
        weighted: w_bw * sum,
        power,
    })
}
