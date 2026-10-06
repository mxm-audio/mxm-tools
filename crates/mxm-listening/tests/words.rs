//! The word attributes, each read on sounds whose numbers are known by construction, and the owner's
//! phrases explained (the plan's §2 item 8 and §4).

use mxm_listening::audibility::{Vocabulary, query_words};
use mxm_listening::describe::{Options, describe_with};
use mxm_listening::reading::Report;
use mxm_listening::{Family, Sound};

const RATE: f64 = 48_000.0;
const TAU: f64 = std::f64::consts::TAU;

fn value(r: &Report, id: &str) -> Option<f64> {
    r.find(id, None).and_then(|x| x.value)
}

/// Steady sines, `(Hz, amplitude)`, for a second, faded in over `attack_ms`.
fn tones(lines: &[(f64, f64)], attack_ms: f64) -> Sound {
    let x: Vec<f32> = (0..RATE as usize)
        .map(|i| {
            let t = i as f64 / RATE;
            let env = (t * 1000.0 / attack_ms).min(1.0);
            (env * lines
                .iter()
                .map(|&(f, a)| a * (TAU * f * t).sin())
                .sum::<f64>()) as f32
        })
        .collect();
    Sound::new("tones", RATE as u32, x)
}

fn words(s: &Sound, perceive: bool) -> Report {
    describe_with(
        s,
        &Options {
            family: Some(Family::Percussive),
            without_perception: !perceive,
            ..Options::default()
        },
    )
}

#[test]
fn brightness_depth_and_warmth_read_their_bands_energy_exactly() {
    // 1 kHz at 0.4 and 5 kHz at 0.1: the energy above 3 kHz is 0.01 against 0.17.
    let r = words(&tones(&[(1000.0, 0.4), (5000.0, 0.1)], 5.0), false);
    let want = 10.0 * (0.01f64 / 0.17).log10();
    let got = value(&r, "words.brightness").unwrap();
    assert!((got - want).abs() < 0.1, "brightness {got} against {want}");
    let c = value(&r, "words.bright_centroid").unwrap();
    assert!((c - 5000.0).abs() < 20.0, "{c}");
    // 100 Hz at 0.3 and 1 kHz at 0.3: half the energy is deep, at 100 Hz.
    let r = words(&tones(&[(100.0, 0.3), (1000.0, 0.3)], 5.0), false);
    let got = value(&r, "words.depth").unwrap();
    assert!((got + 3.01).abs() < 0.1, "depth {got}");
    let c = value(&r, "words.depth_centroid").unwrap();
    assert!((c - 100.0).abs() < 2.0, "{c}");
    // 300 Hz at 0.4 against 2 kHz at 0.2: warmth 20·log10(2).
    let r = words(&tones(&[(300.0, 0.4), (2000.0, 0.2)], 5.0), false);
    let got = value(&r, "words.warmth").unwrap();
    assert!((got - 6.02).abs() < 0.1, "warmth {got}");
}

#[test]
fn a_fast_attack_is_harder_than_a_slow_one_and_a_low_tone_boomier_than_a_high_one() {
    let fast = words(&tones(&[(1000.0, 0.5)], 1.0), false);
    let slow = words(&tones(&[(1000.0, 0.5)], 25.0), false);
    let (f, s) = (
        value(&fast, "words.hardness").unwrap(),
        value(&slow, "words.hardness").unwrap(),
    );
    // A 1 ms fade reads about a millisecond (the envelope's own window widens it); a 25 ms one, its length.
    assert!(
        f < 3.0 && (s - 25.0).abs() < 4.0,
        "fast {f} ms against slow {s} ms"
    );
    let low = words(&tones(&[(80.0, 0.5)], 5.0), true);
    let high = words(&tones(&[(2000.0, 0.5)], 5.0), true);
    let (l, h) = (
        value(&low, "words.boominess").unwrap(),
        value(&high, "words.boominess").unwrap(),
    );
    // The hearing model spreads a low tone's loudness upwards: an 80 Hz tone's has a quarter of it over
    // 280 Hz (73 %), and a 2 kHz tone's none under it.
    assert!(l > 60.0 && h < 5.0, "boominess {l} % against {h} %");
}

#[test]
fn the_words_are_read_but_never_compared() {
    let a = words(&tones(&[(1000.0, 0.4), (5000.0, 0.1)], 5.0), false);
    let b = words(&tones(&[(1000.0, 0.4), (5000.0, 0.3)], 5.0), false);
    let c = mxm_listening::compare::compare(
        &a,
        &b,
        &mxm_listening::audibility::Thresholds::literature(),
        &Vocabulary::owner(),
    );
    assert!(c.findings.iter().all(|f| !f.id.starts_with("words.")));
}

#[test]
fn a_phrase_explains_as_the_rows_that_hold_it() {
    assert_eq!(
        query_words("Rings a little LONG!"),
        ["rings", "a", "little", "long"]
    );
    let v = Vocabulary::parse(
        "attack.rise_time\t*\t*\t+\ta washy attack\tx\ndecay.t60\t*\t*\t+\trings a little long\tx\ndecay.t60\t*\t*\t-\tdies too soon\tx\n",
    );
    let m = v.explain("washy");
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].id, "attack.rise_time");
    assert_eq!(m[0].direction, 1);
    assert_eq!(v.explain("little long")[0].id, "decay.t60");
    assert!(v.explain("purple").is_empty());
    assert!(v.explain("  ").is_empty());
    // The owner's own vocabulary explains a phrase the guide recorded.
    assert!(!Vocabulary::owner().explain("louder").is_empty());
}

/// The owner's words for one reading, every window and direction, for a window that explains a
/// reading on hover; and the owner's thresholds laid over the literature's from a local folder.
#[test]
fn a_readings_phrases_and_the_owners_thresholds() {
    use mxm_listening::Unit;
    use mxm_listening::audibility::{Thresholds, Vocabulary};

    let phrases: Vec<String> = Vocabulary::owner()
        .for_id("decay.t40")
        .into_iter()
        .map(|m| format!("{} {}", m.direction, m.phrase))
        .collect();
    assert!(
        phrases.contains(&"1 the tail lasts longer".to_string()),
        "{phrases:?}"
    );
    assert!(
        phrases.contains(&"-1 the tail is shorter".to_string()),
        "{phrases:?}"
    );
    assert!(Vocabulary::owner().for_id("no.such").is_empty());

    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("owner-thresholds");
    std::fs::create_dir_all(&dir).unwrap();
    let _ = std::fs::remove_file(dir.join("owner-thresholds.tsv"));
    let (none, note) = Thresholds::with_owner(&dir);
    assert!(note.is_none());
    let literature = none.for_reading("decay.t40", Unit::Milliseconds).cloned();
    std::fs::write(
        dir.join("owner-thresholds.tsv"),
        "decay.t40	rel	0.5	test
",
    )
    .unwrap();
    let (owner, note) = Thresholds::with_owner(&dir);
    assert!(note.is_some_and(|n| n.contains("owner's own")));
    let t = owner.for_reading("decay.t40", Unit::Milliseconds).unwrap();
    assert_eq!(t.value, 0.5);
    assert_ne!(Some(t), literature.as_ref());
}
