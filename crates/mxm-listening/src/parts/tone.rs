//! Tone over time: which octaves carry the sound when, how bright each stage is, and where its
//! resonance sits and how narrow it is.
//!
//! Sources: `docs/drum-model-fitting.md` (in mxm-drum-machine) §4 (centroids), §5 and §7 (the
//! resonance profile, `ab_resonance.py`), and §7 *Acoustic drum models in particular* (octave-band
//! envelopes at fixed times, the measure that showed which band was wrong when).

use super::Context;
use crate::reading::{Reading, Resolution, Section, Unit};
use crate::repr::{bands, envelope, spectrum};

/// Octave bands, Hz: 50 Hz to 12.8 kHz, then the top band to 20 kHz where the rate allows. The octave
/// levels also read [`LOW_OCTAVE`] below them.
pub const OCTAVES: [(f64, f64); 9] = [
    (50.0, 100.0),
    (100.0, 200.0),
    (200.0, 400.0),
    (400.0, 800.0),
    (800.0, 1600.0),
    (1600.0, 3200.0),
    (3200.0, 6400.0),
    (6400.0, 12_800.0),
    (12_800.0, 20_000.0),
];

/// The octave under the rest, 25–50 Hz, where a kick's port and its lowest push sound. It is read
/// over [`LOW_OCTAVE_WINDOW_MS`], one period at 25 Hz: a 10 ms window holds a quarter of one, and its
/// level would swing with the phase. The tonal share keeps [`OCTAVES`].
pub const LOW_OCTAVE: (f64, f64) = (25.0, 50.0);
pub const LOW_OCTAVE_WINDOW_MS: f64 = 40.0;

/// Where each octave's level is read: a 10 ms window centred here, ms from the onset.
pub const TIMES_MS: [f64; 11] = [
    5.0, 15.0, 30.0, 50.0, 80.0, 120.0, 180.0, 250.0, 350.0, 500.0, 700.0,
];

/// The brightness windows, ms (the corpus fit's tone windows).
pub const CENTROID_MS: [(f64, f64); 3] = [(20.0, 80.0), (80.0, 200.0), (200.0, 500.0)];

/// The resonance profile's windows, ms, and band, Hz (`ab_resonance.py`).
pub const PROFILE_MS: [(f64, f64); 6] = [
    (0.0, 10.0),
    (10.0, 30.0),
    (30.0, 60.0),
    (60.0, 120.0),
    (120.0, 250.0),
    (250.0, 500.0),
];
pub const PROFILE_BAND: (f64, f64) = (300.0, 18_000.0);
/// The resonance's smoothing span, ± a fraction of frequency. A Q is only compared with a Q of the
/// same span (the guide: ±2 %, ±4 % and ±8 % gave Q ≈ 20, 12 and 5 on one file).
pub const PROFILE_SPAN: f64 = 0.08;

#[must_use]
pub fn read(c: &Context) -> Section {
    let mut readings = Vec::new();
    octave_levels(c, &mut readings);

    for (a, b) in CENTROID_MS {
        let level = c.level_db(a, b);
        let s = c
            .window(a, b)
            .and_then(|w| spectrum::power_spectrum(w, c.rate, 1 << 16));
        let mut r = Reading::new(
            "tone.centroid",
            "Brightness (centroid)",
            if c.covers(b) {
                s.as_ref().and_then(|s| spectrum::centroid(s, 20_000.0))
            } else {
                None
            },
            Unit::Hertz,
            "ab_metrics.py `cent body`, over fixed windows",
        )
        .window(a, b)
        .floor(level);
        if let Some(s) = &s {
            r = r.resolution(s.resolution());
        }
        readings.push(r);
    }

    let top = PROFILE_BAND.1.min(0.45 * c.rate);
    for (a, b) in PROFILE_MS {
        let level = c.level_db(a, b);
        let s = if c.covers(b) {
            c.window(a, b)
                .and_then(|w| spectrum::power_spectrum(w, c.rate, 1 << 16))
        } else {
            None
        };
        let res = s
            .as_ref()
            .and_then(|s| spectrum::resonance(s, PROFILE_BAND.0, top, PROFILE_SPAN));
        let resolution = s.as_ref().map(|s| Resolution {
            span: Some(PROFILE_SPAN),
            ..s.resolution()
        });
        let with = |r: Reading| {
            let r = r.window(a, b).band(PROFILE_BAND.0, top).floor(level);
            match resolution {
                Some(res) => r.resolution(res),
                None => r,
            }
        };
        readings.push(with(Reading::new(
            "tone.median",
            "Where the weighted energy sits (A-weighted median)",
            s.as_ref()
                .and_then(|s| spectrum::weighted_median(s, PROFILE_BAND.0, top)),
            Unit::Hertz,
            "ab_resonance.py median; guide §5",
        )));
        readings.push(with(Reading::new(
            "tone.resonance",
            "Resonance (smoothed spectral peak)",
            res.map(|r| r.0),
            Unit::Hertz,
            "ab_resonance.py res; guide §5",
        )));
        readings.push(with(Reading::new(
            "tone.q",
            "Resonance Q (−3 dB width)",
            res.and_then(|r| r.1),
            Unit::Plain,
            "ab_resonance.py Q; guide §5",
        )));
        readings.push(
            Reading::new(
                "tone.window_level",
                "Window level",
                if c.covers(b) { level } else { None },
                Unit::Decibels,
                "ab_resonance.py level: window RMS against the peak",
            )
            .window(a, b),
        );
    }

    Section::new("tone", "Tone over time", readings)
}

/// Each octave's 10 ms level at [`TIMES_MS`], against the loudest broadband 10 ms window.
fn octave_levels(c: &Context, readings: &mut Vec<Reading>) {
    let w10 = envelope::samples(0.01, c.rate).max(1);
    let x = c.from_onset();
    let reference = envelope::windowed_rms(x, w10)
        .into_iter()
        .fold(0.0f64, f64::max);
    let bands_read = std::iter::once((LOW_OCTAVE, LOW_OCTAVE_WINDOW_MS))
        .chain(OCTAVES.iter().map(|&band| (band, 10.0)));
    for ((lo, hi), window_ms) in bands_read {
        let hi_eff = hi.min(0.45 * c.rate);
        let filtered = if hi_eff > lo {
            bands::band(x, c.rate, lo, hi_eff)
        } else {
            None
        };
        let span = envelope::samples(window_ms / 1000.0, c.rate).max(1);
        for t in TIMES_MS {
            // Centred on `t`, clipped at the onset: the first low window starts there.
            let from = envelope::samples((t - 0.5 * window_ms).max(0.0) / 1000.0, c.rate);
            let level = filtered
                .as_ref()
                .and_then(|y| envelope::rms_at(y, from, span))
                .and_then(|l| envelope::db(l, reference));
            let r = Reading::new(
                "tone.band_level",
                "Octave level",
                level,
                Unit::Decibels,
                "guide §7: 4th-order Butterworth, zero-phase, 10 ms RMS (40 ms under 50 Hz) against the loudest 10 ms",
            )
            .window(t - 5.0, t + 5.0)
            .band(lo, hi_eff.max(lo))
            .resolution(Resolution {
                window_ms,
                bin_hz: None,
                span: None,
            });
            readings.push(if filtered.is_none() {
                Reading {
                    validity: crate::reading::Validity::Absent("band above the Nyquist limit"),
                    value: None,
                    ..r
                }
            } else if envelope::rms_at(x, from, span).is_none() {
                Reading {
                    validity: crate::reading::Validity::Absent("the file ends before this window"),
                    value: None,
                    ..r
                }
            } else if level.is_some_and(|l| l < crate::reading::NUMERICAL_FLOOR_DB) {
                Reading {
                    validity: crate::reading::Validity::Absent("numerically silent"),
                    value: None,
                    ..r
                }
            } else {
                r.floor(level)
            });
        }
    }
}
