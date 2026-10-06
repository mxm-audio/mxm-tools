//! Calibration before the owner sits a session (the plan's §5): the staircase recovers a simulated
//! listener's known threshold; a whole session over a real sound's perturbations gives back that
//! listener's threshold in the reading's own unit; each perturbation moves the reading it names; and
//! sessions become a thresholds table that wins over the literature in its own context.

use mxm_listening::audibility::{Thresholds, Vocabulary};
use mxm_listening::compare::compare;
use mxm_listening::describe::{Options, describe_with};
use mxm_listening::perturb::{Operator, Subject};
use mxm_listening::reading::Unit;
use mxm_listening::session::{self, Session};
use mxm_listening::staircase::{Simulated, Staircase};
use mxm_listening::{Family, Sound};

const TAU: f64 = std::f64::consts::TAU;
const RATE: f64 = 48_000.0;

/// A snare-like hit: a ring, an overtone, and a noise burst in the wires' band.
fn snare() -> Sound {
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut x = vec![0.0f32; 240];
    x.extend((0..(1.2 * RATE) as usize).map(|i| {
        let t = i as f64 / RATE;
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        let white =
            (state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 52) as f64 - 1.0;
        let ring = 0.5 * (-t / 0.2).exp() * (TAU * 200.0 * t).sin()
            + 0.25 * (-t / 0.12).exp() * (TAU * 331.0 * t).sin();
        let wires = 0.25 * (-t / 0.05).exp() * white;
        ((t * 2000.0).min(1.0) * (ring + wires)) as f32
    }));
    Sound::new("snare", RATE as u32, x)
}

#[test]
#[ignore = "slow: run before a release with `cargo test -- --ignored`; minutes in a debug build"]
fn the_staircase_recovers_a_simulated_listener() {
    for threshold in [0.5, 2.0, 10.0] {
        let mut estimates: Vec<f64> = (0..200u64)
            .map(|seed| {
                let mut listener = Simulated::new(threshold, 3.5, 0.02, seed + 1);
                let mut stair = Staircase::new(8.0 * threshold, 0.01 * threshold, 50.0 * threshold);
                while !stair.done() {
                    let correct = listener.answer(stair.size);
                    stair.record(correct);
                }
                stair.estimate().unwrap_or(f64::NAN)
            })
            .filter(|v| v.is_finite())
            .collect();
        assert!(
            estimates.len() >= 190,
            "{} of 200 finished",
            estimates.len()
        );
        estimates.sort_by(f64::total_cmp);
        let median = estimates[estimates.len() / 2];
        let (q1, q3) = (
            estimates[estimates.len() / 4],
            estimates[3 * estimates.len() / 4],
        );
        // The median within 15 % of the truth; one session's spread within a factor of 1.6.
        assert!(
            (median / threshold - 1.0).abs() < 0.15,
            "{threshold}: median {median}"
        );
        assert!(q3 / q1 < 1.6, "{threshold}: quartiles {q1}–{q3}");
    }
}

#[test]
#[ignore = "slow: run before a release with `cargo test -- --ignored`; minutes in a debug build"]
fn a_session_gives_back_a_listeners_threshold_in_the_readings_unit() {
    let subject = Subject::new(snare(), Some(Family::Percussive));
    let mut values = Vec::new();
    for seed in 1..=5u64 {
        let mut s = Session::new(subject.clone(), &[Operator::Level], "test", seed);
        let mut listener = Simulated::new(0.8, 3.5, 0.02, 100 + seed);
        while let Some(trial) = s.next_trial() {
            let right = listener.answer(trial.size);
            let choice = if right {
                trial.odd
            } else {
                (trial.odd + 1) % 3
            };
            s.answer(trial.id, choice).expect("the pending trial");
        }
        let (checks, right) = s.checks();
        assert!(
            checks > 0 && right * 10 >= checks * 8,
            "checks {right} of {checks}"
        );
        let out = s.outcomes(&Thresholds::literature());
        let level = out
            .iter()
            .find(|o| o.reading == "level.body_loudness")
            .expect("the level outcome");
        values.push(level.value);
    }
    values.sort_by(f64::total_cmp);
    let median = values[values.len() / 2];
    // The level operator moves body loudness by exactly its size in dB, so the owner's threshold in
    // dB is the staircase's answer: 0.8 dB, within one session's precision.
    assert!(
        (median / 0.8 - 1.0).abs() < 0.25,
        "median {median} dB of {values:?}"
    );
}

#[test]
#[ignore = "slow: run before a release with `cargo test -- --ignored`; minutes in a debug build"]
fn each_perturbation_moves_the_reading_it_names() {
    let sound = snare();
    let subject = Subject::new(sound.clone(), Some(Family::Percussive));
    let options = Options {
        family: Some(Family::Percussive),
        without_perception: true,
        ..Options::default()
    };
    let reference = describe_with(&sound, &options);
    let ops = [
        (Operator::Level, 1.0),
        (Operator::Pitch, 1.0),
        (Operator::DecayShorter, -1.0),
        (
            Operator::Band {
                lo: 200.0,
                hi: 400.0,
            },
            1.0,
        ),
        (Operator::AttackSofter, 1.0),
        (Operator::Noise, 1.0),
        (Operator::Wobble, 1.0),
        (Operator::Click, 1.0),
    ];
    for (op, side) in ops {
        let (start, _, _) = op.range();
        let candidate = subject
            .perturbed(op, start)
            .unwrap_or_else(|| panic!("{}: not applicable", op.name()));
        assert_eq!(candidate.samples.len(), sound.samples.len());
        let c = compare(
            &reference,
            &describe_with(&candidate, &options),
            &Thresholds::literature(),
            &Vocabulary::owner(),
        );
        let (id, band) = op.reading();
        let f = c
            .findings
            .iter()
            .filter(|f| {
                f.id == id
                    && band.is_none_or(|lo| f.band_hz.is_some_and(|b| (b.0 - lo).abs() < 1.0))
            })
            .max_by(|a, b| a.units.total_cmp(&b.units))
            .unwrap_or_else(|| panic!("{}: {id} not compared", op.name()));
        assert!(
            f.units >= 1.0 && (f.candidate - f.reference) * side > 0.0,
            "{}: {id} {} → {} ({:.1}×)",
            op.name(),
            f.reference,
            f.candidate,
            f.units
        );
        // Every operator but the level's leaves the body loudness where it was.
        if op != Operator::Level {
            let loud = c
                .findings
                .iter()
                .find(|f| f.id == "level.body_loudness")
                .map_or(0.0, |f| f.units);
            assert!(loud < 0.5, "{}: loudness moved {loud:.2}×", op.name());
        }
    }
}

#[test]
#[ignore = "slow: run before a release with `cargo test -- --ignored`; minutes in a debug build"]
fn every_answer_counts_and_the_table_wins_in_its_own_context() {
    use mxm_listening::audibility::Kind;
    use mxm_listening::session::{Answer, fit, table};
    // A listener whose threshold is a 1.5 dB change, answering a staircase that never finished: 24
    // trials, too few reversals for the staircase's own estimate, fitted from every answer.
    let mut listener = Simulated::new(1.5, 3.5, 0.02, 11);
    let mut stair = Staircase::new(8.0, 0.1, 24.0);
    let mut answers = Vec::new();
    for _ in 0..24 {
        let ok = listener.answer(stair.size);
        answers.push(Answer {
            reading: "tone.band_level".into(),
            kind: Kind::Absolute,
            context: "percussive hit".into(),
            delta: stair.size,
            correct: ok,
            session: "2026-09-27".into(),
        });
        stair.record(ok);
    }
    let pairs: Vec<(f64, bool)> = answers.iter().map(|a| (a.delta, a.correct)).collect();
    let f = fit(&pairs).expect("a fit");
    assert!(
        f.threshold > 0.75 && f.threshold < 3.0,
        "threshold {}",
        f.threshold
    );
    let (text, lines) = table(&answers);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let owner = Thresholds::parse(&text);
    let merged = Thresholds::literature().overlaid(owner);
    let own = merged
        .for_reading_in("tone.band_level", Unit::Decibels, Some("percussive hit"))
        .unwrap();
    assert!(
        own.source.starts_with("owner: 24 answers"),
        "{}",
        own.source
    );
    // Elsewhere the literature's stays.
    let other = merged
        .for_reading_in("tone.band_level", Unit::Decibels, Some("unknown"))
        .unwrap();
    assert!(!other.source.starts_with("owner"), "{}", other.source);
    // Every answer right at sizes the listener always hears says only "under" them: not used.
    let easy: Vec<(f64, bool)> = (0..10).map(|_| (8.0, true)).collect();
    let f = fit(&easy).unwrap();
    assert!(f.low.is_none(), "all right bounds nothing below: {f:?}");
}

#[test]
#[ignore = "slow: run before a release with `cargo test -- --ignored`; minutes in a debug build"]
fn a_session_log_reads_back_without_its_check_trials() {
    let text = "# listen session
# date	2026-09-27T08-57-18Z
# sound	snare
# family	percussive hit
# device	headphones
# file	C:/sounds/snare.wav
trial	operator	check	size	odd	choice	correct
1	pitch	false	60	0	0	true
2	band-1600-6400	false	8	2	1	false
3	decay	true	400	2	2	true
";
    let log = session::parse_log(text);
    assert_eq!(log.file.as_deref(), Some("C:/sounds/snare.wav"));
    assert_eq!(log.family, "percussive hit");
    assert_eq!(log.trials.len(), 2);
    assert_eq!(
        log.trials[1].0,
        Operator::Band {
            lo: 1600.0,
            hi: 6400.0
        }
    );
    assert!(!log.trials[1].2);
}

/// A stiff string's note: 30 partials on the stiff-string law (B = 10⁻⁴), falling 6 dB an octave, each
/// decaying as T60 ∝ (f/f₁)^−0.7 from 2 s; with `vibrato`, a held note instead, its pitch swept ±5 cents
/// at 6 Hz (the operator's own vibrato sits at 5 Hz).
fn string(vibrato: bool) -> Sound {
    let f0 = 220.0;
    let len = (3.0 * RATE) as usize;
    let mut x = vec![0.0f32; 480];
    x.extend((0..len).map(|i| {
        let t = i as f64 / RATE;
        let env = if vibrato { (t / 0.05).min(1.0) } else { 1.0 };
        // The vibrato's phase in cycles per hertz: ∫ 2^(c(τ)/1200) dτ with c = 5·sin(2π·6·τ), to first
        // order in c.
        let wobble = if vibrato {
            5.0 * std::f64::consts::LN_2 / 1200.0 * (1.0 - (TAU * 6.0 * t).cos()) / (TAU * 6.0)
        } else {
            0.0
        };
        let mut v = 0.0;
        for n in 1..=30 {
            let nf = f64::from(n);
            let f = nf * f0 * (1.0 + 1e-4 * nf * nf).sqrt();
            let t60 = 2.0 * (f / f0).powf(-0.7);
            let decay = if vibrato {
                1.0
            } else {
                (-6.91 * t / t60).exp()
            };
            v += 0.3 / nf * decay * (TAU * f * (t + wobble)).sin();
        }
        (env * v) as f32
    }));
    Sound::new(if vibrato { "held" } else { "string" }, RATE as u32, x)
}

#[test]
#[ignore = "slow: run before a release with `cargo test -- --ignored`; minutes in a debug build"]
fn each_note_perturbation_moves_the_reading_it_names() {
    for (sound, family, ops) in [
        (
            string(false),
            Family::Note,
            vec![
                (Operator::Tilt, 1.0),
                (Operator::DecayLaw, -1.0),
                (Operator::Inharmonicity, 1.0),
            ],
        ),
        (
            string(true),
            Family::Sustained,
            vec![(Operator::Vibrato, 1.0)],
        ),
    ] {
        let subject = Subject::new(sound.clone(), Some(family));
        let options = Options {
            family: Some(family),
            without_perception: true,
            ..Options::default()
        };
        let reference = describe_with(&sound, &options);
        for (op, side) in ops {
            let (start, _, _) = op.range();
            let candidate = subject
                .perturbed(op, start)
                .unwrap_or_else(|| panic!("{}: not applicable", op.name()));
            assert_eq!(candidate.samples.len(), sound.samples.len());
            let c = compare(
                &reference,
                &describe_with(&candidate, &options),
                &Thresholds::literature(),
                &Vocabulary::owner(),
            );
            let (id, _) = op.reading();
            let f = c
                .findings
                .iter()
                .filter(|f| f.id == id)
                .max_by(|a, b| a.units.total_cmp(&b.units))
                .unwrap_or_else(|| panic!("{}: {id} not compared", op.name()));
            assert!(
                f.units >= 1.0 && (f.candidate - f.reference) * side > 0.0,
                "{}: {id} {} → {} ({:.1}×)",
                op.name(),
                f.reference,
                f.candidate,
                f.units
            );
            let loud = c
                .findings
                .iter()
                .find(|f| f.id == "level.body_loudness")
                .map_or(0.0, |f| f.units);
            assert!(loud < 0.5, "{}: loudness moved {loud:.2}×", op.name());
            // The note's pitch stays: its fundamental is no finding of its own.
            let pitch = c
                .findings
                .iter()
                .find(|f| f.id == "note.tuning")
                .map_or(0.0, |f| f.units);
            assert!(pitch < 1.0, "{}: the tuning moved {pitch:.2}×", op.name());
        }
    }
}
