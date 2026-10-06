//! Band-limited resampling by a Blackman-windowed sinc (Smith, *Digital Audio Resampling*, the
//! bandlimited interpolation description), the cut-off lowered by the ratio when reading faster so
//! nothing folds.

/// Zero crossings each side of the kernel.
const HALF: usize = 16;

/// The value of `x` at the fractional position `pos`, band-limited to `cutoff` of Nyquist.
fn at(x: &[f64], pos: f64, cutoff: f64) -> f64 {
    let half = HALF as f64 / cutoff;
    let lo = (pos - half).ceil().max(0.0) as usize;
    let hi = ((pos + half).floor().max(0.0) as usize).min(x.len().saturating_sub(1));
    if x.is_empty() || lo > hi {
        return 0.0;
    }
    (lo..=hi)
        .map(|k| {
            let d = k as f64 - pos;
            let arg = cutoff * d;
            let sinc = if arg.abs() < 1e-12 {
                1.0
            } else {
                (std::f64::consts::PI * arg).sin() / (std::f64::consts::PI * arg)
            };
            let u = (d / half + 1.0) * 0.5;
            let w = 0.42 - 0.5 * (std::f64::consts::TAU * u).cos()
                + 0.08 * (2.0 * std::f64::consts::TAU * u).cos();
            x[k] * cutoff * sinc * w
        })
        .sum()
}

/// `x` read `ratio` times as fast about the sample `anchor`, which stays where it is: pitch up by the
/// ratio, everything after the anchor sooner; the length is kept.
#[must_use]
pub fn about(x: &[f64], anchor: usize, ratio: f64) -> Vec<f64> {
    let cutoff = (1.0 / ratio).min(1.0);
    (0..x.len())
        .map(|n| {
            at(
                x,
                anchor as f64 + (n as f64 - anchor as f64) * ratio,
                cutoff,
            )
        })
        .collect()
}

/// `x` sampled at `from` Hz, resampled to `to` Hz.
#[must_use]
pub fn rate(x: &[f64], from: f64, to: f64) -> Vec<f64> {
    if !(from > 0.0 && to > 0.0) {
        return Vec::new();
    }
    let step = from / to;
    let cutoff = (1.0 / step).min(1.0);
    let n = ((x.len() as f64) / step).floor() as usize;
    (0..n).map(|k| at(x, k as f64 * step, cutoff)).collect()
}
