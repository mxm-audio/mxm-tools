//! Linear-phase FIR filters for the mode analysis, applied **anticausally**, and the analytic signal.
//!
//! Technique (`research:listening/modal-estimation.md` §5 and §7, after Ege, Boutillon & David 2009
//! and Laroche's subband chain): a Blackman-windowed sinc, applied as `y[j] = Σ h[m]·x[j+m]`. Under that
//! filter a sum of damped sinusoids stays **exactly** a sum of the same damped sinusoids, each scaled by
//! `H(z) = Σ h[m]·z^m` evaluated **at its damped pole** — so an estimate of a mode's amplitude is
//! corrected by that complex gain and nothing else. An IIR filter would add its own poles to the signal
//! as spurious modes, and a centred filter would put its transient on the onset (the research page's
//! check E failed that way first). The last `M − 1` samples are lost; nothing is lost at the start.
//!
//! The analytic signal is the classic frequency-domain construction: zero the negative frequencies and
//! double the positive ones (Marple, "Computing the discrete-time analytic signal via FFT", IEEE TSP
//! 1999), through `mxm-measure`'s FFT.

use mxm_measure::spectrum::{fft, ifft};

use crate::numeric::Complex;

/// The Blackman window of `n` points.
fn blackman(n: usize) -> Vec<f64> {
    if n < 2 {
        return vec![1.0; n];
    }
    let d = (n - 1) as f64;
    (0..n)
        .map(|i| {
            let x = std::f64::consts::TAU * i as f64 / d;
            0.42 - 0.5 * x.cos() + 0.08 * (2.0 * x).cos()
        })
        .collect()
}

/// `sin(πx)/(πx)`.
fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        let px = std::f64::consts::PI * x;
        px.sin() / px
    }
}

/// A linear-phase band-pass from `lo_hz` to `hi_hz` at `rate`, `taps` long (odd), Blackman-windowed and
/// scaled to unit gain at the band's centre. `lo_hz = 0` gives a low-pass. `None` for an empty band, an
/// edge at or past the Nyquist frequency, or fewer than three taps.
#[must_use]
pub fn band_pass(lo_hz: f64, hi_hz: f64, rate: f64, taps: usize) -> Option<Vec<f64>> {
    if taps < 3 || !(lo_hz >= 0.0 && lo_hz < hi_hz && hi_hz < 0.5 * rate) {
        return None;
    }
    let taps = taps | 1;
    let mid = (taps / 2) as f64;
    let (a, b) = (lo_hz / rate, hi_hz / rate);
    let w = blackman(taps);
    let mut h: Vec<f64> = (0..taps)
        .map(|i| {
            let t = i as f64 - mid;
            (2.0 * b * sinc(2.0 * b * t) - 2.0 * a * sinc(2.0 * a * t)) * w[i]
        })
        .collect();
    let centre = if lo_hz > 0.0 {
        0.5 * (lo_hz + hi_hz)
    } else {
        0.0
    };
    let g = response(
        &h,
        Complex::polar(1.0, std::f64::consts::TAU * centre / rate),
    )
    .abs();
    if !(g > 0.0 && g.is_finite()) {
        return None;
    }
    for v in &mut h {
        *v /= g;
    }
    Some(h)
}

/// `H(z) = Σ h[m]·z^m`: the anticausal filter's gain on the sequence `z^j`. At `z = e^{jω}` it is the
/// frequency response (conjugated relative to the causal convention, which the correction accounts
/// for); at a damped pole it is the exact gain the mode's amplitude carries.
#[must_use]
pub fn response(h: &[f64], z: Complex) -> Complex {
    // Horner from the top coefficient.
    h.iter()
        .rev()
        .fold(Complex::ZERO, |acc, &c| acc * z + Complex::new(c, 0.0))
}

/// `y[j] = Σ h[m]·x[j+m]` for every `j` with the whole filter inside `x`, keeping every `step`-th
/// output from `start`: filtering and decimation in one pass, computing only the kept samples.
#[must_use]
pub fn anticausal_decimated(
    x: &[f64],
    h: &[f64],
    start: usize,
    step: usize,
    count: usize,
) -> Vec<f64> {
    let step = step.max(1);
    let mut out = Vec::with_capacity(count);
    for k in 0..count {
        let j = start + k * step;
        if j + h.len() > x.len() {
            break;
        }
        out.push(h.iter().zip(&x[j..j + h.len()]).map(|(a, b)| a * b).sum());
    }
    out
}

/// The analytic signal of `x`, as `(re, im)`: its real part is `x` and its magnitude the Hilbert
/// envelope. `None` for an empty or non-finite input.
#[must_use]
pub fn analytic(x: &[f64]) -> Option<(Vec<f64>, Vec<f64>)> {
    if x.is_empty() || x.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let n = x.len().next_power_of_two();
    let mut re = vec![0.0; n];
    let mut im = vec![0.0; n];
    re[..x.len()].copy_from_slice(x);
    fft(&mut re, &mut im)?;
    for k in 1..n / 2 {
        re[k] *= 2.0;
        im[k] *= 2.0;
    }
    for k in n / 2 + 1..n {
        re[k] = 0.0;
        im[k] = 0.0;
    }
    ifft(&mut re, &mut im)?;
    re.truncate(x.len());
    im.truncate(x.len());
    Some((re, im))
}

/// The Hilbert envelope `|x + j·H{x}|`.
#[must_use]
pub fn hilbert_envelope(x: &[f64]) -> Option<Vec<f64>> {
    let (re, im) = analytic(x)?;
    Some(re.iter().zip(&im).map(|(a, b)| a.hypot(*b)).collect())
}
