//! Tone over time: which octaves carry the sound when, how bright each stage is, and where its
//! resonance sits and how narrow it is.
//!
//! Sources: `docs/drum-model-fitting.md` (in mxm-drum-machine) §4 (centroids), §5 and §7 (the
//! resonance profile, `ab_resonance.py`), and §7 *Acoustic drum models in particular* (octave-band
//! envelopes at fixed times, the measure that showed which band was wrong when).

use super::Context;
use crate::reading::{Reading, Resolution, Section, Table, Unit};
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

    let tables = ring(c, &mut readings);
    let mut section = Section::new("tone", "Tone over time", readings);
    section.tables.extend(tables);
    section
}

/// The ring's spans, ms: each read whole, past the octaves' 0.7 s, so a cymbal's ring is heard where
/// its tone sits. A span the file does not cover is not read.
pub const RING_MS: [(f64, f64); 2] = [(300.0, 1500.0), (1500.0, 3000.0)];
/// Where the ring's lines are looked for, Hz.
pub const RING_BAND: (f64, f64) = (200.0, 16_000.0);
/// A line stands at least this far over the floor, dB: a bin of noise stands 15 dB over its median
/// once in about three billion (its power is exponential), a cymbal's lines 25–50 dB.
pub const LINE_OVER_FLOOR_DB: f64 = 15.0;
/// At most this many lines a span, the strongest, so a candidate's line is found wherever it is.
pub const RING_LINES_MAX: usize = 200;

/// The ring's tone in each of [`RING_MS`], read from one Blackman–Harris spectrum of the whole span
/// (Harris, Proc. IEEE 1978: sidelobes at −92 dB, so a loud line raises no false ones beside it):
/// each octave's share of the span's power (`tone.ring_octave`); its lines, every peak standing
/// [`LINE_OVER_FLOOR_DB`] over the floor (`tone.ring_lines`); and the floor, each third octave's
/// median bin (`tone.ring_floor`). A line's level is its share of the span's power, a sine's power
/// read from its peak (0 dB, a sine alone); the floor's, the share a line at its median bin would
/// have, so a line missing from one sound compares with what that sound holds there.
///
/// The octave levels and the resonance profile end by 0.7 s and hear a band's level, not its lines:
/// a cymbal whose softest strike rang clean tones at 2–5 kHz, modelled as as many partials at random
/// pitches in each band, matched every octave there while its tones stood 15–45 dB under (the owner,
/// 2026-10-12: "the model is lower in pitch/resonance", "visible on the spectrograms").
fn ring(c: &Context, readings: &mut Vec<Reading>) -> Vec<Table> {
    let mut lines = Vec::new();
    let mut floors = Vec::new();
    for (a, b) in RING_MS {
        let level = c.level_db(a, b);
        let read = if c.covers(b) {
            c.window(a, b).and_then(|w| ring_spectrum(w, c.rate))
        } else {
            None
        };
        for (lo, hi) in OCTAVES {
            let hi_eff = hi.min(0.45 * c.rate);
            let share = read.as_ref().filter(|_| hi_eff > lo).and_then(|r| {
                let total: f64 = r.share.iter().sum();
                let part: f64 = r.share[r.bins(lo, hi_eff)].iter().sum();
                (total > 0.0 && part > 0.0).then(|| 10.0 * (part / total).log10())
            });
            readings.push(
                Reading::new(
                    "tone.ring_octave",
                    "Octave share of the ring",
                    share,
                    Unit::Decibels,
                    "Blackman–Harris spectrum of the whole span: the octave's power against the span's",
                )
                .window(a, b)
                .band(lo, hi_eff.max(lo))
                .floor(level),
            );
        }
        let Some(r) = read else { continue };
        let thirds = r.thirds();
        for &(centre, floor) in &thirds {
            floors.push(vec![Some(a), Some(b), Some(centre), Some(floor)]);
        }
        for (hz, db, over) in r.lines(&thirds) {
            lines.push(vec![Some(a), Some(b), Some(hz), Some(db), Some(over)]);
        }
    }
    let (from, to) = (RING_MS[0].0, RING_MS[RING_MS.len() - 1].1);
    vec![
        Table {
            id: "tone.ring_lines",
            title: "The ring's lines: each span's peaks standing 15 dB over the floor".into(),
            columns: vec![
                ("From", Unit::Milliseconds),
                ("To", Unit::Milliseconds),
                ("Frequency", Unit::Hertz),
                ("Level against the span", Unit::Decibels),
                ("Over the floor", Unit::Decibels),
            ],
            rows: lines,
            window_ms: Some((from, to)),
            source: "Blackman–Harris spectrum of the whole span (Harris 1978); a sine's power from its peak",
        },
        Table {
            id: "tone.ring_floor",
            title: "The ring's floor: each third octave's median bin, as a line's level".into(),
            columns: vec![
                ("From", Unit::Milliseconds),
                ("To", Unit::Milliseconds),
                ("Centre", Unit::Hertz),
                ("Floor against the span", Unit::Decibels),
            ],
            rows: floors,
            window_ms: Some((from, to)),
            source: "Blackman–Harris spectrum of the whole span (Harris 1978); the median bin of each third octave",
        },
    ]
}

/// One span's spectrum as shares: each bin's power as the share of the span's power a sine peaking
/// there would hold (`2|X|²·Σw² / ((Σw)²·Σ(xw)²)`, 1 for a sine alone, whatever the window's length
/// or the rate).
struct RingSpectrum {
    share: Vec<f64>,
    bin_hz: f64,
    /// The window's main lobe, ± bins.
    lobe: usize,
}

impl RingSpectrum {
    fn bins(&self, lo: f64, hi: f64) -> std::ops::Range<usize> {
        let at = |hz: f64| ((hz / self.bin_hz).ceil() as usize).min(self.share.len());
        at(lo)..at(hi).max(at(lo))
    }

    /// Each third octave of [`RING_BAND`]: its centre and its median bin, dB.
    fn thirds(&self) -> Vec<(f64, f64)> {
        let top = RING_BAND.1.min(self.share.len() as f64 * self.bin_hz);
        (0..)
            .map(|k| RING_BAND.0 * 2f64.powf(f64::from(k) / 3.0))
            .take_while(|&lo| lo * 2f64.powf(1.0 / 3.0) <= top)
            .filter_map(|lo| {
                let hi = lo * 2f64.powf(1.0 / 3.0);
                let mut part = self.share[self.bins(lo, hi)].to_vec();
                if part.is_empty() {
                    return None;
                }
                let mid = part.len() / 2;
                let (_, median, _) = part.select_nth_unstable_by(mid, f64::total_cmp);
                (*median > 0.0).then(|| ((lo * hi).sqrt(), 10.0 * median.log10()))
            })
            .collect()
    }

    /// The lines: peaks over the floor (`floor_at`), the highest within the main lobe each side,
    /// their frequency and level from a parabola through the log power; the strongest
    /// [`RING_LINES_MAX`], by frequency.
    fn lines(&self, thirds: &[(f64, f64)]) -> Vec<(f64, f64, f64)> {
        let range = self.bins(RING_BAND.0, RING_BAND.1);
        let db = |k: usize| 10.0 * self.share[k].max(1e-300).log10();
        let mut found: Vec<(f64, f64, f64)> = range
            .clone()
            // The highest within its main lobe each side, the first of equals.
            .filter(|&k| {
                k > self.lobe
                    && k + self.lobe < self.share.len()
                    && (k - self.lobe..k).all(|j| self.share[j] < self.share[k])
                    && (k + 1..=k + self.lobe).all(|j| self.share[j] <= self.share[k])
            })
            .filter_map(|k| {
                let (a, b, c) = (db(k - 1), db(k), db(k + 1));
                let d = 0.5 * (a - c) / (a - 2.0 * b + c);
                let d = if d.is_finite() {
                    d.clamp(-0.5, 0.5)
                } else {
                    0.0
                };
                let hz = (k as f64 + d) * self.bin_hz;
                let level = b - 0.25 * (a - c) * d;
                let over = level - floor_at(thirds, hz)?;
                (over >= LINE_OVER_FLOOR_DB).then_some((hz, level, over))
            })
            .collect();
        found.sort_by(|p, q| q.1.total_cmp(&p.1));
        found.truncate(RING_LINES_MAX);
        found.sort_by(|p, q| p.0.total_cmp(&q.0));
        found
    }
}

/// The floor at `hz`, dB: the third octaves' medians, straight between their centres in octaves, the
/// end ones held beyond them.
#[must_use]
pub fn floor_at(thirds: &[(f64, f64)], hz: f64) -> Option<f64> {
    let first = thirds.first()?;
    let last = thirds.last()?;
    if hz <= first.0 {
        return Some(first.1);
    }
    if hz >= last.0 {
        return Some(last.1);
    }
    thirds.windows(2).find(|w| hz <= w[1].0).map(|w| {
        let t = (hz / w[0].0).log2() / (w[1].0 / w[0].0).log2();
        w[0].1 + t * (w[1].1 - w[0].1)
    })
}

/// A span's [`RingSpectrum`], zero-padded to twice the next power of two (a line between bins reads
/// within a few hundredths of a dB). `None` for a short, silent or non-finite span.
fn ring_spectrum(segment: &[f64], rate: f64) -> Option<RingSpectrum> {
    if segment.len() < 1024 || segment.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let n = 2 * segment.len().next_power_of_two();
    let a = [0.35875, 0.48829, 0.14128, 0.01168];
    let m = segment.len() as f64 - 1.0;
    let window: Vec<f64> = (0..segment.len())
        .map(|k| {
            let x = std::f64::consts::TAU * k as f64 / m;
            a[0] - a[1] * x.cos() + a[2] * (2.0 * x).cos() - a[3] * (3.0 * x).cos()
        })
        .collect();
    let sum: f64 = window.iter().sum();
    let sum_sq: f64 = window.iter().map(|w| w * w).sum();
    let mut re = vec![0.0; n];
    let mut im = vec![0.0; n];
    let mut energy = 0.0;
    for (i, (s, w)) in segment.iter().zip(&window).enumerate() {
        re[i] = s * w;
        energy += re[i] * re[i];
    }
    if energy <= 0.0 {
        return None;
    }
    mxm_measure::spectrum::fft(&mut re, &mut im)?;
    let scale = 2.0 * sum_sq / (sum * sum * energy);
    Some(RingSpectrum {
        share: (0..=n / 2)
            .map(|k| scale * (re[k] * re[k] + im[k] * im[k]))
            .collect(),
        bin_hz: rate / n as f64,
        // Blackman–Harris's main lobe: ±4 of the segment's own bins, twice as many padded.
        lobe: 4 * n / segment.len(),
    })
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
