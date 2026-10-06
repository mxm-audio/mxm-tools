//! A pitched note (L5a): the ideal objects' ratios against the research table, and synthetic struck
//! notes whose partials, decay law, tilt and object are known by construction.

use mxm_listening::Sound;
use mxm_listening::audibility::{Thresholds, Vocabulary};
use mxm_listening::compare::compare;
use mxm_listening::describe::{Options, describe, describe_with};
use mxm_listening::family::Family;
use mxm_listening::objects;
use mxm_listening::reading::Report;

const RATE: f64 = 48_000.0;

/// `research:physical-modelling/physical-modelling-synthesis.md` §6.2, computed there from the same
/// equations and checked against Essl & Cook, Falaize & Hélie and *The Sounding Object*.
#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn the_computed_objects_match_the_research_table() {
    let all = objects::all();
    let row = |name: &str| {
        all.iter()
            .find(|o| o.name.starts_with(name))
            .unwrap()
            .ratios
            .clone()
    };
    let close = |got: &[f64], want: &[f64]| {
        assert!(got.len() >= want.len(), "{got:?}");
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() < 0.0015 * w, "{got:?} against {want:?}");
        }
    };
    close(
        &row("uniform bar, free"),
        &[1.0, 2.757, 5.404, 8.933, 13.344, 18.638],
    );
    close(&row("bar clamped"), &[1.0, 6.267, 17.547, 34.386, 56.843]);
    close(
        &row("circular membrane"),
        &[
            1.0, 1.593, 2.136, 2.295, 2.653, 2.917, 3.155, 3.500, 3.598, 3.647,
        ],
    );
}

/// A struck note: partials at `ratios` of `f1`, each starting `tilt` dB an octave down from the
/// fundamental and decaying with `T60 = t60 · (f/f1)^law`, from 20 ms into the file, over a quiet
/// noise floor; `split` adds a second line under the fundamental, `split` Hz away and 6 dB down.
fn note(f1: f64, ratios: &[f64], tilt: f64, t60: f64, law: f64, split: Option<f64>) -> Vec<f32> {
    let mut lines = lines(f1, ratios, tilt, t60, law);
    if let Some(s) = split {
        lines.push((f1 - s, 0.15, t60));
    }
    struck(&lines)
}

/// The lines of [`note`]: `(Hz, amplitude, T60 s)`.
fn lines(f1: f64, ratios: &[f64], tilt: f64, t60: f64, law: f64) -> Vec<(f64, f64, f64)> {
    ratios
        .iter()
        .map(|&r| {
            let amp = 0.3 * 10f64.powf(tilt * r.log2() / 20.0);
            (f1 * r, amp, t60 * r.powf(law))
        })
        .collect()
}

/// `lines` struck together 20 ms into a 3 s file over a floor 80 dB down.
fn struck(lines: &[(f64, f64, f64)]) -> Vec<f32> {
    let len = (3.0 * RATE) as usize;
    let start = (0.02 * RATE) as usize;
    let mut x = vec![0.0f64; len];
    for (i, v) in x.iter_mut().enumerate().skip(start) {
        let t = (i - start) as f64 / RATE;
        for &(hz, amp, t60) in lines {
            let decay = (-(1000f64.ln()) * t / t60).exp();
            *v += amp * decay * (std::f64::consts::TAU * hz * t).sin();
        }
    }
    // A floor 80 dB down, from a fixed pseudo-random sequence.
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    for v in &mut x {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        *v += 3e-5 * ((seed >> 11) as f64 / (1u64 << 53) as f64 - 0.5);
    }
    x.iter().map(|&v| v as f32).collect()
}

fn value(r: &Report, id: &str) -> Option<f64> {
    r.find(id, None).and_then(|x| x.value)
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_free_bar_reads_its_law_its_tilt_and_its_object() {
    let x = note(440.0, &[1.0, 2.7565, 5.4039, 8.9330], -6.0, 2.0, -0.8, None);
    let r = describe(&Sound::new("bar", RATE as u32, x), Some(Family::Note));
    let tuning = value(&r, "note.tuning").unwrap();
    assert!(tuning.abs() < 3.0, "tuning {tuning}");
    // T60 from a line fitted over the fall above the floor: a few percent.
    let t60 = value(&r, "note.t60").unwrap();
    assert!((t60 - 2.0).abs() < 0.1, "T60 {t60}");
    let law = value(&r, "note.decay_law").unwrap();
    assert!((law + 0.8).abs() < 0.08, "law {law}");
    let tilt = value(&r, "note.tilt").unwrap();
    assert!((tilt + 6.0).abs() < 0.8, "tilt {tilt}");
    let cents = value(&r, "note.ideal_cents").unwrap();
    assert!(cents < 5.0, "ideal {cents} c");
    let table = r
        .sections
        .iter()
        .flat_map(|s| &s.tables)
        .find(|t| t.id == "note.partials")
        .unwrap();
    assert!(
        table.title.contains("like the uniform bar, free ends"),
        "{}",
        table.title
    );
    assert!(table.title.contains("A4"), "{}", table.title);
    assert_eq!(value(&r, "note.partials"), Some(4.0));
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_tuned_bar_with_a_split_fundamental_reads_both() {
    // A marimba-like bar: 1 : 4 : 10, the fundamental split 5 Hz by its resonator, partials dying
    // faster than the fundamental as in wood (constant Q, M = −1).
    let x = note(261.63, &[1.0, 4.0, 10.0], -9.0, 1.2, -1.0, Some(5.0));
    let r = describe(&Sound::new("marimba", RATE as u32, x), Some(Family::Note));
    let table = r
        .sections
        .iter()
        .flat_map(|s| &s.tables)
        .find(|t| t.id == "note.partials")
        .unwrap();
    assert!(
        table.title.contains("like the tuned bar 1 : 4 : 10"),
        "{}",
        table.title
    );
    assert!(table.title.contains("C4"), "{}", table.title);
    let split = value(&r, "note.split").unwrap();
    assert!((split - 5.0).abs() < 1.0, "split {split}");
    let law = value(&r, "note.decay_law").unwrap();
    assert!((law + 1.0).abs() < 0.15, "law {law}");
    // Beating with its split partner, the fundamental's decay still reads near its own.
    let t60 = value(&r, "note.t60").unwrap();
    assert!((t60 - 1.2).abs() < 0.15, "T60 {t60}");
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_note_struck_over_the_last_one_is_timed_from_its_own_strike() {
    // The note before rings at a third of the new note's level when the new one is struck, 20 ms in.
    let mut x = note(440.0, &[1.0, 2.7565], -6.0, 2.0, -0.8, None);
    for (i, v) in x.iter_mut().enumerate() {
        let t = i as f64 / RATE + 1.0;
        *v += (0.1 * (-t / 1.5).exp() * (std::f64::consts::TAU * 392.0 * t).sin()) as f32;
    }
    let r = describe(&Sound::new("run cut", RATE as u32, x), Some(Family::Note));
    let onset = r.onset_s.unwrap();
    assert!((onset - 0.02).abs() < 0.002, "onset {onset} s");
    // The percussive rule would start at the file's first sample.
    let p = describe(
        &Sound::new(
            "run cut",
            RATE as u32,
            note(440.0, &[1.0], -6.0, 2.0, -0.8, None),
        ),
        Some(Family::Percussive),
    );
    assert!(p.find("note.tuning", None).is_none());
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn two_notes_compare_partial_by_partial() {
    // The candidate's 4× partial is 4 dB louder and dies twice as fast; its 10× is the reference's.
    let reference = lines(261.63, &[1.0, 4.0, 10.0], -9.0, 1.2, -1.0);
    let mut changed = reference.clone();
    changed[1].1 *= 10f64.powf(4.0 / 20.0);
    changed[1].2 /= 2.0;
    let a = describe(
        &Sound::new("reference", RATE as u32, struck(&reference)),
        Some(Family::Note),
    );
    let b = describe(
        &Sound::new("candidate", RATE as u32, struck(&changed)),
        Some(Family::Note),
    );
    let c = compare(&a, &b, &Thresholds::literature(), &Vocabulary::owner());
    let audible: Vec<_> = c.audible().filter(|f| f.part == "note").collect();
    let at = |id: &str, hz: f64| {
        audible
            .iter()
            .find(|f| f.id == id && f.band_hz.is_some_and(|b| b.0 <= hz && hz <= b.1))
    };
    let level = at("note.partial_level", 1046.5).expect("the 4× partial's level");
    assert!(
        (level.candidate - level.reference - 4.0).abs() < 1.0,
        "{level:?}"
    );
    assert_eq!(
        level.phrase.as_deref(),
        Some("an overtone is louder at the strike")
    );
    let t60 = at("note.partial_t60", 1046.5).expect("the 4× partial's decay");
    assert!(
        (t60.candidate / t60.reference - 0.5).abs() < 0.08,
        "{t60:?}"
    );
    assert!(at("note.partial_level", 2616.3).is_none());
    assert!(at("note.partial_t60", 2616.3).is_none());
    assert!(at("note.partial_tuning", 1046.5).is_none());
    // The fundamental is the same: no tuning, no T60.
    assert!(
        audible
            .iter()
            .all(|f| f.id != "note.tuning" && f.id != "note.t60"),
        "{audible:?}"
    );
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_key_map_reads_a_decay_halving_each_octave() {
    // A free bar at C3, C4 and C5, its fundamental ringing 2 s at middle C and half as long for
    // each octave up; the tilt and the decay law the same at every key.
    let reports: Vec<Report> = [-1.0f64, 0.0, 1.0]
        .iter()
        .map(|&octave| {
            let f1 = mxm_listening::keymap::MIDDLE_C_HZ * 2f64.powf(octave);
            let x = struck(&lines(
                f1,
                &[1.0, 2.7565, 5.4039],
                -6.0,
                2.0 * 0.5f64.powf(octave),
                -0.8,
            ));
            describe(&Sound::new("key", RATE as u32, x), Some(Family::Note))
        })
        .collect();
    let refs: Vec<&Report> = reports.iter().collect();
    let trends = mxm_listening::keymap::trends(&refs);
    let t = |id: &str| trends.iter().find(|t| t.id == id).unwrap();
    let t60 = t("note.t60");
    assert!(t60.log);
    assert!((t60.at_middle_c - 2.0).abs() < 0.1, "{t60:?}");
    assert!((t60.per_octave - 0.5).abs() < 0.03, "{t60:?}");
    let tilt = t("note.tilt");
    assert!(
        (tilt.at_middle_c + 6.0).abs() < 0.8 && tilt.per_octave.abs() < 0.5,
        "{tilt:?}"
    );
    let law = t("note.decay_law");
    assert!(
        (law.at_middle_c + 0.8).abs() < 0.1 && law.per_octave.abs() < 0.1,
        "{law:?}"
    );
    assert_eq!(t60.notes, 3);
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_caller_can_name_the_note_a_sympathetic_line_would_take() {
    // A high tine at Bb5, short, over a lower tine at F3 ringing in sympathy: weaker at the strike,
    // ringing ten times as long, the most energetic line in the file.
    let mut lines = lines(932.33, &[1.0, 2.7565], -6.0, 0.4, -0.8);
    lines.push((174.61, 0.12, 4.0));
    let x = struck(&lines);
    let plain = describe(
        &Sound::new("tine", RATE as u32, x.clone()),
        Some(Family::Note),
    );
    let f1 = value(&plain, "note.fundamental").unwrap();
    assert!(
        (f1 - 174.61).abs() < 2.0,
        "without the note named, the sympathetic line: {f1}"
    );
    let named = describe_with(
        &Sound::new("tine", RATE as u32, x),
        &Options {
            family: Some(Family::Note),
            expect_hz: mxm_listening::split::note_hz("Bb5"),
            ..Options::default()
        },
    );
    let f1 = value(&named, "note.fundamental").unwrap();
    assert!((f1 - 932.33).abs() < 2.0, "the tine named: {f1}");
    assert!(value(&named, "note.tuning").unwrap().abs() < 3.0);
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_tubular_bell_is_heard_at_its_strike_note() {
    // A free bar whose 4th to 6th modes (8.93 : 13.34 : 18.64, near 2 : 3 : 4) are its loudest, as a
    // tube's are, the 4th tuned to twice C5: no partial sits at C5, and the ear hears C5.
    let mode1 = 2.0 * 523.25 / 8.933;
    let ratios = [1.0, 2.7565, 5.4039, 8.9330, 13.3443, 18.6380];
    let levels = [0.02, 0.03, 0.05, 0.3, 0.25, 0.2];
    let lines: Vec<(f64, f64, f64)> = ratios
        .iter()
        .zip(levels)
        .map(|(&r, a)| (mode1 * r, a, 2.5))
        .collect();
    let r = describe(
        &Sound::new("tube", RATE as u32, struck(&lines)),
        Some(Family::Note),
    );
    let heard = value(&r, "note.virtual_pitch").unwrap();
    let cents = 1200.0 * (heard / 523.25).log2();
    assert!(
        cents.abs() < 30.0,
        "heard {heard} Hz, {cents:+.0} c from C5"
    );
    let f1 = value(&r, "note.fundamental").unwrap();
    assert!(
        (1200.0 * (f1 / 523.25).log2()).abs() > 100.0,
        "no partial at C5: {f1}"
    );
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_stiff_string_reads_its_inharmonicity_and_where_it_was_plucked() {
    // A2 plucked a fifth of the way along (harmonics 5, 10, 15 and 20 silent), stiff enough to stretch
    // its 24th harmonic by 11 %, the harmonics dying faster the higher they are.
    let (f0, b, beta) = (110.0, 4e-4, 0.2);
    let lines: Vec<(f64, f64, f64)> = (1..=24)
        .filter_map(|n| {
            let n = f64::from(n);
            let a = 0.3 * (n * std::f64::consts::PI * beta).sin().abs() / n;
            (a > 1e-3).then(|| (n * f0 * (1.0 + b * n * n).sqrt(), a, 3.0 / n.sqrt()))
        })
        .collect();
    let r = describe(
        &Sound::new("string", RATE as u32, struck(&lines)),
        Some(Family::Note),
    );
    let f1 = value(&r, "note.fundamental").unwrap();
    assert!((f1 - f0 * (1.0 + b).sqrt()).abs() < 0.3, "fundamental {f1}");
    let inharmonicity = value(&r, "note.inharmonicity").unwrap();
    assert!((inharmonicity / b - 1.0).abs() < 0.1, "B {inharmonicity}");
    let position = value(&r, "note.excitation_position").unwrap();
    assert!((position - beta).abs() < 0.02, "plucked at {position}");
    assert!(value(&r, "note.harmonics").unwrap() >= 18.0);
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_bar_has_no_harmonic_series() {
    let r = describe(
        &Sound::new(
            "bar",
            RATE as u32,
            note(261.63, &[1.0, 4.0, 10.0], -9.0, 1.2, -1.0, None),
        ),
        Some(Family::Note),
    );
    assert!(value(&r, "note.inharmonicity").is_none());
    assert!(value(&r, "note.excitation_position").is_none());
}

/// One partial at `hz` whose level follows `level(t)` (linear amplitude), 20 ms into a 4 s file.
fn shaped(hz: f64, level: impl Fn(f64) -> f64) -> Vec<f32> {
    let len = (4.0 * RATE) as usize;
    let start = (0.02 * RATE) as usize;
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    (0..len)
        .map(|i| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let noise = 3e-5 * ((seed >> 11) as f64 / (1u64 << 53) as f64 - 0.5);
            let v = if i < start {
                0.0
            } else {
                let t = (i - start) as f64 / RATE;
                level(t) * (std::f64::consts::TAU * hz * t).sin()
            };
            (v + noise) as f32
        })
        .collect()
}

fn decays(t60: f64, t: f64) -> f64 {
    (-(1000f64.ln()) * t / t60).exp()
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_two_stage_decay_and_a_damper_are_told_apart() {
    // A piano's aftersound: a prompt decay (T60 0.5 s) over a slow one 20 dB down (T60 4 s).
    let x = shaped(220.0, |t| 0.3 * decays(0.5, t) + 0.03 * decays(4.0, t));
    let r = describe(
        &Sound::new("aftersound", RATE as u32, x),
        Some(Family::Note),
    );
    // Two lines through a sum of two decays average across its bend: 0.5 s and 4 s read about 3.8.
    let ratio = value(&r, "note.two_stage").unwrap();
    assert!(
        (3.0..5.0).contains(&ratio),
        "second stage {ratio}× the first"
    );
    assert!(value(&r, "note.release_at").is_none());
    // A damper at 1.2 s on a note ringing T60 3 s: 0.15 s after it.
    let x = shaped(220.0, |t| {
        if t < 1.2 {
            0.3 * decays(3.0, t)
        } else {
            0.3 * decays(3.0, 1.2) * decays(0.15, t - 1.2)
        }
    });
    let r = describe(&Sound::new("damped", RATE as u32, x), Some(Family::Note));
    // The energy decay curve bends before the damper comes down: about 0.1 s early here.
    let at = value(&r, "note.release_at").unwrap();
    assert!((1.05..=1.22).contains(&at), "damper at {at} s");
    let t60 = value(&r, "note.release_t60").unwrap();
    assert!((t60 - 0.15).abs() < 0.05, "under the damper {t60} s");
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_set_of_notes_reveals_a_body_resonance_that_stays_put() {
    // Five strings across two octaves, each −6 dB an octave, all through one body that lifts
    // everything near 1 kHz by 8 dB.
    let body = |hz: f64| 8.0 * (-0.5 * ((hz / 1000.0).log2() / 0.33).powi(2)).exp();
    let reports: Vec<Report> = [110.0, 147.0, 196.0, 262.0, 349.0]
        .iter()
        .map(|&f0| {
            let lines: Vec<(f64, f64, f64)> = (1..=12)
                .map(|n| {
                    let hz = f0 * f64::from(n);
                    let db = -6.0 * f64::from(n).log2() + body(hz);
                    (hz, 0.2 * 10f64.powf(db / 20.0), 2.0)
                })
                .collect();
            describe(
                &Sound::new("string", RATE as u32, struck(&lines)),
                Some(Family::Note),
            )
        })
        .collect();
    let refs: Vec<&Report> = reports.iter().collect();
    let bands = mxm_listening::keymap::body(&refs);
    let at = |hz: f64| {
        bands
            .iter()
            .min_by(|a, b| (a.0 / hz).ln().abs().total_cmp(&(b.0 / hz).ln().abs()))
            .map(|b| b.1)
            .unwrap()
    };
    let peak = bands.iter().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
    assert!(
        (peak.0 / 1000.0).log2().abs() <= 1.0 / 3.0 + 1e-9,
        "body peak at {} Hz",
        peak.0
    );
    assert!(
        at(1000.0) - at(250.0) > 4.0 && at(1000.0) - at(4000.0) > 4.0,
        "{bands:?}"
    );
}

/// A held harmonic tone (ten harmonics at 1/n) at `f0`: it starts 80 c flat and settles over 150 ms,
/// drifts `drift` c/s, carries a 5.5 Hz vibrato `depth` c deep, and a breath 30 dB down.
fn held(f0: f64, depth: f64, drift: f64) -> Vec<f32> {
    let len = (2.5 * RATE) as usize;
    let mut phase = 0.0f64;
    let mut seed = 0x0123_4567_89ab_cdefu64;
    (0..len)
        .map(|i| {
            let t = i as f64 / RATE;
            let glide = -80.0 * (-t / 0.05).exp();
            let cents = glide + drift * t + depth * (std::f64::consts::TAU * 5.5 * t).sin();
            phase += std::f64::consts::TAU * f0 * 2f64.powf(cents / 1200.0) / RATE;
            let env = (t / 0.02).min(1.0) * ((2.5 - t) / 0.1).clamp(0.0, 1.0);
            let tone: f64 = (1..=10)
                .map(|n| (f64::from(n) * phase).sin() / f64::from(n))
                .sum();
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let breath = 0.01 * ((seed >> 11) as f64 / (1u64 << 53) as f64 - 0.5);
            (0.2 * env * tone + env * breath) as f32
        })
        .collect()
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_held_note_reads_its_vibrato_drift_and_settling() {
    let r = describe(
        &Sound::new("bowed", RATE as u32, held(293.66, 25.0, 10.0)),
        Some(Family::Sustained),
    );
    let rate = value(&r, "sustain.vibrato_rate").unwrap();
    assert!((rate - 5.5).abs() < 0.2, "vibrato {rate} Hz");
    let depth = value(&r, "sustain.vibrato_depth").unwrap();
    assert!((depth - 25.0).abs() < 5.0, "vibrato ±{depth} c");
    let drift = value(&r, "sustain.drift").unwrap();
    assert!((drift - 10.0).abs() < 3.0, "drift {drift} c/s");
    let settle = value(&r, "sustain.settle").unwrap();
    assert!(settle < 300.0, "settles at {settle} ms");
    assert!(value(&r, "sustain.hnr").unwrap() > 10.0);
    let plain = describe(
        &Sound::new("bowed", RATE as u32, held(293.66, 0.0, 0.0)),
        Some(Family::Sustained),
    );
    assert!(
        value(&plain, "sustain.vibrato_rate").is_none(),
        "no vibrato"
    );
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_body_line_under_every_note_is_found_across_the_set_and_kept_out() {
    // Five strings, each over a body resonance at 100 Hz that rings longer than they do.
    let notes = [147.0, 196.0, 262.0, 349.0, 440.0];
    let sounds: Vec<Sound> = notes
        .iter()
        .map(|&f0| {
            let mut lines: Vec<(f64, f64, f64)> = (1..=10)
                .map(|n| (f0 * f64::from(n), 0.2 / f64::from(n), 2.0))
                .collect();
            lines.push((100.0, 0.2, 4.0));
            Sound::new("guitar", RATE as u32, struck(&lines))
        })
        .collect();
    let first: Vec<Report> = sounds
        .iter()
        .map(|s| describe(s, Some(Family::Note)))
        .collect();
    let refs: Vec<&Report> = first.iter().collect();
    let body = mxm_listening::keymap::recurring_lines(&refs);
    assert!(body.iter().any(|hz| (hz - 100.0).abs() < 1.5), "{body:?}");
    for (s, &f0) in sounds.iter().zip(&notes) {
        let r = describe_with(
            s,
            &Options {
                family: Some(Family::Note),
                room_lines: body.clone(),
                ..Options::default()
            },
        );
        let f1 = value(&r, "note.fundamental").unwrap();
        assert!((f1 / f0 - 1.0).abs() < 0.01, "{f0} Hz read as {f1}");
    }
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_string_reads_its_polarisations_beating_a_strum_and_a_pickups_harmonics() {
    // Two polarisations of one fundamental, 1.5 Hz apart, over a second harmonic.
    let lines = [(220.0, 0.2, 3.0), (221.5, 0.12, 3.0), (441.0, 0.08, 2.0)];
    let r = describe(
        &Sound::new("string", RATE as u32, struck(&lines)),
        Some(Family::Note),
    );
    let beat = value(&r, "note.beat_rate").unwrap();
    assert!((beat - 1.5).abs() < 0.2, "beating at {beat} Hz");
    assert!(value(&r, "note.beat_depth").unwrap() > 3.0);
    assert_eq!(value(&r, "note.onsets"), Some(1.0), "one string, one onset");

    // Three strings of a chord, 30 ms apart, each starting with its pick's 3 ms contact transient:
    // a pluck's broadband click is what parts a string's onset from the strings already sounding.
    let mut x = vec![0.0f32; (3.0 * RATE) as usize];
    let mut seed = 0x5eed_u64;
    for (k, hz) in [196.0f64, 247.0, 294.0].iter().enumerate() {
        let shift = (k as f64 * 0.03 * RATE) as usize;
        let string = struck(&[(*hz, 0.1, 2.0), (2.0 * hz, 0.05, 1.5)]);
        let pick = (0.02 * RATE) as usize + shift;
        for (i, v) in string.iter().enumerate() {
            if i + shift < x.len() {
                x[i + shift] += v;
            }
        }
        for v in x.iter_mut().skip(pick).take((0.003 * RATE) as usize) {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            *v += 0.05 * ((seed >> 11) as f64 / (1u64 << 53) as f64 - 0.5) as f32;
        }
    }
    let r = describe(&Sound::new("strum", RATE as u32, x), Some(Family::Note));
    assert_eq!(
        value(&r, "note.onsets"),
        Some(3.0),
        "{:?}",
        value(&r, "note.onset_spread")
    );
    let spread = value(&r, "note.onset_spread").unwrap();
    assert!((spread - 60.0).abs() < 15.0, "strummed over {spread} ms");

    // A pickup that halves its even harmonics twice over: 12 dB under a 1/n series.
    let lines: Vec<(f64, f64, f64)> = (1..=8)
        .map(|n| {
            let even = if n % 2 == 0 { 0.25 } else { 1.0 };
            (262.0 * f64::from(n), 0.2 * even / f64::from(n), 2.0)
        })
        .collect();
    let r = describe(
        &Sound::new("tine", RATE as u32, struck(&lines)),
        Some(Family::Note),
    );
    let h2 = value(&r, "note.h2").unwrap();
    assert!((h2 + 18.1).abs() < 1.0, "second harmonic {h2} dB");
    let h3 = value(&r, "note.h3").unwrap();
    assert!((h3 + 9.5).abs() < 1.0, "third harmonic {h3} dB");
    let even_odd = value(&r, "note.even_odd").unwrap();
    assert!(
        (even_odd + 9.7).abs() < 1.5,
        "even against odd {even_odd} dB"
    );
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_steady_rich_tone_reads_its_own_fundamental() {
    // Thirty harmonics at 1/n that neither decay nor grow, noise-free, as a synth holds them: a
    // steady partial's pole sits on the unit circle, and one rounded just outside it had been dropped
    // as growing, so the 220 Hz tone read 660.
    for f0 in [110.0, 220.0, 330.0] {
        let x: Vec<f32> = (0..(2.0 * RATE) as usize)
            .map(|i| {
                let t = i as f64 / RATE;
                let tone: f64 = (1..=30)
                    .map(|n| {
                        0.3 / f64::from(n) * (std::f64::consts::TAU * f0 * f64::from(n) * t).sin()
                    })
                    .sum();
                ((t / 0.05).min(1.0) * tone) as f32
            })
            .collect();
        let r = describe(
            &Sound::new("steady", RATE as u32, x),
            Some(Family::Sustained),
        );
        let f = value(&r, "note.fundamental").unwrap();
        assert!((1200.0 * (f / f0).log2()).abs() < 1.0, "{f0}: read {f}");
    }
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_harmonic_deficit_names_the_missing_and_the_sunk_harmonics() {
    // Twelve harmonics of 196 Hz at 1/n; plucked at a quarter of the length, the fourth, eighth and
    // twelfth are missing; blown as a clarinet, the even half sits 25 dB down.
    let read = |level: &dyn Fn(usize) -> Option<f64>| {
        let lines: Vec<(f64, f64, f64)> = (1..=12)
            .filter_map(|n| level(n).map(|a| (196.0 * n as f64, a, 2.5)))
            .collect();
        let r = describe_with(
            &Sound::new("deficit", RATE as u32, struck(&lines)),
            &Options {
                family: Some(Family::Note),
                without_perception: true,
                ..Options::default()
            },
        );
        let orders: Vec<usize> = r
            .sections
            .iter()
            .flat_map(|s| &s.tables)
            .find(|t| t.id == "note.deficit")
            .map(|t| {
                t.rows
                    .iter()
                    .filter_map(|row| row[0])
                    .map(|n| n as usize)
                    .collect()
            })
            .unwrap_or_default();
        (value(&r, "note.deficit"), orders)
    };
    let full = read(&|n| Some(0.2 / n as f64));
    assert_eq!(full.0, Some(0.0), "{full:?}");
    let plucked = read(&|n| (n % 4 != 0).then(|| 0.2 / n as f64));
    assert_eq!(plucked.1, vec![4, 8], "{plucked:?}");
    let clarinet = read(&|n| Some(0.2 / n as f64 * if n % 2 == 0 { 0.056 } else { 1.0 }));
    // The even half through the tenth, and no odd harmonic; the twelfth, 47 dB down, may be too faint
    // to find at all, and then the series ends at the eleventh.
    assert!(
        [2, 4, 6, 8, 10].iter().all(|n| clarinet.1.contains(n))
            && clarinet.1.iter().all(|n| n % 2 == 0),
        "{clarinet:?}"
    );
}
