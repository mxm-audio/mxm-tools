//! Artefacts: a click at the onset, the noise floor, and mains hum — each reported as the file's, not
//! the sound's (the plan's §2 item 1; `docs/drum-model-fitting.md` §2 on clicks, and its traps on
//! hum, floors and a chain's warts: "never fit the recording's floor, hum or other warts").

use super::Context;
use crate::reading::{Reading, Section, Unit};
use crate::repr::{bands, envelope, spectrum};

/// The click band, Hz: above 5 kHz.
pub const CLICK_FROM_HZ: f64 = 5000.0;
/// Mains frequencies, Hz.
pub const MAINS_HZ: [f64; 2] = [50.0, 60.0];

#[must_use]
pub fn read(c: &Context) -> Section {
    let mut readings = Vec::new();
    let x = &c.x;

    // A click: the high band's peak in the first 2 ms against its peak over 2–12 ms. A struck head's
    // high band builds over milliseconds; a step at the onset spikes it at once.
    let hi = 0.45 * c.rate;
    let click = bands::band(x, c.rate, CLICK_FROM_HZ, hi).and_then(|y| {
        let start = c.onset.saturating_sub(envelope::samples(0.001, c.rate));
        let early_end = c.at(2.0).min(y.len());
        let later_end = c.at(12.0).min(y.len());
        let early = y
            .get(start..early_end)?
            .iter()
            .fold(0.0f64, |m, v| m.max(v.abs()));
        let later = y
            .get(early_end..later_end)?
            .iter()
            .fold(0.0f64, |m, v| m.max(v.abs()));
        envelope::db(early, later)
    });
    readings.push(
        Reading::new(
            "artefacts.click",
            "Onset click (high band, first 2 ms against 2–12 ms)",
            click,
            Unit::Decibels,
            "guide §2 (clicks): above 5 kHz; a positive value is a spike at the onset",
        )
        .window(-1.0, 2.0)
        .band(CLICK_FROM_HZ, hi),
    );

    // The noise floor: the quietest 50 ms after the onset that is not digital silence, against the
    // peak; and the level before the onset, where there is at least 20 ms of it.
    let w = envelope::samples(0.05, c.rate).max(1);
    let floor = x[c.onset..]
        .chunks_exact(w)
        .filter(|chunk| chunk.iter().any(|v| *v != 0.0))
        .filter_map(envelope::rms)
        .reduce(f64::min)
        .and_then(|l| envelope::db(l, c.peak));
    readings.push(Reading::new(
        "artefacts.noise_floor",
        "Noise floor (quietest 50 ms)",
        floor,
        Unit::Decibels,
        "against the peak; a measure under a floor measures the floor (guide traps)",
    ));
    let pre = envelope::samples(0.02, c.rate);
    readings.push(if c.onset >= pre {
        let before = envelope::rms(&x[..c.onset.saturating_sub(envelope::samples(0.001, c.rate))])
            .and_then(|l| envelope::db(l, c.peak));
        Reading::new(
            "artefacts.pre_onset",
            "Level before the onset",
            before,
            Unit::Decibels,
            "against the peak",
        )
    } else {
        Reading::absent(
            "artefacts.pre_onset",
            "Level before the onset",
            Unit::Decibels,
            "less than 20 ms before the onset",
            "against the peak",
        )
    });

    // Hum: how far mains and its first two harmonics stand above their neighbourhood in the last
    // second of the file (or its second half), where a drum has decayed and hum has not.
    let tail_len = envelope::samples(1.0, c.rate).min((x.len() - c.onset) / 2);
    let tail = &x[x.len() - tail_len..];
    let s = spectrum::power_spectrum(tail, c.rate, 1 << 17);
    for mains in MAINS_HZ {
        let prominence = s.as_ref().and_then(|s| {
            (1..=3)
                .filter_map(|k| {
                    let f = mains * k as f64;
                    let line = s.power[s.bins(f - 1.5, f + 1.5)]
                        .iter()
                        .copied()
                        .fold(0.0f64, f64::max);
                    let mut around: Vec<f64> = s.power[s.bins(f - 15.0, f - 4.0)]
                        .iter()
                        .chain(&s.power[s.bins(f + 4.0, f + 15.0)])
                        .copied()
                        .collect();
                    if around.is_empty() || line <= 0.0 {
                        return None;
                    }
                    around.sort_by(f64::total_cmp);
                    let median = around[around.len() / 2];
                    (median > 0.0).then(|| 10.0 * (line / median).log10())
                })
                .reduce(f64::min)
        });
        let (id, label) = if mains < 55.0 {
            ("artefacts.hum_50", "Mains hum, 50 Hz family")
        } else {
            ("artefacts.hum_60", "Mains hum, 60 Hz family")
        };
        readings.push(
            Reading::new(
                id,
                label,
                prominence,
                Unit::Decibels,
                "the weakest of the first three harmonics over its neighbourhood; above ~10 dB is hum",
            )
            .band(mains, 3.0 * mains),
        );
    }

    Section::new(
        "artefacts",
        "Artefacts (the file's, not the sound's)",
        readings,
    )
}
