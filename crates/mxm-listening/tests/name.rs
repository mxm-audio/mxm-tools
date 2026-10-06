//! What a file's name claims, and the claims held against a report: agreeing, an octave off, a
//! family only a caller can declare, and nothing to hold a note against.

use mxm_listening::family::Detected;
use mxm_listening::name::{Agreement, Claims, check, claims};
use mxm_listening::reading::{Report, Section};
use mxm_listening::{Family, Reading, Unit};

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

#[test]
fn a_name_claims_notes_runs_families_tempi_dynamics_and_loops() {
    let c = claims("Marimba.yarn.mf.C4B4");
    let (first, last) = c.run.as_ref().expect("a run");
    assert_eq!((first.name.as_str(), last.name.as_str()), ("C4", "B4"));
    assert!(c.note.is_none());
    assert_eq!(c.family.as_ref().map(|f| f.1), Some(Family::Note));
    assert_eq!(c.dynamic.as_deref(), Some("mf"));

    let c = claims("Kick_808_C1");
    assert_eq!(c.family, Some(("Kick".into(), Family::Percussive)));
    let note = c.note.expect("C1");
    assert_eq!(note.name, "C1");
    assert!(close(note.hz, 32.703), "{}", note.hz);

    let c = claims("Piano-A#3-ff");
    assert!(close(c.note.expect("A#3").hz, 233.082));
    assert_eq!(c.dynamic.as_deref(), Some("ff"));

    assert_eq!(
        claims("violin Cs4").note.map(|n| n.name).as_deref(),
        Some("C#4")
    );
    assert_eq!(
        claims("pad_saw_Eb2").note.map(|n| n.name).as_deref(),
        Some("Eb2")
    );
    assert_eq!(
        claims("pad_saw_Eb2").family.map(|f| f.1),
        Some(Family::Voice)
    );
    assert_eq!(claims("Hall IR").family.map(|f| f.1), Some(Family::Impulse));
    assert_eq!(
        claims("snare02").family.map(|f| f.1),
        Some(Family::Percussive)
    );

    let c = claims("drum loop 120 bpm");
    assert!(c.looped);
    assert_eq!(c.tempo_bpm, Some(120.0));
    assert_eq!(claims("funk_96bpm_loop01").tempo_bpm, Some(96.0));

    // Nothing to claim: no note, no known word; a bare letter or a number is not a note.
    assert!(claims("take 7 final").is_empty());
    assert!(claims("B").note.is_none());
    assert_eq!(claims("B").run, None);
}

fn report(family: Family, how: &str, readings: Vec<Reading>) -> Report {
    Report {
        name: "x".into(),
        rate: 48_000,
        duration_s: 1.0,
        family: Detected {
            family,
            how: how.into(),
        },
        onset_s: Some(0.0),
        sections: vec![Section::new("note", "The note", readings)],
        notes: Vec::new(),
    }
}

fn hz(id: &'static str, v: f64) -> Reading {
    Reading::new(id, id, Some(v), Unit::Hertz, "test")
}

#[test]
fn a_claim_is_held_against_what_the_listener_heard() {
    let note = |r: &Report, name: &str| {
        let c = Claims {
            note: claims(name).note,
            ..Claims::default()
        };
        check(&c, r).remove(0)
    };

    // A pitched note: the fundamental or the virtual pitch, whichever is nearer the name.
    let bell = report(
        Family::Note,
        "declared",
        vec![
            hz("note.fundamental", 523.0),
            hz("note.virtual_pitch", 262.5),
        ],
    );
    assert_eq!(note(&bell, "C4").agreement, Agreement::Agrees);
    let off = note(&bell, "C3");
    assert_eq!(off.agreement, Agreement::Differs);
    assert!(off.detail.contains("+1 octave"), "{}", off.detail);
    let wrong = note(&bell, "F#4");
    assert_eq!(wrong.agreement, Agreement::Differs);
    assert!(
        wrong.detail.contains(" c from the name"),
        "{}",
        wrong.detail
    );

    // A hit is held against its rest pitch; with none read, the claim is unmeasured.
    let tom = report(Family::Percussive, "detected", vec![hz("pitch.rest", 98.0)]);
    assert_eq!(note(&tom, "G2").agreement, Agreement::Agrees);
    let silent = report(Family::Percussive, "detected", Vec::new());
    assert_eq!(note(&silent, "G2").agreement, Agreement::Unmeasured);

    // A family: the same agrees; one only a caller declares suggests; a detected clash differs.
    let family = |r: &Report, name: &str| check(&claims(name), r).remove(0);
    assert_eq!(family(&tom, "tom_low").agreement, Agreement::Agrees);
    let piano = family(&tom, "piano");
    assert_eq!(piano.agreement, Agreement::Suggests);
    assert!(piano.detail.contains("declared, never detected"));
    let declared = report(Family::Note, "declared", Vec::new());
    assert_eq!(family(&declared, "kick").agreement, Agreement::Differs);
}
