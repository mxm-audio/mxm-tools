//! The glossary's coverage (`plans/plan-mxm-listener-hud.md` §2.3), against a mechanical inventory
//! rather than reports: a reading's id is a `&'static str` and this crate makes none at runtime, so
//! every id is a literal in the source. A synthetic report would miss the ids only some sounds emit;
//! the scan does not, and the reports are checked against the scan as a second net.

use std::collections::BTreeSet;
use std::path::Path;

use mxm_listening::describe::{Options, describe_with};
use mxm_listening::reading::Report;
use mxm_listening::{Family, Sound, glossary};

/// The prefixes a reading or table id starts with. A literal shaped like an id with any other prefix
/// must be named in [`NOT_IDS`], so a new part cannot slip past the scan.
const PARTS: [&str; 21] = [
    "curve",
    "artefacts",
    "attack",
    "decay",
    "delay",
    "effect",
    "level",
    "modulation",
    "note",
    "perception",
    "pitch",
    "respond",
    "response",
    "space",
    "stereo",
    "sustain",
    "texture",
    "tone",
    "tonality",
    "voice",
    "words",
];

/// Literals shaped like an id that are not one.
const NOT_IDS: [&str; 1] = ["index.md"];

fn sources() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                out.push((path.display().to_string(), text));
            }
        }
    }
    let mut out = Vec::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut out);
    assert!(out.len() > 20, "the scan found the source");
    out
}

/// Every quoted run shaped like `word.word`: lowercase ASCII letters, digits and underscores, one
/// dot, each side starting with a letter. Quote-pairing is not literal-aware on purpose: a false
/// match can only add a literal to classify, never hide one.
fn id_shaped(text: &str) -> Vec<&str> {
    let shaped = |s: &str| {
        let Some((a, b)) = s.split_once('.') else {
            return false;
        };
        [a, b].iter().all(|w| {
            w.starts_with(|c: char| c.is_ascii_lowercase())
                && w.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
    };
    text.match_indices('"')
        .filter_map(|(i, _)| {
            let rest = &text[i + 1..];
            rest.find('"').map(|end| &rest[..end])
        })
        .filter(|s| shaped(s))
        .collect()
}

fn inventory() -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let mut unknown = BTreeSet::new();
    for (path, text) in sources() {
        for literal in id_shaped(&text) {
            let prefix = literal.split('.').next().unwrap();
            if PARTS.contains(&prefix) {
                ids.insert(literal.to_string());
            } else if !NOT_IDS.contains(&literal) {
                unknown.insert(format!("{literal} in {path}"));
            }
        }
    }
    assert!(
        unknown.is_empty(),
        "id-shaped literals with an unknown prefix — add the part to PARTS or the literal to \
         NOT_IDS: {unknown:?}"
    );
    ids
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn every_id_in_the_source_is_explained_and_every_row_is_an_id() {
    let ids = inventory();
    let mut rows = BTreeSet::new();
    for (id, meaning) in glossary::entries() {
        assert!(rows.insert(id.to_string()), "{id} has two rows");
        assert!(!meaning.trim().is_empty(), "{id} explains nothing");
        assert!(!meaning.contains('\t'), "{id}: a tab in its meaning");
    }
    let missing: Vec<_> = ids.difference(&rows).collect();
    assert!(missing.is_empty(), "ids the glossary lacks: {missing:?}");
    let stale: Vec<_> = rows.difference(&ids).collect();
    assert!(
        stale.is_empty(),
        "rows for ids the source no longer holds: {stale:?}"
    );
    assert_eq!(
        glossary::meaning("decay.t20").map(|m| m.starts_with("How long")),
        Some(true)
    );
    assert_eq!(glossary::meaning("no.such"), None);
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn no_id_can_be_made_at_runtime() {
    for (path, text) in sources() {
        for leak in ["Box::leak", ".leak()", "String::leak"] {
            assert!(
                !text.contains(leak),
                "{path} can make a `&'static str` at runtime ({leak}): the scan could miss an id"
            );
        }
    }
}

/// A report's every id, readings' and tables'.
fn ids_of(report: &Report) -> BTreeSet<String> {
    report
        .sections
        .iter()
        .flat_map(|s| {
            s.readings
                .iter()
                .map(|r| r.id)
                .chain(s.tables.iter().map(|t| t.id))
        })
        .map(str::to_string)
        .collect()
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn every_id_a_report_carries_is_in_the_inventory() {
    const TAU: f64 = std::f64::consts::TAU;
    let rate = 48_000.0;
    let sound = |name: &str, seconds: f64, f: &dyn Fn(f64) -> f64| {
        let pre = (0.005 * rate) as usize;
        let mut x = vec![0.0f32; pre];
        x.extend((0..(seconds * rate) as usize).map(|i| f(i as f64 / rate) as f32));
        Sound::new(name, rate as u32, x)
    };
    let hit = sound("hit", 1.0, &|t| {
        0.8 * (-t / 0.15).exp() * (TAU * 180.0 * t).sin()
    });
    let bar = sound("bar", 1.5, &|t| {
        [(1.0, 1.0), (2.76, 0.4), (5.4, 0.2)]
            .iter()
            .map(|(r, a)| a * (-t / (0.6 / r)).exp() * (TAU * 440.0 * r * t).sin())
            .sum::<f64>()
            * 0.5
    });
    let held = sound("held", 2.0, &|t| {
        let pitch = 220.0 * t + 1.2 * (TAU * 5.0 * t).sin() / TAU;
        (1..=6)
            .map(|k| (TAU * k as f64 * pitch).sin() / k as f64)
            .sum::<f64>()
            * 0.3
    });
    // A deterministic noise decaying at a room's pace: each sample a hash of its index.
    let noise = sound("room", 1.0, &|t| {
        let h =
            ((t * rate) as u64 ^ 0x9E37_79B9_7F4A_7C15).wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11;
        ((h as f64 / (1u64 << 53) as f64) * 2.0 - 1.0) * (-t / 0.2).exp() * 0.5
    });

    let inventory = inventory();
    let cases: [(&Sound, Option<Family>, bool); 6] = [
        (&hit, None, true),
        (&bar, Some(Family::Note), false),
        (&held, Some(Family::Sustained), false),
        (&held, Some(Family::Voice), false),
        (&noise, Some(Family::Impulse), false),
        (&noise, None, false),
    ];
    for (sound, family, perception) in cases {
        let report = describe_with(
            sound,
            &Options {
                family,
                without_perception: !perception,
                ..Options::default()
            },
        );
        let outside: Vec<_> = ids_of(&report).difference(&inventory).cloned().collect();
        assert!(
            outside.is_empty(),
            "{} ({family:?}) carries ids the scan did not find: {outside:?}",
            sound.name
        );
    }
}
