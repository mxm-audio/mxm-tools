//! Set 1, legato: two notes joined with no new attack, the change taking 40 to 250 ms.
//!
//! Shapes: (a) a smooth glide; (b) a glide past the target by 30 to 80 cents that settles; (c) a
//! bow change (a breath for the voice): the tone drops 6 to 20 dB and 20 to 60 ms of noise with no
//! clear pitch lie between the notes; (d) any of them with the second note 6 to 15 dB quieter.
//! Intervals of 1, 2, 3, 5 and 7 semitones, up and down; notes of 0.3 to 1.2 s. Files 01 and 02
//! are additive (a, b, d, and the controls: a held note with a 6 Hz vibrato of ±50 cents, a bend
//! out by a semitone and back within 300 ms); file 03 holds the changes (c), on String Studio for
//! the strings, whose notes each hold one pitch, and additive for the voice.

use super::*;
use crate::dsp::{Biquad, white};

const TRANSITIONS_MS: [f64; 6] = [40.0, 80.0, 120.0, 160.0, 200.0, 250.0];
const INTERVALS: [i32; 10] = [1, -1, 2, -2, 3, -3, 5, -5, 7, -7];

pub const INSTRUMENTS: [&str; 4] = ["cello", "viola", "violin", "voice"];

/// The range a pair's notes stay in, MIDI.
fn range(instrument: &str) -> (f64, f64) {
    match instrument {
        "cello" => (41.0, 67.0),
        "viola" => (49.0, 74.0),
        "violin" => (57.0, 86.0),
        "voice" => (55.0, 76.0),
        other => panic!("no legato range for {other}"),
    }
}

#[derive(Clone, Copy, Debug)]
enum Shape {
    Smooth,
    Overshoot { cents: f64, settle: f64 },
    Change { dip_db: f64, gap: f64 },
}

#[derive(Clone, Copy, Debug)]
struct Pair {
    midi: f64,
    interval: i32,
    transition: f64,
    shape: Shape,
    second_db: f64,
    first: f64,
    second: f64,
}

fn draw_pair(
    rng: &mut Rng,
    instrument: &str,
    interval: i32,
    transition: f64,
    shape: Shape,
    second_db: f64,
) -> Pair {
    let (lo, hi) = range(instrument);
    let (lo, hi) = if interval > 0 {
        (lo, hi - interval as f64)
    } else {
        (lo - interval as f64, hi)
    };
    Pair {
        midi: rng.range(lo, hi).round(),
        interval,
        transition,
        shape,
        second_db,
        first: rng.range(0.3, 1.2),
        second: rng.range(0.3, 1.2),
    }
}

fn pair_params(p: &Pair) -> Json {
    let mut params = obj(vec![
        ("interval_semitones", Json::from(p.interval)),
        ("transition_ms", (p.transition * 1000.0).into()),
        ("second_note_db", p.second_db.into()),
        ("first_note_s", p.first.into()),
        ("second_note_s", p.second.into()),
    ]);
    match p.shape {
        Shape::Smooth => params.push("shape", "smooth glide"),
        Shape::Overshoot { cents, settle } => {
            params.push("shape", "glide with overshoot");
            params.push("overshoot_cents", cents);
            params.push("settle_ms", settle * 1000.0);
        }
        Shape::Change { dip_db, gap } => {
            params.push("shape", "bow change");
            params.push("dip_db", dip_db);
            params.push("noise_ms", gap * 1000.0);
        }
    }
    params
}

/// The first index past `from` where `amp` reaches `fraction` of `level`, s; and the last.
fn sounding_span(amp: &[f64], rate: f64, level: f64) -> (f64, f64) {
    let threshold = 0.01 * level;
    let first = amp.iter().position(|&a| a >= threshold).unwrap_or(0);
    let last = amp.iter().rposition(|&a| a >= threshold).unwrap_or(0);
    (first as f64 / rate, last as f64 / rate)
}

/// An additive pair: one line from the first note's attack through the second's release.
fn additive_pair(profile: &Profile, p: &Pair, rate: f64, rng: &mut Rng, kind: &str) -> Case {
    let mut case = Case::new(kind, "additive", false);
    let (hz1, detune1) = centre(rng, p.midi);
    let (hz2, detune2) = centre(rng, p.midi + p.interval as f64);
    let target = cents(hz1, hz2);
    let level1 = db_to_gain(-6.0 + rng.range(-1.5, 1.5));
    let level2 = level1 * db_to_gain(p.second_db);
    let glide = p.first;
    let off = glide + p.second;
    let duration = off + profile.release * 1.3;
    let n = (duration * rate).round() as usize;
    let jit = jitter(rng, n, rate, 2.0);
    let drift = wander(rng, n, rate, 0.4);
    let (gap_start, gap_end) = match p.shape {
        Shape::Change { gap, .. } => {
            let start = glide + (p.transition - gap) / 2.0;
            (start, start + gap)
        }
        _ => (f64::NAN, f64::NAN),
    };
    let curve = |t: f64| -> f64 {
        let x = (t - glide) / p.transition;
        match p.shape {
            Shape::Smooth => target * smoothstep(x),
            Shape::Overshoot { cents, settle } => {
                let over = target + target.signum() * cents;
                if t < glide + p.transition {
                    over * smoothstep(x)
                } else {
                    over - (over - target) * smoothstep((t - glide - p.transition) / settle)
                }
            }
            Shape::Change { .. } => {
                if t < (gap_start + gap_end) / 2.0 {
                    0.0
                } else {
                    target
                }
            }
        }
    };
    let dip = match p.shape {
        Shape::Change { dip_db, .. } => db_to_gain(-dip_db),
        _ => 1.0,
    };
    let noise_in_gap = db_to_gain(-6.0 - profile.noise_db);
    let tone = |t: f64| -> (f64, f64) {
        let held = if t < off {
            match p.shape {
                Shape::Change { .. } => {
                    if t < glide {
                        level1
                    } else if t < gap_start {
                        level1
                            * (1.0
                                + (dip - 1.0)
                                    * smoothstep((t - glide) / (gap_start - glide).max(1e-9)))
                    } else if t < gap_end {
                        let edge = 0.005;
                        let fade_out = 1.0 - smoothstep((t - gap_start) / edge);
                        let fade_in = smoothstep((t - (gap_end - edge)) / edge);
                        level1 * dip * fade_out + level2 * dip * fade_in
                    } else {
                        let rise = (glide + p.transition - gap_end).max(1e-9);
                        level2 * (dip + (1.0 - dip) * smoothstep((t - gap_end) / rise))
                    }
                }
                _ => level1 + (level2 - level1) * smoothstep((t - glide) / p.transition),
            }
        } else {
            level2 * release(t - off, profile.release)
        };
        let amp = held * attack(t, profile.attack);
        let noise = if t >= gap_start && t < gap_end {
            let edge = 0.003;
            let shape = smoothstep((t - gap_start) / edge)
                * (1.0 - smoothstep((t - (gap_end - edge)) / edge));
            amp + level1 * dip * noise_in_gap * shape
        } else {
            amp
        };
        (amp, noise)
    };
    let line_rate = rate;
    let mut line = Line::from_fn(rate, duration, |t| {
        let i = ((t * line_rate) as usize).min(n - 1);
        let hz = hz1 * 2f64.powf((curve(t) + jit[i]) / 1200.0);
        let (amp, noise) = tone(t);
        let level_db =
            crate::dsp::gain_to_db(if t < glide { level1 } else { level2 } / db_to_gain(-6.0));
        (hz, amp * drift[i], noise * drift[i], brightness(level_db))
    });
    line.harmonic_db.clear();
    let (onset, _) = sounding_span(&line.amp, rate, level1);
    let end = off + profile.release;
    let sounding = move |t: f64| t >= onset && t < end && !(t >= gap_start && t < gap_end);
    additive_case(&mut case, profile, &line, rate, rng, 0.0, sounding);
    let (first_end, second_onset) = match p.shape {
        Shape::Change { .. } => (gap_start, gap_end),
        _ => (glide, glide),
    };
    case.notes.push(Note {
        onset,
        end: first_end,
        hz: hz1,
        params: obj(vec![
            ("detune_cents", detune1.into()),
            ("level_db", crate::dsp::gain_to_db(level1).into()),
        ]),
    });
    case.notes.push(Note {
        onset: second_onset,
        end,
        hz: hz2,
        params: obj(vec![
            ("detune_cents", detune2.into()),
            ("level_db", crate::dsp::gain_to_db(level2).into()),
        ]),
    });
    case.params = pair_params(p);
    case
}

/// A bow change on String Studio: two notes, each rendered alone at its pitch, the first fading
/// by the dip into the gap, the second rising out of it, bow noise between.
fn string_change(ctx: &mut Context, instrument: &str, p: &Pair, rng: &mut Rng) -> Result<Case> {
    let rate = ctx.rate;
    let mut case = Case::new("legato", strings::ENGINE, false);
    let (hz1, detune1) = centre(rng, p.midi);
    let (hz2, detune2) = centre(rng, p.midi + p.interval as f64);
    let Shape::Change { dip_db, gap } = p.shape else {
        unreachable!("a String Studio pair is a bow change")
    };
    let pre = (p.transition - gap) / 2.0;
    let dip = db_to_gain(-dip_db);
    let level2 = db_to_gain(p.second_db);
    let velocity2 = (0.8 * db_to_gain(p.second_db / 2.0)).clamp(0.3, 1.0);

    // The first note's own fade is past the gap: the dip's gain ends it.
    let first = string_note(ctx, instrument, hz1, 0.8, p.first + pre + 0.1, 0.1)?;
    // Times from the first note's onset, which sits at `T0` in the case.
    const T0: f64 = 0.1;
    let gap_start = p.first + pre;
    let fade_from = p.first;
    let note1 = place_string_note(
        &mut case,
        rate,
        &first,
        T0,
        |t| {
            if t < fade_from {
                1.0
            } else if t < gap_start {
                1.0 + (dip - 1.0) * smoothstep((t - fade_from) / pre.max(1e-9))
            } else {
                dip * (1.0 - smoothstep((t - gap_start) / 0.005))
            }
        },
        obj(vec![("detune_cents", detune1.into())]),
    );
    let gap_end = gap_start + gap;
    let second = string_note(ctx, instrument, hz2, velocity2, p.second, 0.2)?;
    let rise = pre.max(0.01);
    let note2 = place_string_note(
        &mut case,
        rate,
        &second,
        T0 + gap_end,
        |t| level2 * (dip + (1.0 - dip) * smoothstep(t / rise)),
        obj(vec![("detune_cents", detune2.into())]),
    );

    // Bow noise in the gap: band-limited noise with a rough, fast-moving level, 6 dB under the
    // first note's level at the gap.
    let level_at_gap = {
        let a = ((T0 + gap_start - 0.03) * rate) as usize;
        let b = ((T0 + gap_start) * rate) as usize;
        let rms =
            (case.audio[a..b].iter().map(|v| v * v).sum::<f64>() / (b - a).max(1) as f64).sqrt();
        rms * db_to_gain(-6.0)
    };
    let n = (gap * rate).round() as usize;
    let mut noise = white(rng, n);
    for mut filter in [
        Biquad::highpass(rate, 1500.0, 0.707),
        Biquad::highpass(rate, 1500.0, 0.707),
        Biquad::lowpass(rate, 7000.0, 0.707),
    ] {
        filter.run_all(&mut noise);
    }
    let rough = slow_noise(rng, n, rate, 60.0);
    let rms = (noise.iter().map(|v| v * v).sum::<f64>() / n.max(1) as f64)
        .sqrt()
        .max(1e-300);
    let burst: Vec<f64> = (0..n)
        .map(|i| {
            let t = i as f64 / rate;
            let edge = smoothstep(t / 0.003) * (1.0 - smoothstep((t - (gap - 0.003)) / 0.003));
            noise[i] / rms * level_at_gap * db_to_gain(3.0 * rough[i]) * edge
        })
        .collect();
    case.mix(rate, T0 + gap_start, &burst);
    // The gap has no pitch: whatever the fades left there is not a note's.
    case.set_f0(note1.end.min(T0 + gap_start), T0 + gap_end, |_| 0.0);
    let note1 = Note {
        end: note1.end.min(T0 + gap_end),
        ..note1
    };
    case.notes.push(note1);
    case.notes.push(note2);
    case.params = pair_params(p);
    Ok(case)
}

/// A held note with a 6 Hz vibrato of ±50 cents: one note.
fn vibrato_control(profile: &Profile, rng: &mut Rng, rate: f64, midi: f64) -> Case {
    let mut case = Case::new("held note with vibrato", "additive", true);
    let (hz, detune) = centre(rng, midi);
    let hold = 2.0;
    let duration = hold + profile.release * 1.3;
    let n = (duration * rate).round() as usize;
    let jit = jitter(rng, n, rate, 1.5);
    let phase0 = rng.range(0.0, std::f64::consts::TAU);
    let level = db_to_gain(-6.0);
    let line = Line::from_fn(rate, duration, |t| {
        let i = ((t * rate) as usize).min(n - 1);
        let depth = 50.0 * smoothstep((t - 0.1) / 0.15);
        let c = depth * (std::f64::consts::TAU * 6.0 * t + phase0).sin() + jit[i];
        let amp = level * attack(t, profile.attack) * release(t - hold, profile.release);
        (hz * 2f64.powf(c / 1200.0), amp, amp, 0.7)
    });
    let (onset, _) = sounding_span(&line.amp, rate, level);
    let end = hold + profile.release;
    additive_case(&mut case, profile, &line, rate, rng, 0.0, move |t| {
        t >= onset && t < end
    });
    case.notes.push(Note {
        onset,
        end,
        hz,
        params: obj(vec![("detune_cents", detune.into())]),
    });
    case.params = obj(vec![
        ("vibrato_hz", 6.0.into()),
        ("vibrato_cents", 50.0.into()),
        ("note_s", hold.into()),
    ]);
    case
}

/// A note bent out by a semitone and back within 300 ms: one note.
fn bend_control(profile: &Profile, rng: &mut Rng, rate: f64, midi: f64, up: bool) -> Case {
    let mut case = Case::new("bend out and back", "additive", true);
    let (hz, detune) = centre(rng, midi);
    let hold = 1.8;
    let at = 0.7;
    let duration = hold + profile.release * 1.3;
    let n = (duration * rate).round() as usize;
    let jit = jitter(rng, n, rate, 1.5);
    let level = db_to_gain(-6.0);
    let sign = if up { 1.0 } else { -1.0 };
    let line = Line::from_fn(rate, duration, |t| {
        let i = ((t * rate) as usize).min(n - 1);
        let x = ((t - at) / 0.3).clamp(0.0, 1.0);
        let c = sign * 100.0 * (std::f64::consts::PI * x).sin().powi(2) + jit[i];
        let amp = level * attack(t, profile.attack) * release(t - hold, profile.release);
        (hz * 2f64.powf(c / 1200.0), amp, amp, 0.7)
    });
    let (onset, _) = sounding_span(&line.amp, rate, level);
    let end = hold + profile.release;
    additive_case(&mut case, profile, &line, rate, rng, 0.0, move |t| {
        t >= onset && t < end
    });
    case.notes.push(Note {
        onset,
        end,
        hz,
        params: obj(vec![("detune_cents", detune.into())]),
    });
    case.params = obj(vec![
        ("bend_cents", (sign * 100.0).into()),
        ("bend_at_s", at.into()),
        ("bend_ms", 300.0.into()),
    ]);
    case
}

/// A held note on String Studio: one note.
fn held_string_control(
    ctx: &mut Context,
    instrument: &str,
    rng: &mut Rng,
    midi: f64,
) -> Result<Case> {
    let mut case = Case::new("held note", strings::ENGINE, true);
    let (hz, detune) = centre(rng, midi);
    let rendered = string_note(ctx, instrument, hz, 0.8, 2.5, 0.25)?;
    let note = place_string_note(
        &mut case,
        ctx.rate,
        &rendered,
        0.1,
        |_| 1.0,
        obj(vec![("detune_cents", detune.into())]),
    );
    case.notes.push(note);
    case.params = obj(vec![("note_s", 2.5.into())]);
    Ok(case)
}

/// The set's files for `instrument` at the context's rate.
pub fn files(ctx: &mut Context, instrument: &str) -> Result<Vec<Take>> {
    let index = INSTRUMENTS
        .iter()
        .position(|&i| i == instrument)
        .expect("a legato instrument");
    let profile = additive::profile(instrument);
    let mut takes = Vec::new();
    let (lo, hi) = range(instrument);
    let mid = ((lo + hi) / 2.0).round();

    // 01: smooth glides, two per transition length, and the vibrato control.
    let seed = seed_of(&format!("legato-{instrument}-01"));
    let mut draw = Rng::stream(seed, "pairs");
    let mut cases = Vec::new();
    for (t, &ms) in TRANSITIONS_MS.iter().enumerate() {
        for j in 0..2 {
            let interval = INTERVALS[(t * 2 + j + index * 3) % INTERVALS.len()];
            let p = draw_pair(
                &mut draw,
                instrument,
                interval,
                ms / 1000.0,
                Shape::Smooth,
                0.0,
            );
            cases.push(additive_pair(
                &profile,
                &p,
                ctx.rate,
                &mut Rng::stream(seed, &format!("case{}", cases.len())),
                "legato",
            ));
        }
    }
    let at = draw.range(2.0, cases.len() as f64 - 2.0) as usize;
    cases.insert(
        at,
        vibrato_control(&profile, &mut Rng::stream(seed, "control"), ctx.rate, mid),
    );
    takes.push(take(
        "legato",
        instrument,
        1,
        ctx.rate,
        cases,
        &mut Rng::stream(seed, "gaps"),
        Json::Obj(Vec::new()),
    ));

    // 02: overshoots, quieter second notes, and the bend control.
    let seed = seed_of(&format!("legato-{instrument}-02"));
    let mut draw = Rng::stream(seed, "pairs");
    let mut cases = Vec::new();
    for (t, &ms) in TRANSITIONS_MS.iter().enumerate() {
        let interval = INTERVALS[(t * 3 + index) % INTERVALS.len()];
        let shape = Shape::Overshoot {
            cents: draw.range(30.0, 80.0),
            settle: draw.range(0.06, 0.15),
        };
        let p = draw_pair(&mut draw, instrument, interval, ms / 1000.0, shape, 0.0);
        cases.push(additive_pair(
            &profile,
            &p,
            ctx.rate,
            &mut Rng::stream(seed, &format!("case{}", cases.len())),
            "legato",
        ));
    }
    for (t, &ms) in TRANSITIONS_MS.iter().enumerate() {
        let interval = INTERVALS[(t * 7 + index + 5) % INTERVALS.len()];
        let shape = if t % 2 == 0 {
            Shape::Smooth
        } else {
            Shape::Overshoot {
                cents: draw.range(30.0, 80.0),
                settle: draw.range(0.06, 0.15),
            }
        };
        let second_db = -draw.range(6.0, 15.0);
        let p = draw_pair(
            &mut draw,
            instrument,
            interval,
            ms / 1000.0,
            shape,
            second_db,
        );
        cases.push(additive_pair(
            &profile,
            &p,
            ctx.rate,
            &mut Rng::stream(seed, &format!("case{}", cases.len())),
            "legato",
        ));
    }
    let at = draw.range(2.0, cases.len() as f64 - 2.0) as usize;
    cases.insert(
        at,
        bend_control(
            &profile,
            &mut Rng::stream(seed, "control"),
            ctx.rate,
            mid,
            index % 2 == 0,
        ),
    );
    takes.push(take(
        "legato",
        instrument,
        2,
        ctx.rate,
        cases,
        &mut Rng::stream(seed, "gaps"),
        Json::Obj(Vec::new()),
    ));

    // 03: bow changes (breaths for the voice), some with the second note quieter, and a held note.
    let seed = seed_of(&format!("legato-{instrument}-03"));
    let mut draw = Rng::stream(seed, "pairs");
    let mut cases = Vec::new();
    let mut specs = Vec::new();
    for (t, &ms) in TRANSITIONS_MS.iter().enumerate() {
        let interval = INTERVALS[(t * 3 + index + 1) % INTERVALS.len()];
        specs.push((interval, ms, 0.0));
    }
    for k in 0..4 {
        let ms = TRANSITIONS_MS[(k * 2 + index) % TRANSITIONS_MS.len()];
        let interval = INTERVALS[(k * 3 + index + 2) % INTERVALS.len()];
        specs.push((interval, ms, -draw.range(6.0, 15.0)));
    }
    for (interval, ms, second_db) in specs {
        let transition = ms / 1000.0;
        let gap = draw.range(0.02, 0.06_f64.min(transition));
        let shape = Shape::Change {
            dip_db: draw.range(6.0, 20.0),
            gap,
        };
        let p = draw_pair(
            &mut draw, instrument, interval, transition, shape, second_db,
        );
        let mut rng = Rng::stream(seed, &format!("case{}", cases.len()));
        cases.push(if instrument == "voice" {
            additive_pair(&profile, &p, ctx.rate, &mut rng, "legato")
        } else {
            string_change(ctx, instrument, &p, &mut rng)?
        });
    }
    let at = draw.range(2.0, cases.len() as f64 - 2.0) as usize;
    let control = if instrument == "voice" {
        let mut c = vibrato_control(&profile, &mut Rng::stream(seed, "control"), ctx.rate, mid);
        c.kind = "held note with vibrato".into();
        c
    } else {
        held_string_control(ctx, instrument, &mut Rng::stream(seed, "control"), mid)?
    };
    cases.insert(at, control);
    takes.push(take(
        "legato",
        instrument,
        3,
        ctx.rate,
        cases,
        &mut Rng::stream(seed, "gaps"),
        Json::Obj(Vec::new()),
    ));
    Ok(takes)
}
