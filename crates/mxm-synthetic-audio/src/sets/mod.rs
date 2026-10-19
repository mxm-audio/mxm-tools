//! The sets: what each case plays, built from the two engines.

pub mod dynamics;
pub mod hum;
pub mod legato;
pub mod reentry;
pub mod repeated;
pub mod soft;
pub mod vibrato;
pub mod weak;

use crate::additive::{self, Line, Profile};
use crate::dsp::{Rng, db_to_gain, slow_noise, smoothstep};
use crate::json::{Json, obj};
use crate::strings::{self, Rendered, StringNote, Strings};
use crate::truth::{Case, Note, Take};
use crate::vst::Result;

pub fn midi_hz(midi: f64) -> f64 {
    440.0 * 2f64.powf((midi - 69.0) / 12.0)
}

pub fn cents(from: f64, to: f64) -> f64 {
    1200.0 * (to / from).log2()
}

/// The seed of a file: FNV-1a of its slug without a rate or reverb suffix, so a copy at 44.1 kHz
/// plays the same cases.
pub fn seed_of(name: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in name.bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// What renders the sets: the rate, and String Studio when a set needs it.
pub struct Context<'a> {
    pub rate: f64,
    pub strings: &'a mut Strings,
}

/// A note's intended centre: equal temperament from A 440, off by a player's few cents.
pub fn centre(rng: &mut Rng, midi: f64) -> (f64, f64) {
    let detune = (rng.normal() * 4.0).clamp(-10.0, 10.0);
    (midi_hz(midi + detune / 100.0), detune)
}

/// A few cents of pitch jitter per sample: a slow wander and a faster flutter.
pub fn jitter(rng: &mut Rng, n: usize, rate: f64, cents_rms: f64) -> Vec<f64> {
    let slow = slow_noise(rng, n, rate, 4.0);
    let fast = slow_noise(rng, n, rate, 18.0);
    slow.iter()
        .zip(&fast)
        .map(|(s, f)| cents_rms * (0.9 * s + 0.35 * f))
        .collect()
}

/// A slow level wander, as a gain per sample: `db_rms` dB RMS.
pub fn wander(rng: &mut Rng, n: usize, rate: f64, db_rms: f64) -> Vec<f64> {
    slow_noise(rng, n, rate, 1.2)
        .into_iter()
        .map(|v| db_to_gain(db_rms * v))
        .collect()
}

/// The attack's gain `t` s after a note starts.
pub fn attack(t: f64, time: f64) -> f64 {
    if t <= 0.0 { 0.0 } else { smoothstep(t / time) }
}

/// The release's gain `t` s after it starts: down 40 dB at `time`.
pub fn release(t: f64, time: f64) -> f64 {
    if t <= 0.0 {
        1.0
    } else {
        10f64.powf(-2.0 * t / time)
    }
}

/// Brightness for a level `db` under full.
pub fn brightness(db: f64) -> f64 {
    (0.75 + db / 30.0).clamp(0.0, 1.0)
}

/// An additive case's audio and f0 from a line: the f0 is the line's own, sampled each
/// millisecond where `sounding(t)`.
pub fn additive_case(
    case: &mut Case,
    profile: &Profile,
    line: &Line,
    rate: f64,
    rng: &mut Rng,
    at: f64,
    sounding: impl Fn(f64) -> bool,
) {
    let audio = additive::render(profile, line, rate, rng);
    case.mix(rate, at, &audio);
    let duration = line.hz.len() as f64 / rate;
    let hz = &line.hz;
    case.set_f0(at, at + duration, |t| {
        let local = t - at;
        if !sounding(local) {
            return 0.0;
        }
        let i = ((local * rate).round() as usize).min(hz.len() - 1);
        hz[i]
    });
}

/// A String Studio note placed in a case: `gain(t)` (s from its onset) shapes it, and its onset
/// and end are read again from the shaped samples alone. Returns the note's truth, at the pitch
/// it sounds (its measured steady pitch).
pub fn place_string_note(
    case: &mut Case,
    rate: f64,
    rendered: &Rendered,
    onset_at: f64,
    gain: impl Fn(f64) -> f64,
    extra: Json,
) -> Note {
    let hz = rendered.measured_hz;
    let shaped: Vec<f64> = rendered
        .audio
        .iter()
        .enumerate()
        .map(|(i, v)| v * gain(i as f64 / rate - rendered.onset))
        .collect();
    let (onset, end, _) = crate::measure::onset_and_end(&shaped, rate);
    let start = onset_at - onset;
    case.mix(
        rate,
        start.max(0.0),
        &shaped[((-start).max(0.0) * rate) as usize..],
    );
    case.set_f0(start + onset, start + end, |_| hz);
    let mut params = obj(vec![
        ("engine", strings::ENGINE.into()),
        ("key", Json::from(rendered.key as usize)),
        ("velocity", rendered.velocity.into()),
        ("bend_cents", rendered.bend_cents.into()),
        (
            "asked_hz",
            (hz * 2f64.powf(rendered.error_cents / 1200.0)).into(),
        ),
        (
            "pitch_from",
            "measured steady pitch of the note rendered alone".into(),
        ),
        ("steady_spread_cents", rendered.spread_cents.into()),
        ("noise_db_under_harmonics", rendered.noise_db.into()),
        (
            "fundamental_db_under_loudest_harmonic",
            rendered.fundamental_db.into(),
        ),
        ("settle_cents", rendered.settle_cents.into()),
        ("settle_ms", rendered.settle_ms.into()),
        ("release_drift_cents", rendered.release_cents.into()),
        (
            "release_drift_from_ms_after_note_off",
            rendered.release_from_ms.into(),
        ),
    ]);
    if let Json::Obj(pairs) = extra {
        for (k, v) in pairs {
            params.push(&k, v);
        }
    }
    Note {
        onset: start + onset,
        end: start + end,
        hz,
        params,
    }
}

/// Renders a String Studio note at `hz`, held `hold` s and then faded over `fade` s (to -40 dB).
pub fn string_note(
    ctx: &mut Context,
    instrument: &str,
    hz: f64,
    velocity: f64,
    hold: f64,
    fade: f64,
) -> Result<Rendered> {
    ctx.strings.note(
        instrument,
        &StringNote {
            hz,
            velocity,
            hold,
            tail: 0.0,
            automation: Vec::new(),
            fade,
        },
        ctx.rate,
    )
}

/// One note of a String Studio line: where its onset goes in the case, s, the pitch asked for,
/// how long it is held and how it fades, its velocity.
#[derive(Clone, Copy, Debug)]
pub struct Planned {
    pub at: f64,
    pub midi: f64,
    pub hold: f64,
    pub fade: f64,
    pub velocity: f64,
}

/// Renders a line of String Studio notes into `case`, each alone and summed, and returns their
/// truth: a note that the next begins on (or over) ends where the next begins.
pub fn string_line(
    ctx: &mut Context,
    case: &mut Case,
    instrument: &str,
    plan: &[Planned],
    rng: &mut Rng,
) -> Result<Vec<Note>> {
    let mut notes: Vec<Note> = Vec::new();
    for p in plan {
        let (hz, detune) = centre(rng, p.midi);
        let rendered = string_note(ctx, instrument, hz, p.velocity, p.hold, p.fade)?;
        let note = place_string_note(
            case,
            ctx.rate,
            &rendered,
            p.at,
            |_| 1.0,
            obj(vec![("detune_cents", detune.into())]),
        );
        if let Some(last) = notes.last_mut()
            && last.end > note.onset
        {
            last.end = note.onset;
        }
        notes.push(note);
    }
    // The f0 is each note's own from its onset to its end, the later note's where two overlap.
    for note in &notes {
        case.set_f0(note.onset, note.end, |_| note.hz);
    }
    Ok(notes)
}

/// A melodic line's plan: phrases of 2 to 5 notes in `range` (MIDI), mostly steps, the notes 0.3
/// to 1.2 s and 30 to 80 ms apart, the rests between phrases 0.3 to 2 s, filling about `length` s.
pub fn melody(rng: &mut Rng, range: (f64, f64), length: f64, start: f64) -> Vec<Planned> {
    let mut plan = Vec::new();
    let mut t = start;
    let mut midi = ((range.0 + range.1) / 2.0).round();
    while t < start + length {
        let count = 2 + (rng.unit() * 4.0) as usize;
        for _ in 0..count {
            let step = *rng.pick(&[
                -7.0, -5.0, -4.0, -3.0, -2.0, -2.0, -1.0, 1.0, 2.0, 2.0, 3.0, 4.0, 5.0, 7.0,
            ]);
            midi = (midi + step).clamp(range.0, range.1);
            let hold = rng.range(0.3, 1.2);
            plan.push(Planned {
                at: t,
                midi,
                hold,
                fade: 0.08,
                velocity: rng.range(0.6, 0.9),
            });
            t += hold + rng.range(0.03, 0.08);
        }
        // The last note of a phrase rings out a little longer before the rest.
        if let Some(last) = plan.last_mut() {
            last.fade = 0.2;
        }
        t += rng.range(0.3, 2.0);
    }
    plan
}

/// A file of cases, spaced by seeded gaps of 0.5 to 1 s, normalised so its peak is -3 dBFS
/// (unless `absolute`, whose levels are the case's own).
pub fn take(
    set: &str,
    instrument: &str,
    nn: usize,
    rate: f64,
    cases: Vec<Case>,
    rng: &mut Rng,
    params: Json,
) -> Take {
    let gaps = (0..cases.len()).map(|_| rng.range(0.5, 1.0)).collect();
    Take {
        set: set.to_owned(),
        instrument: instrument.to_owned(),
        nn,
        suffix: String::new(),
        rate,
        seed: seed_of(&format!("{set}-{instrument}-{nn:02}")),
        cases,
        lead: 0.5,
        tail: 0.5,
        gaps,
        params,
    }
}

/// Scales `audio` so its peak is `peak_dbfs`; returns the gain, dB.
pub fn normalise(audio: &mut [f64], peak_dbfs: f64) -> f64 {
    let peak = audio.iter().fold(0.0f64, |p, v| p.max(v.abs()));
    if peak == 0.0 {
        return 0.0;
    }
    let gain = db_to_gain(peak_dbfs) / peak;
    for v in audio.iter_mut() {
        *v *= gain;
    }
    crate::dsp::gain_to_db(gain)
}
