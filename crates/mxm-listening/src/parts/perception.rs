//! What a listener's ear makes of the hit, after ECMA-418-2: loudness, sharpness, tonality and
//! roughness over time from the onset (`crate::perception`). Every value assumes the owner's fixed
//! level (a full-scale sine is 94 dB SPL), heard diotically in a free field; the standard's single
//! values, which discard the first 300 ms, are not quoted for a hit.

use super::Context;
use crate::perception::{self, Perception};
use crate::reading::{Reading, Resolution, Section, Table, Unit};
use crate::sound::Sound;

/// The stage windows, ms from the onset.
pub const WINDOWS_MS: [(f64, f64); 4] =
    [(0.0, 50.0), (50.0, 200.0), (200.0, 500.0), (500.0, 1000.0)];
/// The curve's times, ms from the onset.
pub const CURVE_MS: [f64; 14] = [
    5.0, 10.0, 20.0, 30.0, 50.0, 75.0, 100.0, 150.0, 200.0, 300.0, 400.0, 600.0, 800.0, 1000.0,
];
/// How much of the sound after the onset is analysed, s: the last window and the roughness blocks.
const ANALYSED_S: f64 = 1.2;

const ASSUMED: &str = "ECMA-418-2 (4th ed.), the Sottek hearing model, played with its body at −20 dB K-weighted against full scale, as the A/B page matches sounds, at the owner's fixed level (a full-scale sine is 94 dB SPL), diotic, free field; a value every 5.3 ms, each closing a block of 21 ms (above 340 Hz) to 171 ms (below 85 Hz); tonality is smoothed at 3.5 Hz and lags a hit";

/// The body loudness every sound is played at before the models hear it: −20 dB K-weighted RMS
/// against full scale (`prep::body_rms`), the measure the A/B page matches sounds on, so the models
/// hear what the owner hears on the page and a gain alone changes nothing here. Under the owner's
/// level a full-scale sine is 94 dB SPL, which puts the body near 71 dB SPL.
pub const BODY_DBFS: f64 = -20.0;

pub fn read(c: &Context) -> Section {
    let onset_s = c.onset as f64 / c.rate;
    let body = f64::from(crate::prep::body_rms(&c.raw, c.rate as u32));
    let gain = if body > 0.0 {
        10f64.powf(BODY_DBFS / 20.0) / body
    } else {
        1.0
    };
    let sound = Sound::new(
        "perception",
        c.rate as u32,
        c.raw
            .iter()
            .map(|&v| (f64::from(v) * gain) as f32)
            .collect(),
    );
    let Some(p) = perception::analyse(&sound, onset_s + ANALYSED_S) else {
        return Section::new("perception", "What the ear makes of it", Vec::new());
    };
    // Index of the value at `ms` from the onset on a time base of `step` seconds.
    let index = |ms: f64, step: f64| ((onset_s + ms / 1000.0) / step).round() as usize;
    let range = |a: f64, b: f64, step: f64| index(a, step)..index(b, step).max(index(a, step) + 1);
    let fine = Perception::STEP_S;
    let rough = Perception::ROUGHNESS_STEP_S;
    let resolution = |_step: f64, block_ms: f64| Resolution {
        window_ms: block_ms,
        bin_hz: None,
        span: None,
    };
    let mut readings = Vec::new();

    let from = index(0.0, fine);
    let top = |curve: &[f64]| {
        curve
            .iter()
            .enumerate()
            .skip(from)
            .fold(
                (from, 0.0f64),
                |(bi, bv), (i, &v)| if v > bv { (i, v) } else { (bi, bv) },
            )
    };
    // How loud: the standard's loudness (Clause 8). It weights tonal against noise loudness, both
    // smoothed at 3.5 Hz, so on a hit it lags by tens of milliseconds and is quoted for its peak only.
    let (_, peak) = top(&p.loudness);
    readings.push(
        Reading::new(
            "perception.loudness_peak",
            "Loudness at its peak",
            (peak > 0.0).then_some(peak),
            Unit::Sone,
            "ECMA-418-2 (4th ed.) Clause 8 loudness at its maximum; its 3.5 Hz smoothing lags and lowers a hit's peak alike for any two hits compared; played with its body at −20 dB K-weighted against full scale, as the A/B page matches sounds, at the owner's fixed level (a full-scale sine is 94 dB SPL), diotic, free field",
        )
        .resolution(resolution(fine, 171.0)),
    );
    // When and for how long: the hearing model's basis loudness (Clause 5), which only its blocks
    // smooth.
    let (basis_at, basis_peak) = top(&p.basis);
    readings.push(
        Reading::new(
            "perception.loudness_peak_at",
            "When it sounds loudest",
            (basis_peak > 0.0).then_some((basis_at as f64 * fine - onset_s) * 1000.0),
            Unit::Milliseconds,
            "the basis loudness's maximum (ECMA-418-2 Formula (26)), from the onset; each value closes a block of 21 ms above 340 Hz, up to 171 ms below 85 Hz",
        )
        .resolution(resolution(fine, 21.3)),
    );
    let half = p
        .basis
        .iter()
        .enumerate()
        .skip(basis_at)
        .find(|(_, v)| **v < 0.5 * basis_peak)
        .map(|(i, _)| (i - basis_at) as f64 * fine * 1000.0);
    // Boominess (a word, `parts::words`): the loudness under 280 Hz's share, weighted by the loudness
    // itself, from the onset until the basis loudness falls under a tenth of its peak.
    let loud_end = p
        .basis
        .iter()
        .enumerate()
        .skip(basis_at)
        .find(|(_, v)| **v < 0.1 * basis_peak)
        .map_or(p.basis.len(), |(i, _)| i);
    let (num, den) = (from..loud_end)
        .filter_map(|i| Some((*p.low_share.get(i)?, *p.basis.get(i)?)))
        .fold((0.0, 0.0), |(n, d), (s, w)| (n + s * w, d + w));
    readings.push(
        Reading::new(
            "words.boominess",
            "Boominess: the loudness under 280 Hz, as a share of the whole",
            (den > 0.0).then(|| 100.0 * num / den),
            Unit::Percent,
            "Audio Commons D5.2's word, as the number behind it: Hatano & Hashimoto's booming index reads the loudness under 280 Hz; here the basis specific loudness's share there (ECMA-418-2 Formula (25)), weighted by the loudness, from the onset until it falls under a tenth of its peak",
        )
        .resolution(resolution(fine, 21.3)),
    );
    readings.push(
        Reading::new(
            "perception.loud_for",
            "How long it stays at least half as loud",
            if basis_peak > 0.0 { half } else { None },
            Unit::Milliseconds,
            "from the basis loudness's peak until it falls under half of it (about 10 dB of level)",
        )
        .resolution(resolution(fine, 21.3)),
    );

    for (a, b) in WINDOWS_MS {
        let r = range(a, b, fine);
        let loud = p
            .basis
            .get(r.clone())
            .map(|w| w.iter().copied().fold(0.0f64, f64::max));
        readings.push(
            Reading::new(
                "perception.loudness",
                "Loudness (peak in the window)",
                loud.filter(|v| *v > 0.0),
                Unit::Sone,
                "the hearing model's basis loudness (ECMA-418-2 Formula (26)): before the tonal weighting, so noise reads louder than in the standard's loudness; at the page's matched body level and the owner's fixed level, diotic, free field",
            )
            .window(a, b)
            .resolution(resolution(fine, 21.3)),
        );
        // Sharpness weighted by loudness, so the window's quiet end does not decide it.
        let (num, den) = r
            .clone()
            .filter_map(|i| Some((p.sharpness.get(i).copied().flatten()?, *p.basis.get(i)?)))
            .fold((0.0, 0.0), |(n, d), (s, w)| (n + s * w, d + w));
        readings.push(
            Reading::new(
                "perception.sharpness",
                "Sharpness (loudness-weighted mean)",
                (den > 0.0).then(|| num / den),
                Unit::Acum,
                "DIN 45692's weighting (unverified form) on the Sottek model's specific basis loudness, calibrated to 1 acum on 1 kHz narrow-band noise at 60 dB SPL; at the page's matched body level",
            )
            .window(a, b)
            .resolution(resolution(fine, 21.3)),
        );
        let tonal = p
            .tonality
            .get(r.clone())
            .map(|w| w.iter().map(|t| t.0).fold(0.0f64, f64::max));
        readings.push(
            Reading::new(
                "perception.tonality",
                "Tonality (peak in the window)",
                tonal,
                Unit::Tonality,
                ASSUMED,
            )
            .window(a, b)
            .resolution(resolution(fine, 21.3)),
        );
        let rr = range(a, b, rough);
        let roughness = p
            .roughness
            .get(rr)
            .map(|w| w.iter().copied().fold(0.0f64, f64::max));
        readings.push(
            Reading::new(
                "perception.roughness",
                "Roughness (peak in the window)",
                roughness,
                Unit::Asper,
                "ECMA-418-2 (4th ed.) roughness, at the page's matched body level; its 341 ms blocks and 0.5 s release smear a hit's first 100 ms; this implementation reads 1.04 asper at the standard's 1-asper calibration point",
            )
            .window(a, b)
            .resolution(resolution(rough, 341.3)),
        );
    }

    let mut section = Section::new("perception", "What the ear makes of it", readings);
    let rows = CURVE_MS
        .iter()
        .map(|&ms| {
            let i = index(ms, fine);
            let (t, f) = p.tonality.get(i).copied().unwrap_or((0.0, 0.0));
            vec![
                Some(ms),
                p.basis.get(i).copied(),
                p.loudness.get(i).copied(),
                p.sharpness.get(i).copied().flatten(),
                Some(t),
                (t > 0.0).then_some(f),
                p.roughness.get(index(ms, rough)).copied(),
            ]
        })
        .collect();
    section.tables.push(Table {
        id: "perception.curve",
        title: "Loudness, sharpness, tonality and roughness from the onset".into(),
        columns: vec![
            ("Time", Unit::Milliseconds),
            ("Basis loudness", Unit::Sone),
            ("Loudness (smoothed)", Unit::Sone),
            ("Sharpness", Unit::Acum),
            ("Tonality", Unit::Tonality),
            ("Tonal frequency", Unit::Hertz),
            ("Roughness", Unit::Asper),
        ],
        rows,
        window_ms: None,
        source: ASSUMED,
    });
    section
}
