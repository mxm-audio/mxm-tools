//! What a window draws, as the listener computes it (`plans/plan-mxm-listener-hud.md` §2.3): the decay
//! envelope, the spectrum and a spectrogram, read out of what the analysis already builds, under the
//! readings' contracts — every value finite or absent, never NaN; every curve saying its window,
//! resolution and source. None of it is a new measurement, and none carries a threshold or a verdict.
//! The modes, the partials and the perceptual curves are tables in the report already.

use crate::parts::Context;
use crate::reading::{Resolution, Unit};
use crate::repr::{envelope, spectrum};
use crate::sound::Sound;
use crate::unexplained::{erb_hz, erb_number, frame_len, frames_in};

/// A line of points: where each is (ms from the onset, or Hz) and its value, absent where silent.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    pub id: &'static str,
    pub label: &'static str,
    pub points: Vec<(f64, Option<f64>)>,
    pub x_unit: Unit,
    pub y_unit: Unit,
    pub resolution: Resolution,
    pub source: &'static str,
}

/// Level over time and frequency.
#[derive(Clone, Debug, PartialEq)]
pub struct Spectrogram {
    pub id: &'static str,
    pub label: &'static str,
    /// Each frame's centre, ms from the onset.
    pub times_ms: Vec<f64>,
    /// Each band's edges, Hz, equally spaced in ERB number.
    pub bands_hz: Vec<(f64, f64)>,
    /// dB against the loudest cell, `[frame][band]`; absent where a cell is silent.
    pub cells_db: Vec<Vec<Option<f64>>>,
    pub resolution: Resolution,
    pub source: &'static str,
}

/// Everything a window draws of one sound; each absent where the sound gives nothing to draw.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Curves {
    pub envelope: Option<Curve>,
    pub spectrum: Option<Curve>,
    pub spectrogram: Option<Spectrogram>,
}

/// The spectrum's bands: this many to an octave.
pub const SPECTRUM_BANDS_PER_OCTAVE: f64 = 24.0;
/// The spectrogram's bands, equally spaced in ERB number.
pub const SPECTROGRAM_BANDS: usize = 40;
/// The spectrogram's lowest and highest frequencies, Hz: the unexplained map's range.
pub const SPECTROGRAM_HZ: (f64, f64) = (50.0, 16_000.0);

/// The curves of `sound`, measured from `onset_s` — the report's onset, so the curves line up with
/// its windows (a note's onset is not a hit's) — or from the percussive onset when none is given.
/// Silence and a non-finite sample give no curves, as they give no readings.
#[must_use]
pub fn curves(sound: &Sound, onset_s: Option<f64>) -> Curves {
    let Some(mut context) = Context::new(&sound.samples, sound.rate) else {
        return Curves::default();
    };
    if let Some(onset) = onset_s.map(|s| envelope::samples(s, context.rate)) {
        if onset < context.x.len() {
            context.onset = onset;
        }
    }
    Curves {
        envelope: envelope_curve(&context),
        spectrum: spectrum_curve(&context),
        spectrogram: spectrogram(sound, &context),
    }
}

/// The decay part's own envelope, in dB against its loudest window: what `decay.*` is read from.
fn envelope_curve(c: &Context) -> Option<Curve> {
    let (env, hop_ms, window_ms) = crate::parts::decay::decay_envelope(c);
    let loudest = env.iter().copied().fold(0.0f64, f64::max);
    if env.is_empty() || loudest <= 0.0 {
        return None;
    }
    Some(Curve {
        id: "curve.envelope",
        label: "Level over time, against its loudest moment",
        points: env
            .iter()
            .enumerate()
            .map(|(k, &v)| (k as f64 * hop_ms, envelope::db(v, loudest)))
            .collect(),
        x_unit: Unit::Milliseconds,
        y_unit: Unit::Decibels,
        resolution: Resolution {
            window_ms,
            bin_hz: None,
            span: None,
        },
        source: "the decay part's envelope: a sliding RMS every 1 ms over one period of the ring, each point at its window's start",
    })
}

/// The whole sound's spectrum from the onset, pooled into bands a 24th of an octave wide.
fn spectrum_curve(c: &Context) -> Option<Curve> {
    let x = c.from_onset();
    let s = spectrum::power_spectrum(x, c.rate, x.len())?;
    let top = (0.49 * c.rate).min(20_000.0);
    let step = 2f64.powf(1.0 / SPECTRUM_BANDS_PER_OCTAVE);
    let half = step.sqrt();
    let mut centres = Vec::new();
    let mut f = 20.0;
    while f * half <= top {
        centres.push(f);
        f *= step;
    }
    let powers: Vec<Option<f64>> = centres
        .iter()
        .map(|&f| {
            let bins = s.bins(f / half, f * half);
            let n = bins.len();
            (n > 0).then(|| s.power[bins].iter().sum::<f64>() / n as f64)
        })
        .collect();
    let loudest = powers.iter().flatten().copied().fold(0.0f64, f64::max);
    if loudest <= 0.0 {
        return None;
    }
    let db = |p: f64| (p > 0.0).then(|| 10.0 * (p / loudest).log10());
    Some(Curve {
        id: "curve.spectrum",
        label: "Level against frequency, against the loudest band",
        points: centres
            .into_iter()
            .zip(powers)
            .map(|(f, p)| (f, p.and_then(db)))
            .collect(),
        x_unit: Unit::Hertz,
        y_unit: Unit::Decibels,
        resolution: Resolution {
            window_ms: x.len() as f64 / c.rate * 1000.0,
            bin_hz: Some(s.bin_hz),
            span: Some(half - 1.0),
        },
        source: "a Hann-windowed spectrum of the whole sound from its onset, each band the mean power of its bins; a band holding no bin is absent",
    })
}

/// The unexplained map's frames in finer bands: the sound over time and frequency.
fn spectrogram(sound: &Sound, c: &Context) -> Option<Spectrogram> {
    let (lo, hi) = (SPECTROGRAM_HZ.0, SPECTROGRAM_HZ.1.min(0.49 * c.rate));
    if hi <= lo {
        return None;
    }
    let (e0, e1) = (erb_number(lo), erb_number(hi));
    let step = (e1 - e0) / SPECTROGRAM_BANDS as f64;
    let bands: Vec<(f64, f64)> = (0..SPECTROGRAM_BANDS)
        .map(|k| {
            (
                erb_hz(e0 + k as f64 * step),
                erb_hz(e0 + (k + 1) as f64 * step),
            )
        })
        .collect();
    let (frames, loudest) = frames_in(sound, c.onset, &bands);
    if frames.is_empty() || loudest <= 0.0 {
        return None;
    }
    let n = frame_len(c.rate);
    Some(Spectrogram {
        id: "curve.spectrogram",
        label: "Level over time and frequency, against the loudest cell",
        times_ms: frames.iter().map(|f| f.0).collect(),
        cells_db: frames
            .iter()
            .map(|(_, p)| {
                p.iter()
                    .map(|&v| (v > 0.0).then(|| 10.0 * (v / loudest).log10()))
                    .collect()
            })
            .collect(),
        bands_hz: bands,
        resolution: Resolution {
            window_ms: n as f64 / c.rate * 1000.0,
            bin_hz: Some(c.rate / n as f64),
            span: None,
        },
        source: "the unexplained map's frames (a Hann-windowed spectrum every quarter frame, pooled with equal weight per ERB) in bands equally spaced on the ERB-number scale (Glasberg & Moore 1990)",
    })
}
