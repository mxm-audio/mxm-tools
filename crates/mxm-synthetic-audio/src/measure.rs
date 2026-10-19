//! Measuring a rendered String Studio note: its steady pitch, its harmonics' levels, its level over
//! time and how its pitch settles after the attack.
//!
//! None of it touches the audio. A note's steady pitch is what the note is tuned by (a static bend
//! before it starts, see `strings.rs`) and checked against; its onset and end are read from its
//! own level, rendered alone.

use std::f64::consts::TAU;

/// The Hann-windowed DFT of `x[a..b]` at `f` Hz, as an amplitude: a sinusoid of amplitude A there
/// reads A.
pub fn amplitude_at(x: &[f64], rate: f64, a: usize, b: usize, f: f64) -> f64 {
    let n = b - a;
    let (mut re, mut im, mut wsum) = (0.0, 0.0, 0.0);
    let w = TAU * f / rate;
    // A rotating phasor rather than a sin and cos per sample; renormalised every block.
    let (step_c, step_s) = (w.cos(), -w.sin());
    let (mut c, mut s) = (1.0f64, 0.0f64);
    for (j, &v) in x[a..b].iter().enumerate() {
        let window = 0.5 - 0.5 * (TAU * j as f64 / n as f64).cos();
        re += v * window * c;
        im += v * window * s;
        wsum += window;
        let (nc, ns) = (c * step_c - s * step_s, c * step_s + s * step_c);
        c = nc;
        s = ns;
        if j % 1024 == 1023 {
            let norm = c.hypot(s);
            c /= norm;
            s /= norm;
        }
    }
    2.0 * re.hypot(im) / wsum
}

/// The f0 maximising the sum of the first harmonics' amplitudes over `[t0, t1]`, searched within
/// `span_cents` of `guess` and refined by golden section. A bowed string's partials are locked to
/// one period, so their sum peaks at its f0 sharply; for a plucked string the fundamental alone is
/// asked (`harmonics = 1`), since its partials are stretched.
pub fn steady_pitch(
    x: &[f64],
    rate: f64,
    t0: f64,
    t1: f64,
    guess: f64,
    span_cents: f64,
    harmonics: usize,
) -> f64 {
    let a = ((t0 * rate) as usize).min(x.len());
    let b = ((t1 * rate) as usize).min(x.len());
    if b <= a + 16 {
        return guess;
    }
    let k_max = harmonics.min((5000.0 / guess) as usize).max(1);
    let score = |cents: f64| -> f64 {
        let f = guess * 2f64.powf(cents / 1200.0);
        (1..=k_max)
            .map(|k| amplitude_at(x, rate, a, b, k as f64 * f))
            .sum()
    };
    // A coarse grid finds the peak's lobe; golden section narrows it.
    let step = 2.0;
    let mut best = (f64::MIN, 0.0);
    let mut c = -span_cents;
    while c <= span_cents {
        let s = score(c);
        if s > best.0 {
            best = (s, c);
        }
        c += step;
    }
    let (mut lo, mut hi) = (best.1 - step, best.1 + step);
    let g = 0.618_033_988_749_895;
    let (mut m1, mut m2) = (hi - g * (hi - lo), lo + g * (hi - lo));
    let (mut s1, mut s2) = (score(m1), score(m2));
    for _ in 0..40 {
        if s1 > s2 {
            hi = m2;
            m2 = m1;
            s2 = s1;
            m1 = hi - g * (hi - lo);
            s1 = score(m1);
        } else {
            lo = m1;
            m1 = m2;
            s1 = s2;
            m2 = lo + g * (hi - lo);
            s2 = score(m2);
        }
    }
    guess * 2f64.powf(((lo + hi) / 2.0) / 1200.0)
}

/// Each of the first `count` harmonics' amplitude, dB, over `[t0, t1]`: the power within a
/// quarter of f0 of each harmonic, Hann-windowed, as a sinusoid's amplitude (Parseval), so a
/// partial that wanders a cent or two (jitter, a bowed string's tremolo) reads its full level,
/// where a single DFT bin read up to 5 dB low.
pub fn harmonic_levels(x: &[f64], rate: f64, t0: f64, t1: f64, f0: f64, count: usize) -> Vec<f64> {
    let a = ((t0 * rate) as usize).min(x.len());
    let b = ((t1 * rate) as usize).min(x.len());
    if b <= a + 64 {
        return vec![f64::NEG_INFINITY; count];
    }
    let len = b - a;
    let n = (len * 4).next_power_of_two();
    let mut planner = mxm_fft::Planner::<f64>::new();
    let fft = planner.real(n);
    let mut signal = vec![0.0; n];
    let mut window_energy = 0.0;
    for (j, v) in x[a..b].iter().enumerate() {
        let w = 0.5 - 0.5 * (TAU * j as f64 / len as f64).cos();
        signal[j] = v * w;
        window_energy += w * w;
    }
    let mut spectrum = vec![mxm_fft::Complex::new(0.0, 0.0); fft.spectrum_len()];
    let mut scratch = fft.make_scratch();
    fft.forward(&mut signal, &mut spectrum, &mut scratch)
        .expect("planned lengths");
    let bin = rate / n as f64;
    (1..=count)
        .map(|k| {
            let centre = k as f64 * f0;
            let lo = (((centre - f0 / 4.0) / bin).floor().max(1.0)) as usize;
            let hi = (((centre + f0 / 4.0) / bin).ceil() as usize).min(spectrum.len() - 1);
            let power: f64 = spectrum[lo..=hi].iter().map(|c| c.norm_sqr()).sum();
            // A sinusoid of amplitude A puts N·A²·Σw²/4 into its half of the spectrum.
            let amplitude = (4.0 * power / (n as f64 * window_energy)).sqrt();
            crate::dsp::gain_to_db(amplitude)
        })
        .collect()
}

/// The share of `[t0, t1]`'s energy near a harmonic, from half f0 up to 6 kHz (or `harmonics`):
/// 1 for a pure harmonic tone, less for noise or subharmonics. "Near" is a tenth of f0, or 12 Hz,
/// or the window's resolution, whichever is widest: a low bowed string's slow tremolo puts
/// sidebands about 10 Hz either side of each harmonic (String Studio's C#2, 2026-10-19), which are
/// no noise, while a raucous regime's subharmonics, at f0 / 2 or f0 / 3, stay outside it.
pub fn harmonic_share(x: &[f64], rate: f64, t0: f64, t1: f64, f0: f64, harmonics: f64) -> f64 {
    let a = ((t0 * rate) as usize).min(x.len());
    let b = ((t1 * rate) as usize).min(x.len());
    if b <= a + 64 {
        return 1.0;
    }
    let n = ((b - a) * 4).next_power_of_two();
    let mut planner = mxm_fft::Planner::<f64>::new();
    let fft = planner.real(n);
    let mut signal = vec![0.0; n];
    let len = b - a;
    for (j, v) in x[a..b].iter().enumerate() {
        signal[j] = v * (0.5 - 0.5 * (TAU * j as f64 / len as f64).cos());
    }
    let mut spectrum = vec![mxm_fft::Complex::new(0.0, 0.0); fft.spectrum_len()];
    let mut scratch = fft.make_scratch();
    fft.forward(&mut signal, &mut spectrum, &mut scratch)
        .expect("planned lengths");
    let top = (6000.0f64).min(harmonics * f0);
    let near_hz = (f0 / 10.0).max(12.0).max(2.5 / (t1 - t0));
    let (mut near, mut all) = (0.0, 0.0);
    for (k, c) in spectrum.iter().enumerate() {
        let f = k as f64 * rate / n as f64;
        if f < 0.5 * f0 || f > top {
            continue;
        }
        let e = c.norm_sqr();
        all += e;
        let h = (f / f0).round();
        if h >= 1.0 && (f - h * f0).abs() < near_hz {
            near += e;
        }
    }
    if all == 0.0 { 1.0 } else { near / all }
}

/// RMS level over 5 ms windows every 1 ms, dB.
pub fn level(x: &[f64], rate: f64) -> Vec<f64> {
    let win = (0.005 * rate) as usize;
    let hop = rate / 1000.0;
    let count = (x.len() as f64 / hop) as usize;
    let mut prefix = vec![0.0f64; x.len() + 1];
    for (i, v) in x.iter().enumerate() {
        prefix[i + 1] = prefix[i] + v * v;
    }
    (0..count)
        .map(|m| {
            let centre = (m as f64 * hop) as usize;
            let a = centre.saturating_sub(win / 2);
            let b = (centre + win / 2).min(x.len());
            let mean = (prefix[b] - prefix[a]) / (b - a).max(1) as f64;
            10.0 * mean.max(1e-30).log10()
        })
        .collect()
}

/// Where a note rendered alone begins to sound and where its release has fallen 40 dB: the first
/// and last milliseconds its level is within 40 dB of its peak. s.
pub fn onset_and_end(x: &[f64], rate: f64) -> (f64, f64, f64) {
    let level = level(x, rate);
    let peak = level.iter().cloned().fold(f64::MIN, f64::max);
    let threshold = peak - 40.0;
    let onset = level.iter().position(|&l| l >= threshold).unwrap_or(0);
    let end = level.iter().rposition(|&l| l >= threshold).unwrap_or(0) + 1;
    (onset as f64 / 1000.0, end as f64 / 1000.0, peak)
}

/// The period at `t`, by the lag that best matches one stretch of the signal with the next
/// (the normalised form of YIN's difference: de Cheveigné & Kawahara, *YIN, a fundamental
/// frequency estimator for speech and music*, JASA 111, 2002), searched near `hz`. Returns f0.
pub fn local_f0(x: &[f64], rate: f64, t: f64, hz: f64) -> Option<f64> {
    let period = rate / hz;
    let (lo, hi) = ((period * 0.9) as usize, (period * 1.1).ceil() as usize + 1);
    let w = (period * 1.5) as usize;
    let i = (t * rate) as usize;
    if i + hi + w + 1 >= x.len() {
        return None;
    }
    let a = &x[i..i + w];
    let ea: f64 = a.iter().map(|v| v * v).sum();
    let corr = |lag: usize| -> f64 {
        let b = &x[i + lag..i + lag + w];
        let eb: f64 = b.iter().map(|v| v * v).sum();
        let ab: f64 = a.iter().zip(b).map(|(p, q)| p * q).sum();
        ab / (ea * eb).sqrt().max(1e-300)
    };
    let scores: Vec<f64> = (lo..=hi).map(corr).collect();
    let (j, &best) = scores
        .iter()
        .enumerate()
        .max_by(|p, q| p.1.total_cmp(q.1))?;
    if best < 0.5 {
        return None;
    }
    let d = if j > 0 && j + 1 < scores.len() {
        let (y0, y1, y2) = (scores[j - 1], scores[j], scores[j + 1]);
        0.5 * (y0 - y2) / (y0 - 2.0 * y1 + y2)
    } else {
        0.0
    };
    Some(rate / ((lo + j) as f64 + d))
}

/// How the pitch strays from `steady` between `from` and `to`, s: the largest deviation, cents,
/// and the first and last times it is more than 5 cents off, ms after `from` (`None` if never).
pub fn deviation(
    x: &[f64],
    rate: f64,
    from: f64,
    to: f64,
    steady: f64,
) -> (f64, Option<f64>, Option<f64>) {
    let mut largest = 0.0f64;
    let (mut first, mut last) = (None, None);
    let mut t = from + 0.005;
    while t < to {
        if let Some(f) = local_f0(x, rate, t, steady) {
            let cents = 1200.0 * (f / steady).log2();
            largest = largest.max(cents.abs());
            if cents.abs() > 5.0 {
                let ms = (t - from) * 1000.0;
                first.get_or_insert(ms);
                last = Some(ms);
            }
        }
        t += 0.002;
    }
    (largest, first, last)
}

/// How the pitch settles after `onset`: the largest deviation from `steady` in the first
/// `window` s, cents, and when it last leaves 5 cents of it, ms after the onset.
pub fn settling(x: &[f64], rate: f64, onset: f64, steady: f64, window: f64) -> (f64, f64) {
    let (largest, _, last) = deviation(x, rate, onset, onset + window, steady);
    (largest, last.unwrap_or(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_harmonic_tones_pitch_to_a_hundredth_of_a_cent() {
        let rate = 48_000.0;
        let f = 131.5;
        let x: Vec<f64> = (0..48_000)
            .map(|i| {
                let t = i as f64 / rate;
                (1..8)
                    .map(|k| (TAU * k as f64 * f * t + k as f64).sin() / k as f64)
                    .sum()
            })
            .collect();
        let got = steady_pitch(&x, rate, 0.1, 0.9, 130.81, 60.0, 8);
        assert!((1200.0 * (got / f).log2()).abs() < 0.01, "{got}");
        let local = local_f0(&x, rate, 0.5, 131.0).unwrap();
        assert!((1200.0 * (local / f).log2()).abs() < 1.0, "{local}");
        let levels = harmonic_levels(&x, rate, 0.1, 0.9, f, 3);
        assert!((levels[1] - crate::dsp::gain_to_db(0.5)).abs() < 0.05);
    }
}
