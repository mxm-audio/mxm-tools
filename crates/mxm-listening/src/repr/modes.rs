//! High-resolution modal analysis: a sound's modes — frequency, decay, amplitude, phase — from a
//! window of it, resolving modes closer together than any spectrogram window can.
//!
//! **Technique** (`research:listening/modal-estimation.md`, written from Badeau, David & Richard 2006,
//! Ege, Boutillon & David 2009 and Sirdey et al. 2011; no implementation opened):
//! 1. **Subband chain** (§7): an anticausal Blackman-windowed FIR low-pass and decimation to about
//!    three times the band's top, then an anticausal FIR band-pass. Both keep a sum of damped
//!    sinusoids exactly such a sum (`repr::fir`). The band stays in the first Nyquist zone, so the
//!    signal stays real and each mode is a conjugate pole pair — which keeps the whole analysis in
//!    real arithmetic, with this crate's real SVD and eigenvalue solver.
//! 2. **ESPRIT** (§3): the Hankel matrix `n × l` with `n ≈ N/3` (the efficient shape, §2); its first `p`
//!    left singular vectors `W`; `Φ = W↓⁺ W↑` by least squares; the poles are `Φ`'s eigenvalues.
//! 3. **Order by ESTER** (§6): `J(p) = 1/‖W↑ − W↓Φ(p)‖₂²` over even `p`; the order is the greatest `p`
//!    with `J(p)` above [`ESTER_THRESHOLD`] — deliberately generous, since an over-estimated order
//!    keeps the true poles among its eigenvalues and the rest are pruned.
//! 4. **Pruning** (§6): growing poles, poles outside the band and poles without a positive frequency.
//! 5. **Amplitudes and phases** (§5) by real least squares on `r^i·cos(θi)` and `r^i·sin(θi)`, then
//!    corrected by the two filters' complex gains **at each damped pole**.
//!
//! What it cannot do (§9): model the contact (start windows after it), follow a glide within a window
//! (a gliding mode leaves a phantom — use short windows early), or separate a dense metal field.

use crate::numeric::{Complex, Matrix, eigenvalues, least_squares, svd};
use crate::repr::fir;

/// One damped sinusoid: `A·e^{−decay·t}·cos(2π·hz·t + phase)` from the window's start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mode {
    pub hz: f64,
    /// The amplitude decay rate, 1/s.
    pub decay: f64,
    /// Amplitude at the window's start, in the file's units.
    pub amplitude: f64,
    pub phase: f64,
}

impl Mode {
    /// Time to fall 60 dB, s.
    #[must_use]
    pub fn t60(&self) -> f64 {
        (1000.0f64).ln() / self.decay
    }

    /// The decay in dB/s (negative).
    #[must_use]
    pub fn db_per_s(&self) -> f64 {
        -20.0 / std::f64::consts::LN_10 * self.decay
    }
}

/// The modes of one band and window, and how they were found.
#[derive(Clone, Debug, PartialEq)]
pub struct BandModes {
    pub modes: Vec<Mode>,
    /// The model order ESTER chose (twice the number of real modes it allowed).
    pub order: usize,
    /// The analysed rate after decimation, and the number of samples analysed.
    pub rate: f64,
    pub samples: usize,
}

/// ESTER's acceptance level: an order whose rotational-invariance error `‖E‖₂` is below 0.1. The
/// research page's check A found a relative rule choosing too low an order at 40 dB and this fixed
/// level choosing a little high; high is what pruning handles (**chosen**).
pub const ESTER_THRESHOLD: f64 = 100.0;
/// The most real modes one band may hold: about 25 complex components is the practical limit of a
/// subspace method (§2), so 12 conjugate pairs.
pub const MAX_MODES: usize = 12;
/// The most samples one analysis takes after decimation: the SVD grows as the cube.
pub const MAX_SAMPLES: usize = 600;
/// The fewest samples one analysis should have after decimation (**chosen**).
pub const MIN_SAMPLES: usize = 120;
/// The most of its window each filter stage may span (**chosen**: the analysis then lags the window
/// by a quarter of it at most. Filters confined inside the window instead, at 0.15 each, left the
/// middle window's bands so wide that a strong gliding mode below them collapsed ESTER's order and hid
/// the ring; at 0.25 inside, the research page's check A lost its decay at 60 dB).
pub const FILTER_SHARE: f64 = 0.25;
/// A fitted mode holding more than this share of its band's energy over the window is part of a
/// cancelling structure and is pruned (**chosen**: a real mode holds at most all of it; beating
/// neighbours shift the share a little either way).
pub const CANCELLING_ENERGY: f64 = 1.5;
/// A corrected mode holding more than this share of the file's own energy in its band and window is
/// not a mode of the file (**chosen**, as [`CANCELLING_ENERGY`]; the band filter's skirts add a little).
pub const FILE_ENERGY: f64 = 2.0;

/// A held note's pole may grow this fast and still be read as steady, 1/s (8.7 dB/s): a partial that
/// neither decays nor grows sits on the unit circle, and rounding puts it just outside as often as
/// inside. Rejecting every growing pole dropped a synthetic 220 Hz tone's fundamental, second and
/// fourth harmonics in turn from window to window, and its fundamental read 660 Hz.
pub const STEADY_GROWTH: f64 = 1.0;
/// A steady pole's decay is held at this, 1/s: a T60 of 1000 s rather than an infinite one.
pub const STEADY_DECAY: f64 = 6.907_755_278_982_137 / 1000.0;

/// Analyses `len` samples of `x` from `start` in the band `lo_hz..hi_hz`. `None` where the band does
/// not fit below the Nyquist frequency, too few samples remain, or no order passes ESTER (noise).
/// Only decaying poles are modes: a percussive hit's analysis.
#[must_use]
pub fn analyse_band(
    x: &[f64],
    rate: f64,
    start: usize,
    len: usize,
    lo_hz: f64,
    hi_hz: f64,
) -> Option<BandModes> {
    analyse_band_with(x, rate, start, len, lo_hz, hi_hz, false)
}

/// [`analyse_band`], and where `steady`, a pole growing slower than [`STEADY_GROWTH`] is a steady
/// mode ([`STEADY_DECAY`]): a held note's analysis.
#[must_use]
pub fn analyse_band_with(
    x: &[f64],
    rate: f64,
    start: usize,
    len: usize,
    lo_hz: f64,
    hi_hz: f64,
    steady: bool,
) -> Option<BandModes> {
    if !(lo_hz > 0.0 && lo_hz < hi_hz && hi_hz < 0.4 * rate) {
        return None;
    }
    // The filters run forward from each sample, so the analysed signal lags the window by half their
    // span: exact for a mode that decays exponentially throughout, wrong for a drum's first hundred
    // milliseconds. Uncapped, a low band's filters spanned 200–500 ms, and "10–60 ms" at 141–283 Hz
    // read about 110–160 ms: the resynthesis self-test found the strongest early mode (185 Hz, −3 dB)
    // missing. So each stage may span at most [`FILTER_SHARE`] of the window: the analysed samples
    // cover the window and the filters reach past its end by at most half its length. A capped filter
    // has a wider transition and its band admits more of its neighbours; the keep range and the
    // correction at the damped pole still hold.
    let budget = ((len as f64 * FILTER_SHARE) as usize).max(3);
    // Stage 1: low-pass above `hi` with a 0.5·hi transition where the window allows, and decimate to
    // ≥ 3·hi — but keep [`MIN_SAMPLES`] in the window, since resolution comes from the window's
    // length and a fit on a few dozen samples is unstable (a 50 ms window at a low band's rate left 34,
    // and a spurious +37 dB mode) — and keep the aliases of the transition above the band.
    let taps1 = ((5.5 * rate / (0.5 * hi_hz)).ceil() as usize).min(budget) | 1;
    let transition1 = 5.5 * rate / taps1 as f64;
    let cutoff1 = (1.2 * hi_hz)
        .max(hi_hz + 0.5 * transition1)
        .min(0.45 * rate);
    let d1 = ((rate / (3.0 * hi_hz)).floor() as usize)
        .min(len / MIN_SAMPLES)
        .min((rate / (hi_hz + cutoff1 + 0.5 * transition1)).floor() as usize)
        .max(1);
    let r1 = rate / d1 as f64;
    let h1 = if d1 > 1 {
        fir::band_pass(0.0, cutoff1, rate, taps1)?
    } else {
        vec![1.0]
    };
    // Stage 2: band-pass with a transition a quarter of the lower edge (at most a quarter octave) where
    // the window allows.
    let taps2 = ((5.5 * r1 / (0.25 * lo_hz)).ceil() as usize).min((budget / d1).max(3)) | 1;
    let h2 = fir::band_pass(0.75 * lo_hz, (1.2 * hi_hz).min(0.45 * r1), r1, taps2)?;
    let wanted = (len / d1).min(MAX_SAMPLES);
    let y1 = fir::anticausal_decimated(x, &h1, start, d1, wanted + h2.len());
    let u = fir::anticausal_decimated(&y1, &h2, 0, 1, wanted);
    let n_samples = u.len();
    if n_samples < 24 {
        return None;
    }
    // No mode can be louder than twice the sound itself over the analysed stretch.
    let local_peak = x[start..(start + len).min(x.len())]
        .iter()
        .fold(0.0f64, |m, v| m.max(v.abs()));
    // Nor hold more energy over the window than the file itself has in the band there, measured
    // directly on the file. The filters' gain at a decaying pole corrects a true mode exactly, but a
    // low band's filters are long (about 440 ms at 50–100 Hz), and a "mode" fitted to rumble came back
    // corrected 37 dB above the band's real content — the resynthesis self-test found it.
    let file_band_energy = {
        let pad = (0.05 * rate) as usize;
        let a = start.saturating_sub(pad);
        let b = (start + len + pad).min(x.len());
        crate::repr::bands::band(&x[a..b], rate, lo_hz, hi_hz).map(|y| {
            y[start - a..(start + len).min(x.len()) - a]
                .iter()
                .map(|v| v * v)
                .sum::<f64>()
        })
    };
    let found = esprit(&u, 2 * MAX_MODES)?;
    // Pole → (hz, decay); amplitudes; correction by both filters at the damped pole.
    let mut poles: Vec<(f64, f64)> = found
        .poles
        .iter()
        .filter(|z| z.im > 1e-12)
        .map(|z| (z.arg() * r1 / std::f64::consts::TAU, -r1 * z.abs().ln()))
        .filter(|&(hz, decay)| {
            (decay > 0.0 || (steady && decay > -STEADY_GROWTH)) && hz >= lo_hz && hz <= hi_hz
        })
        .map(|(hz, decay)| {
            (
                hz,
                if steady {
                    decay.max(STEADY_DECAY)
                } else {
                    decay
                },
            )
        })
        .collect();
    poles.sort_by(|a, b| a.0.total_cmp(&b.0));
    // A mode holding more energy over the window than the whole band does is half of a spurious
    // structure whose parts cancel — what an over-estimated order leaves when poles land close
    // together. Remove the worst and refit until none is left (§6's pruning, applied to energy). A
    // first rule compared amplitudes with the band's peak and let a low mode through 20 dB above the
    // band's real content: the resynthesis self-test found it.
    let band_energy: f64 = u.iter().map(|v| v * v).sum();
    let fitted = loop {
        let fitted = amplitudes(&u, r1, &poles)?;
        let energy = |k: usize, amp: f64| -> f64 {
            let r2 = (-2.0 * poles[k].1 / r1).exp();
            let sum = if r2 < 1.0 {
                (1.0 - r2.powi(u.len() as i32)) / (1.0 - r2)
            } else {
                u.len() as f64
            };
            0.5 * amp * amp * sum
        };
        let worst = fitted
            .iter()
            .enumerate()
            .map(|(k, (amp, _))| (k, energy(k, *amp)))
            .filter(|&(_, e)| e > CANCELLING_ENERGY * band_energy)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i);
        match worst {
            Some(i) => {
                poles.remove(i);
            }
            None => break fitted,
        }
    };
    let mut modes = Vec::with_capacity(poles.len());
    for (&(hz, decay), (amp, phase)) in poles.iter().zip(fitted) {
        let s = Complex::new(-decay, std::f64::consts::TAU * hz);
        let g = fir::response(&h1, exp(s.scale(1.0 / rate)))
            * fir::response(&h2, exp(s.scale(1.0 / r1)));
        let gain = g.abs();
        if gain.is_nan() || gain <= 1e-6 || amp / gain > 2.0 * local_peak {
            continue;
        }
        if let Some(e_band) = file_band_energy {
            let a0 = amp / gain;
            let r2 = (-2.0 * decay / rate).exp();
            let count = len.min(x.len() - start) as i32;
            let sum = if r2 < 1.0 {
                (1.0 - r2.powi(count)) / (1.0 - r2)
            } else {
                f64::from(count)
            };
            if 0.5 * a0 * a0 * sum > FILE_ENERGY * e_band {
                continue;
            }
        }
        modes.push(Mode {
            hz,
            decay,
            amplitude: amp / gain,
            phase: wrap(phase - g.arg()),
        });
    }
    Some(BandModes {
        modes,
        order: found.order,
        rate: r1,
        samples: n_samples,
    })
}

fn exp(z: Complex) -> Complex {
    Complex::polar(z.re.exp(), z.im)
}

fn wrap(phase: f64) -> f64 {
    let t = std::f64::consts::TAU;
    let p = phase.rem_euclid(t);
    if p > std::f64::consts::PI { p - t } else { p }
}

/// The poles ESPRIT found, and the order ESTER chose.
struct Found {
    poles: Vec<Complex>,
    order: usize,
}

/// ESPRIT with ESTER's order over even orders up to `max_order`.
fn esprit(u: &[f64], max_order: usize) -> Option<Found> {
    let n_total = u.len();
    let n = (n_total + 1) / 3;
    let l = n_total + 1 - n;
    if n < 4 || l < n {
        return None;
    }
    // Xᵀ, l × n: row t holds u[t..t+n]. Its right singular vectors are X's left ones.
    let mut xt = Matrix::zeros(l, n);
    for t in 0..l {
        for j in 0..n {
            xt.set(t, j, u[t + j]);
        }
    }
    let decomposition = svd(&xt)?;
    if decomposition.s[0].is_nan() || decomposition.s[0] <= 0.0 {
        return None;
    }
    let max_order = max_order.min(n - 2) & !1;
    let mut best: Option<(usize, Matrix)> = None;
    let mut p = 2;
    while p <= max_order {
        if decomposition.s[p - 1] <= 1e-12 * decomposition.s[0] {
            break;
        }
        let (phi, err) = invariance(&decomposition.v, n, p)?;
        let j = if err > 0.0 {
            1.0 / (err * err)
        } else {
            f64::INFINITY
        };
        if j >= ESTER_THRESHOLD {
            best = Some((p, phi));
        }
        p += 2;
    }
    let (order, phi) = best?;
    Some(Found {
        poles: eigenvalues(&phi)?,
        order,
    })
}

/// `Φ = W↓⁺ W↑` for the first `p` columns of `v` (`n × n`), and `‖W↑ − W↓Φ‖₂`.
fn invariance(v: &Matrix, n: usize, p: usize) -> Option<(Matrix, f64)> {
    let mut down = Matrix::zeros(n - 1, p);
    let mut up = Matrix::zeros(n - 1, p);
    for i in 0..n - 1 {
        for j in 0..p {
            down.set(i, j, v.get(i, j));
            up.set(i, j, v.get(i + 1, j));
        }
    }
    let mut phi = Matrix::zeros(p, p);
    for j in 0..p {
        let column: Vec<f64> = (0..n - 1).map(|i| up.get(i, j)).collect();
        let solution = least_squares(&down, &column)?;
        for (i, s) in solution.into_iter().enumerate() {
            phi.set(i, j, s);
        }
    }
    let fitted = down.mul(&phi)?;
    let mut e = up.clone();
    for (a, b) in e.data.iter_mut().zip(&fitted.data) {
        *a -= b;
    }
    let norm = svd(&e).map_or(0.0, |d| d.s[0]);
    Some((phi, norm))
}

/// Refits the amplitude and phase of every mode of one window jointly, on the window's own samples:
/// ESPRIT's frequencies and decays kept, and the amplitudes the least-squares fit of all the modes at
/// once to `x` from `start` for `len` samples (band-limited zero-phase to twice the highest mode, then
/// decimated). A band's own fit reads its filtered, lagging signal and carries the amplitude back to
/// the window's start along the fitted decay; on a drum's first fifty milliseconds that overshot —
/// the snare's early 381 Hz mode came out 6.5 dB above everything the file had in its band, and the
/// resynthesis self-test heard it. Modes whose refit is a cancelling structure are dropped as in
/// [`analyse_band`]. The amplitudes and phases are at the window's start.
pub fn refit(x: &[f64], rate: f64, start: usize, len: usize, modes: &mut Vec<Mode>) {
    if modes.is_empty() || start >= x.len() {
        return;
    }
    let len = len.min(x.len() - start);
    let top = modes.iter().fold(0.0f64, |m, v| m.max(v.hz));
    let hi = (2.0 * top).min(0.45 * rate);
    let lo = REFIT_LOW_HZ.min(0.5 * modes.iter().fold(f64::INFINITY, |m, v| m.min(v.hz)));
    let pad = (0.05 * rate) as usize;
    let a = start.saturating_sub(pad);
    let b = (start + len + pad).min(x.len());
    let Some(y) = crate::repr::bands::band(&x[a..b], rate, lo, hi) else {
        return;
    };
    let d = ((rate / (2.5 * hi)).floor() as usize).max(1);
    let u: Vec<f64> = y[start - a..start - a + len]
        .iter()
        .step_by(d)
        .copied()
        .collect();
    let r = rate / d as f64;
    let window_energy: f64 = u.iter().map(|v| v * v).sum();
    loop {
        let poles: Vec<(f64, f64)> = modes.iter().map(|m| (m.hz, m.decay)).collect();
        let Some(fitted) = amplitudes(&u, r, &poles) else {
            return;
        };
        let energy = |m: &Mode, amp: f64| -> f64 {
            let r2 = (-2.0 * m.decay / r).exp();
            let sum = if r2 < 1.0 {
                (1.0 - r2.powi(u.len() as i32)) / (1.0 - r2)
            } else {
                u.len() as f64
            };
            0.5 * amp * amp * sum
        };
        let worst = modes
            .iter()
            .zip(&fitted)
            .enumerate()
            .map(|(k, (m, (amp, _)))| (k, energy(m, *amp)))
            .filter(|&(_, e)| e > CANCELLING_ENERGY * window_energy)
            .max_by(|p, q| p.1.total_cmp(&q.1))
            .map(|(k, _)| k);
        if let Some(k) = worst {
            modes.remove(k);
            continue;
        }
        for (m, (amp, phase)) in modes.iter_mut().zip(fitted) {
            m.amplitude = amp;
            m.phase = phase;
        }
        return;
    }
}

/// The joint refit's lowest band edge, Hz: below every drum mode the listener reads.
pub const REFIT_LOW_HZ: f64 = 20.0;

/// Real least-squares amplitude and phase of each pole `(hz, decay)` in `u` sampled at `rate`.
fn amplitudes(u: &[f64], rate: f64, poles: &[(f64, f64)]) -> Option<Vec<(f64, f64)>> {
    if poles.is_empty() {
        return Some(Vec::new());
    }
    let mut a = Matrix::zeros(u.len(), 2 * poles.len());
    for (k, &(hz, decay)) in poles.iter().enumerate() {
        let r = (-decay / rate).exp();
        let theta = std::f64::consts::TAU * hz / rate;
        let mut rn = 1.0;
        for i in 0..u.len() {
            let (s, c) = (theta * i as f64).sin_cos();
            a.set(i, 2 * k, rn * c);
            a.set(i, 2 * k + 1, rn * s);
            rn *= r;
        }
    }
    let x = least_squares(&a, u)?;
    Some(
        x.chunks(2)
            .map(|ab| {
                // A·cos(θi + φ) = A cos φ·cos θi − A sin φ·sin θi.
                let (c, s) = (ab[0], ab[1]);
                (c.hypot(s), (-s).atan2(c))
            })
            .collect(),
    )
}

/// The bands the mode analysis walks for a percussive sound: an octave wide, centred every half
/// octave from 50 Hz, up to 2 kHz — the range where a drum's head modes can be told apart; above it the
/// wires and the metal are a noise field, read as texture. Each band keeps only the modes in its
/// central half-octave, so every frequency is claimed once.
#[must_use]
pub fn percussion_bands(rate: f64) -> Vec<(f64, f64, f64, f64)> {
    bands_up_to(2000.0_f64.min(0.35 * rate))
}

/// A pitched note's bands: the same half-octave steps, reaching 20 kHz (or 0.35 of the rate). A drum's
/// stop at 2 kHz, above which a snare holds wire noise; a bar's partials reach far higher (a C6 bar's
/// fourth partial is at 4.2 kHz).
#[must_use]
pub fn note_bands(rate: f64) -> Vec<(f64, f64, f64, f64)> {
    bands_up_to(20_000.0_f64.min(0.35 * rate))
}

fn bands_up_to(top: f64) -> Vec<(f64, f64, f64, f64)> {
    let mut bands = Vec::new();
    let mut centre = 50.0f64;
    while centre * std::f64::consts::SQRT_2 <= top {
        let (lo, hi) = (
            centre / std::f64::consts::SQRT_2,
            centre * std::f64::consts::SQRT_2,
        );
        let keep = (centre / 2f64.powf(0.25), centre * 2f64.powf(0.25));
        bands.push((lo, hi, keep.0, keep.1));
        centre *= std::f64::consts::SQRT_2;
    }
    bands
}
