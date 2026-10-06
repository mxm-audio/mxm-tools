//! The scene — what the HUD shows, decided without drawing — on synthetic sounds whose answers are
//! known: the crosshair locks on a hit's rest pitch and a note's fundamental, and says why when there
//! is nothing to lock on; a note's partials become blips; and every number shown is a reading's.

use mxm_listener_hud::scene::{Lock, Role, Scene, Shown, build, with_unit};
use mxm_listening::describe::{Options, describe_with};
use mxm_listening::reading::{Report, Unit, Validity};
use mxm_listening::{Family, Sound};

const TAU: f64 = std::f64::consts::TAU;
const RATE: f64 = 48_000.0;

/// 10 ms of silence, then the partials `(ratio to 220 Hz, level, τ s)` decaying.
fn sound(name: &str, seconds: f64, partials: &[(f64, f64, f64)]) -> Sound {
    let samples = (0..(seconds * RATE) as usize)
        .map(|i| {
            let t = i as f64 / RATE - 0.01;
            if t < 0.0 {
                return 0.0;
            }
            let s: f64 = partials
                .iter()
                .map(|(r, a, tau)| a * (-t / tau).exp() * (TAU * 220.0 * r * t).sin())
                .sum();
            (0.5 * s) as f32
        })
        .collect();
    Sound::new(name, RATE as u32, samples)
}

fn read(sound: &Sound, family: Option<Family>) -> Report {
    describe_with(
        sound,
        &Options {
            family,
            without_perception: true,
            ..Options::default()
        },
    )
}

/// Every value the scene shows is a reading of the report: same id, same value, same window.
fn traced(scene: &Scene, report: &Report) {
    let shown: Vec<&Shown> = scene
        .parts
        .iter()
        .flat_map(|p| &p.key)
        .chain(&scene.headline)
        .collect();
    assert!(!shown.is_empty());
    for s in shown {
        let found = report.sections.iter().flat_map(|x| &x.readings).any(|r| {
            r.id == s.id
                && r.value.map(f64::to_bits) == s.value.map(f64::to_bits)
                && r.window_ms == s.window_ms
                && r.band_hz == s.band_hz
        });
        assert!(
            found,
            "{} = {:?} is no reading of the report",
            s.id, s.value
        );
    }
}

#[test]
fn a_hit_locks_on_its_rest_pitch_and_shows_only_readings() {
    let hit = sound("hit", 1.0, &[(1.0, 1.0, 0.3), (2.0, 0.3, 0.2)]);
    let report = read(&hit, None);
    let scene = build(&report);
    assert_eq!(scene.family, Family::Percussive);
    match &scene.lock {
        Lock::Locked {
            id,
            note,
            hz,
            cents,
            ..
        } => {
            assert_eq!(*id, "pitch.rest");
            assert_eq!(note, "A3");
            assert!((hz - 220.0).abs() < 2.0, "{hz}");
            assert!(cents.abs() < 15.0, "{cents}");
        }
        Lock::NoLock { why } => panic!("no lock: {why}"),
    }
    assert!(!scene.blips.is_empty(), "the hit's modes are blips");
    traced(&scene, &report);
    // The parts are the report's sections, in order, each with its role.
    assert_eq!(scene.parts.len(), report.sections.len());
    for (p, s) in scene.parts.iter().zip(&report.sections) {
        assert_eq!(p.part, s.part);
        assert_eq!(p.readings, s.readings.len());
    }
    assert_eq!(
        scene
            .parts
            .iter()
            .find(|p| p.part == "decay")
            .map(|p| p.role),
        Some(Role::Time)
    );
}

#[test]
fn a_note_locks_on_its_fundamental_and_its_partials_are_blips() {
    // A plucked string's first six harmonics.
    let partials: Vec<(f64, f64, f64)> = (1..=6)
        .map(|k| (k as f64, 1.0 / k as f64, 1.2 / k as f64))
        .collect();
    let note = sound("note", 2.0, &partials);
    let report = read(&note, Some(Family::Note));
    let scene = build(&report);
    match &scene.lock {
        Lock::Locked { id, note, .. } => {
            assert_eq!(*id, "note.fundamental");
            assert_eq!(note, "A3");
        }
        Lock::NoLock { why } => panic!("no lock: {why}"),
    }
    assert!(scene.blips.iter().all(|b| b.table == "note.partials"));
    for k in 1..=4 {
        let want = 220.0 * f64::from(k);
        assert!(
            scene.blips.iter().any(|b| (b.hz / want - 1.0).abs() < 0.01),
            "no blip near {want} Hz: {:?}",
            scene.blips
        );
    }
    traced(&scene, &report);
}

#[test]
fn nothing_to_lock_on_says_why() {
    let silence = Sound::new("silence", RATE as u32, vec![0.0; 48_000]);
    let scene = build(&read(&silence, None));
    assert!(matches!(&scene.lock, Lock::NoLock { why } if why.contains("nothing was measured")));
    assert!(scene.parts.is_empty());

    let room = sound("room", 1.0, &[(1.0, 1.0, 0.2), (3.7, 0.8, 0.15)]);
    let scene = build(&read(&room, Some(Family::Impulse)));
    assert!(matches!(&scene.lock, Lock::NoLock { why } if why.contains("impulse response")));
}

#[test]
fn an_absent_reading_is_shown_as_no_signal() {
    let absent = Shown {
        id: "decay.t40",
        label: "Fall to −40 dB",
        value: None,
        unit: Unit::Milliseconds,
        validity: Validity::Absent("never falls that far"),
        window_ms: None,
        band_hz: None,
    };
    assert_eq!(with_unit(&absent), "NO SIGNAL");
    let present = Shown {
        value: Some(308.26),
        validity: Validity::Valid,
        ..absent
    };
    assert_eq!(with_unit(&present), "308.3 ms");
}
