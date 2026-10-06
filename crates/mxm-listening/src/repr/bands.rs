//! Zero-phase Butterworth band-passes: the band filter every one of the snare's band measurements used
//! (`docs/drum-model-fitting.md` §7, a fourth-order Butterworth run forward and backward).
//!
//! **Technique:** the analogue Butterworth low-pass prototype (poles on the unit circle's left half),
//! the low-pass to band-pass transformation `s → (s² + ω₀²)/(B·s)`, and the bilinear transform with
//! both edges prewarped (Oppenheim & Schafer, *Discrete-Time Signal Processing*, §7.1–7.4; Parks &
//! Burrus, *Digital Filter Design*, ch. 7). Written here from those descriptions. The result is
//! grouped into second-order sections, each pole pair with one zero at `z = 1` and one at `z = −1`,
//! and scaled to unit gain at the band's centre.
//!
//! Forward-backward filtering squares the magnitude and cancels the phase: the band edges are −6 dB,
//! not −3 dB, exactly as `sosfiltfilt` gives. Both ends are padded by odd extension so the start-up
//! transient falls outside the signal; the filter starts from rest, where `sosfiltfilt` starts from its
//! step response — the two differ only within a few periods of the edge, and every measure here is
//! taken from an onset 5 ms into the file (`prep::trim`).

use crate::numeric::Complex;

/// One second-order section, `b` over `a` with `a₀ = 1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Section {
    pub b: [f64; 3],
    pub a: [f64; 2],
}

/// A Butterworth band-pass of prototype order `order` (so `2·order` poles) from `lo_hz` to `hi_hz` at
/// `rate`. `None` for an order of zero, an edge outside `(0, rate/2)`, or `lo ≥ hi`.
#[must_use]
pub fn butterworth_band_pass(
    order: usize,
    lo_hz: f64,
    hi_hz: f64,
    rate: f64,
) -> Option<Vec<Section>> {
    let nyquist = 0.5 * rate;
    if order == 0 || !(lo_hz > 0.0 && lo_hz < hi_hz && hi_hz < nyquist) {
        return None;
    }
    let fs2 = 2.0 * rate;
    let w1 = fs2 * (std::f64::consts::PI * lo_hz / rate).tan();
    let w2 = fs2 * (std::f64::consts::PI * hi_hz / rate).tan();
    let w0 = (w1 * w2).sqrt();
    let bw = w2 - w1;
    // Analogue band-pass poles: each prototype pole p gives the roots of s² − p·B·s + ω₀² = 0.
    let mut poles = Vec::with_capacity(2 * order);
    for k in 0..order {
        let theta = std::f64::consts::PI * (2 * k + order + 1) as f64 / (2 * order) as f64;
        let p = Complex::polar(1.0, theta);
        let pb = p.scale(bw);
        let disc = pb * pb - Complex::new(4.0 * w0 * w0, 0.0);
        let root = sqrt(disc);
        for s in [(pb + root).scale(0.5), (pb - root).scale(0.5)] {
            // Bilinear transform: z = (2fs + s) / (2fs − s).
            let z = (Complex::new(fs2, 0.0) + s) / (Complex::new(fs2, 0.0) - s);
            poles.push(z);
        }
    }
    let mut sections = Vec::with_capacity(order);
    let mut upper: Vec<Complex> = poles.iter().copied().filter(|z| z.im > 1e-12).collect();
    let mut real: Vec<f64> = poles
        .iter()
        .filter(|z| z.im.abs() <= 1e-12)
        .map(|z| z.re)
        .collect();
    upper.sort_by(|a, b| a.arg().total_cmp(&b.arg()));
    real.sort_by(f64::total_cmp);
    for z in upper {
        sections.push(Section {
            b: [1.0, 0.0, -1.0],
            a: [-2.0 * z.re, z.norm_sqr()],
        });
    }
    for pair in real.chunks(2) {
        let (p, q) = (pair[0], *pair.get(1).unwrap_or(&0.0));
        sections.push(Section {
            b: [1.0, 0.0, -1.0],
            a: [-(p + q), p * q],
        });
    }
    if sections.len() != order {
        return None;
    }
    // Unit gain at the centre: ω₀ mapped back through the prewarping.
    let centre = 2.0 * (w0 / fs2).atan();
    let g = response(&sections, centre).abs();
    if !(g > 0.0 && g.is_finite()) {
        return None;
    }
    let k = 1.0 / g;
    for v in &mut sections[0].b {
        *v *= k;
    }
    Some(sections)
}

/// The principal square root of a complex number.
fn sqrt(z: Complex) -> Complex {
    let r = z.abs();
    let re = (0.5 * (r + z.re)).max(0.0).sqrt();
    let im = (0.5 * (r - z.re)).max(0.0).sqrt();
    Complex::new(re, if z.im < 0.0 { -im } else { im })
}

/// The cascade's frequency response at `omega` radians per sample.
#[must_use]
pub fn response(sections: &[Section], omega: f64) -> Complex {
    let z1 = Complex::polar(1.0, -omega);
    let z2 = z1 * z1;
    sections.iter().fold(Complex::ONE, |acc, s| {
        let num = Complex::new(s.b[0], 0.0) + z1.scale(s.b[1]) + z2.scale(s.b[2]);
        let den = Complex::ONE + z1.scale(s.a[0]) + z2.scale(s.a[1]);
        acc * (num / den)
    })
}

/// Runs the cascade over `x` in place, from rest (direct form II transposed).
pub fn filter(sections: &[Section], x: &mut [f64]) {
    for s in sections {
        let (mut z1, mut z2) = (0.0, 0.0);
        for v in x.iter_mut() {
            let input = *v;
            let y = s.b[0] * input + z1;
            z1 = s.b[1] * input - s.a[0] * y + z2;
            z2 = s.b[2] * input - s.a[1] * y;
            *v = y;
        }
    }
}

/// Zero-phase filtering: forward, then backward, with both ends padded by odd extension.
#[must_use]
pub fn filter_zero_phase(sections: &[Section], x: &[f64]) -> Vec<f64> {
    if x.len() < 2 {
        return x.to_vec();
    }
    let pad = (3 * (2 * sections.len() + 1)).min(x.len() - 1);
    let (first, last) = (x[0], x[x.len() - 1]);
    let mut y = Vec::with_capacity(x.len() + 2 * pad);
    y.extend((1..=pad).rev().map(|i| 2.0 * first - x[i]));
    y.extend_from_slice(x);
    y.extend((1..=pad).map(|i| 2.0 * last - x[x.len() - 1 - i]));
    filter(sections, &mut y);
    y.reverse();
    filter(sections, &mut y);
    y.reverse();
    y[pad..pad + x.len()].to_vec()
}

/// `x` band-passed from `lo_hz` to `hi_hz` by a fourth-order Butterworth, zero-phase. `None` where the
/// band does not fit below the Nyquist frequency.
#[must_use]
pub fn band(x: &[f64], rate: f64, lo_hz: f64, hi_hz: f64) -> Option<Vec<f64>> {
    let sections = butterworth_band_pass(4, lo_hz, hi_hz, rate)?;
    Some(filter_zero_phase(&sections, x))
}
