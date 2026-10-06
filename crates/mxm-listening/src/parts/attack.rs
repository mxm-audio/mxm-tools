//! The attack: how the hit starts, how fast it rises, which way it moves first, how its level falls
//! away over the first 100 ms, and how bright its first 20 ms are.
//!
//! Sources: `docs/drum-model-fitting.md` §3–§4 and §7 (*Acoustic drum models in particular*),
//! `ab_metrics.py` (`jump`, `pk ms`, `cent early`) and `ab_residuals.py` (early windows, roll-off).

use super::Context;
use crate::reading::{Reading, Section, Unit};
use crate::repr::{envelope, spectrum};

/// The early windows, ms from the onset (`ab_residuals.py`).
pub const WINDOWS_MS: [(f64, f64); 4] = [(0.0, 3.0), (3.0, 10.0), (10.0, 30.0), (30.0, 100.0)];

#[must_use]
pub fn read(c: &Context) -> Section {
    let mut readings = Vec::new();

    // The first two samples against the peak: near 1 means the sound starts at full amplitude.
    let first = c.x[c.onset..]
        .iter()
        .take(2)
        .fold(0.0f64, |m, s| m.max(s.abs()));
    readings.push(Reading::new(
        "attack.first_step",
        "First-sample step",
        Some(first / c.peak),
        Unit::Ratio,
        "ab_metrics.py `jump`; guide §4",
    ));

    readings.push(Reading::new(
        "attack.peak_time",
        "Time to peak",
        Some(c.ms(c.peak_at.saturating_sub(c.onset))),
        Unit::Milliseconds,
        "ab_metrics.py `pk ms`; guide §4",
    ));

    readings.push(rise_time(c));

    // The direction of the first half-cycle that reaches 30 % of the peak.
    let polarity = c.x[c.onset..]
        .iter()
        .find(|s| s.abs() >= 0.3 * c.peak)
        .map(|s| s.signum());
    readings.push(Reading::new(
        "attack.polarity",
        "First half-cycle direction",
        polarity,
        Unit::Sign,
        "guide §3 (onset view); +1 is the file's positive direction",
    ));

    for (a, b) in WINDOWS_MS {
        let level = c.level_db(a, b);
        readings.push(
            Reading::new(
                "attack.window_level",
                "Early window level",
                if c.covers(b) { level } else { None },
                Unit::Decibels,
                "ab_residuals.py window levels, against the peak",
            )
            .window(a, b)
            .floor(level),
        );
    }

    let early = c.window(0.0, 20.0);
    let level = c.level_db(0.0, 20.0);
    let s = early.and_then(|w| spectrum::power_spectrum(w, c.rate, 1 << 16));
    let mut centroid = Reading::new(
        "attack.early_centroid",
        "Early brightness (centroid)",
        s.as_ref().and_then(|s| spectrum::centroid(s, 20_000.0)),
        Unit::Hertz,
        "ab_metrics.py `cent early`; guide §4",
    )
    .window(0.0, 20.0)
    .floor(level);
    if let Some(s) = &s {
        centroid = centroid.resolution(s.resolution());
    }
    readings.push(centroid);

    let s = early.and_then(|w| spectrum::power_spectrum(w, c.rate, 1 << 15));
    let mut rolloff = Reading::new(
        "attack.early_rolloff",
        "Early roll-off (85 % of power below)",
        s.as_ref()
            .and_then(|s| spectrum::rolloff(s, 40.0, 18_000.0_f64.min(0.45 * c.rate), 0.85)),
        Unit::Hertz,
        "ab_residuals.py `rolloff_hz`",
    )
    .window(0.0, 20.0)
    .band(40.0, 18_000.0_f64.min(0.45 * c.rate))
    .floor(level);
    if let Some(s) = &s {
        rolloff = rolloff.resolution(s.resolution());
    }
    readings.push(rolloff);

    // The largest step between neighbouring samples anywhere in the file: a click is a step.
    let step = mxm_measure::observe::worst_step(&c.raw);
    let mut worst = Reading::new(
        "attack.worst_step",
        "Largest sample step",
        step.and_then(|(_, s)| envelope::db(s, c.peak)),
        Unit::Decibels,
        "mxm-measure `worst_step`, against the peak; guide §2 (clicks)",
    );
    if let Some((at, _)) = step {
        let ms = (at as f64 - c.onset as f64) / c.rate * 1000.0;
        worst = worst.window(ms, ms);
    }
    readings.push(worst);

    Section::new("attack", "Attack", readings)
}

/// 10–90 % rise of the 1 ms RMS envelope (linear amplitude) over the first 50 ms, interpolated
/// between samples.
fn rise_time(c: &Context) -> Reading {
    let reading = |v| {
        Reading::new(
            "attack.rise_time",
            "Rise time (10–90 %)",
            v,
            Unit::Milliseconds,
            "guide §4 and §7; the snare's attack measure",
        )
        .window(0.0, 50.0)
    };
    let x = c.from_onset();
    let end = envelope::samples(0.05, c.rate).min(x.len());
    let half = (envelope::samples(0.0005, c.rate)).max(1);
    // Prefix sums of squares from the onset.
    let mut prefix = vec![0.0; x.len() + 1];
    for (i, s) in x.iter().enumerate() {
        prefix[i + 1] = prefix[i] + s * s;
    }
    let env: Vec<f64> = (0..end)
        .map(|i| {
            let a = i.saturating_sub(half);
            let b = (i + half).min(x.len());
            ((prefix[b] - prefix[a]) / (b - a) as f64).sqrt()
        })
        .collect();
    let top = env.iter().copied().fold(0.0f64, f64::max);
    if top.is_nan() || top <= 0.0 {
        return reading(None);
    }
    let cross = |level: f64| -> Option<f64> {
        let i = env.iter().position(|&e| e >= level)?;
        if i == 0 {
            return Some(0.0);
        }
        let (e0, e1) = (env[i - 1], env[i]);
        let frac = if e1 > e0 {
            (level - e0) / (e1 - e0)
        } else {
            0.0
        };
        Some(i as f64 - 1.0 + frac)
    };
    let rise = match (cross(0.1 * top), cross(0.9 * top)) {
        (Some(a), Some(b)) => Some((b - a) / c.rate * 1000.0),
        _ => None,
    };
    reading(rise)
}
