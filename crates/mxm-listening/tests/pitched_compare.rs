//! A pitched comparison's two fragile readings, against values known by construction: a stiffness
//! under the timbre threshold at the note's pitch is no difference, and the time to the peak sample
//! is not compared between notes while it still is between drums (ops' `plans/plan-mxm-twf.md`
//! §4.1, the seam test).

use mxm_listening::audibility::{Kind, Threshold, Thresholds, Vocabulary};
use mxm_listening::compare::{compare, inharmonicity_heard};
use mxm_listening::describe::{Options, describe_with};
use mxm_listening::{Family, Report, Sound};

const TAU: f64 = std::f64::consts::TAU;
const RATE: u32 = 48_000;

#[test]
fn the_timbre_threshold_follows_the_published_fit() {
    // `research:listening/percussion-perception.md` §5.2's table, from ln B = 2.57 ln f0 − 26.5.
    for (f0, b) in [(55.0, 9.2e-8), (440.0, 1.9e-5), (3520.0, 4.0e-3)] {
        let got = inharmonicity_heard(f0);
        // The table prints two figures.
        assert!((got / b - 1.0).abs() < 0.03, "{f0} Hz: {got:e}, want {b:e}");
    }
}

#[test]
fn a_stretch_under_the_timbre_threshold_is_no_difference() {
    let floor = inharmonicity_heard(440.0);
    let t = Threshold {
        kind: Kind::Relative,
        value: 0.3,
        source: String::new(),
        floor: Some(floor),
    };
    // Five times apart, both under 1.9·10⁻⁵, and one a fitted hair under zero: nothing.
    assert_eq!(t.units(1e-6, 5e-6), 0.0);
    assert_eq!(t.units(1e-6, -1e-7), 0.0);
    // Above it, a share of the reference's B as before: twice it is ln 2 / ln 1.3 thresholds.
    let two = t.units(1e-4, 2e-4);
    assert!((two - 2f64.ln() / 1.3f64.ln()).abs() < 1e-9, "{two}");
}

fn report(x: Vec<f32>, family: Family) -> Report {
    describe_with(
        &Sound::new("sound", RATE, x),
        &Options {
            family: Some(family),
            without_perception: true,
            ..Options::default()
        },
    )
}

/// One second of a note, 220 Hz and its harmonics to the sixth, decaying, its loudest sample at
/// `peak_ms`: where the partials' phases meet, a few cents decide it.
fn note(peak_ms: f64) -> Vec<f32> {
    let n = RATE as usize;
    let rate = f64::from(RATE);
    let mut x: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f64 / rate;
            let s: f64 = (1..=6)
                .map(|k| 0.1 / k as f64 * (TAU * 220.0 * k as f64 * t + k as f64).sin())
                .sum();
            (s * (-t / 0.6).exp()) as f32
        })
        .collect();
    x[0] = 0.5;
    x[(peak_ms / 1000.0 * rate) as usize] = 0.9;
    x
}

#[test]
fn the_peak_sample_is_compared_between_drums_not_notes() {
    let peak_time = |family: Family| {
        compare(
            &report(note(0.0), family),
            &report(note(30.0), family),
            &Thresholds::literature(),
            &Vocabulary::owner(),
        )
        .findings
        .iter()
        .any(|f| f.id == "attack.peak_time")
    };
    assert!(peak_time(Family::Percussive));
    assert!(!peak_time(Family::Note));
}
