//! The early window, the joint refit, the rebuild and the explanations, on sounds built from known
//! parts: what the resynthesis self-test found wrong in the listener stays found.

use mxm_listening::audibility::{Thresholds, Vocabulary};
use mxm_listening::compare::compare;
use mxm_listening::describe::{Options, describe_with};
use mxm_listening::{Family, Report, Sound};

const TAU: f64 = std::f64::consts::TAU;
const RATE: f64 = 48_000.0;

/// Damped modes `(hz, amplitude, t60 seconds)` from sample 0, plus a noise burst of level `noise`
/// decaying with a 20 ms time constant, 1.5 s long.
fn drum(modes: &[(f64, f64, f64)], noise: f64) -> Vec<f64> {
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    (0..(1.5 * RATE) as usize)
        .map(|i| {
            let t = i as f64 / RATE;
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            let white = (state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64
                / (1u64 << 52) as f64
                - 1.0;
            let body: f64 = modes
                .iter()
                .map(|&(hz, a, t60)| a * (-6.91 * t / t60).exp() * (TAU * hz * t).sin())
                .sum();
            body + noise * (-t / 0.02).exp() * white
        })
        .collect()
}

fn sound(name: &str, x: &[f64]) -> Sound {
    Sound::new(name, RATE as u32, x.iter().map(|&v| v as f32).collect())
}

fn percussive(s: &Sound) -> Report {
    describe_with(
        s,
        &Options {
            family: Some(Family::Percussive),
            without_perception: true,
            ..Options::default()
        },
    )
}

fn value(r: &Report, id: &str) -> Option<f64> {
    r.sections
        .iter()
        .flat_map(|s| &s.readings)
        .find(|x| x.id == id)
        .and_then(|x| x.value)
}

/// The rows (Hz, level dB) of the modes table whose window starts at `from_ms`.
fn modes_at(r: &Report, from_ms: f64) -> Vec<(f64, f64)> {
    r.sections
        .iter()
        .flat_map(|s| &s.tables)
        .find(|t| {
            t.id == "pitch.modes" && t.window_ms.is_some_and(|w| (w.0 - from_ms).abs() < 1e-9)
        })
        .map(|t| {
            t.rows
                .iter()
                .filter_map(|row| Some((row[0]?, row[2]?)))
                .collect()
        })
        .unwrap_or_default()
}

fn level_near(rows: &[(f64, f64)], hz: f64) -> Option<f64> {
    rows.iter()
        .filter(|r| (r.0 / hz - 1.0).abs() < 0.02)
        .map(|r| r.1)
        .next()
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_fast_early_mode_is_read_in_the_early_window_at_its_level() {
    // A strong mode that is gone within 100 ms under a weak long ring: the early window must read the
    // early window. Its low band's filters once spanned 200 ms, read the window 100 ms late, and lost
    // the mode — the snare's 185 Hz, −3 dB. At 10 ms the fast mode stands 14.0 dB over the ring.
    let x = drum(&[(190.0, 1.0, 0.09), (262.0, 0.1, 0.9)], 0.0);
    let r = percussive(&sound("early", &x));
    let attack = value(&r, "pitch.attack").expect("the attack's strongest mode");
    assert!(
        (attack - 190.0).abs() < 2.0,
        "attack's strongest mode {attack} Hz"
    );
    let rows = modes_at(&r, 10.0);
    let fast = level_near(&rows, 190.0).expect("the fast mode in 10–60 ms");
    let ring = level_near(&rows, 262.0).expect("the ring in 10–60 ms");
    let want =
        20.0 * ((-6.91 * 0.01 / 0.09f64).exp() / (0.1 * (-6.91 * 0.01 / 0.9f64).exp())).log10();
    assert!(
        (fast - ring - want).abs() < 1.5,
        "fast over ring {:.1} dB, want {want:.1}",
        fast - ring
    );
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn the_rebuild_of_a_drum_keeps_its_pitch_and_decay() {
    let x = drum(
        &[(180.0, 0.6, 1.2), (286.0, 0.3, 0.6), (431.0, 0.15, 0.3)],
        0.2,
    );
    let original = sound("drum", &x);
    let rebuilt = mxm_listening::resynth::resynthesize(&original, &[]).expect("a rebuild");
    let (a, b) = (percussive(&original), percussive(&rebuilt));
    for id in ["pitch.rest", "pitch.strongest"] {
        let (va, vb) = (value(&a, id).unwrap(), value(&b, id).unwrap());
        assert!((vb / va - 1.0).abs() < 0.003, "{id}: {va} → {vb}");
    }
    let c = compare(&a, &b, &Thresholds::literature(), &Vocabulary::owner());
    let pitch: Vec<String> = c
        .audible()
        .filter(|f| f.part == "pitch" || f.part == "decay")
        .map(|f| {
            format!(
                "{} {:?}: {} → {}",
                f.id, f.window_ms, f.reference, f.candidate
            )
        })
        .collect();
    assert!(pitch.is_empty(), "pitch or decay differences: {pitch:#?}");
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_rest_pitch_on_different_modes_says_why() {
    // The ring at 262 Hz, a lower mode 9 dB under it in the reference and 15 dB under in the
    // candidate: the rule's 12 dB line lies between, so the rest pitches are different modes.
    let under = |db: f64| 10f64.powf(-db / 20.0);
    let a = drum(&[(262.0, 0.5, 1.2), (200.0, 0.5 * under(9.0), 1.2)], 0.0);
    let b = drum(&[(262.0, 0.5, 1.2), (200.0, 0.5 * under(15.0), 1.2)], 0.0);
    let (ra, rb) = (
        percussive(&sound("nine", &a)),
        percussive(&sound("fifteen", &b)),
    );
    assert!((value(&ra, "pitch.rest").unwrap() - 200.0).abs() < 2.0);
    assert!((value(&rb, "pitch.rest").unwrap() - 262.0).abs() < 2.0);
    let c = compare(&ra, &rb, &Thresholds::literature(), &Vocabulary::owner());
    let rest = c
        .findings
        .iter()
        .find(|f| f.id == "pitch.rest")
        .expect("a rest-pitch finding");
    let why = rest.detail.as_deref().expect("the rule's margin");
    assert!(why.contains("chose different modes"), "{why}");
    assert!(why.contains("200 Hz is 9."), "{why}");
    assert!(
        why.contains("and 15.") || why.contains("and 14.") || why.contains("and 16."),
        "{why}"
    );
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_difference_no_reading_covers_is_reported_as_unexplained() {
    let base = drum(&[(180.0, 0.6, 1.2), (286.0, 0.3, 0.6)], 0.2);
    // A 1.2 kHz burst from 720 to 780 ms: between every windowed reading's windows (at 580 ms and
    // 2 kHz the wires' burst reading heard it, and rightly explained it).
    let mut burst = base.clone();
    for (i, v) in burst.iter_mut().enumerate() {
        let t = i as f64 / RATE;
        if (0.72..0.78).contains(&t) {
            *v += 0.03 * (TAU * 1200.0 * t).sin();
        }
    }
    let (sa, sb) = (sound("base", &base), sound("burst", &burst));
    let (ra, rb) = (percussive(&sa), percussive(&sb));
    let mut same = compare(&ra, &ra, &Thresholds::literature(), &Vocabulary::owner());
    same.map(&sa, &sa);
    assert!(
        same.regions.is_empty(),
        "identical sounds: {:?}",
        same.regions
    );
    let mut c = compare(&ra, &rb, &Thresholds::literature(), &Vocabulary::owner());
    c.map(&sa, &sb);
    let found: Vec<_> = c.unexplained().collect();
    assert!(
        found.iter().any(|r| r.where_ms.0 < 780.0
            && r.where_ms.1 > 720.0
            && r.band_hz.0 <= 1200.0
            && r.band_hz.1 >= 1200.0),
        "unexplained: {found:?}; all regions: {:?}; audible: {:#?}",
        c.regions,
        c.audible()
            .map(|f| (f.id, f.window_ms, f.band_hz, f.units))
            .collect::<Vec<_>>()
    );
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn the_rebuild_of_a_note_keeps_its_partials_and_laws() {
    // A struck stiff string: sixteen partials, reaching 3.6 kHz, over a drum analysis's 2 kHz.
    let rate = 48_000.0;
    let mut x = vec![0.0; 240];
    x.extend((0..(3.0 * rate) as usize).map(|i| {
        let t = i as f64 / rate;
        let v: f64 = (1..=16)
            .map(|n| {
                let nf = f64::from(n);
                let f = nf * 220.0 * (1.0 + 2e-4 * nf * nf).sqrt();
                let a = 0.3 * 10f64.powf(-6.0 * (f / 220.0).log2() / 20.0);
                let t60 = 2.0 * (f / 220.0).powf(-0.7);
                a * (-6.91 * t / t60).exp() * (std::f64::consts::TAU * f * t).sin()
            })
            .sum();
        (t / 0.002).min(1.0) * v
    }));
    let original = sound("note", &x);
    let rebuilt = mxm_listening::resynth::rebuild_with(&original, &[], Some(Family::Note))
        .expect("a rebuild")
        .sound;
    let read = |s: &Sound| {
        describe_with(
            s,
            &Options {
                family: Some(Family::Note),
                without_perception: true,
                ..Options::default()
            },
        )
    };
    let (a, b) = (read(&original), read(&rebuilt));
    let c = compare(&a, &b, &Thresholds::literature(), &Vocabulary::owner());
    let note: Vec<String> = c
        .audible()
        .filter(|f| f.part == "note")
        .map(|f| {
            format!(
                "{} {:?}: {} → {}",
                f.id, f.band_hz, f.reference, f.candidate
            )
        })
        .collect();
    assert!(note.is_empty(), "the note's readings differ: {note:#?}");
}
