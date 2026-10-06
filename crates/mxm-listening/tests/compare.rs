//! Comparison against differences known by construction: the same sound gives nothing audible; one
//! known change comes back as that change, with the right sign, near the top, and a change well under
//! its threshold does not come back at all (the plan's §6 perturbation suite, on synthetic drums).

use mxm_listening::audibility::{Thresholds, Vocabulary};
use mxm_listening::compare::{Comparison, compare};
use mxm_listening::describe::{Options, describe_with};
use mxm_listening::{Family, Report, Sound};

const TAU: f64 = std::f64::consts::TAU;
const RATE: f64 = 48_000.0;

/// A synthetic tom: two modes and a short noise burst, from parameters a perturbation changes exactly.
#[derive(Clone, Copy)]
struct Drum {
    f0: f64,
    tau: f64,
    gain: f64,
    noise: f64,
    rise_ms: f64,
    wobble: f64,
}

impl Default for Drum {
    fn default() -> Self {
        Self {
            f0: 180.0,
            tau: 0.25,
            gain: 0.7,
            noise: 0.15,
            rise_ms: 0.5,
            wobble: 0.0,
        }
    }
}

impl Drum {
    fn render(&self, name: &str) -> Sound {
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut x = vec![0.0f32; 240];
        x.extend((0..(1.5 * RATE) as usize).map(|i| {
            let t = i as f64 / RATE;
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            let white = (state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64
                / (1u64 << 52) as f64
                - 1.0;
            let rise = (t * 1000.0 / self.rise_ms).min(1.0);
            // 20 Hz: inside the ring's ±15 % band at 180 Hz, which is all the wobble measure hears.
            let am = 1.0 + self.wobble * (TAU * 20.0 * t).sin();
            let body = (-t / self.tau).exp() * am * (TAU * self.f0 * t).sin()
                + 0.35 * (-t / (0.6 * self.tau)).exp() * (TAU * 1.59 * self.f0 * t).sin();
            let burst = self.noise * (-t / 0.02).exp() * white;
            (self.gain * rise * (body + burst)) as f32
        }));
        Sound::new(name, RATE as u32, x)
    }
}

fn report(d: Drum, name: &str) -> Report {
    describe_with(
        &d.render(name),
        &Options {
            family: Some(Family::Percussive),
            without_perception: true,
            ..Options::default()
        },
    )
}

fn run(reference: Drum, candidate: Drum) -> Comparison {
    compare(
        &report(reference, "reference"),
        &report(candidate, "candidate"),
        &Thresholds::literature(),
        &Vocabulary::owner(),
    )
}

/// The rank (0-based) of the first audible finding with this id, and its sign.
fn rank(c: &Comparison, id: &str) -> Option<(usize, f64)> {
    c.audible()
        .enumerate()
        .find(|(_, f)| f.id == id)
        .map(|(i, f)| (i, (f.candidate - f.reference).signum()))
}

#[test]
#[ignore = "slow: run before a release with `cargo test -- --ignored`; over 30 s"]
fn the_same_sound_has_no_audible_difference() {
    let c = run(Drum::default(), Drum::default());
    let audible: Vec<_> = c.audible().map(|f| f.id).collect();
    assert!(audible.is_empty(), "audible: {audible:?}");
}

#[test]
#[ignore = "slow: run before a release with `cargo test -- --ignored`; over 30 s"]
fn a_longer_ring_comes_back_as_a_longer_ring() {
    let c = run(
        Drum::default(),
        Drum {
            tau: 0.45,
            ..Drum::default()
        },
    );
    let (r, sign) = rank(&c, "decay.late_level").expect("the late level differs");
    assert!(sign > 0.0 && r < 8, "late level at rank {r}");
    assert!(
        c.audible()
            .any(|f| f.phrase.as_deref() == Some("the skin rings too long"))
    );
}

#[test]
#[ignore = "slow: run before a release with `cargo test -- --ignored`; over 30 s"]
fn a_higher_drum_comes_back_as_higher_pitched() {
    let c = run(
        Drum::default(),
        Drum {
            f0: 180.0 * 2f64.powf(60.0 / 1200.0),
            ..Drum::default()
        },
    );
    let f = c
        .audible()
        .find(|f| f.id == "pitch.rest")
        .expect("the rest pitch differs");
    assert!(f.candidate > f.reference);
    assert!((f.candidate / f.reference - 2f64.powf(60.0 / 1200.0)).abs() < 0.003);
    assert_eq!(
        f.phrase.as_deref(),
        Some("higher pitched (the tone of the drum skin)")
    );
}

#[test]
#[ignore = "slow: run before a release with `cargo test -- --ignored`; over 30 s"]
fn a_softer_attack_comes_back_as_a_softer_attack() {
    let c = run(
        Drum::default(),
        Drum {
            rise_ms: 6.0,
            ..Drum::default()
        },
    );
    let (r, sign) = rank(&c, "attack.rise_time").expect("the rise differs");
    assert!(sign > 0.0 && r < 6, "rise at rank {r}");
}

#[test]
#[ignore = "slow: run before a release with `cargo test -- --ignored`; over 30 s"]
fn a_quieter_render_is_a_fader_difference() {
    let c = run(
        Drum::default(),
        Drum {
            gain: 0.7 * 10f64.powf(-3.0 / 20.0),
            ..Drum::default()
        },
    );
    let f = c
        .audible()
        .find(|f| f.id == "level.body_loudness")
        .expect("the loudness differs");
    assert!((f.candidate - f.reference + 3.0).abs() < 0.05);
    assert!(f.eq_reachable, "a fader can do this");
    // Nothing about the sound itself changed: every other audible finding is a level a fader moves.
    let other: Vec<_> = c
        .audible()
        .filter(|f| !f.eq_reachable)
        .map(|f| f.id)
        .collect();
    assert!(other.is_empty(), "{other:?}");
}

#[test]
#[ignore = "slow: run before a release with `cargo test -- --ignored`; over 30 s"]
fn a_wobbling_ring_comes_back_as_a_wobble() {
    let c = run(
        Drum::default(),
        Drum {
            wobble: 0.12,
            ..Drum::default()
        },
    );
    let f = c
        .findings
        .iter()
        .find(|f| f.id == "modulation.ring_wobble")
        .expect("the wobble is compared");
    assert!(
        f.candidate > f.reference && f.units >= 1.0,
        "{} → {} ({}×)",
        f.reference,
        f.candidate,
        f.units
    );
    assert!(f.sample, "a single render's wobble is a sample");
}

#[test]
#[ignore = "slow: run before a release with `cargo test -- --ignored`; over 30 s"]
fn a_change_well_under_threshold_is_not_reported() {
    // 10 % on the ring's time constant: under the 25 % decay threshold.
    let c = run(
        Drum::default(),
        Drum {
            tau: 0.275,
            ..Drum::default()
        },
    );
    let audible: Vec<_> = c
        .audible()
        .filter(|f| f.part == "decay")
        .map(|f| (f.id, f.units))
        .collect();
    assert!(audible.iter().all(|(_, u)| *u < 2.0), "{audible:?}");
    assert!(rank(&c, "decay.t20").is_none(), "{audible:?}");
}

#[test]
#[ignore = "slow: run before a release with `cargo test -- --ignored`; over 30 s"]
fn a_change_comes_back_at_its_size_in_thresholds() {
    // The plan's §6 sweep: each change at 0.5, 2 and 4 of its reading's threshold comes back as that
    // reading at that many thresholds, on its side, and under one threshold it is not reported.
    // Sizes are read in the threshold's own terms: pitch in steps of 0.6 %, the ring's T60 in steps of
    // 25 %, loudness in dB.
    type Change = fn(f64) -> Drum;
    let changes: [(&str, Change); 3] = [
        ("pitch.rest", |k| Drum {
            f0: 180.0 * 1.006f64.powf(k),
            ..Drum::default()
        }),
        ("pitch.rest_t60", |k| Drum {
            tau: 0.25 * 1.25f64.powf(k),
            ..Drum::default()
        }),
        ("level.body_loudness", |k| Drum {
            gain: 0.7 * 10f64.powf(-k / 20.0),
            ..Drum::default()
        }),
    ];
    for (id, change) in changes {
        for k in [0.5, 2.0, 4.0] {
            let c = run(Drum::default(), change(k));
            let f = c
                .findings
                .iter()
                .find(|f| f.id == id)
                .unwrap_or_else(|| panic!("{id} at {k}: not compared"));
            // The measure's own resolution: a quarter of a threshold, or a tenth of the size.
            let tolerance = (0.1 * k).max(0.25);
            assert!(
                (f.units - k).abs() <= tolerance,
                "{id} at {k} thresholds read {:.2}",
                f.units
            );
            if k < 1.0 {
                assert!(!c.audible().any(|g| g.id == id), "{id} at {k}: reported");
            } else {
                let rising = id != "level.body_loudness";
                assert_eq!(f.candidate > f.reference, rising, "{id} at {k}: wrong side");
            }
        }
    }
}
