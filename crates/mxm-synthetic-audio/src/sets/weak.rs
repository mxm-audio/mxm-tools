//! Set 4, weak-fundamental: notes from A#1 to A4 whose fundamental lies 0, 10, 20, 30 or 40 dB
//! under the second harmonic, the odd harmonics 3, 5 and 7 each 20 to 40 dB under the loudest.
//! The truth is the fundamental. Controls, both at the upper octave: a note an octave up (nothing
//! at all at half its pitch), and a tone of even harmonics only, whose true pitch is the octave
//! above its missing fundamental.
//!
//! The bassoon and trombone are additive, their harmonics set by level. The viola and double bass
//! are String Studio notes, their harmonics set by a narrow peaking filter at each (a linear
//! filter on a note that holds one pitch), measured and corrected until within `LEVEL_DB`; the
//! even-harmonics-only tone is a note plus itself half a period later, which cancels its odd
//! harmonics. Every note's levels are measured back into the sidecar.

use super::*;
use crate::dsp::Biquad;
use crate::measure::harmonic_levels;

pub const INSTRUMENTS: [&str; 4] = ["bassoon", "viola", "trombone", "double-bass"];

const UNDER_H2_DB: [f64; 5] = [0.0, 10.0, 20.0, 30.0, 40.0];

/// How close a String Studio note's harmonics must come to their targets, dB.
const LEVEL_DB: f64 = 0.75;

fn range(instrument: &str) -> (f64, f64) {
    match instrument {
        "bassoon" => (34.0, 65.0),
        "viola" => (48.0, 69.0),
        "trombone" => (40.0, 65.0),
        "double-bass" => (34.0, 55.0),
        other => panic!("no weak-fundamental range for {other}"),
    }
}

fn strings_engine(instrument: &str) -> bool {
    matches!(instrument, "viola" | "double-bass")
}

/// The targets: H1 `under` dB under H2, H3, H5 and H7 each `odd[i]` under the loudest of the
/// others (H2, H4, H6, H8), from the levels `l` (dB, H1 first).
fn targets(l: &[f64], under: f64, odd: [f64; 3]) -> Vec<(usize, f64)> {
    let loudest = [2usize, 4, 6, 8]
        .iter()
        .map(|&k| l[k - 1])
        .fold(f64::MIN, f64::max);
    vec![
        (1, l[1] - under),
        (3, loudest - odd[0]),
        (5, loudest - odd[1]),
        (7, loudest - odd[2]),
    ]
}

/// What a note's harmonics came to: H1 under H2 and H3, H5, H7 under the loudest even one, dB.
fn achieved(l: &[f64]) -> Json {
    let loudest = [2usize, 4, 6, 8]
        .iter()
        .map(|&k| l[k - 1])
        .fold(f64::MIN, f64::max);
    obj(vec![
        ("measured_h1_under_h2_db", (l[1] - l[0]).into()),
        ("measured_h3_under_loudest_db", (loudest - l[2]).into()),
        ("measured_h5_under_loudest_db", (loudest - l[4]).into()),
        ("measured_h7_under_loudest_db", (loudest - l[6]).into()),
        ("measured_harmonic_levels_db", Json::from(l.to_vec())),
    ])
}

/// `x` through a narrow peaking filter at each `(harmonic, gain dB)` of `f0`, a quarter of f0
/// wide.
fn shape_harmonics(x: &[f64], rate: f64, f0: f64, gains: &[(usize, f64)]) -> Vec<f64> {
    let mut y = x.to_vec();
    for &(k, gain) in gains {
        if gain.abs() < 0.01 {
            continue;
        }
        let f = k as f64 * f0;
        let mut filter = Biquad::peaking(rate, f, 4.0 * k as f64, gain);
        filter.run_all(&mut y);
    }
    y
}

/// `x` plus itself half a period of `f0` later, halved: its odd harmonics cancel. The delay is
/// read with a windowed-sinc interpolator (32 taps, Blackman).
fn even_only(x: &[f64], rate: f64, f0: f64) -> Vec<f64> {
    let delay = rate / f0 / 2.0;
    let whole = delay.floor() as isize;
    let frac = delay - delay.floor();
    let taps = 16isize;
    let kernel: Vec<f64> = (-taps + 1..=taps)
        .map(|j| {
            let u = j as f64 - frac;
            let sinc = if u.abs() < 1e-12 {
                1.0
            } else {
                (std::f64::consts::PI * u).sin() / (std::f64::consts::PI * u)
            };
            let w = (u + taps as f64) / (2.0 * taps as f64);
            let blackman = 0.42 - 0.5 * (std::f64::consts::TAU * w).cos()
                + 0.08 * (2.0 * std::f64::consts::TAU * w).cos();
            sinc * blackman
        })
        .collect();
    (0..x.len() as isize)
        .map(|i| {
            let mut delayed = 0.0;
            for (n, j) in (-taps + 1..=taps).enumerate() {
                let at = i - whole - j;
                if at >= 0 && (at as usize) < x.len() {
                    delayed += kernel[n] * x[at as usize];
                }
            }
            0.5 * (x[i as usize] + delayed)
        })
        .collect()
}

/// An additive weak-fundamental note (or, with `even`, its even harmonics only).
fn additive_note(
    profile: &Profile,
    rate: f64,
    rng: &mut Rng,
    midi: f64,
    length: f64,
    under: Option<(f64, [f64; 3])>,
    even: bool,
) -> Case {
    let profile = Profile {
        shimmer_db: 0.15,
        ..*profile
    };
    let (hz, detune) = centre(rng, midi);
    let bright = 0.6;
    let tilt = profile.tilt_soft + (profile.tilt_loud - profile.tilt_soft) * bright;
    let natural: Vec<f64> = (1..=12)
        .map(|k| {
            let mut db =
                -tilt * (k as f64).log2() + profile_envelope(&profile, k as f64 * hz, hz, bright);
            if k % 2 == 0 && (k as f64 * hz) < profile.even_until_hz {
                db += profile.even_db;
            }
            db
        })
        .collect();
    let mut overrides = Vec::new();
    let mut params = obj(vec![("note_s", length.into())]);
    if let Some((level, odd)) = under {
        for (k, target) in targets(&natural, level, odd) {
            overrides.push((k, target - natural[k - 1]));
        }
        params.push("h1_under_h2_db", level);
        params.push("h3_h5_h7_under_loudest_db", Json::from(odd.to_vec()));
    }
    let duration = length + profile.release * 1.3;
    let n = (duration * rate).round() as usize;
    let jit = jitter(rng, n, rate, 1.5);
    let level = db_to_gain(-6.0);
    let mut line = Line::from_fn(rate, duration, |t| {
        let i = ((t * rate) as usize).min(n - 1);
        let amp = level * attack(t, profile.attack) * release(t - length, profile.release);
        (hz * 2f64.powf(jit[i] / 1200.0), amp, amp, bright)
    });
    line.harmonic_db = overrides;
    if even {
        line.only = Some(|k| k % 2 == 0);
    }
    let onset = line
        .amp
        .iter()
        .position(|&a| a >= 0.01 * level)
        .unwrap_or(0) as f64
        / rate;
    let end = length + profile.release;
    let mut case = Case::new(
        if even {
            "even harmonics only"
        } else if under.is_some() {
            "weak fundamental"
        } else {
            "octave-up note"
        },
        "additive",
        under.is_none(),
    );
    let sounding_hz = if even { 2.0 * hz } else { hz };
    let audio = additive::render(&profile, &line, rate, rng);
    case.mix(rate, 0.0, &audio);
    let hz_line = line.hz.clone();
    case.set_f0(onset, end, |t| {
        let i = ((t * rate).round() as usize).min(hz_line.len() - 1);
        if even { 2.0 * hz_line[i] } else { hz_line[i] }
    });
    let measured = harmonic_levels(&case.audio, rate, onset + 0.2, length - 0.05, hz, 8);
    let mut note_params = obj(vec![("detune_cents", detune.into())]);
    if (under.is_some() || even)
        && let Json::Obj(pairs) = achieved(&measured)
    {
        for (k, v) in pairs {
            note_params.push(&k, v);
        }
    }
    if even {
        params.push("missing_fundamental_hz", hz);
        params.push(
            "sounding",
            "harmonics 2, 4, 6 ... of the missing fundamental: the octave above it",
        );
    }
    case.notes.push(Note {
        onset,
        end,
        hz: sounding_hz,
        params: note_params,
    });
    case.params = params;
    case
}

fn profile_envelope(profile: &Profile, f: f64, f0: f64, bright: f64) -> f64 {
    profile.envelope_db_at(f, f0, bright)
}

/// A String Studio weak-fundamental note: rendered alone, its harmonics shaped and checked.
fn string_weak(
    ctx: &mut Context,
    instrument: &str,
    rng: &mut Rng,
    midi: f64,
    length: f64,
    under: Option<(f64, [f64; 3])>,
    even: bool,
) -> Result<Case> {
    let rate = ctx.rate;
    let (hz, detune) = centre(rng, midi);
    let rendered = string_note(ctx, instrument, hz, 0.8, length, 0.2)?;
    let f0 = rendered.measured_hz;
    let (t0, t1) = (rendered.onset + 0.15, length - 0.05);
    let mut audio = rendered.audio.clone();
    let mut params = obj(vec![("note_s", length.into())]);
    let mut note_params = obj(vec![("detune_cents", detune.into())]);
    if let Some((level, odd)) = under {
        let natural = harmonic_levels(&audio, rate, t0, t1, f0, 8);
        let goal = targets(&natural, level, odd);
        let mut gains: Vec<(usize, f64)> = goal
            .iter()
            .map(|&(k, t)| (k, (t - natural[k - 1]).clamp(-60.0, 20.0)))
            .collect();
        for _ in 0..6 {
            audio = shape_harmonics(&rendered.audio, rate, f0, &gains);
            let now = harmonic_levels(&audio, rate, t0, t1, f0, 8);
            // The goal moves with the even harmonics the filters brush; re-aim at it.
            let goal = targets(&now, level, odd);
            let worst = goal
                .iter()
                .map(|&(k, t)| (t - now[k - 1]).abs())
                .fold(0.0, f64::max);
            if worst <= LEVEL_DB {
                break;
            }
            for (g, &(k, t)) in gains.iter_mut().zip(&goal) {
                g.1 = (g.1 + t - now[k - 1]).clamp(-70.0, 25.0);
            }
        }
        params.push("h1_under_h2_db", level);
        params.push("h3_h5_h7_under_loudest_db", Json::from(odd.to_vec()));
        params.push(
            "filter_gains_db",
            Json::Arr(
                gains
                    .iter()
                    .map(|&(k, g)| obj(vec![("harmonic", Json::from(k)), ("gain_db", g.into())]))
                    .collect(),
            ),
        );
        if let Json::Obj(pairs) = achieved(&harmonic_levels(&audio, rate, t0, t1, f0, 8)) {
            for (k, v) in pairs {
                note_params.push(&k, v);
            }
        }
    }
    if even {
        audio = even_only(&audio, rate, f0);
        params.push("missing_fundamental_hz", f0);
        params.push(
            "sounding",
            "the note plus itself half a period later: its even harmonics, the octave above it",
        );
        if let Json::Obj(pairs) = achieved(&harmonic_levels(&audio, rate, t0, t1, f0, 8)) {
            for (k, v) in pairs {
                note_params.push(&k, v);
            }
        }
    }
    let mut shaped = rendered.clone();
    shaped.audio = audio;
    if even {
        shaped.measured_hz = 2.0 * f0;
    }
    let mut case = Case::new(
        if even {
            "even harmonics only"
        } else if under.is_some() {
            "weak fundamental"
        } else {
            "octave-up note"
        },
        strings::ENGINE,
        under.is_none(),
    );
    let note = place_string_note(&mut case, rate, &shaped, 0.1, |_| 1.0, note_params);
    case.notes.push(note);
    case.params = params;
    Ok(case)
}

pub fn files(ctx: &mut Context, instrument: &str) -> Result<Vec<Take>> {
    let seed = seed_of(&format!("weak-fundamental-{instrument}"));
    let mut draw = Rng::stream(seed, "notes");
    let (lo, hi) = range(instrument);
    let profile = (!strings_engine(instrument)).then(|| additive::profile(instrument));
    let mut cases = Vec::new();
    // Three pitches per level, spread over the range.
    for (i, &under) in UNDER_H2_DB.iter().enumerate() {
        for j in 0..3 {
            let span = (hi - lo) / 3.0;
            let midi = (lo + span * j as f64 + draw.range(0.0, span)).round();
            let odd = [
                draw.range(20.0, 40.0),
                draw.range(20.0, 40.0),
                draw.range(20.0, 40.0),
            ];
            let length = draw.range(1.0, 1.6);
            let mut rng = Rng::stream(seed, &format!("note{i}-{j}"));
            cases.push(match &profile {
                Some(p) => additive_note(
                    p,
                    ctx.rate,
                    &mut rng,
                    midi,
                    length,
                    Some((under, odd)),
                    false,
                ),
                None => string_weak(
                    ctx,
                    instrument,
                    &mut rng,
                    midi,
                    length,
                    Some((under, odd)),
                    false,
                )?,
            });
        }
    }
    // Shuffled, so the levels don't come in order.
    for i in (1..cases.len()).rev() {
        let j = ((draw.unit() * (i + 1) as f64) as usize).min(i);
        cases.swap(i, j);
    }
    // The controls, at the upper octave of a note low in the range.
    let midi = (lo + draw.range(0.0, (hi - lo) / 2.0 - 12.0).max(0.0)).round();
    let mut rng = Rng::stream(seed, "controls");
    let (up, even) = match &profile {
        Some(p) => (
            additive_note(p, ctx.rate, &mut rng, midi + 12.0, 1.4, None, false),
            additive_note(p, ctx.rate, &mut rng, midi, 1.4, None, true),
        ),
        None => (
            string_weak(ctx, instrument, &mut rng, midi + 12.0, 1.4, None, false)?,
            string_weak(ctx, instrument, &mut rng, midi, 1.4, None, true)?,
        ),
    };
    let mut even = even;
    even.control = true;
    cases.insert(cases.len() / 3, up);
    cases.insert(2 * cases.len() / 3, even);
    Ok(vec![take(
        "weak-fundamental",
        instrument,
        1,
        ctx.rate,
        cases,
        &mut Rng::stream(seed, "gaps"),
        Json::Obj(Vec::new()),
    )])
}
