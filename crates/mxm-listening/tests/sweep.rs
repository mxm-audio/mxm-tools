//! The plan's §6 perturbation sweep for the families after the drums (`compare`'s own sweep holds a
//! drum's): struck and held notes, a synth voice and impulse responses. Each change, made at 0.5, 2
//! and 4 of its reading's threshold on a synthetic sound built from the parameter it moves, comes back
//! as that reading at that many thresholds, on its side; under one threshold it is not reported.

use mxm_listening::audibility::{Thresholds, Vocabulary};
use mxm_listening::compare::compare;
use mxm_listening::describe::{Options, describe_with};
use mxm_listening::reading::Report;
use mxm_listening::respond::respond;
use mxm_listening::stimulus::{Kind, Stimulus};
use mxm_listening::{Family, Sound};

const TAU: f64 = std::f64::consts::TAU;
const RATE: f64 = 48_000.0;
const SIZES: [f64; 3] = [0.5, 2.0, 4.0];

/// Compares `candidate(k)` with the reference at each size and checks the reading `id` (in the band
/// holding `band_hz`, where given). `tolerance` is the measure's resolution in thresholds, the reason
/// written beside each call; a tenth of the size is always allowed.
fn sweep(
    id: &str,
    band_hz: Option<f64>,
    reference: &Report,
    candidate: impl Fn(f64) -> Report,
    rising: bool,
    tolerance: f64,
) {
    for k in SIZES {
        let c = compare(
            reference,
            &candidate(k),
            &Thresholds::literature(),
            &Vocabulary::owner(),
        );
        let hit = |f: &&mxm_listening::compare::Finding| {
            f.id == id && band_hz.is_none_or(|hz| f.band_hz.is_some_and(|b| b.0 <= hz && hz < b.1))
        };
        let f = c
            .findings
            .iter()
            .find(hit)
            .unwrap_or_else(|| panic!("{id} at {k}: not compared"));
        assert!(
            (f.units - k).abs() <= tolerance.max(0.1 * k),
            "{id} at {k} thresholds read {:.2} ({} → {})",
            f.units,
            f.reference,
            f.candidate
        );
        if k < 1.0 {
            assert!(!c.audible().any(|g| hit(&g)), "{id} at {k}: reported");
        } else {
            assert_eq!(f.candidate > f.reference, rising, "{id} at {k}: wrong side");
        }
    }
}

/// A struck stiff string: sixteen partials on the stiff-string law, their levels on a tilt against
/// log frequency and their decays on a law against it.
#[derive(Clone, Copy)]
struct Struck {
    f0: f64,
    b: f64,
    tilt: f64,
    t60: f64,
    law: f64,
}

const STRUCK: Struck = Struck {
    f0: 220.0,
    b: 2e-4,
    tilt: -6.0,
    t60: 2.0,
    law: -0.7,
};

impl Struck {
    fn report(self) -> Report {
        let mut x = vec![0.0f32; 240];
        x.extend((0..(3.0 * RATE) as usize).map(|i| {
            let t = i as f64 / RATE;
            let v: f64 = (1..=16)
                .map(|n| {
                    let nf = f64::from(n);
                    let f = nf * self.f0 * (1.0 + self.b * nf * nf).sqrt();
                    let octaves = (f / self.f0).log2();
                    let a = 0.3 * 10f64.powf(self.tilt * octaves / 20.0);
                    let t60 = self.t60 * (f / self.f0).powf(self.law);
                    a * (-6.91 * t / t60).exp() * (TAU * f * t).sin()
                })
                .sum();
            ((t / 0.002).min(1.0) * v) as f32
        }));
        describe_with(
            &Sound::new("struck", RATE as u32, x),
            &Options {
                family: Some(Family::Note),
                without_perception: true,
                ..Options::default()
            },
        )
    }
}

#[test]
#[ignore = "slow: run before a release with `cargo test -- --ignored`; over 30 s"]
fn a_struck_notes_changes_come_back_at_their_sizes() {
    let reference = STRUCK.report();
    // The fundamental is read to a hundredth of a cent on a noise-free partial.
    sweep(
        "note.tuning",
        None,
        &reference,
        |k| {
            Struck {
                f0: 220.0 * 2f64.powf(5.0 * k / 1200.0),
                ..STRUCK
            }
            .report()
        },
        true,
        0.1,
    );
    // A T60 is fitted over the fall's first 40 dB: a few per cent, a fifth of a threshold.
    sweep(
        "note.t60",
        None,
        &reference,
        |k| {
            Struck {
                t60: 2.0 * 1.25f64.powf(k),
                ..STRUCK
            }
            .report()
        },
        true,
        0.2,
    );
    sweep(
        "note.tilt",
        None,
        &reference,
        |k| {
            Struck {
                tilt: -6.0 + k,
                ..STRUCK
            }
            .report()
        },
        true,
        0.2,
    );
    // The law is a line through sixteen fitted T60s: each a few per cent off, the line 0.03.
    sweep(
        "note.decay_law",
        None,
        &reference,
        |k| {
            Struck {
                law: -0.7 + 0.15 * k,
                ..STRUCK
            }
            .report()
        },
        true,
        0.25,
    );
    sweep(
        "note.inharmonicity",
        None,
        &reference,
        |k| {
            Struck {
                b: 2e-4 * 1.3f64.powf(k),
                ..STRUCK
            }
            .report()
        },
        true,
        0.2,
    );
}

/// A held harmonic tone with a 5.5 Hz vibrato `depth` cents deep.
fn held(f0: f64, depth: f64) -> Report {
    let mut phase = 0.0f64;
    let x: Vec<f32> = (0..(2.5 * RATE) as usize)
        .map(|i| {
            let t = i as f64 / RATE;
            let cents = depth * (TAU * 5.5 * t).sin();
            phase += TAU * f0 * 2f64.powf(cents / 1200.0) / RATE;
            let env = (t / 0.02).min(1.0) * ((2.5 - t) / 0.1).clamp(0.0, 1.0);
            let tone: f64 = (1..=10)
                .map(|n| (f64::from(n) * phase).sin() / f64::from(n))
                .sum();
            (0.2 * env * tone) as f32
        })
        .collect();
    describe_with(
        &Sound::new("held", RATE as u32, x),
        &Options {
            family: Some(Family::Sustained),
            without_perception: true,
            ..Options::default()
        },
    )
}

#[test]
#[ignore = "slow: run before a release with `cargo test -- --ignored`; over 30 s"]
fn a_held_notes_changes_come_back_at_their_sizes() {
    let reference = held(293.66, 20.0);
    // The held pitch is the median of a track every 10 ms: a vibrato's swing averages out to a cent.
    sweep(
        "note.tuning",
        None,
        &reference,
        |k| held(293.66 * 2f64.powf(5.0 * k / 1200.0), 20.0),
        true,
        0.25,
    );
    // The depth is the power within 1 Hz of the track spectrum's peak: a few per cent.
    sweep(
        "sustain.vibrato_depth",
        None,
        &reference,
        |k| held(293.66, 20.0 * 1.25f64.powf(k)),
        true,
        0.25,
    );
}

/// A synth voice played a note: an additive saw at 220 Hz, a linear attack, the sustain `sustain_db`
/// under the peak (reached by a 50 ms decay), a 100 ms release.
fn voice(attack_s: f64, sustain_db: f64) -> Report {
    let mut s = Stimulus::new(Kind::Note {
        key: 57,
        velocity: 100,
    });
    s.seconds = 1.5;
    s.tail_s = 1.0;
    let (on, off) = (s.lead_s, s.lead_s + s.seconds);
    let sustain = 10f64.powf(sustain_db / 20.0);
    let level = |t: f64| {
        if t < attack_s {
            t / attack_s
        } else {
            sustain + (1.0 - sustain) * (-(t - attack_s) / 0.05).exp()
        }
    };
    let y: Vec<f32> = (0..s.len())
        .map(|n| {
            let t = n as f64 / RATE;
            let e = if t < on {
                0.0
            } else if t < off {
                level(t - on)
            } else {
                level(off - on) * (-(t - off) / 0.1).exp()
            };
            let saw: f64 = (1..=100)
                .map(|k| (TAU * f64::from(k) * 220.0 * (t - on)).sin() / f64::from(k))
                .sum();
            (0.1 * e * saw) as f32
        })
        .collect();
    respond(&s, &[y], s.rate, "voice").expect("the voice is read")
}

#[test]
#[ignore = "slow: run before a release with `cargo test -- --ignored`; over 30 s"]
fn a_voices_changes_come_back_at_their_sizes() {
    let reference = voice(0.06, -6.0);
    // The envelope is an RMS over two periods (9 ms at 220 Hz), read every millisecond: near a short
    // attack's top it blends the rise into the decay, and a 20 ms attack 2 ms longer read 0.9 of a
    // threshold rather than 0.5. On a 60 ms attack the millisecond is the resolution, a fifth.
    sweep(
        "voice.attack",
        None,
        &reference,
        |k| voice(0.06 * 1.25f64.powf(k), -6.0),
        true,
        0.2,
    );
    sweep(
        "voice.sustain",
        None,
        &reference,
        |k| voice(0.06, -6.0 - k),
        false,
        0.1,
    );
}

fn sweep_through(s: &Stimulus, system: impl Fn(&[f64]) -> Vec<f64>) -> Report {
    let y: Vec<f32> = system(&s.render()).iter().map(|&v| v as f32).collect();
    respond(s, &[y], s.rate, "ir").expect("the response is read")
}

fn low_pass(x: &[f64], f0: f64) -> Vec<f64> {
    let w = TAU * f0 / RATE;
    let (alpha, c) = (w.sin() * std::f64::consts::FRAC_1_SQRT_2, w.cos());
    let (b, a) = (
        [(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0],
        [1.0 + alpha, -2.0 * c, 1.0 - alpha],
    );
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    x.iter()
        .map(|&v| {
            let y = (b[0] * v + b[1] * x1 + b[2] * x2 - a[1] * y1 - a[2] * y2) / a[0];
            (x2, x1, y2, y1) = (x1, v, y1, y);
            y
        })
        .collect()
}

fn echo(x: &[f64], d: usize) -> Vec<f64> {
    let mut y = vec![0.0; x.len()];
    for n in 0..x.len() {
        y[n] = x[n] + if n >= d { 0.5 * y[n - d] } else { 0.0 };
    }
    y
}

fn room(x: &[f64], t60: f64) -> Vec<f64> {
    let noise = mxm_measure::stimulus::noise(x.len(), 11, 1.0);
    let start = x.iter().position(|v| *v != 0.0).unwrap_or(0);
    (0..x.len())
        .map(|n| {
            if n < start {
                return 0.0;
            }
            let t = (n - start) as f64 / RATE;
            0.5 * f64::from(noise[n]) * 10f64.powf(-3.0 * t / t60)
        })
        .collect()
}

#[test]
#[ignore = "slow: run before a release with `cargo test -- --ignored`; over 30 s"]
fn an_impulse_responses_changes_come_back_at_their_sizes() {
    let mut sweep_s = Stimulus::new(Kind::Sweep {
        from_hz: 20.0,
        to_hz: 20_000.0,
    });
    sweep_s.seconds = 3.0;
    let reference = sweep_through(&sweep_s, |x| low_pass(x, 2000.0));
    // The edge is interpolated on a 1/24-octave grid: well under a tenth of 4.5 %.
    sweep(
        "response.high_edge",
        None,
        &reference,
        |k| sweep_through(&sweep_s, |x| low_pass(x, 2000.0 * 1.045f64.powf(k))),
        true,
        0.1,
    );
    let mut impulse = Stimulus::new(Kind::Impulse);
    impulse.tail_s = 3.0;
    let reference = sweep_through(&impulse, |x| echo(x, 4800));
    sweep(
        "delay.time",
        None,
        &reference,
        |k| {
            sweep_through(&impulse, |x| {
                echo(x, (4800.0 * 1.25f64.powf(k)).round() as usize)
            })
        },
        true,
        0.05,
    );
    let mut long = Stimulus::new(Kind::Impulse);
    long.tail_s = 6.0;
    let reference = sweep_through(&long, |x| room(x, 1.0));
    // A noise decay's T30 wanders by a few per cent with its draw.
    sweep(
        "space.t30",
        Some(1000.0),
        &reference,
        |k| sweep_through(&long, |x| room(x, 1.0 * 1.25f64.powf(k))),
        true,
        0.25,
    );
}
