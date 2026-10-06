//! A sustained note (L5c; the plan's §2 item 11, "sustained excitation": the bow, a blown tube): the
//! onset transient until the pitch holds, pitch drift, vibrato and jitter, and the noise riding the
//! partials.
//!
//! Every reading comes from one pitch track: the fundamental followed from the onset in Hann windows
//! [`TRACK_WINDOW_MS`] long every [`TRACK_HOP_MS`], each the strongest bin within a quarter-tone of the
//! last, refined by a parabola through its log magnitudes (the guide's line track, carried over the
//! whole note). The track in cents around its median is then read as the sum the plan names: a line
//! (drift), a sinusoid between 3 and 10 Hz (vibrato: its rate, and its depth as the peak deviation)
//! and what is left (jitter, its RMS). Harmonics-to-noise is the energy on the note's harmonics
//! against the energy between them over the held part.

use super::Context;
use crate::reading::{Reading, Section, Unit};
use crate::repr::spectrum;

pub const TRACK_WINDOW_MS: f64 = 46.0;
pub const TRACK_HOP_MS: f64 = 10.0;
/// Each window's pitch is sought within this many cents of the last.
pub const TRACK_SPAN_CENTS: f64 = 50.0;
/// The pitch holds once it stays within this many cents of the held pitch…
pub const SETTLE_CENTS: f64 = 20.0;
/// …for this long, ms…
pub const SETTLE_MS: f64 = 100.0;
/// …or its mean over this long does, ms: a vibrato's period at 4 Hz.
pub const SETTLE_MEAN_MS: f64 = 250.0;
/// Vibrato is sought between these rates, Hz…
pub const VIBRATO_HZ: (f64, f64) = (3.0, 10.0);
/// …its power summed this far either side of its peak, Hz…
pub const VIBRATO_BAND_HZ: f64 = 1.0;
/// …and taken when its peak stands this many times over the range's median power.
pub const VIBRATO_PROMINENCE: f64 = 10.0;
/// The held part ends where the level falls this far under its median, dB: the release is not held.
pub const HELD_DB: f64 = 12.0;
/// A harmonic's band for harmonics-to-noise, as a share of the fundamental either side.
pub const HNR_BAND: f64 = 0.15;

/// The pitch track: `(ms from the onset, Hz, level)` for each window, following `f0`.
#[must_use]
pub fn track(c: &Context, f0: f64) -> Vec<(f64, f64, f64)> {
    let mut out = Vec::new();
    let half = TRACK_WINDOW_MS / 2.0;
    let mut centre = half;
    let mut last = f0;
    let span = 2f64.powf(TRACK_SPAN_CENTS / 1200.0);
    while c.covers(centre + half) {
        let found = c
            .window(centre - half, centre + half)
            .and_then(|w| spectrum::power_spectrum(w, c.rate, 1 << 15))
            .and_then(|s| {
                let bins = s.bins(last / span, last * span);
                let k = bins.max_by(|a, b| s.power[*a].total_cmp(&s.power[*b]))?;
                if k == 0 || k + 1 >= s.power.len() || s.power[k] <= 0.0 {
                    return None;
                }
                let (a, b, g) = (
                    s.power[k - 1].max(1e-300).ln(),
                    s.power[k].ln(),
                    s.power[k + 1].max(1e-300).ln(),
                );
                let d = a - 2.0 * b + g;
                let offset = if d < 0.0 { 0.5 * (a - g) / d } else { 0.0 };
                Some((s.hz(k) + offset * s.bin_hz, s.power[k]))
            });
        if let Some((hz, power)) = found {
            out.push((centre, hz, power));
            last = hz;
        }
        centre += TRACK_HOP_MS;
    }
    out
}

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    Some(v[v.len() / 2])
}

/// The held stretch of a track: from where the pitch holds to where the level falls [`HELD_DB`]
/// under its median.
fn held(points: &[(f64, f64, f64)]) -> Option<(usize, usize, f64)> {
    let level = median(points.iter().map(|p| p.2).collect())?;
    let floor = level * 10f64.powf(-HELD_DB / 10.0);
    let end = points
        .iter()
        .rposition(|p| p.2 >= floor)
        .map_or(points.len(), |i| i + 1);
    let pitch = median(points[..end].iter().map(|p| p.1).collect())?;
    let cents = |hz: f64| 1200.0 * (hz / pitch).log2();
    let need = ((SETTLE_MS / TRACK_HOP_MS) as usize).max(1);
    // A vibrato deeper than the tolerance never stays inside it — a calibration's 30-cent vibrato read
    // "never holds" — so the pitch also holds where its mean over [`SETTLE_MEAN_MS`], a vibrato's
    // period at 4 Hz, does.
    let half = ((SETTLE_MEAN_MS / 2.0 / TRACK_HOP_MS) as usize).max(1);
    let mut prefix = vec![0.0; end + 1];
    for (i, p) in points[..end].iter().enumerate() {
        prefix[i + 1] = prefix[i] + cents(p.1);
    }
    let mean_at = |i: usize| (prefix[i + half + 1] - prefix[i - half]) / (2 * half + 1) as f64;
    let start = (0..end).find(|&i| {
        let raw = points[i..(i + need).min(end)]
            .iter()
            .all(|p| cents(p.1).abs() <= SETTLE_CENTS);
        let smooth = i >= half
            && i + need + half < end
            && (i..i + need).all(|k| mean_at(k).abs() <= SETTLE_CENTS);
        raw || smooth
    })?;
    (end > start + 2 * need).then_some((start, end, pitch))
}

/// A track's drift per second, its modulation's rate (Hz) and peak depth where it has one, and what
/// the two leave (RMS).
pub(crate) type Modulation = (f64, Option<(f64, f64)>, f64);

/// A modulation between `range_hz` in a track `(s, value)` sampled every `hop_s` after its line is
/// removed: `(drift per s, (rate Hz, depth peak), jitter RMS)`, the jitter what the line and the
/// modulation leave. The rate is the peak of the track's
/// spectrum in that range and the depth the peak deviation of a sinusoid holding the power within
/// [`VIBRATO_BAND_HZ`] of it — a player's vibrato wanders in rate over a note, and one fitted sinusoid
/// caught little of a flute's. `None` without a peak standing [`VIBRATO_PROMINENCE`] over the range's
/// median power, or for a track too short.
pub(crate) fn modulation(
    points: &[(f64, f64)],
    hop_s: f64,
    range_hz: (f64, f64),
) -> Option<Modulation> {
    let n = points.len();
    if n < 32 {
        return None;
    }
    let nf = n as f64;
    let mt = points.iter().map(|p| p.0).sum::<f64>() / nf;
    let my = points.iter().map(|p| p.1).sum::<f64>() / nf;
    let stt: f64 = points.iter().map(|p| (p.0 - mt).powi(2)).sum();
    if stt <= 0.0 {
        return None;
    }
    let drift = points.iter().map(|p| (p.0 - mt) * (p.1 - my)).sum::<f64>() / stt;
    let residual: Vec<f64> = points
        .iter()
        .map(|p| p.1 - my - drift * (p.0 - mt))
        .collect();
    let rest = (residual.iter().map(|r| r * r).sum::<f64>() / nf).sqrt();
    let size = (4 * n).next_power_of_two();
    let mut re = vec![0.0; size];
    let mut im = vec![0.0; size];
    re[..n].copy_from_slice(&residual);
    mxm_measure::spectrum::fft(&mut re, &mut im)?;
    let rate = 1.0 / hop_s;
    let bin = rate / size as f64;
    let power: Vec<f64> = (0..size / 2)
        .map(|k| re[k] * re[k] + im[k] * im[k])
        .collect();
    let range: Vec<usize> = (0..size / 2)
        .filter(|&k| {
            let hz = k as f64 * bin;
            hz >= range_hz.0 && hz <= range_hz.1
        })
        .collect();
    let peak = *range
        .iter()
        .max_by(|a, b| power[**a].total_cmp(&power[**b]))?;
    let mut sorted: Vec<f64> = range.iter().map(|&k| power[k]).collect();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[sorted.len() / 2].max(1e-300);
    if power[peak] < VIBRATO_PROMINENCE * median {
        return Some((drift, None, rest));
    }
    // The band's power, one-sided, back to a sinusoid's peak deviation: a sinusoid of amplitude A over
    // n samples, zero-padded, holds A²·n/2 over its whole peak (Parseval), counted once here.
    let half = (VIBRATO_BAND_HZ / bin).round() as usize;
    let band: f64 = (peak.saturating_sub(half)..=(peak + half).min(size / 2 - 1))
        .map(|k| power[k])
        .sum::<f64>();
    // Parseval: a sinusoid of amplitude A over n samples, zero-padded to `size`, holds about
    // size·n·A²/4 in its positive-frequency peak.
    let depth = (4.0 * band / (size as f64 * nf)).sqrt();
    let jitter = (rest * rest - depth * depth / 2.0).max(0.0).sqrt();
    Some((drift, Some((peak as f64 * bin, depth)), jitter))
}

/// Harmonics-to-noise over the held stretch, dB: the energy within [`HNR_BAND`] of each harmonic
/// against the energy between them, up to the tenth harmonic.
fn harmonics_to_noise(c: &Context, from_ms: f64, to_ms: f64, pitch: f64) -> Option<f64> {
    let w = c.window(from_ms, to_ms)?;
    let s = spectrum::power_spectrum(w, c.rate, (w.len().next_power_of_two() * 2).max(1 << 14))?;
    let (mut on, mut off) = (0.0, 0.0);
    let top = (10.5 * pitch).min(0.45 * c.rate);
    for k in s.bins(0.5 * pitch, top) {
        let hz = s.hz(k);
        let n = (hz / pitch).round();
        if n >= 1.0 && (hz - n * pitch).abs() <= HNR_BAND * pitch {
            on += s.power[k];
        } else {
            off += s.power[k];
        }
    }
    (on > 0.0 && off > 0.0).then(|| 10.0 * (on / off).log10())
}

/// The fluctuation strength of the whole sound, vacil (ECMA-418-2 Clause 9), heard as the A/B page
/// plays it: its body loudness matched to [`super::perception::BODY_DBFS`], as the other perceptual
/// models hear it. `None` for a sound too short to read.
fn fluctuation(c: &Context) -> Option<f64> {
    let body = f64::from(crate::prep::body_rms(&c.raw, c.rate as u32));
    let gain = if body > 0.0 {
        10f64.powf(super::perception::BODY_DBFS / 20.0) / body
    } else {
        1.0
    };
    let sound = crate::sound::Sound::new(
        "fluctuation",
        c.rate as u32,
        c.raw
            .iter()
            .map(|&v| (f64::from(v) * gain) as f32)
            .collect(),
    );
    crate::perception::fluctuation::analyse(&crate::perception::hearing::pressure(&sound))?.single()
}

/// The held pitch, Hz: the median of the pitch track, following `f0`, over the held stretch. A held
/// note's tuning is read from it: the fundamental's own frequency comes from the mode analysis's short
/// early windows, which catch a vibrato at one point of its swing — a 30-cent vibrato moved a note's
/// tuning three thresholds with its mean pitch unchanged.
#[must_use]
pub fn held_pitch(c: &Context, f0: f64) -> Option<f64> {
    held(&track(c, f0)).map(|h| h.2)
}

/// The sustain section, following the fundamental `f0`; `perceive` reads its fluctuation strength
/// too, which costs as much as the rest.
#[must_use]
pub fn read(c: &Context, f0: Option<f64>, perceive: bool) -> Section {
    let src = "the fundamental tracked over the note, read as a line, a 3–10 Hz sinusoid and what is left";
    let src_hnr = "the energy within 15 % of the fundamental around each harmonic against the energy between, to the tenth harmonic";
    let ids: [(&'static str, &'static str, Unit); 9] = [
        (
            "sustain.settle",
            "Until the pitch holds",
            Unit::Milliseconds,
        ),
        (
            "sustain.drift",
            "Pitch drift over the held part, cents a second",
            Unit::Plain,
        ),
        ("sustain.vibrato_rate", "Vibrato rate", Unit::Hertz),
        ("sustain.vibrato_depth", "Vibrato depth (peak)", Unit::Cents),
        (
            "sustain.jitter",
            "Pitch jitter (what the line and the vibrato leave)",
            Unit::Cents,
        ),
        (
            "sustain.tremolo_rate",
            "Tremolo rate (the level's vibrato)",
            Unit::Hertz,
        ),
        (
            "sustain.tremolo_depth",
            "Tremolo depth (peak)",
            Unit::Decibels,
        ),
        (
            "sustain.hnr",
            "Harmonics against the noise between them",
            Unit::Decibels,
        ),
        ("sustain.held", "The held part", Unit::Milliseconds),
    ];
    let absent = |why: &'static str| {
        Section::new(
            "sustain",
            "The sustain: pitch holding, drift, vibrato and noise",
            ids.iter()
                .map(|&(id, label, unit)| {
                    Reading::absent(
                        id,
                        label,
                        unit,
                        why,
                        if id == "sustain.hnr" { src_hnr } else { src },
                    )
                })
                .collect(),
        )
    };
    let Some(f0) = f0 else {
        return absent("no fundamental");
    };
    let points = track(c, f0);
    let Some((start, end, pitch)) = held(&points) else {
        return absent("the pitch never holds");
    };
    let cents: Vec<(f64, f64)> = points[start..end]
        .iter()
        .map(|p| (p.0 / 1000.0, 1200.0 * (p.1 / pitch).log2()))
        .collect();
    let (from_ms, to_ms) = (points[start].0, points[end - 1].0);
    let v = modulation(&cents, TRACK_HOP_MS / 1000.0, VIBRATO_HZ);
    let levels: Vec<(f64, f64)> = points[start..end]
        .iter()
        .map(|p| (p.0 / 1000.0, 10.0 * p.2.max(1e-300).log10()))
        .collect();
    let t = modulation(&levels, TRACK_HOP_MS / 1000.0, VIBRATO_HZ);
    let hnr = harmonics_to_noise(c, from_ms, to_ms, pitch);
    let rate_depth =
        |m: Option<Modulation>, i: usize, j: usize, what: &'static str| match m.and_then(|x| x.1) {
            Some((rate, depth)) => [
                Reading::new(ids[i].0, ids[i].1, Some(rate), ids[i].2, src),
                Reading::new(ids[j].0, ids[j].1, Some(depth), ids[j].2, src),
            ],
            None => [
                Reading::absent(ids[i].0, ids[i].1, ids[i].2, what, src),
                Reading::absent(ids[j].0, ids[j].1, ids[j].2, what, src),
            ],
        };
    let [vibrato_rate, vibrato_depth] =
        rate_depth(v, 2, 3, "no vibrato: no peak between 3 and 10 Hz");
    let [tremolo_rate, tremolo_depth] =
        rate_depth(t, 5, 6, "no tremolo: no peak between 3 and 10 Hz");
    let readings = vec![
        Reading::new(ids[0].0, ids[0].1, Some(from_ms), ids[0].2, src),
        Reading::new(ids[1].0, ids[1].1, v.map(|x| x.0), Unit::Plain, src),
        vibrato_rate,
        vibrato_depth,
        Reading::new(ids[4].0, ids[4].1, v.map(|x| x.2), ids[4].2, src),
        tremolo_rate,
        tremolo_depth,
        Reading::new(ids[7].0, ids[7].1, hnr, ids[7].2, src_hnr),
        Reading::new(ids[8].0, ids[8].1, Some(to_ms - from_ms), ids[8].2, src),
        fluctuation_reading(c, perceive),
    ];
    Section::new(
        "sustain",
        "The sustain: pitch holding, drift, vibrato and noise",
        readings,
    )
}

/// A held note's fluctuation strength, the sustain section's one perceptual reading; absent when
/// `perceive` is off. `describe`'s second stage fills it in with this same function, so a reading made
/// in stages is the reading made at once.
#[must_use]
pub fn fluctuation_reading(c: &Context, perceive: bool) -> Reading {
    match perceive.then(|| fluctuation(c)).flatten() {
        Some(v) => Reading::new(
            "sustain.fluctuation",
            "Fluctuation strength (ECMA-418-2)",
            Some(v),
            Unit::Vacil,
            "ECMA-418-2 Clause 9, the whole sound at the page's level",
        ),
        None => Reading::absent(
            "sustain.fluctuation",
            "Fluctuation strength (ECMA-418-2)",
            Unit::Vacil,
            "not read: too short, or the perceptual models left out",
            "ECMA-418-2 Clause 9, the whole sound at the page's level",
        ),
    }
}
