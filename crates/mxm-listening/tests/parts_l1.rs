//! L1's parts on sounds whose answers are known by construction: a two-mode drum with a glide, a
//! tone with a known wobble, bursts against steady noise, a click, hum, and a set with a room line.

use mxm_listening::describe::{Options, describe_with};
use mxm_listening::reading::{Report, Validity};
use mxm_listening::{Family, Sound, set};

const TAU: f64 = std::f64::consts::TAU;
const RATE: f64 = 48_000.0;

fn value(r: &Report, id: &str, window_from: Option<f64>) -> Option<f64> {
    r.find(id, window_from).and_then(|x| x.value)
}

fn percussive(sound: &Sound) -> Report {
    describe_with(
        sound,
        &Options {
            family: Some(Family::Percussive),
            without_perception: true,
            ..Options::default()
        },
    )
}

/// A deterministic noise source (xorshift64*), uniform in −1..1.
struct Noise(u64);
impl Noise {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 52) as f64 - 1.0
    }
}

/// 5 ms of silence, then `f(t)` for `seconds`.
fn sound(name: &str, seconds: f64, f: impl Fn(f64) -> f64) -> Sound {
    let mut x = vec![0.0f32; 240];
    x.extend((0..(seconds * RATE) as usize).map(|i| f(i as f64 / RATE) as f32));
    Sound::new(name, RATE as u32, x)
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_drum_s_rest_pitch_overtone_and_glide_are_read() {
    // A 200 Hz fundamental gliding down 40 cents over its first 50 ms, an overtone at 1.59×, and a
    // quieter low mode at 0.62× that dies fast: the rest pitch is 200 Hz, the overtone's ratio 1.59,
    // and early the fundamental is sharp.
    // The glide's phase, integrated once: f(t) = 200·2^{(40/1200)·e^{−t/0.02}}.
    let n = (1.5 * RATE) as usize;
    let mut phase = Vec::with_capacity(n);
    let mut ph = 0.0;
    for k in 0..n {
        phase.push(ph);
        let t = k as f64 / RATE;
        ph += TAU * 200.0 * 2f64.powf(40.0 / 1200.0 * (-t / 0.02).exp()) / RATE;
    }
    let s = sound("drum", 1.5, |t| {
        let i = ((t * RATE) as usize).min(n - 1);
        0.6 * (-t / 0.35).exp() * phase[i].sin()
            + 0.25 * (-t / 0.25).exp() * (TAU * 318.0 * t).sin()
            + 0.3 * (-t / 0.03).exp() * (TAU * 124.0 * t).sin()
    });
    let r = percussive(&s);
    let rest = value(&r, "pitch.rest", None).expect("a rest pitch");
    assert!((rest - 200.0).abs() < 1.0, "rest {rest}");
    // The glide at 20 ms (a 0–40 ms window) averages about +17 c of a 40 c glide decaying in 20 ms;
    // by 100 ms it has settled.
    let early = value(&r, "pitch.glide", Some(0.0)).expect("an early glide reading");
    assert!(early > 8.0, "early glide {early} c");
    let late = value(&r, "pitch.glide", Some(80.0)).expect("a late glide reading");
    assert!(late.abs() < 3.0, "late glide {late} c");
    let table = r
        .sections
        .iter()
        .flat_map(|s| &s.tables)
        .find(|t| t.id == "pitch.modes" && t.window_ms == Some((60.0, 160.0)))
        .expect("modes 60–160 ms");
    assert!(
        table
            .rows
            .iter()
            .any(|row| row[1].is_some_and(|ratio| (ratio - 1.59).abs() < 0.01)),
        "the 1.59× overtone"
    );
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_ring_s_wobble_scales_with_its_depth_and_a_steady_ring_is_clean() {
    // A 250 Hz ring amplitude-modulated at 30 Hz, 4 % and 8 % deep, against the same ring steady. The
    // measure is the guide's (a 30 ms moving average removed, then 15–300 Hz), which passes only part
    // of a slow wobble, so what is asserted is what a comparison relies on: the reading doubles with
    // the depth and stands clear of a steady ring.
    let ring = |depth: f64| {
        sound("ring", 1.0, move |t| {
            0.7 * (-t / 0.4).exp()
                * (1.0 + depth * (TAU * 30.0 * t).sin())
                * (TAU * 250.0 * t).sin()
        })
    };
    let w8 = value(&percussive(&ring(0.08)), "modulation.ring_wobble", None).unwrap();
    let w4 = value(&percussive(&ring(0.04)), "modulation.ring_wobble", None).unwrap();
    let w0 = value(&percussive(&ring(0.0)), "modulation.ring_wobble", None).unwrap();
    assert!(
        (w8 / w4 - 2.0).abs() < 0.2,
        "8 % reads {w8}, 4 % reads {w4}"
    );
    assert!(w8 > 5.0 * w0 && w0 < 0.5, "8 % reads {w8}, steady {w0}");
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn clustered_slaps_read_as_clustered_and_steady_noise_does_not() {
    let mut n = Noise(0x1234_5678_9ABC_DEF1);
    let steady: Vec<f64> = (0..(1.0 * RATE) as usize).map(|_| n.next()).collect();
    let mut n = Noise(0x0FED_CBA9_8765_4321);
    // Bursts: noise gated on for 3 ms every 11 ms, so the 1 ms level is correlated over a few ms.
    let bursty: Vec<f64> = (0..(1.0 * RATE) as usize)
        .map(|i| {
            let t = i as f64 / RATE;
            let on = (t * 1000.0) % 11.0 < 3.0;
            n.next() * if on { 1.0 } else { 0.1 }
        })
        .collect();
    let body = |t: f64| 0.8 * (-t / 0.05).exp() * (TAU * 200.0 * t).sin();
    let a = sound("steady", 1.0, |t| {
        body(t) + 0.05 * (-t / 0.3).exp() * steady[(t * RATE) as usize % steady.len()]
    });
    let b = sound("bursty", 1.0, |t| {
        body(t) + 0.05 * (-t / 0.3).exp() * bursty[(t * RATE) as usize % bursty.len()]
    });
    let ra = percussive(&a);
    let rb = percussive(&b);
    let ac_a = value(&ra, "texture.burst_autocorrelation", Some(1.0)).unwrap();
    let ac_b = value(&rb, "texture.burst_autocorrelation", Some(1.0)).unwrap();
    assert!(ac_a.abs() < 0.15, "steady noise clusters {ac_a}");
    assert!(ac_b > 0.3, "bursts {ac_b}");
    assert!(matches!(
        ra.find("texture.burst_swing", None).unwrap().validity,
        Validity::Sample
    ));
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_step_at_the_onset_reads_as_a_click() {
    // Two sounds, one starting smoothly and one with a step: the step's high band spikes at once.
    let smooth = sound("smooth", 0.6, |t| {
        let ramp = (t / 0.002).min(1.0);
        0.8 * ramp * (-t / 0.1).exp() * (TAU * 180.0 * t).sin()
    });
    let step = sound("step", 0.6, |t| {
        0.8 * (-t / 0.1).exp() * (TAU * 180.0 * t + 1.2).sin()
    });
    let a = value(&percussive(&smooth), "artefacts.click", None).unwrap();
    let b = value(&percussive(&step), "artefacts.click", None).unwrap();
    assert!(b > a + 10.0, "click: smooth {a} dB, step {b} dB");
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn mains_hum_under_a_decayed_hit_is_found() {
    let hum = |t: f64| {
        0.001
            * ((TAU * 50.0 * t).sin()
                + 0.5 * (TAU * 100.0 * t).sin()
                + 0.3 * (TAU * 150.0 * t).sin())
    };
    let with = sound("hum", 3.0, |t| {
        0.8 * (-t / 0.05).exp() * (TAU * 300.0 * t).sin() + hum(t)
    });
    let without = sound("clean", 3.0, |t| {
        0.8 * (-t / 0.05).exp() * (TAU * 300.0 * t).sin()
    });
    let a = value(&percussive(&with), "artefacts.hum_50", None).unwrap();
    assert!(a > 20.0, "hum {a} dB");
    let b = value(&percussive(&without), "artefacts.hum_50", None);
    assert!(b.is_none_or(|v| v < 10.0), "no hum {b:?}");
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_line_every_take_shares_is_the_room() {
    // Four takes of a drum at slightly different pitches, each with the same faint 168 Hz line that
    // outlasts it: the set names 168 Hz as the room's, and keeps it out of the rest pitch.
    let takes: Vec<Sound> = [230.0, 233.0, 228.0, 231.0]
        .iter()
        .enumerate()
        .map(|(k, &f0)| {
            sound(&format!("take {k}"), 2.0, move |t| {
                0.8 * (-t / 0.2).exp() * (TAU * f0 * t).sin()
                    + 0.002 * (-t / 3.0).exp() * (TAU * 168.0 * t).sin()
            })
        })
        .collect();
    let report = set::analyse(
        &[("one layer".to_string(), takes)],
        Some(Family::Percussive),
    );
    assert!(
        report.room_lines.iter().any(|hz| (hz - 168.0).abs() < 1.0),
        "room lines {:?}",
        report.room_lines
    );
    let rest = report.groups[0].stat("pitch.rest", None).unwrap();
    assert!((rest.mean - 230.5).abs() < 3.0, "rest {}", rest.mean);
    assert!(
        rest.spread > 0.5 && rest.spread < 3.0,
        "spread {}",
        rest.spread
    );
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn random_wires_read_as_white_noise_a_rattle_above_it_and_the_rattle_share_follows_its_level() {
    // The owner, 2026-09-27: "the rattle from the springs is a little more pronounced on the
    // original". Plain random wires read white noise's kurtosis through this measure (2.75, not 3:
    // the 5 ms envelope is divided out); wires that rattle — distinct slaps, here a quarter of the
    // 1 ms steps four times louder — read above it; more wire energy raises the rattle's share.
    let mut n = Noise(0x5DEE_CE66_D1CE_4E5B);
    let random: Vec<f64> = (0..(1.0 * RATE) as usize).map(|_| n.next()).collect();
    let mut n = Noise(0x2545_F491_4F6C_DD1D);
    let slaps: Vec<f64> = (0..1000)
        .map(|_| if n.next() > 0.5 { 4.0 } else { 1.0 })
        .collect();
    let body = |t: f64| 0.6 * (-t / 0.1).exp() * (TAU * 200.0 * t).sin();
    let wires = |t: f64, gain: f64| {
        gain * 0.15 * (-t / 0.2).exp() * random[(t * RATE) as usize % random.len()]
    };
    let noisy = percussive(&sound("random", 1.0, |t| body(t) + wires(t, 1.0)));
    let louder = percussive(&sound("louder", 1.0, |t| body(t) + wires(t, 2f64.sqrt())));
    let rattly = percussive(&sound("rattly", 1.0, |t| {
        body(t) + wires(t, 0.5) * slaps[((t * 1000.0) as usize).min(999)]
    }));
    let kurt = |r: &Report| value(r, "texture.slap_kurtosis", Some(30.0)).unwrap();
    assert!(
        (kurt(&noisy) - 2.75).abs() < 0.25,
        "random wires {}",
        kurt(&noisy)
    );
    assert!(
        kurt(&rattly) > kurt(&noisy) + 0.3,
        "rattle {} against noise {}",
        kurt(&rattly),
        kurt(&noisy)
    );
    // 3 dB more wire noise raises the rattle's share by most of 3 dB (the whole grows with it).
    let rattle = |r: &Report| value(r, "texture.rattle", Some(30.0)).unwrap();
    let rise = rattle(&louder) - rattle(&noisy);
    assert!(rise > 1.5 && rise < 3.1, "rattle rose {rise} dB");
}
