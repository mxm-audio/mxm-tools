//! Set 6, re-entry: a loud note, a rest of 0.5 to 3 s that the other players' bleed fills (a quiet
//! low-passed chord, 35 to 50 dB under the loud note), then a quieter note 10 to 25 dB under it at
//! another pitch. Once with a 5 ms click 50 to 150 ms before the quieter note, once with the
//! quieter note's own soft onset (a few cycles at its pitch 30 to 40 dB down, then its proper
//! attack), and once plain. Control: the click alone in the rest, which is no note.
//!
//! The bleed is no one's note: it is in no annotation. A soft onset's note begins where its faint
//! cycles begin (where it begins to sound); its proper attack's time is in the sidecar.

use super::*;
use crate::dsp::{Biquad, white};

pub const INSTRUMENTS: [&str; 3] = ["violin", "cello", "saxophone"];

/// The loud note's sustained RMS, before the file is scaled.
const LOUD: f64 = 0.25;

fn range(instrument: &str) -> (f64, f64) {
    match instrument {
        "violin" => (57.0, 86.0),
        "cello" => (38.0, 67.0),
        "saxophone" => (49.0, 80.0),
        other => panic!("no re-entry range for {other}"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Variant {
    Plain,
    Click,
    SoftOnset,
    /// The control: the click, and no note after it.
    ClickAlone,
}

/// The bleed: a chord of three or four string notes, low-passed at 1 kHz, `level` RMS, over
/// `duration` s with 80 ms fades.
fn bleed(rng: &mut Rng, rate: f64, duration: f64, level: f64) -> (Vec<f64>, Json) {
    let profile = additive::profile(if rng.unit() < 0.5 { "viola" } else { "cello" });
    let root = rng.range(45.0, 57.0).round();
    let shape = *rng.pick(&[
        [0.0, 4.0, 7.0, 12.0],
        [0.0, 3.0, 7.0, 10.0],
        [0.0, 7.0, 12.0, 16.0],
    ]);
    let count = 3 + (rng.unit() < 0.5) as usize;
    let n = (duration * rate).round() as usize;
    let mut sum = vec![0.0; n];
    let mut pitches = Vec::new();
    for &interval in shape.iter().take(count) {
        let (hz, _) = centre(rng, root + interval);
        pitches.push(Json::from(hz));
        let jit = jitter(rng, n, rate, 3.0);
        let line = Line::from_fn(rate, duration, |t| {
            let i = ((t * rate) as usize).min(n - 1);
            let fade = smoothstep(t / 0.08) * smoothstep((duration - t) / 0.08);
            (hz * 2f64.powf(jit[i] / 1200.0), fade, fade, 0.4)
        });
        for (s, v) in sum
            .iter_mut()
            .zip(additive::render(&profile, &line, rate, rng))
        {
            *s += v;
        }
    }
    for filter in [
        Biquad::lowpass(rate, 1000.0, 0.707),
        Biquad::lowpass(rate, 1000.0, 0.707),
    ] {
        sum = crate::dsp::zero_phase(&sum, filter);
    }
    let rms = (sum.iter().map(|v| v * v).sum::<f64>() / n.max(1) as f64)
        .sqrt()
        .max(1e-300);
    for v in sum.iter_mut() {
        *v *= level / rms;
    }
    (
        sum,
        obj(vec![
            ("bleed_hz", Json::Arr(pitches)),
            ("bleed_lowpass_hz", 1000.0.into()),
        ]),
    )
}

/// A 5 ms click: a decaying burst of high-passed noise, `peak` at its loudest.
fn click(rng: &mut Rng, rate: f64, peak: f64) -> Vec<f64> {
    let n = (0.005 * rate).round() as usize;
    let mut x = white(rng, n);
    let mut f = Biquad::highpass(rate, 800.0, 0.707);
    f.run_all(&mut x);
    let top = x.iter().fold(0.0f64, |p, v| p.max(v.abs())).max(1e-300);
    (0..n)
        .map(|i| x[i] / top * peak * (-(i as f64 / rate) / 0.0012).exp())
        .collect()
}

fn rms_of(x: &[f64]) -> f64 {
    (x.iter().map(|v| v * v).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

/// One case on String Studio.
#[allow(clippy::too_many_arguments)]
fn string_case(
    ctx: &mut Context,
    instrument: &str,
    rng: &mut Rng,
    variant: Variant,
    loud_midi: f64,
    quiet_midi: f64,
    rest: f64,
    quiet_db: f64,
    bleed_db: f64,
) -> Result<Case> {
    let rate = ctx.rate;
    let kind = match variant {
        Variant::Plain => "loud, rest with bleed, quieter note",
        Variant::Click => "loud, rest with bleed, click, quieter note",
        Variant::SoftOnset => "loud, rest with bleed, quieter note with a soft onset",
        Variant::ClickAlone => "loud, rest with bleed, click alone",
    };
    let mut case = Case::new(kind, strings::ENGINE, variant == Variant::ClickAlone);
    let (hz1, d1) = centre(rng, loud_midi);
    let loud = string_note(ctx, instrument, hz1, 0.85, 1.2, 0.25)?;
    let steady = |r: &strings::Rendered, hold: f64| -> f64 {
        let (a, b) = (
            ((r.onset + 0.15) * rate) as usize,
            ((hold - 0.05) * rate) as usize,
        );
        rms_of(&r.audio[a..b])
    };
    let s1 = LOUD / steady(&loud, 1.2).max(1e-12);
    let note1 = place_string_note(
        &mut case,
        rate,
        &loud,
        0.1,
        |_| s1,
        obj(vec![
            ("detune_cents", d1.into()),
            ("role", "loud note".into()),
        ]),
    );
    let rest_start = note1.end;
    let quiet_at = rest_start + rest;
    let mut params = obj(vec![
        ("rest_s", rest.into()),
        ("bleed_db_under_loud", bleed_db.into()),
        ("quieter_note_db_under_loud", quiet_db.into()),
    ]);
    let (chord, chord_params) = bleed(rng, rate, rest + 0.4, LOUD * db_to_gain(bleed_db));
    case.mix(rate, rest_start - 0.2, &chord);
    if let Json::Obj(p) = chord_params {
        for (k, v) in p {
            params.push(&k, v);
        }
    }
    case.notes.push(note1);
    if matches!(variant, Variant::Click | Variant::ClickAlone) {
        let before = rng.range(0.05, 0.15);
        let peak = LOUD * db_to_gain(quiet_db) * rng.range(0.7, 1.6);
        let c = click(rng, rate, peak);
        case.mix(rate, quiet_at - before, &c);
        params.push("click_ms_before_note", before * 1000.0);
        params.push(
            "click_peak_db_under_loud_rms",
            crate::dsp::gain_to_db(peak / LOUD),
        );
    }
    if variant == Variant::ClickAlone {
        // The rest goes on past where the note would have been.
        case.audio.resize(
            case.audio.len().max(((quiet_at + 1.0) * rate) as usize),
            0.0,
        );
        case.params = params;
        return Ok(case);
    }
    let (hz2, d2) = centre(rng, quiet_midi);
    let quiet = string_note(ctx, instrument, hz2, 0.6, 1.1, 0.25)?;
    let s2 = LOUD * db_to_gain(quiet_db) / steady(&quiet, 1.1).max(1e-12);
    if variant == Variant::SoftOnset {
        // A few cycles of the note's own steady sound, 30 to 40 dB down, just before its attack.
        let cycles = (3.0 + rng.unit() * 4.0).floor();
        let under = -rng.range(30.0, 40.0);
        let length = cycles / quiet.measured_hz;
        let from = ((quiet.onset + 0.3) * rate) as usize;
        let n = (length * rate).round() as usize;
        let faint: Vec<f64> = (0..n)
            .map(|i| {
                let t = i as f64 / rate;
                quiet.audio[from + i] * s2 * db_to_gain(under) * smoothstep(t / (0.5 * length))
            })
            .collect();
        case.mix(rate, quiet_at - length, &faint);
        let note2 = place_string_note(
            &mut case,
            rate,
            &quiet,
            quiet_at,
            |_| s2,
            obj(vec![
                ("detune_cents", d2.into()),
                ("role", "quieter note".into()),
                ("attack_at_s", quiet_at.into()),
            ]),
        );
        case.set_f0(quiet_at - length, quiet_at, |_| note2.hz);
        params.push("soft_onset_cycles", cycles);
        params.push("soft_onset_db_under_note", under);
        case.notes.push(Note {
            onset: quiet_at - length,
            ..note2
        });
    } else {
        let note2 = place_string_note(
            &mut case,
            rate,
            &quiet,
            quiet_at,
            |_| s2,
            obj(vec![
                ("detune_cents", d2.into()),
                ("role", "quieter note".into()),
            ]),
        );
        case.notes.push(note2);
    }
    case.params = params;
    Ok(case)
}

/// One case, additive.
#[allow(clippy::too_many_arguments)]
fn additive_case_of(
    profile: &Profile,
    rate: f64,
    rng: &mut Rng,
    variant: Variant,
    loud_midi: f64,
    quiet_midi: f64,
    rest: f64,
    quiet_db: f64,
    bleed_db: f64,
) -> Case {
    let kind = match variant {
        Variant::Plain => "loud, rest with bleed, quieter note",
        Variant::Click => "loud, rest with bleed, click, quieter note",
        Variant::SoftOnset => "loud, rest with bleed, quieter note with a soft onset",
        Variant::ClickAlone => "loud, rest with bleed, click alone",
    };
    let mut case = Case::new(kind, "additive", variant == Variant::ClickAlone);
    let note = |rng: &mut Rng,
                midi: f64,
                length: f64,
                level: f64,
                pre: Option<(f64, f64)>|
     -> (Vec<f64>, Note, Vec<f64>, f64) {
        let (hz, detune) = centre(rng, midi);
        // A soft onset: `cycles` at `under` dB before the proper attack.
        let lead = pre.map_or(0.0, |(cycles, _)| cycles / hz);
        let duration = lead + length + profile.release * 1.3;
        let n = (duration * rate).round() as usize;
        let jit = jitter(rng, n, rate, 1.5);
        let line = Line::from_fn(rate, duration, |t| {
            let i = ((t * rate) as usize).min(n - 1);
            let faint = pre.map_or(0.0, |(_, under)| {
                db_to_gain(under)
                    * smoothstep(t / (0.5 * lead.max(1e-9)))
                    * (t < lead + profile.attack) as u8 as f64
            });
            let body = attack(t - lead, profile.attack);
            let amp = level * faint.max(body) * release(t - lead - length, profile.release);
            (
                hz * 2f64.powf(jit[i] / 1200.0),
                amp,
                amp,
                brightness(crate::dsp::gain_to_db(level / LOUD)),
            )
        });
        let onset = if pre.is_some() {
            0.0
        } else {
            line.amp
                .iter()
                .position(|&a| a >= 0.01 * level)
                .unwrap_or(0) as f64
                / rate
        };
        let audio = additive::render(profile, &line, rate, rng);
        let end = lead + length + profile.release;
        (
            audio,
            Note {
                onset,
                end,
                hz,
                params: obj(vec![
                    ("detune_cents", detune.into()),
                    ("attack_at_s", lead.into()),
                ]),
            },
            line.hz,
            lead,
        )
    };
    let loud_level = LOUD * 1.4;
    let (a1, n1, hz1, _) = note(rng, loud_midi, 1.2, loud_level, None);
    case.mix(rate, 0.1, &a1);
    case.set_f0(0.1 + n1.onset, 0.1 + n1.end, |t| {
        hz1[(((t - 0.1) * rate) as usize).min(hz1.len() - 1)]
    });
    let rest_start = 0.1 + n1.end;
    let quiet_at = rest_start + rest;
    let mut params = obj(vec![
        ("rest_s", rest.into()),
        ("bleed_db_under_loud", bleed_db.into()),
        ("quieter_note_db_under_loud", quiet_db.into()),
    ]);
    let (chord, chord_params) = bleed(rng, rate, rest + 0.4, LOUD * db_to_gain(bleed_db));
    case.mix(rate, rest_start - 0.2, &chord);
    if let Json::Obj(p) = chord_params {
        for (k, v) in p {
            params.push(&k, v);
        }
    }
    case.notes.push(Note {
        onset: 0.1 + n1.onset,
        end: 0.1 + n1.end,
        ..n1
    });
    if matches!(variant, Variant::Click | Variant::ClickAlone) {
        let before = rng.range(0.05, 0.15);
        let peak = LOUD * db_to_gain(quiet_db) * rng.range(0.7, 1.6);
        let c = click(rng, rate, peak);
        case.mix(rate, quiet_at - before, &c);
        params.push("click_ms_before_note", before * 1000.0);
        params.push(
            "click_peak_db_under_loud_rms",
            crate::dsp::gain_to_db(peak / LOUD),
        );
    }
    if variant == Variant::ClickAlone {
        case.audio.resize(
            case.audio.len().max(((quiet_at + 1.0) * rate) as usize),
            0.0,
        );
        case.params = params;
        return case;
    }
    let pre = (variant == Variant::SoftOnset)
        .then(|| ((3.0 + rng.unit() * 4.0).floor(), -rng.range(30.0, 40.0)));
    let (a2, n2, hz2, lead) = note(rng, quiet_midi, 1.1, loud_level * db_to_gain(quiet_db), pre);
    // The proper attack lands at `quiet_at`; a soft onset's cycles come before it.
    let at = quiet_at - lead;
    case.mix(rate, at, &a2);
    case.set_f0(at + n2.onset, at + n2.end, |t| {
        hz2[(((t - at) * rate) as usize).min(hz2.len() - 1)]
    });
    if let Some((cycles, under)) = pre {
        params.push("soft_onset_cycles", cycles);
        params.push("soft_onset_db_under_note", under);
    }
    let mut n2 = Note {
        onset: at + n2.onset,
        end: at + n2.end,
        ..n2
    };
    n2.params.push("attack_at_s", quiet_at);
    case.notes.push(n2);
    case.params = params;
    case
}

pub fn files(ctx: &mut Context, instrument: &str) -> Result<Vec<Take>> {
    let seed = seed_of(&format!("re-entry-{instrument}"));
    let mut draw = Rng::stream(seed, "cases");
    let (lo, hi) = range(instrument);
    let mut variants = Vec::new();
    for v in [Variant::Plain, Variant::Click, Variant::SoftOnset] {
        for _ in 0..4 {
            variants.push(v);
        }
    }
    variants.push(Variant::ClickAlone);
    variants.push(Variant::ClickAlone);
    for i in (1..variants.len()).rev() {
        let j = ((draw.unit() * (i + 1) as f64) as usize).min(i);
        variants.swap(i, j);
    }
    let profile = (instrument == "saxophone").then(|| additive::profile(instrument));
    let mut cases = Vec::new();
    for (i, &variant) in variants.iter().enumerate() {
        let loud_midi = draw.range(lo, hi).round();
        let mut quiet_midi = draw.range(lo, hi).round();
        if quiet_midi == loud_midi {
            quiet_midi += if quiet_midi < hi { 2.0 } else { -2.0 };
        }
        let rest = draw.range(0.5, 3.0);
        let quiet_db = -draw.range(10.0, 25.0);
        let bleed_db = -draw.range(35.0, 50.0);
        let mut rng = Rng::stream(seed, &format!("case{i}"));
        let mut case = match &profile {
            Some(p) => additive_case_of(
                p, ctx.rate, &mut rng, variant, loud_midi, quiet_midi, rest, quiet_db, bleed_db,
            ),
            None => string_case(
                ctx, instrument, &mut rng, variant, loud_midi, quiet_midi, rest, quiet_db, bleed_db,
            )?,
        };
        case.params.push("variant", format!("{variant:?}"));
        cases.push(case);
    }
    Ok(vec![take(
        "re-entry",
        instrument,
        1,
        ctx.rate,
        cases,
        &mut Rng::stream(seed, "gaps"),
        Json::Obj(Vec::new()),
    )])
}
