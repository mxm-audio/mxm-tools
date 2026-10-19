//! Set 3, hum: a melodic line with rests of 0.3 to 2 s over a steady hum that runs the whole
//! file: 50 Hz mains with harmonics to 400 Hz, 60 Hz mains the same, a single 33 Hz tone, or a
//! 25 to 40 Hz rumble, at -40, -30, -20 or -10 dB under the music's loudest (its loudest 100 ms
//! RMS against the hum's RMS). Each instrument plays one line, on String Studio, under each hum
//! (the levels rotated across the instruments, so each hum is heard at three); the controls are
//! the same line with no hum, and a double-bass line whose lowest notes are a real E1 and C1.

use super::*;
use crate::dsp::{Biquad, white, zero_phase};
use std::f64::consts::TAU;

pub const INSTRUMENTS: [&str; 3] = ["cello", "violin", "double-bass"];

const HUMS: [&str; 4] = ["mains-50", "mains-60", "tone-33", "rumble"];
const LEVELS: [f64; 4] = [-40.0, -30.0, -20.0, -10.0];

fn range(instrument: &str) -> (f64, f64) {
    match instrument {
        "cello" => (36.0, 67.0),
        "violin" => (55.0, 88.0),
        "double-bass" => (31.0, 55.0),
        other => panic!("no hum range for {other}"),
    }
}

/// `n` samples of a hum, unit RMS, and what it is.
fn hum(kind: &str, n: usize, rate: f64, rng: &mut Rng) -> (Vec<f64>, Json) {
    let mut out = vec![0.0; n];
    let params = match kind {
        "mains-50" | "mains-60" => {
            let f = if kind == "mains-50" { 50.0 } else { 60.0 };
            // Mains hum's harmonics: the odd ones strongest (a transformer's and a rectifier's),
            // each a little unsteady in level.
            let mut levels = Vec::new();
            let mut k = 1;
            while f * k as f64 <= 400.0 {
                let db = if k == 1 {
                    0.0
                } else if k % 2 == 1 {
                    -4.0 - 2.0 * k as f64
                } else {
                    -10.0 - 2.5 * k as f64
                };
                let db = db + rng.range(-2.0, 2.0);
                let phase = rng.range(0.0, TAU);
                let wobble = slow_noise(rng, n, rate, 0.3);
                for (i, v) in out.iter_mut().enumerate() {
                    *v += db_to_gain(db + 0.3 * wobble[i])
                        * (TAU * f * k as f64 * i as f64 / rate + phase).sin();
                }
                levels.push(Json::from(db));
                k += 1;
            }
            obj(vec![
                ("hum", kind.into()),
                ("hum_hz", f.into()),
                ("harmonic_levels_db", Json::Arr(levels)),
            ])
        }
        "tone-33" => {
            let phase = rng.range(0.0, TAU);
            for (i, v) in out.iter_mut().enumerate() {
                *v = (TAU * 33.0 * i as f64 / rate + phase).sin();
            }
            obj(vec![("hum", kind.into()), ("hum_hz", 33.0.into())])
        }
        "rumble" => {
            out = white(rng, n);
            for filter in [
                Biquad::highpass(rate, 25.0, 0.707),
                Biquad::highpass(rate, 25.0, 0.707),
                Biquad::lowpass(rate, 40.0, 0.707),
                Biquad::lowpass(rate, 40.0, 0.707),
            ] {
                out = zero_phase(&out, filter);
            }
            obj(vec![
                ("hum", kind.into()),
                ("rumble_band_hz", Json::from(vec![25.0, 40.0])),
            ])
        }
        other => panic!("no hum {other}"),
    };
    let rms = (out.iter().map(|v| v * v).sum::<f64>() / n as f64)
        .sqrt()
        .max(1e-300);
    for v in out.iter_mut() {
        *v /= rms;
    }
    (out, params)
}

/// The loudest 100 ms RMS of `audio`.
fn loudest(audio: &[f64], rate: f64) -> f64 {
    let w = (0.1 * rate) as usize;
    let mut best = 0.0f64;
    let mut sum = 0.0;
    for i in 0..audio.len() {
        sum += audio[i] * audio[i];
        if i >= w {
            sum -= audio[i - w] * audio[i - w];
        }
        if i + 1 >= w {
            best = best.max(sum / w as f64);
        }
    }
    best.sqrt()
}

pub fn files(ctx: &mut Context, instrument: &str) -> Result<Vec<Take>> {
    let index = INSTRUMENTS
        .iter()
        .position(|&i| i == instrument)
        .expect("a hum instrument");
    let seed = seed_of(&format!("hum-{instrument}"));
    let mut rng = Rng::stream(seed, "line");
    let plan = melody(&mut rng, range(instrument), 17.0, 0.0);
    let mut line = Case::new("melodic line", strings::ENGINE, false);
    line.notes = string_line(
        ctx,
        &mut line,
        instrument,
        &plan,
        &mut Rng::stream(seed, "notes"),
    )?;
    line.params = obj(vec![(
        "line",
        "phrases of 2 to 5 notes, rests of 0.3 to 2 s".into(),
    )]);
    let rate = ctx.rate;

    let mut takes = Vec::new();
    let one = |mut case: Case, nn: usize, params: Json| -> Take {
        case.audio
            .resize(case.audio.len() + (0.3 * rate) as usize, 0.0);
        let mut t = take(
            "hum",
            instrument,
            nn,
            rate,
            vec![case],
            &mut Rng::new(0),
            params,
        );
        t.lead = 1.0;
        t.tail = 1.0;
        t
    };
    for (j, kind) in HUMS.iter().enumerate() {
        let level = LEVELS[(index + j) % LEVELS.len()];
        takes.push(one(
            line.clone(),
            j + 1,
            obj(vec![
                ("hum", kind.to_string().into()),
                ("hum_db_under_loudest", level.into()),
            ]),
        ));
    }
    let mut control = line.clone();
    control.control = true;
    control.kind = "melodic line, no hum".into();
    takes.push(one(control, 5, obj(vec![("hum", "none".into())])));

    if instrument == "double-bass" {
        // A line whose real lowest notes are an E1 and a C1, no hum.
        let mut rng = Rng::stream(seed, "low line");
        let mut plan = melody(&mut rng, (31.0, 45.0), 13.0, 0.0);
        let mut t = plan.last().map_or(0.0, |p| p.at + p.hold + 1.0);
        for midi in [28.0, 31.0, 24.0, 28.0] {
            let hold = rng.range(0.9, 1.5);
            plan.push(Planned {
                at: t,
                midi,
                hold,
                fade: 0.25,
                velocity: 0.8,
            });
            t += hold + rng.range(0.5, 1.2);
        }
        let mut low = Case::new("line down to E1 and C1, no hum", strings::ENGINE, true);
        low.notes = string_line(
            ctx,
            &mut low,
            instrument,
            &plan,
            &mut Rng::stream(seed, "low notes"),
        )?;
        low.params = obj(vec![(
            "lowest_notes",
            Json::from(vec!["E1 41.2 Hz", "C1 32.7 Hz"]),
        )]);
        takes.push(one(low, 6, obj(vec![("hum", "none".into())])));
    }
    Ok(takes)
}

/// Mixes a take's hum in, once it is laid out: `hum_db_under_loudest` under the music's loudest
/// 100 ms, for the whole file. Called on the laid-out audio.
pub fn add_hum(take: &mut Take, audio: &mut [f64]) {
    let Json::Obj(pairs) = &take.params else {
        return;
    };
    let kind = pairs
        .iter()
        .find(|(k, _)| k == "hum")
        .and_then(|(_, v)| match v {
            Json::Str(s) => Some(s.clone()),
            _ => None,
        });
    let level = pairs
        .iter()
        .find(|(k, _)| k == "hum_db_under_loudest")
        .and_then(|(_, v)| match v {
            Json::Num(x) => Some(*x),
            _ => None,
        });
    let (Some(kind), Some(level)) = (kind, level) else {
        return;
    };
    if kind == "none" {
        return;
    }
    let loud = loudest(audio, take.rate);
    let mut rng = Rng::stream(take.seed, "hum");
    let (h, params) = hum(&kind, audio.len(), take.rate, &mut rng);
    let gain = loud * db_to_gain(level);
    for (v, x) in audio.iter_mut().zip(&h) {
        *v += gain * x;
    }
    take.params.push("hum_detail", params);
    take.params.push(
        "music_loudest_100ms_rms_dbfs_before_gain",
        crate::dsp::gain_to_db(loud),
    );
}
