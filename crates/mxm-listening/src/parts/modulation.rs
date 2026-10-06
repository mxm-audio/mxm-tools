//! Modulation: whether the ring wobbles or is a steady, "too clean" tone, how wide its line is, what
//! share of the tail it carries, which close modes beat, and how rough each octave is.
//!
//! Sources: `docs/drum-model-fitting.md` (in mxm-drum-machine) §7 ("the skin rings a little long":
//! not the decay but a ring that is a steady tone where the recording's wobbles — 2 % against 9 %
//! over 15–300 Hz; "the tail's pitch is too clean"; "there is a grrrr sound": modulation depth
//! across 15–300 Hz per band). The wobble and roughness of one render are **samples**, as the guide
//! measured 5.7–16.7 % for one cell across noise realisations.

use super::Context;
use crate::reading::{Reading, Section, Table, Unit};
use crate::repr::{bands, envelope, fir, spectrum};

use super::pitch::WindowModes;

/// The ring band's half width around the ring's line, as a fraction.
pub const RING_BAND: f64 = 0.15;
/// The modulation band of a wobble or roughness, Hz.
pub const WOBBLE_HZ: (f64, f64) = (15.0, 300.0);
/// Octave bands roughness is read in, Hz.
pub const ROUGH_BANDS: [(f64, f64); 5] = [
    (100.0, 200.0),
    (200.0, 400.0),
    (400.0, 800.0),
    (800.0, 1600.0),
    (1600.0, 3200.0),
];
/// Close modes within this many hertz beat audibly (**chosen**: a 12 Hz beat is still heard as a
/// wobble rather than as roughness).
pub const BEAT_HZ: f64 = 12.0;

#[must_use]
pub fn read(c: &Context, ring_hz: Option<f64>, windows: &[WindowModes]) -> Section {
    let mut readings = Vec::new();
    let x = c.from_onset();

    // The ring's wobble: its band's Hilbert envelope over 100–400 ms, less its 30 ms moving average,
    // band-passed 15–300 Hz, as a share of the mean envelope.
    let level = c.level_db(100.0, 400.0);
    let wobble = ring_hz.and_then(|f| {
        depth(
            x,
            c.rate,
            f * (1.0 - RING_BAND),
            f * (1.0 + RING_BAND),
            0.1,
            0.4,
        )
    });
    readings.push(
        Reading::new(
            "modulation.ring_wobble",
            "Ring wobble (envelope modulation, 15 Hz to 15 % of the ring)",
            wobble,
            Unit::Percent,
            "guide §7: the recording's ring read 9 %, the model's 2 %; the ring's ±15 % band passes modulation up to 15 % of its frequency",
        )
        .window(100.0, 400.0)
        .floor(level)
        .sample(),
    );

    // The ring line's width at −3 and −10 dB, over 100–500 ms.
    let s = c
        .window(100.0, 500.0)
        .filter(|_| c.covers(500.0))
        .and_then(|w| spectrum::power_spectrum(w, c.rate, 1 << 18));
    for (drop_db, label) in [
        (3.0, "Ring line width at −3 dB"),
        (10.0, "Ring line width at −10 dB"),
    ] {
        let width = match (&s, ring_hz) {
            (Some(s), Some(f)) => line_width(s, f, drop_db),
            _ => None,
        };
        // The window's own width for a steady tone at the same place: a line no wider is unresolved.
        let own = ring_hz.and_then(|f| {
            let n = envelope::samples(0.4, c.rate);
            let tone: Vec<f64> = (0..n)
                .map(|i| (std::f64::consts::TAU * f * i as f64 / c.rate).sin())
                .collect();
            let t = spectrum::power_spectrum(&tone, c.rate, 1 << 18)?;
            line_width(&t, f, drop_db)
        });
        let mut r = Reading::new(
            if drop_db < 5.0 {
                "modulation.ring_width_3"
            } else {
                "modulation.ring_width_10"
            },
            label,
            width,
            Unit::Hertz,
            "guide §7: a steady line is narrow; a wobbling one is wide",
        )
        .window(100.0, 500.0)
        .floor(c.level_db(100.0, 500.0));
        if let Some(s) = &s {
            r = r.resolution(s.resolution());
        }
        if let (Some(wv), Some(o)) = (width, own) {
            if wv <= 1.15 * o && r.validity == crate::reading::Validity::Valid {
                r.validity = crate::reading::Validity::Unresolved;
            }
        }
        readings.push(r);
    }

    // The ring's share of the sound, 100–300 and 300–600 ms.
    for (a, b) in [(100.0, 300.0), (300.0, 600.0)] {
        let share = ring_hz.and_then(|f| {
            let seg = c.window(a, b).filter(|_| c.covers(b))?;
            let whole = envelope::rms(seg)?;
            let ring = bands::band(x, c.rate, f * (1.0 - RING_BAND), f * (1.0 + RING_BAND))?;
            let (i, j) = (
                envelope::samples(a / 1000.0, c.rate),
                envelope::samples(b / 1000.0, c.rate),
            );
            let part = envelope::rms(ring.get(i..j.min(ring.len()))?)?;
            envelope::db(part, whole)
        });
        readings.push(
            Reading::new(
                "modulation.ring_share",
                "The ring's share of the sound",
                share,
                Unit::Decibels,
                "guide §7: ring band (±15 %) against everything",
            )
            .window(a, b)
            .floor(c.level_db(a, b)),
        );
    }

    // Roughness: 15–300 Hz modulation depth of each octave's envelope over 40–400 ms.
    for (lo, hi) in ROUGH_BANDS {
        if hi >= 0.45 * c.rate {
            continue;
        }
        let d = depth(x, c.rate, lo, hi, 0.04, 0.4);
        readings.push(
            Reading::new(
                "modulation.roughness",
                "Roughness (15–300 Hz envelope modulation)",
                d,
                Unit::Percent,
                "guide §7 (\"there is a grrrr sound\")",
            )
            .window(40.0, 400.0)
            .band(lo, hi)
            .floor(c.level_db(40.0, 400.0))
            .sample(),
        );
    }

    let mut section = Section::new("modulation", "Modulation", readings);
    section.tables.push(beats(windows));
    section
}

/// The 15–300 Hz modulation depth of `x`'s band `lo..hi` between `from` and `to` seconds, percent.
fn depth(x: &[f64], rate: f64, lo: f64, hi: f64, from: f64, to: f64) -> Option<f64> {
    let (a, b) = (envelope::samples(from, rate), envelope::samples(to, rate));
    if b > x.len() || hi >= 0.45 * rate {
        return None;
    }
    let y = bands::band(x, rate, lo, hi)?;
    let env = fir::hilbert_envelope(&y)?;
    let seg = &env[a..b];
    // Less the 30 ms moving average: what is left is the fast part of the envelope.
    let w = envelope::samples(0.03, rate).max(1);
    let mut prefix = vec![0.0; seg.len() + 1];
    for (i, v) in seg.iter().enumerate() {
        prefix[i + 1] = prefix[i] + v;
    }
    let half = w / 2;
    let slow: Vec<f64> = (0..seg.len())
        .map(|i| {
            let (p, q) = (i.saturating_sub(half), (i + half + 1).min(seg.len()));
            (prefix[q] - prefix[p]) / (q - p) as f64
        })
        .collect();
    let fast: Vec<f64> = seg.iter().zip(&slow).map(|(e, s)| e - s).collect();
    let sections = bands::butterworth_band_pass(1, WOBBLE_HZ.0, WOBBLE_HZ.1, rate)?;
    let ripple = bands::filter_zero_phase(&sections, &fast);
    let mean = slow.iter().sum::<f64>() / slow.len() as f64;
    let rms = envelope::rms(&ripple)?;
    (mean > 0.0).then(|| 100.0 * rms / mean)
}

/// The width of the line nearest `hz`, `drop_db` below its peak, walking outward.
fn line_width(s: &spectrum::Spectrum, hz: f64, drop_db: f64) -> Option<f64> {
    let around = s.bins(hz * 0.9, hz * 1.1);
    let (k, peak) = around
        .clone()
        .map(|k| (k, s.power[k]))
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    if peak <= 0.0 {
        return None;
    }
    let level = peak * 10f64.powf(-drop_db / 10.0);
    let lo = (around.start..k).rev().find(|&i| s.power[i] <= level)?;
    let hi = (k + 1..around.end).find(|&i| s.power[i] <= level)?;
    Some(s.hz(hi) - s.hz(lo))
}

/// Pairs of believed modes closer than [`BEAT_HZ`], per window: they beat at their difference.
fn beats(windows: &[WindowModes]) -> Table {
    let mut rows = Vec::new();
    for w in windows {
        let modes: Vec<_> = w.modes.iter().filter(|(_, l)| *l >= -50.0).collect();
        for (i, (a, la)) in modes.iter().enumerate() {
            for (b, lb) in modes.iter().skip(i + 1) {
                let d = (a.hz - b.hz).abs();
                if d > 0.0 && d <= BEAT_HZ {
                    let (lo, hi) = if a.hz < b.hz { (a, b) } else { (b, a) };
                    rows.push(vec![
                        Some(w.window_ms.0),
                        Some(lo.hz),
                        Some(hi.hz),
                        Some(d),
                        Some((la - lb).abs()),
                    ]);
                }
            }
        }
    }
    Table {
        id: "modulation.beats",
        title: "Close mode pairs (they beat at their difference)".into(),
        columns: vec![
            ("Window from", Unit::Milliseconds),
            ("Lower mode", Unit::Hertz),
            ("Upper mode", Unit::Hertz),
            ("Beat", Unit::Hertz),
            ("Level difference", Unit::Decibels),
        ],
        rows,
        window_ms: None,
        source: "repr::modes; the guide's two-head (0,m) pairs and split twins beat",
    }
}
