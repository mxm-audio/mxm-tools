//! Set 5, dynamics: held notes of 1 s at -40, -34, -30 and -24 dB under loud neighbours, once with
//! silence between and once legato; then a quiet passage of several notes 30 to 40 dB under the
//! rest, filling 30 to 60 % of the file. A noise floor at -70 dBFS runs throughout, and every note
//! is pitched and must be found.
//!
//! Levels here are absolute: the loud notes peak at -6 dBFS, and a note's level "under" them is
//! its sustained RMS against theirs (each measured into the sidecar). The clarinet and voice are
//! additive, their legato one continuous line; the violin's notes are String Studio's, each
//! rendered alone, their legato re-bowed with no gap.

use super::*;

pub const INSTRUMENTS: [&str; 3] = ["clarinet", "violin", "voice"];

const QUIET_DB: [f64; 4] = [-40.0, -34.0, -30.0, -24.0];

/// The loud notes' sustained RMS before the file is scaled, as a gain.
const LOUD: f64 = 0.25;

fn range(instrument: &str) -> (f64, f64) {
    match instrument {
        "clarinet" => (50.0, 82.0),
        "violin" => (57.0, 86.0),
        "voice" => (55.0, 76.0),
        other => panic!("no dynamics range for {other}"),
    }
}

/// One note of a passage: its pitch, length and level under the loud notes (0 for a loud one).
#[derive(Clone, Copy, Debug)]
struct Step {
    midi: f64,
    length: f64,
    under: f64,
    /// Silence after it, s (0: the next follows at once, legato).
    gap: f64,
}

/// A passage as one additive line: notes with silence between start and stop; legato ones change
/// pitch and level over 60 ms with no new attack.
fn additive_passage(
    profile: &Profile,
    steps: &[Step],
    rate: f64,
    rng: &mut Rng,
    kind: &str,
    control: bool,
) -> Case {
    let mut case = Case::new(kind, "additive", control);
    // Group legato runs into lines.
    let mut t = 0.0;
    let mut i = 0;
    while i < steps.len() {
        let mut run = vec![steps[i]];
        while run.last().unwrap().gap == 0.0 && i + run.len() < steps.len() {
            run.push(steps[i + run.len()]);
        }
        let centres: Vec<(f64, f64)> = run.iter().map(|s| centre(rng, s.midi)).collect();
        let starts: Vec<f64> = run
            .iter()
            .scan(0.0, |acc, s| {
                let at = *acc;
                *acc += s.length;
                Some(at)
            })
            .collect();
        let length: f64 = run.iter().map(|s| s.length).sum();
        let duration = length + profile.release * 1.3;
        let n = (duration * rate).round() as usize;
        let jit = jitter(rng, n, rate, 1.5);
        let drift = wander(rng, n, rate, 0.3);
        let change = 0.06;
        let gain = |db: f64| LOUD * db_to_gain(db);
        let at_time = |time: f64| -> (f64, f64) {
            // The pitch (cents from the first) and the level, moving over `change` at each join.
            let mut cents_now = 0.0;
            let mut level = gain(run[0].under);
            for k in 1..run.len() {
                let x = smoothstep((time - starts[k]) / change);
                let c = 1200.0 * (centres[k].0 / centres[0].0).log2();
                cents_now +=
                    (c - (if k == 1 {
                        0.0
                    } else {
                        1200.0 * (centres[k - 1].0 / centres[0].0).log2()
                    })) * x;
                level += (gain(run[k].under) - gain(run[k - 1].under)) * x;
            }
            (cents_now, level)
        };
        let line = Line::from_fn(rate, duration, |time| {
            let j = ((time * rate) as usize).min(n - 1);
            let (c, level) = at_time(time);
            let amp = level
                * drift[j]
                * attack(time, profile.attack)
                * release(time - length, profile.release);
            let db = crate::dsp::gain_to_db(level / LOUD);
            (
                centres[0].0 * 2f64.powf((c + jit[j]) / 1200.0),
                amp,
                amp,
                brightness(db),
            )
        });
        let first_level = gain(run[0].under);
        let onset = line
            .amp
            .iter()
            .position(|&a| a >= 0.01 * first_level)
            .unwrap_or(0) as f64
            / rate;
        let last_level = gain(run.last().unwrap().under);
        let _ = last_level;
        let end = length + profile.release;
        let audio = additive::render(profile, &line, rate, rng);
        case.mix(rate, t, &audio);
        let hz_line = line.hz.clone();
        let base = t;
        case.set_f0(base + onset, base + end, |time| {
            let j = (((time - base) * rate).round() as usize).min(hz_line.len() - 1);
            hz_line[j]
        });
        for (k, s) in run.iter().enumerate() {
            let on = base + if k == 0 { onset } else { starts[k] };
            let off = base
                + if k + 1 < run.len() {
                    starts[k + 1]
                } else {
                    end
                };
            case.notes.push(Note {
                onset: on,
                end: off,
                hz: centres[k].0,
                params: obj(vec![
                    ("detune_cents", centres[k].1.into()),
                    ("under_loud_db", s.under.into()),
                    ("legato_into_next", (k + 1 < run.len()).into()),
                ]),
            });
        }
        t += end + run.last().unwrap().gap;
        i += run.len();
    }
    case
}

/// A passage on String Studio: each note rendered alone, scaled to its level, placed; a legato
/// note's successor starts as it ends (re-bowed).
fn string_passage(
    ctx: &mut Context,
    steps: &[Step],
    rng: &mut Rng,
    kind: &str,
    control: bool,
) -> Result<Case> {
    let rate = ctx.rate;
    let mut case = Case::new(kind, strings::ENGINE, control);
    let mut t = 0.1;
    let mut notes: Vec<Note> = Vec::new();
    for s in steps {
        let (hz, detune) = centre(rng, s.midi);
        let velocity = (0.85 * db_to_gain(s.under / 3.0)).clamp(0.3, 1.0);
        let fade = if s.gap == 0.0 { 0.06 } else { 0.2 };
        let rendered = string_note(ctx, "violin", hz, velocity, s.length + 0.03, fade)?;
        // Scale the note so its sustained RMS is the step's level.
        let (a, b) = (
            ((rendered.onset + 0.1) * rate) as usize,
            ((s.length - 0.05) * rate) as usize,
        );
        let rms = (rendered.audio[a..b].iter().map(|v| v * v).sum::<f64>() / (b - a).max(1) as f64)
            .sqrt();
        let scale = LOUD * db_to_gain(s.under) / rms.max(1e-12);
        let note = place_string_note(
            &mut case,
            rate,
            &rendered,
            t,
            |_| scale,
            obj(vec![
                ("detune_cents", detune.into()),
                ("under_loud_db", s.under.into()),
                ("legato_into_next", (s.gap == 0.0).into()),
            ]),
        );
        if let Some(last) = notes.last_mut()
            && last.end > note.onset
        {
            last.end = note.onset;
        }
        t = note.onset + s.length - rendered.onset + s.gap;
        notes.push(note);
    }
    for note in &notes {
        case.set_f0(note.onset, note.end, |_| note.hz);
    }
    case.notes = notes;
    Ok(case)
}

pub fn files(ctx: &mut Context, instrument: &str) -> Result<Vec<Take>> {
    let seed = seed_of(&format!("dynamics-{instrument}"));
    let mut draw = Rng::stream(seed, "notes");
    let (lo, hi) = range(instrument);
    let mut pitch = || draw.range(lo, hi).round();
    let mut sections: Vec<(&str, Vec<Step>, bool)> = Vec::new();
    // Quiet notes between loud ones: with silence between, then legato.
    for (kind, gap) in [
        ("quiet notes between loud, silence between", 0.5),
        ("quiet notes between loud, legato", 0.0),
    ] {
        let mut steps = Vec::new();
        for &under in &QUIET_DB {
            steps.push(Step {
                midi: pitch(),
                length: 1.0,
                under: 0.0,
                gap,
            });
            steps.push(Step {
                midi: pitch(),
                length: 1.0,
                under,
                gap,
            });
        }
        steps.push(Step {
            midi: pitch(),
            length: 1.0,
            under: 0.0,
            gap: 0.0,
        });
        sections.push((kind, steps, false));
    }
    let mut draw = Rng::stream(seed, "quiet passage");
    // The quiet passage: about 45 % of the file, its notes 30 to 40 dB under, between loud phrases.
    let mut quiet = Vec::new();
    let mut length = 0.0;
    while length < 22.0 {
        let s = Step {
            midi: draw.range(lo, hi).round(),
            length: draw.range(0.5, 1.5),
            under: -draw.range(30.0, 40.0),
            gap: if draw.unit() < 0.5 {
                0.0
            } else {
                draw.range(0.2, 0.8)
            },
        };
        length += s.length + s.gap;
        quiet.push(s);
    }
    let loud_phrase = |draw: &mut Rng| -> Vec<Step> {
        (0..3)
            .map(|_| Step {
                midi: draw.range(lo, hi).round(),
                length: draw.range(0.6, 1.2),
                under: 0.0,
                gap: 0.0,
            })
            .collect()
    };
    let before = loud_phrase(&mut draw);
    let after = loud_phrase(&mut draw);
    sections.push(("loud phrase", before, false));
    sections.push(("quiet passage, 30 to 40 dB under", quiet, false));
    sections.push(("loud phrase", after, false));

    let profile = (instrument != "violin").then(|| additive::profile(instrument));
    let mut cases = Vec::new();
    for (n, (kind, steps, control)) in sections.iter().enumerate() {
        let mut rng = Rng::stream(seed, &format!("section{n}"));
        let mut case = match &profile {
            Some(p) => additive_passage(p, steps, ctx.rate, &mut rng, kind, *control),
            None => string_passage(ctx, steps, &mut rng, kind, *control)?,
        };
        case.params = obj(vec![("section", (*kind).into())]);
        cases.push(case);
    }
    let mut t = take(
        "dynamics",
        instrument,
        1,
        ctx.rate,
        cases,
        &mut Rng::stream(seed, "gaps"),
        obj(vec![
            ("absolute_levels", true.into()),
            ("loud_notes_peak_dbfs", (-6.0).into()),
            ("noise_floor_dbfs", (-70.0).into()),
        ]),
    );
    t.gaps = vec![1.0; t.cases.len()];
    Ok(vec![t])
}

/// Sets a laid-out dynamics take's level (its loud notes peaking at -6 dBFS), adds the -70 dBFS
/// noise floor, and measures every note's level into its truth.
pub fn finish_levels(take: &mut Take, laid: &mut crate::truth::Laid) {
    let rate = take.rate;
    // The loud notes' peak: within each loud note's span.
    let mut peak = 0.0f64;
    for (note, _) in &laid.notes {
        if matches!(&note.params, Json::Obj(p) if p.iter().any(|(k, v)| k == "under_loud_db" && matches!(v, Json::Num(x) if *x == 0.0)))
        {
            let (a, b) = (
                (note.onset * rate) as usize,
                ((note.end * rate) as usize).min(laid.audio.len()),
            );
            peak = laid.audio[a..b].iter().fold(peak, |p, v| p.max(v.abs()));
        }
    }
    let gain = db_to_gain(-6.0) / peak.max(1e-12);
    for v in laid.audio.iter_mut() {
        *v *= gain;
    }
    let mut rng = Rng::stream(take.seed, "noise floor");
    let floor = db_to_gain(-70.0);
    for v in laid.audio.iter_mut() {
        *v += floor * rng.normal();
    }
    take.params.push("gain_db", crate::dsp::gain_to_db(gain));
    for (note, _) in laid.notes.iter_mut() {
        let (a, b) = (
            ((note.onset + 0.1).min(note.end) * rate) as usize,
            (((note.end - 0.05).max(note.onset + 0.1)) * rate).min(laid.audio.len() as f64)
                as usize,
        );
        if b > a {
            let rms = (laid.audio[a..b].iter().map(|v| v * v).sum::<f64>() / (b - a) as f64).sqrt();
            note.params
                .push("measured_rms_dbfs", crate::dsp::gain_to_db(rms));
        }
    }
}
