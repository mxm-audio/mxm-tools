//! Word attributes (the plan's §2 item 8): the words people use for timbre — hardness, depth,
//! brightness, warmth; boominess is read with the perceptual models (`parts::perception`) — each
//! given as the numbers behind it rather than a score.
//!
//! The words are the Audio Commons project's (Pearce, Brookes & Mason, *Audio Commons D5.2: first
//! prototype of timbral characterisation tools*, 2017), whose descriptions tie each to its evidence:
//! brightness to the energy above 3 kHz and where it sits, depth to the energy of the low range and
//! where it sits, hardness to the attack's steepness and its brightness. Their regressions, fitted to
//! their listeners, are not reproduced — no implementation was opened — and a 0–100 score would hide
//! what this crate exists to show. Warmth's bands are chosen here: the low mids against the upper
//! mids.

use super::Context;
use crate::reading::{Reading, Section, Unit};
use crate::repr::{envelope, spectrum};

/// Brightness is read above this, Hz (Audio Commons D5.2).
pub const BRIGHT_HZ: f64 = 3000.0;
/// Depth is read over this range, Hz.
pub const DEPTH_HZ: (f64, f64) = (30.0, 200.0);
/// Warmth: the first range's energy against the second's, Hz. **Chosen.**
pub const WARM_HZ: (f64, f64) = (150.0, 600.0);
pub const UPPER_MID_HZ: (f64, f64) = (600.0, 6000.0);
/// The words are read from the onset until the level stays this far under its loudest 10 ms, dB…
pub const BODY_DB: f64 = 40.0;
/// …and for at most this long, s.
pub const BODY_MAX_S: f64 = 2.0;
/// Hardness reads the attack's steepest rise within this long of the onset, ms, on an RMS envelope
/// over 1 ms…
pub const HARD_WITHIN_MS: f64 = 30.0;
/// …and its brightness over this long, ms.
pub const HARD_CENTROID_MS: f64 = 10.0;

/// The body the words are read over: from the onset until the 10 ms level stays [`BODY_DB`] under its
/// loudest, at most [`BODY_MAX_S`]; ms.
fn body_ms(c: &Context) -> f64 {
    let w = ((0.01 * c.rate) as usize).max(1);
    let x = c.from_onset();
    let levels = envelope::windowed_rms(&x[..x.len().min((BODY_MAX_S * c.rate) as usize)], w);
    let top = levels.iter().copied().fold(0.0, f64::max);
    let floor = top * 10f64.powf(-BODY_DB / 20.0);
    let last = levels
        .iter()
        .rposition(|&l| l >= floor)
        .map_or(0, |i| i + 1);
    (last as f64 * 10.0).max(20.0)
}

/// The words section.
#[must_use]
pub fn read(c: &Context) -> Section {
    let src = "Audio Commons D5.2's words (Pearce, Brookes & Mason 2017), given as the numbers behind them: the Hann spectrum of the body, from the onset until the level stays 40 dB under its loudest (at most 2 s)";
    let src_hard = "Audio Commons D5.2's hardness, as the numbers behind it: the steepest rise of a 1 ms RMS envelope within 30 ms of the onset, as the time a rise that steep takes to a sine's RMS at the peak, and the first 10 ms's centroid";
    let to = body_ms(c);
    let s = c
        .window(0.0, to)
        .and_then(|w| spectrum::power_spectrum(w, c.rate, 1 << 16));
    let energy = |lo: f64, hi: f64| -> f64 {
        s.as_ref().map_or(0.0, |s| {
            s.bins(lo, hi.min(0.5 * c.rate)).map(|k| s.power[k]).sum()
        })
    };
    let all = energy(20.0, 20_000.0);
    let share = |lo: f64, hi: f64| {
        let e = energy(lo, hi);
        (e > 0.0 && all > 0.0).then(|| 10.0 * (e / all).log10())
    };
    let centroid = |lo: f64, hi: f64| {
        s.as_ref().and_then(|s| {
            let (mut num, mut den) = (0.0, 0.0);
            for k in s.bins(lo, hi.min(0.5 * c.rate)) {
                num += s.hz(k) * s.power[k];
                den += s.power[k];
            }
            (den > 0.0).then(|| num / den)
        })
    };
    let (w_lo, w_hi) = (
        energy(WARM_HZ.0, WARM_HZ.1),
        energy(UPPER_MID_HZ.0, UPPER_MID_HZ.1),
    );
    let warmth = (w_lo > 0.0 && w_hi > 0.0).then(|| 10.0 * (w_lo / w_hi).log10());
    // Hardness: the attack's steepest rise on the linear envelope (a 1 ms RMS every 0.25 ms), as the
    // time a rise that steep would take from silence to the peak. In dB the steepest rise of any ramp
    // is at its very start, whatever its length: a 1 ms and a 25 ms fade read alike.
    let rise = {
        let w = ((0.001 * c.rate) as usize).max(1);
        let hop = (w / 4).max(1);
        let end = ((HARD_WITHIN_MS / 1000.0 * c.rate) as usize + w).min(c.x.len() - c.onset);
        let lead = c.onset.min(w);
        let x = &c.x[c.onset - lead..c.onset + end];
        let levels: Vec<f64> = (0..x.len().saturating_sub(w))
            .step_by(hop)
            .map(|i| envelope::rms(&x[i..i + w]).unwrap_or(0.0) / c.peak)
            .collect();
        let step_ms = hop as f64 / c.rate * 1000.0;
        let steepest = levels
            .windows(2)
            .map(|p| (p[1] - p[0]) / step_ms)
            .fold(0.0f64, f64::max);
        // The RMS of a sine at the peak is 0.707 of it: the time to that level.
        (steepest > 0.0).then(|| std::f64::consts::FRAC_1_SQRT_2 / steepest)
    };
    let attack_centroid = c
        .window(0.0, HARD_CENTROID_MS)
        .and_then(|w| spectrum::power_spectrum(w, c.rate, 1 << 14))
        .and_then(|s| spectrum::centroid(&s, 20_000.0));
    let window = |r: Reading| r.window(0.0, to);
    Section::new(
        "words",
        "Words for the timbre, with the numbers behind them",
        vec![
            window(Reading::new(
                "words.brightness",
                "Brightness: the energy above 3 kHz against the whole",
                share(BRIGHT_HZ, 20_000.0),
                Unit::Decibels,
                src,
            )),
            window(Reading::new(
                "words.bright_centroid",
                "Brightness: where the energy above 3 kHz sits (its centroid)",
                centroid(BRIGHT_HZ, 20_000.0),
                Unit::Hertz,
                src,
            )),
            window(Reading::new(
                "words.depth",
                "Depth: the energy from 30 to 200 Hz against the whole",
                share(DEPTH_HZ.0, DEPTH_HZ.1),
                Unit::Decibels,
                src,
            )),
            window(Reading::new(
                "words.depth_centroid",
                "Depth: where the energy from 30 to 200 Hz sits (its centroid)",
                centroid(DEPTH_HZ.0, DEPTH_HZ.1),
                Unit::Hertz,
                src,
            )),
            window(Reading::new(
                "words.warmth",
                "Warmth: 150–600 Hz against 600 Hz–6 kHz",
                warmth,
                Unit::Decibels,
                src,
            )),
            Reading::new(
                "words.hardness",
                "Hardness: the attack's steepest rise, as the time it would take to the peak (shorter is harder)",
                rise,
                Unit::Milliseconds,
                src_hard,
            )
            .window(0.0, HARD_WITHIN_MS),
            Reading::new(
                "words.hard_centroid",
                "Hardness: the attack's brightness (the first 10 ms's centroid)",
                attack_centroid,
                Unit::Hertz,
                src_hard,
            )
            .window(0.0, HARD_CENTROID_MS),
        ],
    )
}
