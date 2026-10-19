//! Set 8, repeated: the same pitch twice with no gap, re-bowed, re-tongued, re-sung or
//! re-plucked, the level dipping 0, 3, 6 or 12 dB between them. Each is two notes. Control: a held
//! note with a slow 3 dB swell, which is one note.
//!
//! The violin and plucked bass are String Studio notes, the second rendered afresh (its own
//! attack) and the first ending into it; the flute and voice are additive, the second note's
//! re-articulation a dip in level and brightness with a breath of noise.

use super::*;
use crate::dsp::{Biquad, white};

pub const INSTRUMENTS: [&str; 4] = ["violin", "flute", "voice", "plucked-bass"];

const DIPS_DB: [f64; 4] = [0.0, 3.0, 6.0, 12.0];

fn range(instrument: &str) -> (f64, f64) {
    match instrument {
        "violin" => (57.0, 86.0),
        "flute" => (60.0, 93.0),
        "voice" => (55.0, 76.0),
        "plucked-bass" => (28.0, 55.0),
        other => panic!("no repeated range for {other}"),
    }
}

fn articulation(instrument: &str) -> &'static str {
    match instrument {
        "violin" => "re-bowed",
        "flute" => "re-tongued",
        "voice" => "re-sung",
        _ => "re-plucked",
    }
}

const LEVEL: f64 = 0.25;

/// Two String Studio notes at one pitch: the second rendered afresh at the end of the first, the
/// first's level falling `dip` dB over the 40 ms before the join and the second's rising from it.
fn string_pair(
    ctx: &mut Context,
    instrument: &str,
    rng: &mut Rng,
    midi: f64,
    dip: f64,
    first: f64,
    second: f64,
) -> Result<Case> {
    let rate = ctx.rate;
    let mut case = Case::new(articulation(instrument), strings::ENGINE, false);
    let (hz, detune) = centre(rng, midi);
    let plucked = instrument == "plucked-bass";
    let a = string_note(ctx, instrument, hz, 0.8, first + 0.1, 0.05)?;
    let b = string_note(
        ctx,
        instrument,
        hz,
        0.8,
        second,
        if plucked { 0.4 } else { 0.2 },
    )?;
    let g = db_to_gain(-dip);
    let join = first;
    let note1 = place_string_note(
        &mut case,
        rate,
        &a,
        0.1,
        |t| {
            if t < join - 0.04 {
                1.0
            } else if t < join {
                1.0 + (g - 1.0) * smoothstep((t - (join - 0.04)) / 0.04)
            } else {
                g * (1.0 - smoothstep((t - join) / 0.01))
            }
        },
        obj(vec![("detune_cents", detune.into())]),
    );
    let note2 = place_string_note(
        &mut case,
        rate,
        &b,
        0.1 + join,
        |t| g + (1.0 - g) * smoothstep(t / 0.04),
        obj(vec![("detune_cents", detune.into())]),
    );
    let note1 = Note {
        end: note1.end.min(note2.onset),
        ..note1
    };
    case.set_f0(note1.onset, note1.end, |_| note1.hz);
    case.set_f0(note2.onset, note2.end, |_| note2.hz);
    case.notes.push(note1);
    case.notes.push(note2);
    case.params = obj(vec![
        ("dip_db", dip.into()),
        ("articulation", articulation(instrument).into()),
        ("first_note_s", first.into()),
        ("second_note_s", second.into()),
    ]);
    Ok(case)
}

/// Two additive notes at one pitch as one line: the level and brightness dip at the join and a
/// short breath of noise marks the new tongue or voice.
#[allow(clippy::too_many_arguments)]
fn additive_pair(
    profile: &Profile,
    rate: f64,
    rng: &mut Rng,
    midi: f64,
    dip: f64,
    first: f64,
    second: f64,
    instrument: &str,
) -> Case {
    let mut case = Case::new(articulation(instrument), "additive", false);
    let (hz, detune) = centre(rng, midi);
    let g = db_to_gain(-dip);
    let join = first;
    let duration = first + second + profile.release * 1.3;
    let n = (duration * rate).round() as usize;
    let jit = jitter(rng, n, rate, 1.5);
    let level_at = |t: f64| -> f64 {
        let fall = 1.0 + (g - 1.0) * smoothstep((t - (join - 0.04)) / 0.04);
        let rise = g + (1.0 - g) * smoothstep((t - join) / 0.05);
        if t < join { fall } else { rise }
    };
    let line = Line::from_fn(rate, duration, |t| {
        let i = ((t * rate) as usize).min(n - 1);
        let amp = LEVEL
            * level_at(t)
            * attack(t, profile.attack)
            * release(t - first - second, profile.release);
        // The new articulation starts darker and brightens, like any attack.
        let fresh = smoothstep((t - join) / 0.08);
        let bright = if t < join { 0.7 } else { 0.45 + 0.25 * fresh };
        (hz * 2f64.powf(jit[i] / 1200.0), amp, amp, bright)
    });
    let onset = line
        .amp
        .iter()
        .position(|&a| a >= 0.01 * LEVEL)
        .unwrap_or(0) as f64
        / rate;
    let end = first + second + profile.release;
    additive_case(&mut case, profile, &line, rate, rng, 0.0, move |t| {
        t >= onset && t < end
    });
    // The articulation's breath: 20 ms of noise in the instrument's band at the join.
    let m = (0.02 * rate) as usize;
    let mut breath = white(rng, m);
    let (lo, hi) = profile.noise_band;
    for mut f in [
        Biquad::highpass(rate, lo, 0.707),
        Biquad::lowpass(rate, hi.min(0.45 * rate), 0.707),
    ] {
        f.run_all(&mut breath);
    }
    let rms = (breath.iter().map(|v| v * v).sum::<f64>() / m as f64)
        .sqrt()
        .max(1e-300);
    let level = LEVEL * db_to_gain(-24.0);
    let breath: Vec<f64> = (0..m)
        .map(|i| {
            let t = i as f64 / m as f64;
            breath[i] / rms * level * (std::f64::consts::PI * t).sin()
        })
        .collect();
    case.mix(rate, join - 0.01, &breath);
    case.notes.push(Note {
        onset,
        end: join,
        hz,
        params: obj(vec![("detune_cents", detune.into())]),
    });
    case.notes.push(Note {
        onset: join,
        end,
        hz,
        params: obj(vec![("detune_cents", detune.into())]),
    });
    case.params = obj(vec![
        ("dip_db", dip.into()),
        ("articulation", articulation(instrument).into()),
        ("first_note_s", first.into()),
        ("second_note_s", second.into()),
        ("articulation_noise_db_under_note", (-24.0).into()),
    ]);
    case
}

/// The control: a held note with a slow 3 dB swell, one note.
fn swell(
    ctx: &mut Context,
    instrument: &str,
    profile: Option<&Profile>,
    rng: &mut Rng,
    midi: f64,
) -> Result<Case> {
    let rate = ctx.rate;
    let length = 2.4;
    let (hz, detune) = centre(rng, midi);
    let shape = |t: f64| {
        db_to_gain(
            3.0 * (std::f64::consts::PI * (t / length).clamp(0.0, 1.0))
                .sin()
                .powi(2),
        )
    };
    let mut case = match profile {
        Some(p) => {
            let mut case = Case::new("held note with a slow swell", "additive", true);
            let duration = length + p.release * 1.3;
            let n = (duration * rate).round() as usize;
            let jit = jitter(rng, n, rate, 1.5);
            let line = Line::from_fn(rate, duration, |t| {
                let i = ((t * rate) as usize).min(n - 1);
                let amp = LEVEL * shape(t) * attack(t, p.attack) * release(t - length, p.release);
                (
                    hz * 2f64.powf(jit[i] / 1200.0),
                    amp,
                    amp,
                    0.55 + 0.1 * (shape(t) - 1.0),
                )
            });
            let onset = line
                .amp
                .iter()
                .position(|&a| a >= 0.01 * LEVEL)
                .unwrap_or(0) as f64
                / rate;
            let end = length + p.release;
            additive_case(&mut case, p, &line, rate, rng, 0.0, move |t| {
                t >= onset && t < end
            });
            case.notes.push(Note {
                onset,
                end,
                hz,
                params: obj(vec![("detune_cents", detune.into())]),
            });
            case
        }
        None => {
            let mut case = Case::new("held note with a slow swell", strings::ENGINE, true);
            let fade = if instrument == "plucked-bass" {
                0.4
            } else {
                0.25
            };
            let rendered = string_note(ctx, instrument, hz, 0.8, length, fade)?;
            let note = place_string_note(
                &mut case,
                rate,
                &rendered,
                0.1,
                shape,
                obj(vec![("detune_cents", detune.into())]),
            );
            case.notes.push(note);
            case
        }
    };
    case.params = obj(vec![("swell_db", 3.0.into()), ("note_s", length.into())]);
    Ok(case)
}

pub fn files(ctx: &mut Context, instrument: &str) -> Result<Vec<Take>> {
    let seed = seed_of(&format!("repeated-{instrument}"));
    let mut draw = Rng::stream(seed, "pairs");
    let (lo, hi) = range(instrument);
    let profile = matches!(instrument, "flute" | "voice").then(|| additive::profile(instrument));
    let mut plan = Vec::new();
    for &dip in &DIPS_DB {
        for _ in 0..3 {
            plan.push(dip);
        }
    }
    for i in (1..plan.len()).rev() {
        let j = ((draw.unit() * (i + 1) as f64) as usize).min(i);
        plan.swap(i, j);
    }
    let mut cases = Vec::new();
    for (i, &dip) in plan.iter().enumerate() {
        let midi = draw.range(lo, hi).round();
        let (first, second) = (draw.range(0.4, 1.0), draw.range(0.5, 1.2));
        let mut rng = Rng::stream(seed, &format!("case{i}"));
        cases.push(match &profile {
            Some(p) => additive_pair(p, ctx.rate, &mut rng, midi, dip, first, second, instrument),
            None => string_pair(ctx, instrument, &mut rng, midi, dip, first, second)?,
        });
    }
    for k in 0..2 {
        let midi = draw.range(lo, hi).round();
        let mut rng = Rng::stream(seed, &format!("control{k}"));
        let control = swell(ctx, instrument, profile.as_ref(), &mut rng, midi)?;
        cases.insert(4 + 6 * k, control);
    }
    Ok(vec![take(
        "repeated",
        instrument,
        1,
        ctx.rate,
        cases,
        &mut Rng::stream(seed, "gaps"),
        Json::Obj(Vec::new()),
    )])
}
