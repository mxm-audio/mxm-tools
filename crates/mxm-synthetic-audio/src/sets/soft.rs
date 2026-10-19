//! Set 7, soft-attack: notes whose attack rises over 30, 60, 100, 200 or 300 ms after digital
//! silence (file 01) or a -70 dBFS floor (file 02) lasting 1 to 40 s; short notes of 0.1 to 0.2 s
//! after a long rest; and notes with a faint breath (a bow's contact, for the cello) 30 to 80 ms
//! before them, which is no note.
//!
//! A rise is a gain ramp (raised cosine) over the note's own attack; the note begins to sound
//! where its level first comes within 40 dB of its peak, measured.

use super::*;
use crate::dsp::{Biquad, white};

pub const INSTRUMENTS: [&str; 3] = ["horn", "flute", "cello"];

const RISES_MS: [f64; 5] = [30.0, 60.0, 100.0, 200.0, 300.0];

fn range(instrument: &str) -> (f64, f64) {
    match instrument {
        "horn" => (41.0, 72.0),
        "flute" => (60.0, 93.0),
        "cello" => (38.0, 67.0),
        other => panic!("no soft-attack range for {other}"),
    }
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    Rise(f64),
    Short(f64),
    Precursor {
        before: f64,
        length: f64,
        under: f64,
    },
}

/// A faint breath (or bow contact): band-passed noise, `length` s, `level` RMS, with soft edges.
fn breath(rng: &mut Rng, rate: f64, length: f64, level: f64, band: (f64, f64)) -> Vec<f64> {
    let n = (length * rate).round() as usize;
    let mut x = white(rng, n);
    for mut f in [
        Biquad::highpass(rate, band.0, 0.707),
        Biquad::lowpass(rate, band.1, 0.707),
    ] {
        f.run_all(&mut x);
    }
    let rms = (x.iter().map(|v| v * v).sum::<f64>() / n.max(1) as f64)
        .sqrt()
        .max(1e-300);
    (0..n)
        .map(|i| {
            let t = i as f64 / rate;
            x[i] / rms
                * level
                * smoothstep(t / (0.4 * length))
                * smoothstep((length - t) / (0.4 * length))
        })
        .collect()
}

fn case_for(
    ctx: &mut Context,
    instrument: &str,
    profile: Option<&Profile>,
    rng: &mut Rng,
    kind: Kind,
    midi: f64,
    silence: f64,
) -> Result<Case> {
    let rate = ctx.rate;
    let label = match kind {
        Kind::Rise(_) => "soft attack after a rest",
        Kind::Short(_) => "short note after a long rest",
        Kind::Precursor { .. } => "note with a faint breath before it",
    };
    let engine = if profile.is_some() {
        "additive"
    } else {
        strings::ENGINE
    };
    let mut case = Case::new(label, engine, false);
    let length = match kind {
        Kind::Short(l) => l,
        _ => rng.range(0.8, 1.5),
    };
    let rise = match kind {
        Kind::Rise(ms) => ms / 1000.0,
        _ => 0.0,
    };
    let level = 0.25;
    // The note's onset lands `silence` into the case: the rest comes first.
    let at = silence;
    let mut params = obj(vec![
        ("rest_before_at_least_s", silence.into()),
        ("note_s", length.into()),
    ]);
    let note = match profile {
        Some(p) => {
            let (hz, detune) = centre(rng, midi);
            let attack_time = if rise > 0.0 { rise } else { p.attack };
            let duration = length + p.release * 1.3;
            let n = (duration * rate).round() as usize;
            let jit = jitter(rng, n, rate, 1.5);
            let line = Line::from_fn(rate, duration, |t| {
                let i = ((t * rate) as usize).min(n - 1);
                let amp = level * attack(t, attack_time) * release(t - length, p.release);
                // A slow rise is a soft one: darker as it starts.
                (
                    hz * 2f64.powf(jit[i] / 1200.0),
                    amp,
                    amp,
                    brightness(-12.0) + 0.4 * attack(t, attack_time),
                )
            });
            let onset = line
                .amp
                .iter()
                .position(|&a| a >= 0.01 * level)
                .unwrap_or(0) as f64
                / rate;
            let audio = additive::render(p, &line, rate, rng);
            case.mix(rate, at - onset, &audio);
            let hz_line = line.hz.clone();
            let base = at - onset;
            let end = length + p.release;
            case.set_f0(at, base + end, |t| {
                hz_line[(((t - base) * rate) as usize).min(hz_line.len() - 1)]
            });
            Note {
                onset: at,
                end: base + end,
                hz,
                params: obj(vec![("detune_cents", detune.into())]),
            }
        }
        None => {
            let (hz, detune) = centre(rng, midi);
            let rendered = string_note(
                ctx,
                instrument,
                hz,
                if rise > 0.0 { 0.6 } else { 0.8 },
                length,
                0.2,
            )?;
            place_string_note(
                &mut case,
                rate,
                &rendered,
                at,
                |t| {
                    if rise > 0.0 {
                        smoothstep(t / rise)
                    } else {
                        1.0
                    }
                },
                obj(vec![("detune_cents", detune.into())]),
            )
        }
    };
    if rise > 0.0 {
        params.push("rise_ms", rise * 1000.0);
        params.push("rise", "raised cosine, by gain, from the note's start");
    }
    if let Kind::Precursor {
        before,
        length: breath_length,
        under,
    } = kind
    {
        let band = match instrument {
            "flute" => (1500.0, 9000.0),
            "horn" => (600.0, 4000.0),
            _ => (2000.0, 8000.0),
        };
        let b = breath(rng, rate, breath_length, level * db_to_gain(under), band);
        case.mix(rate, note.onset - before - breath_length, &b);
        params.push("breath_ms_before_note", before * 1000.0);
        params.push("breath_ms", breath_length * 1000.0);
        params.push("breath_db_under_note", under);
        params.push(
            "breath",
            if instrument == "cello" {
                "the bow's contact: band-passed noise"
            } else {
                "band-passed noise"
            },
        );
    }
    case.notes.push(note);
    case.params = params;
    Ok(case)
}

pub fn files(ctx: &mut Context, instrument: &str) -> Result<Vec<Take>> {
    let seed = seed_of(&format!("soft-attack-{instrument}"));
    let (lo, hi) = range(instrument);
    let profile = (instrument != "cello").then(|| additive::profile(instrument));
    let mut takes = Vec::new();
    // The rests before each note, s: one file has the 40 s one.
    let rests: [[f64; 9]; 2] = [
        [1.0, 3.0, 12.0, 2.0, 6.0, 1.5, 8.0, 4.0, 2.5],
        [40.0, 1.0, 2.0, 5.0, 1.5, 3.0, 10.0, 2.0, 1.2],
    ];
    for (f, rests) in rests.iter().enumerate() {
        let nn = f + 1;
        let mut draw = Rng::stream(seed, &format!("file{nn}"));
        let mut kinds: Vec<Kind> = RISES_MS.iter().map(|&ms| Kind::Rise(ms)).collect();
        kinds.push(Kind::Short(draw.range(0.1, 0.2)));
        kinds.push(Kind::Short(draw.range(0.1, 0.2)));
        for _ in 0..2 {
            kinds.push(Kind::Precursor {
                before: draw.range(0.03, 0.08),
                length: draw.range(0.02, 0.05),
                under: -draw.range(25.0, 35.0),
            });
        }
        for i in (1..kinds.len()).rev() {
            let j = ((draw.unit() * (i + 1) as f64) as usize).min(i);
            kinds.swap(i, j);
        }
        // Short notes come after the longest rests.
        let mut order: Vec<usize> = (0..rests.len()).collect();
        order.sort_by(|&a, &b| rests[b].total_cmp(&rests[a]));
        let mut cases = Vec::new();
        let mut assigned = vec![0.0; kinds.len()];
        let shorts: Vec<usize> = kinds
            .iter()
            .enumerate()
            .filter(|(_, k)| matches!(k, Kind::Short(_)))
            .map(|(i, _)| i)
            .collect();
        let mut longest = order.iter();
        for &s in &shorts {
            assigned[s] = rests[*longest.next().expect("rests")];
        }
        let mut rest_iter = order.iter().skip(shorts.len());
        for (i, a) in assigned.iter_mut().enumerate() {
            if !shorts.contains(&i) {
                *a = rests[*rest_iter.next().expect("rests")];
            }
        }
        for (i, &kind) in kinds.iter().enumerate() {
            let midi = draw.range(lo, hi).round();
            let mut rng = Rng::stream(seed, &format!("file{nn}-case{i}"));
            cases.push(case_for(
                ctx,
                instrument,
                profile.as_ref(),
                &mut rng,
                kind,
                midi,
                assigned[i],
            )?);
        }
        let mut params = obj(vec![(
            "before_each_note",
            Json::from(if nn == 1 {
                "digital silence"
            } else {
                "a -70 dBFS noise floor"
            }),
        )]);
        if nn == 2 {
            params.push("noise_floor_dbfs", -70.0);
        }
        let mut t = take(
            "soft-attack",
            instrument,
            nn,
            ctx.rate,
            cases,
            &mut Rng::stream(seed, "gaps"),
            params,
        );
        // The rests are inside the cases; between cases, only a short tail.
        t.gaps = vec![0.3; t.cases.len()];
        t.lead = 0.0;
        takes.push(t);
    }
    Ok(takes)
}
