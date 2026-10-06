//! The window end to end on synthetic sounds (the plan's §5): a stale reading never paints over a
//! newer drop, no path reaches anything shown, and a dropped sound is read and its parts listed.
//! Every sound here is written by the test; no recording is ever read.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use mxm_audio_file::{Target, write};
use mxm_listener_hud::Hud;
use mxm_listener_hud::analysis::{self, Request, Stage};
use mxm_listener_hud::worker::Outcome;

/// A folder name that must never reach the screen or a report.
const PRIVATE: &str = "private-folder-name";

fn scratch(test: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("listener-hud")
        .join(test)
        .join(PRIVATE)
}

/// A struck A3: 220 Hz and its octave, decaying, after 10 ms of silence.
fn write_struck_a3(path: &Path) {
    let rate = 48_000;
    let samples: Vec<f32> = (0..rate * 3 / 2)
        .map(|i| {
            let t = i as f64 / f64::from(rate) - 0.01;
            if t < 0.0 {
                return 0.0;
            }
            let tau = std::f64::consts::TAU;
            let ring = 0.6 * (tau * 220.0 * t).sin() + 0.2 * (tau * 440.0 * t).sin();
            (ring * (-t / 0.3).exp()) as f32
        })
        .collect();
    write(path, &samples, 1, rate, Target::WavFloat32).unwrap();
}

/// One wheel notch of `points` over wherever the pointer is: up (positive) zooms in.
fn wheel(harness: &mut Harness<'static, Hud>, points: f32) {
    harness.input_mut().events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, points),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::default(),
    });
    harness.run_steps(1);
}

#[derive(Debug)]
struct Dropped(PathBuf);

impl egui::DroppedFile for Dropped {
    fn path(&self) -> &Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

fn harness() -> Harness<'static, Hud> {
    Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_ui_state(|ui, hud: &mut Hud| hud.ui(ui), Hud::new())
}

/// Steps the window until `done` holds, the worker running meanwhile.
fn step_until(harness: &mut Harness<'static, Hud>, done: impl Fn(&Hud) -> bool) {
    let started = Instant::now();
    while !done(harness.state()) {
        assert!(
            started.elapsed() < Duration::from_secs(300),
            "the reading never arrived"
        );
        harness.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    harness.run_steps(2);
}

#[test]
fn a_stale_reading_never_paints_over_a_newer_drop() {
    let mut hud = Hud::new();
    hud.open(PathBuf::from("first.wav"));
    let first = hud.current().unwrap().seq;
    hud.open(PathBuf::from("second.wav"));
    let second = hud.current().unwrap().seq;

    hud.apply(first, Outcome::Failed("late".into()));
    assert!(hud.current().unwrap().failed.is_none());
    hud.apply(second, Outcome::Failed("now".into()));
    assert_eq!(hud.current().unwrap().failed.as_deref(), Some("now"));
    assert_eq!(hud.current().unwrap().name, "second");
}

/// The name, every notice and every report carry the file's stem only — including a refused file's
/// and a cut stereo file's, whose notices are the likeliest to quote a path.
#[test]
fn no_path_reaches_a_name_a_notice_or_a_report() {
    let dir = scratch("no-path");
    std::fs::create_dir_all(&dir).unwrap();

    // Stereo and longer than the listener keeps: read only as far as its load.
    let long = dir.join("secret-take.wav");
    let frames = mxm_listening::prep::HIT_MAX_FRAMES + 480;
    let stereo: Vec<f32> = (0..frames)
        .flat_map(|i| {
            let s = (0.3 * (i as f64 * 0.05).sin()) as f32;
            [s, -s]
        })
        .collect();
    write(&long, &stereo, 2, 48_000, Target::WavFloat32).unwrap();
    let request = Request {
        path: long.clone(),
        name: analysis::display_name(&long),
        family: None,
        expect_hz: None,
    };
    assert_eq!(request.name, "secret-take");
    let mut stages = Vec::new();
    analysis::analyse(&request, &mut |stage| {
        stages.push(stage);
        false
    });
    let Some(Stage::Loaded(loaded)) = stages.first() else {
        panic!("{stages:?}")
    };
    let notices = analysis::notices(loaded, None);
    assert_eq!(notices.len(), 2, "{notices:?}");
    assert!(notices.iter().all(|n| !n.contains(PRIVATE)), "{notices:?}");

    // Not audio: refused in the decoder's words, which carry no path.
    let refused = dir.join("not-audio.wav");
    std::fs::write(&refused, b"text, not a sound").unwrap();
    let mut harness = harness();
    harness
        .input_mut()
        .dropped_files
        .push(Arc::new(Dropped(refused)));
    step_until(&mut harness, |hud| {
        hud.current().is_some_and(|c| c.refused.is_some())
    });
    harness.get_by_label_contains("Refused:");
    harness.get_by_label("not-audio");
    assert!(harness.query_by_label_contains(PRIVATE).is_none());
}

#[test]
fn a_dropped_sound_is_read_and_every_part_listed() {
    let dir = scratch("drop");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("struck-a3.wav");
    write_struck_a3(&path);

    let mut harness = harness();
    harness.get_by_label_contains("Drop a sound file");
    harness
        .input_mut()
        .dropped_files
        .push(Arc::new(Dropped(path)));
    step_until(&mut harness, |hud| {
        hud.current().is_some_and(|c| c.full.is_some())
    });

    let current = harness.state().current().unwrap();
    assert!(current.quick.is_some(), "the first readout came first");
    let report = current.report().unwrap();
    assert_eq!(report.name, "struck-a3");
    assert!(report.sections.iter().any(|s| s.part == "perception"));

    harness.get_by_label("struck-a3");
    harness.get_by_label_contains("First readout");
    harness.get_by_label_contains("readings in");
    // The HUD shows the parts as glyphs; LIST shows every reading at once.
    harness.get_by_label("DECAY part");
    harness.get_by_label("LIST").click();
    harness.run_steps(2);
    harness.get_by_label_contains("Decay —");
    harness.get_by_label_contains("Read as one sound from its first onset");
    assert!(harness.query_by_label_contains(PRIVATE).is_none());

    // The sounds to hear: the listened sound at once, the rebuild's parts once it has landed. Not
    // clicked here — a test never opens a device; `tests/playback.rs` holds the transport.
    step_until(&mut harness, |hud| {
        hud.current().is_some_and(|c| c.rebuilt.is_some())
    });
    for which in ["LISTENED", "MODES", "NOISE", "REBUILD", "STOP"] {
        assert!(
            !harness.get_by_label(which).accesskit_node().is_disabled(),
            "{which} is not enabled"
        );
    }
    let rebuilt = harness.state().current().unwrap().rebuilt.clone().unwrap();
    let listened = harness.state().current().unwrap().loaded.clone().unwrap();
    assert_eq!(
        rebuilt.whole.len(),
        listened.samples.len(),
        "the rebuild keeps the time base"
    );
}

#[test]
fn turning_the_ring_reads_the_sound_again_as_declared() {
    let dir = scratch("ring");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("struck-a3.wav");
    write_struck_a3(&path);

    let mut harness = harness();
    harness
        .input_mut()
        .dropped_files
        .push(Arc::new(Dropped(path)));
    step_until(&mut harness, |hud| {
        hud.current().is_some_and(|c| c.quick.is_some())
    });
    let before = harness.state().current().unwrap().seq;

    harness.get_by_label("NOTE").click();
    step_until(&mut harness, |hud| {
        hud.current()
            .is_some_and(|c| c.seq != before && c.quick.is_some())
    });
    let report = harness.state().current().unwrap().report().unwrap();
    assert_eq!(report.family.how, "declared");
    assert_eq!(report.family.family, mxm_listening::Family::Note);
    assert!(report.sections.iter().any(|s| s.part == "note"));
}

/// The name is a claim held against the reading (the owner: "often useful. Though it can be
/// misleading"): a struck A3 named A3 agrees, and hovering a reading explains it from the glossary.
#[test]
fn the_name_is_held_against_what_the_listener_hears() {
    let dir = scratch("claims");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("tom_A3.wav");
    write_struck_a3(&path);

    let mut harness = harness();
    harness
        .input_mut()
        .dropped_files
        .push(Arc::new(Dropped(path)));
    step_until(&mut harness, |hud| {
        hud.current().is_some_and(|c| c.quick.is_some())
    });
    harness.get_by_label_contains("The name says: A3 (220.0 Hz)");
    // Both claims agree: "tom" with the hit, A3 with the rest pitch.
    assert_eq!(harness.get_all_by_label("AGREES").count(), 2);
    harness.get_by_label_contains("family — \"tom\": read as a percussive hit");
    harness.get_by_label_contains("note — A3: the rest pitch is");

    // The wheel over a glyph zooms into it: two notches, zoom 2, every reading of the part, each
    // explained on hover — the glossary, the smallest change heard and the owner's words.
    harness.get_by_label("DECAY part").hover();
    harness.run_steps(2);
    for _ in 0..2 {
        wheel(&mut harness, 30.0);
    }
    assert_eq!(
        harness.state().focus().map(|f| (f.part, f.level)),
        Some(("decay", 2))
    );
    harness.get_by_label("Fall to −40 dB").hover();
    harness.run_steps(60);
    harness.get_by_label_contains("How long until it stays 40 dB quieter");
    harness.get_by_label_contains("Your words:");
    // And back out: down past zoom 1 closes it.
    harness.get_by_label("DECAY part").hover();
    harness.run_steps(2);
    for _ in 0..3 {
        wheel(&mut harness, -30.0);
    }
    assert_eq!(harness.state().focus(), None);
}

/// A name that suggests a family only a caller declares offers to declare it, and one click reads
/// the sound again as that family.
#[test]
fn a_family_the_name_suggests_is_one_click_away() {
    let dir = scratch("suggest");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("piano_A3.wav");
    write_struck_a3(&path);

    let mut harness = harness();
    harness
        .input_mut()
        .dropped_files
        .push(Arc::new(Dropped(path)));
    step_until(&mut harness, |hud| {
        hud.current().is_some_and(|c| c.quick.is_some())
    });
    harness.get_by_label("SUGGESTS");
    harness.get_by_label("Read as NOTE").click();
    step_until(&mut harness, |hud| {
        hud.ring() == mxm_listener_hud::hud::Ring::Note
            && hud.current().is_some_and(|c| c.quick.is_some())
    });
    let report = harness.state().current().unwrap().report().unwrap();
    assert_eq!(report.family.family, mxm_listening::Family::Note);
    // Declared, the family agrees; the note is held against a note's pitch now — the fundamental or
    // the virtual pitch, whichever is nearer the name — not a hit's rest pitch.
    assert_eq!(harness.get_all_by_label("AGREES").count(), 2);
    let note = harness.get_by_label_contains("note — A3: the ");
    let text = format!("{note:?}");
    assert!(
        (text.contains("fundamental") || text.contains("virtual pitch"))
            && !text.contains("rest pitch"),
        "{text}"
    );
}
