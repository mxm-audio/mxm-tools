//! `respond`: a response read against the stimulus it answers (the plan's §2 items 9 and 10).
//!
//! The stimulus is rendered again from its sidecar (`stimulus::Stimulus::parse` checks the
//! fingerprint), so every reading knows what was played. An impulse, a sweep or noise is deconvolved
//! into an impulse response — the response's spectrum divided by the stimulus's, regularised where the
//! stimulus holds no energy — and read as one (`parts::impulse`): for Farina's sweep, the harmonic
//! distortion's responses fall before the linear one and wrap to the end of the circular result, out
//! of the window read. Noise is also read for a moving delay, and is deconvolved only where its
//! coherence says the system is linear and time-invariant. The other kinds are read by
//! `parts::effect`; a note by `describe` as a synth voice, released where the sidecar says. A stereo response is also read for width,
//! and two responses driven left and right make the true-stereo matrix.

use crate::describe::{Options, describe_with};
use crate::family::{Detected, Family};
use crate::parts::{effect, impulse};
use crate::reading::{Reading, Report, Section, Table, Unit};
use crate::sound::Sound;
use crate::stimulus::{Channel, Kind, Stimulus};

/// Noise is deconvolved into an impulse response only when its coherence reaches this.
pub const COHERENT: f64 = 0.9;
/// The deconvolution's regularisation, against the stimulus's largest power: where the stimulus holds
/// less than this, the response's spectrum is not divided up.
pub const REGULARISE: f64 = 1e-6;

/// Deconvolution keeps this long before lag zero, s: a band-limited impulse rings on both sides of its
/// peak, and cutting the ringing before it away took 0.7 dB off a flat response.
pub const BEFORE_S: f64 = 0.005;

/// The impulse response of `y` to `x`: `Y·X̄ / (|X|² + ε)`, circular over at least both lengths, from
/// `before` samples ahead of lag zero (wrapped from the end) to `len` samples after it.
#[must_use]
pub fn deconvolve(x: &[f64], y: &[f64], before: usize, len: usize) -> Option<Vec<f64>> {
    let n = (x.len() + y.len()).next_power_of_two();
    let mut xr = vec![0.0; n];
    let mut xi = vec![0.0; n];
    let mut yr = vec![0.0; n];
    let mut yi = vec![0.0; n];
    xr[..x.len()].copy_from_slice(x);
    yr[..y.len()].copy_from_slice(y);
    mxm_measure::spectrum::fft(&mut xr, &mut xi)?;
    mxm_measure::spectrum::fft(&mut yr, &mut yi)?;
    let top = (0..n)
        .map(|k| xr[k] * xr[k] + xi[k] * xi[k])
        .fold(0.0, f64::max);
    if top <= 0.0 {
        return None;
    }
    let eps = REGULARISE * top;
    let mut hr = vec![0.0; n];
    let mut hi = vec![0.0; n];
    for k in 0..n {
        let d = xr[k] * xr[k] + xi[k] * xi[k] + eps;
        hr[k] = (yr[k] * xr[k] + yi[k] * xi[k]) / d;
        hi[k] = (yi[k] * xr[k] - yr[k] * xi[k]) / d;
    }
    mxm_measure::spectrum::ifft(&mut hr, &mut hi)?;
    let before = before.min(n / 2);
    let mut out = hr[n - before..].to_vec();
    out.extend_from_slice(&hr[..len.min(n / 2)]);
    Some(out)
}

/// How long an impulse response read from a stimulus is: to the end of the response for an impulse,
/// the tail (and 100 ms) for a sweep or noise — never less than 300 ms, the space analysis's floor.
fn ir_len(s: &Stimulus, response_len: usize) -> usize {
    let rate = f64::from(s.rate);
    let len = match s.kind {
        Kind::Impulse => response_len.saturating_sub(s.start()),
        _ => ((s.tail_s + 0.1) * rate) as usize,
    };
    len.max((0.3 * rate) as usize)
}

/// The band a stimulus excites, for its frequency response.
fn band(s: &Stimulus) -> (f64, f64) {
    let top = (0.45 * f64::from(s.rate)).min(20_000.0);
    match s.kind {
        Kind::Sweep { from_hz, to_hz } => (from_hz.max(10.0), to_hz.min(top)),
        _ => (20.0, top),
    }
}

/// The response's channels as `f64`, checked.
fn prepare(s: &Stimulus, channels: &[Vec<f32>], rate: u32) -> Result<Vec<Vec<f64>>, String> {
    if rate != s.rate {
        return Err(format!(
            "the response is at {rate} Hz and the stimulus at {} Hz: render it at the stimulus's rate",
            s.rate
        ));
    }
    if channels.is_empty() || channels.len() > 2 || channels.iter().any(Vec::is_empty) {
        return Err("a response has one or two channels, and samples".into());
    }
    if channels.iter().flatten().any(|v| !v.is_finite()) {
        return Err(
            "the response holds a sample that is not a number: a DSP failure is not measured"
                .into(),
        );
    }
    Ok(channels
        .iter()
        .map(|c| c.iter().map(|&v| f64::from(v)).collect())
        .collect())
}

/// Reads one response against its stimulus.
///
/// # Errors
/// A response at another rate, with no samples or more than two channels, or holding a non-finite
/// sample.
pub fn respond(
    s: &Stimulus,
    channels: &[Vec<f32>],
    rate: u32,
    name: &str,
) -> Result<Report, String> {
    let ys = prepare(s, channels, rate)?;
    let mono = effect::mean_channel(&ys);
    let r = f64::from(rate);
    let mut notes: Vec<String> = s
        .settings
        .iter()
        .map(|(k, v)| format!("set: {k} = {v}"))
        .collect();
    let how = format!("declared by its stimulus sidecar ({})", s.kind.name());
    if let Kind::Note { key, velocity } = s.kind {
        let sound = Sound::new(name, rate, mono.iter().map(|&v| v as f32).collect());
        let mut report = describe_with(
            &sound,
            &Options {
                family: Some(Family::Voice),
                expect_hz: s.note_hz(),
                note_off_s: Some(s.lead_s + s.seconds),
                ..Options::default()
            },
        );
        report.family.how = how;
        notes.push(format!(
            "the note: key {key} at velocity {velocity}, on at {:.3} s and off {:.3} s later",
            s.lead_s, s.seconds
        ));
        report.notes.splice(0..0, notes);
        return Ok(report);
    }
    let mut sections = Vec::new();
    let driven = s.render();
    let impulse_like = matches!(s.kind, Kind::Impulse | Kind::Sweep { .. });
    let coherent = match s.kind {
        Kind::Noise { .. } => {
            let c = effect::coherence(&driven, &mono, r);
            if c.is_some_and(|c| c < COHERENT) {
                notes.push(format!(
                    "the coherence with the noise is {:.2}: the system is not linear and time-invariant, so no impulse response is read from it",
                    c.unwrap_or(0.0)
                ));
            }
            c.is_some_and(|c| c >= COHERENT)
        }
        _ => false,
    };
    let family = if impulse_like || coherent {
        Family::Impulse
    } else {
        Family::Effect
    };
    if impulse_like || coherent {
        let len = ir_len(s, mono.len());
        let before = (BEFORE_S * r) as usize;
        let irs: Vec<Vec<f64>> = ys
            .iter()
            .filter_map(|y| deconvolve(&driven, y, before, len))
            .collect();
        let (mut read, more) = impulse::read(&irs, r, band(s), before);
        sections.append(&mut read);
        notes.extend(more);
    }
    if matches!(s.kind, Kind::Noise { .. }) {
        sections.push(effect::noise_section(&ys, s));
    }
    if let Some(section) = effect::section_for(&mono, s) {
        sections.push(section);
    }
    if let Some(section) = effect::stereo_section(&ys, s.start()) {
        sections.push(section);
    }
    Ok(Report {
        name: name.to_string(),
        rate,
        duration_s: mono.len() as f64 / r,
        family: Detected { family, how },
        onset_s: Some(s.lead_s),
        sections,
        notes,
    })
}

/// The true-stereo matrix from two responses, one to a stimulus driving the left input and one to the
/// same stimulus driving the right, each read on both outputs: every path's energy against the
/// strongest's. `None` unless both are stereo responses to impulse-like stimuli, one left and one
/// right.
#[must_use]
pub fn matrix(left: (&Stimulus, &[Vec<f32>]), right: (&Stimulus, &[Vec<f32>])) -> Option<Section> {
    let paths = |(s, ch): (&Stimulus, &[Vec<f32>]), side: Channel| -> Option<[f64; 2]> {
        if s.channel != side || ch.len() != 2 {
            return None;
        }
        if !matches!(
            s.kind,
            Kind::Impulse | Kind::Sweep { .. } | Kind::Noise { .. }
        ) {
            return None;
        }
        let ys = prepare(s, ch, s.rate).ok()?;
        let x = s.render();
        let len = ir_len(s, ys[0].len());
        let before = (BEFORE_S * f64::from(s.rate)) as usize;
        let e = |y: &[f64]| {
            deconvolve(&x, y, before, len).map(|h| h.iter().map(|v| v * v).sum::<f64>())
        };
        Some([e(&ys[0])?, e(&ys[1])?])
    };
    let [ll, lr] = paths(left, Channel::Left)?;
    let [rl, rr] = paths(right, Channel::Right)?;
    let top = ll.max(lr).max(rl).max(rr);
    if top <= 0.0 {
        return None;
    }
    let db = |e: f64| (e > 0.0).then(|| 10.0 * (e / top).log10());
    let ratio = |a: f64, b: f64| (a > 0.0 && b > 0.0).then(|| 10.0 * (a / b).log10());
    let src = "each input driven alone, each output's impulse response energy (deconvolved) against the strongest path's";
    let mut section = Section::new(
        "stereo",
        "The true-stereo matrix",
        vec![
            Reading::new(
                "stereo.cross_left",
                "Left input into the right output, against into the left",
                ratio(lr, ll),
                Unit::Decibels,
                src,
            ),
            Reading::new(
                "stereo.cross_right",
                "Right input into the left output, against into the right",
                ratio(rl, rr),
                Unit::Decibels,
                src,
            ),
            Reading::new(
                "stereo.inputs",
                "Left input's paths against the right input's",
                ratio(ll + lr, rl + rr),
                Unit::Decibels,
                src,
            ),
        ],
    );
    section.tables.push(Table {
        id: "stereo.matrix",
        title: "Paths: rows the input, columns the output, against the strongest".into(),
        columns: vec![
            ("Into left", Unit::Decibels),
            ("Into right", Unit::Decibels),
        ],
        rows: vec![vec![db(ll), db(lr)], vec![db(rl), db(rr)]],
        window_ms: None,
        source: src,
    });
    Some(section)
}

/// The readings worth following across settings.
pub const AGAINST_SETTINGS: [&str; 25] = [
    "response.gain",
    "response.peak_hz",
    "response.peak_db",
    "response.q",
    "response.low_edge",
    "response.high_edge",
    "effect.line_hz",
    "effect.line_level",
    "effect.gain",
    "effect.thd",
    "effect.h2",
    "effect.h3",
    "effect.threshold",
    "effect.ratio",
    "effect.attack",
    "effect.release",
    "effect.mod_rate",
    "effect.mod_depth",
    "delay.time",
    "delay.feedback",
    "voice.peak_hz",
    "voice.peak_db",
    "voice.line_hz",
    "voice.aliasing",
    "voice.zipper",
];

/// Readings against a setting that differs across responses (a filter's peak against its cutoff, a
/// line on silence against the resonance): one row per response in the setting's order, one column per
/// reading any of them holds. `None` when no numeric setting differs.
#[must_use]
pub fn against_settings(responses: &[(&Stimulus, &Report)]) -> Option<Section> {
    let names: Vec<&str> = responses
        .iter()
        .flat_map(|(s, _)| s.settings.iter().map(|(k, _)| k.as_str()))
        .collect();
    let setting = |s: &Stimulus, name: &str| {
        s.settings
            .iter()
            .find(|(k, _)| k == name)
            .and_then(|(_, v)| v.parse::<f64>().ok())
    };
    let name = names.iter().find(|n| {
        let values: Vec<Option<f64>> = responses.iter().map(|(s, _)| setting(s, n)).collect();
        values.iter().all(Option::is_some) && values.windows(2).any(|w| w[0] != w[1])
    })?;
    let first = |r: &Report, id: &str| r.find(id, None).cloned();
    let ids: Vec<(&'static str, Unit, &'static str)> = AGAINST_SETTINGS
        .iter()
        .filter_map(|id| {
            let reading: Reading = responses
                .iter()
                .find_map(|(_, r)| first(r, id).filter(|x| x.value.is_some()))?;
            Some((reading.id, reading.unit, reading.label))
        })
        .collect();
    let mut rows: Vec<(f64, Vec<Option<f64>>)> = responses
        .iter()
        .map(|(s, r)| {
            let v = setting(s, name).unwrap_or(f64::NAN);
            let mut row = vec![Some(v)];
            row.extend(
                ids.iter()
                    .map(|(id, _, _)| first(r, id).and_then(|x| x.value)),
            );
            (v, row)
        })
        .collect();
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut columns: Vec<(&'static str, Unit)> = vec![("The setting", Unit::Plain)];
    columns.extend(ids.iter().map(|(_, unit, label)| (*label, *unit)));
    let mut section = Section::new("settings", "Against the setting", Vec::new());
    section.tables.push(Table {
        id: "respond.settings",
        title: format!("Readings against `{name}`"),
        columns,
        rows: rows.into_iter().map(|r| r.1).collect(),
        window_ms: None,
        source: "each response's own readings, in the order of the setting",
    });
    Some(section)
}
