//! Tonal or noise: whether the energy in a band, or at a spectral peak, is a predictable line — a mode,
//! a partial — or noise, however resonant or filtered, that only looks like one in a spectrum.
//!
//! **Technique: spectral unpredictability**, the measure perceptual audio coders use to tell tonal
//! components from noise-like ones (ISO/IEC 11172-3, MPEG-1 Audio, psychoacoustic model 2's
//! "unpredictability measure"; written here from that description). In a short-time spectrum with a
//! fixed hop, a stationary or exponentially decaying sinusoid advances its phase by the same step every
//! frame and changes its magnitude by the same factor, so each frame's complex value is predicted from
//! the two before it: `X̂ₜ = (|Xₜ₋₁|²/|Xₜ₋₂|)·e^{j(2φₜ₋₁ − φₜ₋₂)}` — the magnitude extrapolated
//! geometrically (the standard's linear step fails a fast decay over a long hop). The unpredictability
//! `c = |Xₜ − X̂ₜ| / (|Xₜ| + |X̂ₜ|)` is near 0 for a line and about 0.5–1 for noise, however narrow a
//! filter made it: its phase wanders and its magnitude flickers.
//!
//! **The hop decides what counts as noise.** Noise of bandwidth `B` stays predictable for about `1/B`;
//! a mode of decay `α'` has a line `α'/π` wide and stays predictable far longer. A 5 ms hop read
//! resonant noise at Q 20 (50 Hz wide at 1 kHz) as 73 % tonal; the hop here, about 32 ms, sits between
//! a drum mode's coherence (a T60 of 0.3 s is a 7 Hz line) and a resonance's. A resonance much
//! narrower than that is a continuously excited mode, and reads as one. Asked by the owner (the plan's
//! revision 11): "there must be a way to recognise when something is tuned/filtered/resonant noise and
//! not regular harmonics."

use super::Context;
use crate::reading::{Reading, Section, Table, Unit};
use crate::repr::spectrum;

/// The analysis frame and hop, ms (see *The hop decides what counts as noise*).
pub const FRAME_MS: f64 = 42.7;
pub const HOP_MS: f64 = 32.0;
/// Below this unpredictability a bin is a line; above [`NOISE_ABOVE`] it is noise (**chosen**, from
/// the measure's behaviour: a line reads below 0.1, Gaussian noise about 0.6).
pub const TONAL_BELOW: f64 = 0.2;
pub const NOISE_ABOVE: f64 = 0.4;
/// The windows the tonal share is read in, ms.
/// Three frames need about 110 ms, so the first window starts at 20 ms and spans 180.
pub const WINDOWS_MS: [(f64, f64); 2] = [(20.0, 200.0), (200.0, 500.0)];

/// One frame's complex spectrum.
struct Frame {
    re: Vec<f64>,
    im: Vec<f64>,
}

/// The frames from `from_ms` to `to_ms`, Hann-windowed, `n` points each.
fn frames(c: &Context, from_ms: f64, to_ms: f64, n: usize, hop: usize) -> Vec<Frame> {
    let window = spectrum::hann(n);
    let mut out = Vec::new();
    let mut start = c.at(from_ms);
    let end = c.at(to_ms).min(c.x.len());
    while start + n <= end {
        let mut re: Vec<f64> = c.x[start..start + n]
            .iter()
            .zip(&window)
            .map(|(x, w)| x * w)
            .collect();
        let mut im = vec![0.0; n];
        if mxm_measure::spectrum::fft(&mut re, &mut im).is_none() {
            break;
        }
        re.truncate(n / 2 + 1);
        im.truncate(n / 2 + 1);
        out.push(Frame { re, im });
        start += hop;
    }
    out
}

/// Per frame (from the third) and bin: `(power, unpredictability)`.
fn unpredictability(frames: &[Frame]) -> Vec<Vec<(f64, f64)>> {
    frames
        .windows(3)
        .map(|w| {
            let (a, b, x) = (&w[0], &w[1], &w[2]);
            (0..x.re.len())
                .map(|k| {
                    let (ma, pa) = (a.re[k].hypot(a.im[k]), a.im[k].atan2(a.re[k]));
                    let (mb, pb) = (b.re[k].hypot(b.im[k]), b.im[k].atan2(b.re[k]));
                    let m_hat = if ma > 0.0 { mb * mb / ma } else { 0.0 };
                    let p_hat = 2.0 * pb - pa;
                    let (hr, hi) = (m_hat * p_hat.cos(), m_hat * p_hat.sin());
                    let mx = x.re[k].hypot(x.im[k]);
                    let dist = (x.re[k] - hr).hypot(x.im[k] - hi);
                    let denom = mx + m_hat;
                    let c = if denom > 0.0 {
                        (dist / denom).min(1.0)
                    } else {
                        1.0
                    };
                    (mx * mx, c)
                })
                .collect()
        })
        .collect()
}

#[must_use]
pub fn read(c: &Context) -> Section {
    let n = ((FRAME_MS / 1000.0 * c.rate) as usize).next_power_of_two();
    let hop = ((HOP_MS / 1000.0 * c.rate) as usize).max(1);
    let bin_hz = c.rate / n as f64;
    let mut readings = Vec::new();
    let bands = super::tone::OCTAVES;
    for (a, b) in WINDOWS_MS {
        let level = c.level_db(a, b);
        let f = frames(c, a, b, n, hop);
        let u = unpredictability(&f);
        for (lo, hi) in bands {
            let hi_eff = hi.min(0.45 * c.rate);
            if hi_eff <= lo {
                continue;
            }
            let (k0, k1) = (
                (lo / bin_hz).ceil() as usize,
                (hi_eff / bin_hz).ceil() as usize,
            );
            let (mut tonal, mut total) = (0.0, 0.0);
            for frame in &u {
                for &(p, cu) in frame.get(k0..k1.min(frame.len())).unwrap_or(&[]) {
                    total += p;
                    if cu < TONAL_BELOW {
                        tonal += p;
                    }
                }
            }
            let share =
                (total > 0.0 && !u.is_empty() && c.covers(b)).then(|| 100.0 * tonal / total);
            readings.push(
                Reading::new(
                    "tonality.tonal_share",
                    "Tonal share (energy in predictable lines)",
                    share,
                    Unit::Percent,
                    "spectral unpredictability (MPEG-1 psychoacoustic model 2): a line is below 0.2",
                )
                .window(a, b)
                .band(lo, hi_eff)
                .floor(level),
            );
        }
    }

    // The main spectral peaks of 20–200 ms, each judged a line or resonant noise.
    let mut section = Section::new("tonality", "Tonal or noise", readings);
    section.tables.push(peaks(c, n, hop, bin_hz));
    section
}

/// The ten strongest spectral peaks of 20–200 ms between 40 Hz and 16 kHz: frequency, level against
/// the strongest, energy-weighted unpredictability, the peak's −3 dB width, and the verdict (1 a line,
/// 0 resonant noise, 0.5 mixed).
fn peaks(c: &Context, n: usize, hop: usize, bin_hz: f64) -> Table {
    let f = frames(c, 20.0, 200.0, n, hop);
    let u = unpredictability(&f);
    let bins = u.first().map_or(0, Vec::len);
    let mut rows = Vec::new();
    if bins > 2 && !u.is_empty() {
        let power: Vec<f64> = (0..bins)
            .map(|k| u.iter().map(|fr| fr[k].0).sum())
            .collect();
        let lo = (40.0 / bin_hz).ceil() as usize;
        let hi = ((16_000.0f64.min(0.45 * c.rate)) / bin_hz) as usize;
        let mut candidates: Vec<usize> = (lo.max(1)..hi.min(bins - 1))
            .filter(|&k| power[k] > power[k - 1] && power[k] >= power[k + 1])
            .collect();
        candidates.sort_by(|a, b| power[*b].total_cmp(&power[*a]));
        let strongest = candidates.first().map_or(0.0, |&k| power[k]);
        for &k in candidates.iter().take(10) {
            if power[k] < strongest * 1e-4 {
                break;
            }
            // Energy-weighted unpredictability over the peak's three bins.
            let (mut num, mut den) = (0.0, 0.0);
            for fr in &u {
                for &(m, c) in &fr[k - 1..=k + 1] {
                    num += m * c;
                    den += m;
                }
            }
            let cu = if den > 0.0 { num / den } else { 1.0 };
            // −3 dB width in the summed spectrum.
            let half = power[k] / 2.0;
            let left = (1..=k).rev().find(|&j| power[j] <= half).unwrap_or(0);
            let right = (k..bins).find(|&j| power[j] <= half).unwrap_or(bins - 1);
            let verdict = if cu < TONAL_BELOW {
                1.0
            } else if cu > NOISE_ABOVE {
                0.0
            } else {
                0.5
            };
            rows.push(vec![
                Some(k as f64 * bin_hz),
                Some(10.0 * (power[k] / strongest).log10()),
                Some(cu),
                Some((right - left) as f64 * bin_hz),
                Some(verdict),
            ]);
        }
    }
    Table {
        id: "tonality.peaks",
        title: "Spectral peaks, 20–200 ms: a line (1), resonant noise (0) or mixed (0.5)".into(),
        columns: vec![
            ("Frequency", Unit::Hertz),
            ("Level against the strongest", Unit::Decibels),
            ("Unpredictability", Unit::Plain),
            ("Width at −3 dB", Unit::Hertz),
            ("Line or noise", Unit::Plain),
        ],
        rows,
        window_ms: Some((20.0, 200.0)),
        source: "spectral unpredictability (MPEG-1 psychoacoustic model 2)",
    }
}
