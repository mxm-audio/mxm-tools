//! `describe` on sounds whose answers are known by construction, and the invariants every report
//! keeps: never NaN, absent rather than zero, the same answer at every rate.

use mxm_listening::reading::{Report, Validity};
use mxm_listening::repr::spectrum::{Spectrum, resonance};
use mxm_listening::{Family, Sound, describe};

const TAU: f64 = std::f64::consts::TAU;

/// 5 ms of silence, then `a·e^{−t/τ}·sin(2πft)` for `seconds`.
fn damped(rate: f64, hz: f64, tau: f64, seconds: f64) -> Sound {
    let pre = (0.005 * rate) as usize;
    let mut x = vec![0.0f32; pre];
    let n = (seconds * rate) as usize;
    x.extend((0..n).map(|i| {
        let t = i as f64 / rate;
        (0.8 * (-t / tau).exp() * (TAU * hz * t).sin()) as f32
    }));
    Sound::new("damped", rate as u32, x)
}

fn value(r: &Report, id: &str, window_from: Option<f64>) -> f64 {
    let reading = r
        .find(id, window_from)
        .unwrap_or_else(|| panic!("{id} missing"));
    reading
        .value
        .unwrap_or_else(|| panic!("{id} absent: {:?}", reading.validity))
}

fn no_nan(r: &Report) {
    for s in &r.sections {
        for x in &s.readings {
            if let Some(v) = x.value {
                assert!(v.is_finite(), "{} is {v}", x.id);
            }
            if matches!(x.validity, Validity::Absent(_)) {
                assert!(x.value.is_none(), "{} absent with a value", x.id);
            }
        }
    }
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_damped_tone_decays_at_its_known_rate_at_every_rate() {
    // τ = 0.2 s: the amplitude falls 20 dB in τ·ln 10 = 460.5 ms, at 8.686/τ = 43.4 dB/s.
    for rate in mxm_measure::RATES {
        let r = describe(&damped(rate, 220.0, 0.2, 3.0), None);
        no_nan(&r);
        assert_eq!(
            r.family.family,
            Family::Percussive,
            "{rate}: {}",
            r.family.how
        );
        let t20 = value(&r, "decay.t20", None);
        assert!((t20 - 460.5).abs() < 6.0, "{rate}: t20 {t20}");
        let t40 = value(&r, "decay.t40", None);
        assert!((t40 - 921.0).abs() < 8.0, "{rate}: t40 {t40}");
        let early = value(&r, "decay.early_slope", None);
        assert!((early + 43.43).abs() < 1.0, "{rate}: early slope {early}");
        let late = value(&r, "decay.late_slope", Some(300.0));
        assert!((late + 43.43).abs() < 1.0, "{rate}: late slope {late}");
        // The 50 ms level at 500 ms, against the loudest 10 ms in the first 30 ms, is the exponential's
        // fall over ~0.5 s (the window centres 25 ms later; the reference sits near the start).
        let late500 = value(&r, "decay.late_level", Some(500.0));
        assert!(
            (late500 + 43.43 * 0.52).abs() < 1.5,
            "{rate}: level at 500 ms {late500}"
        );
    }
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_two_stage_decay_shows_two_slopes() {
    // A fast first stage (τ = 30 ms) over a quiet slow ring (τ = 0.5 s, 30 dB down).
    let rate = 48_000.0;
    let pre = 240;
    let mut x = vec![0.0f32; pre];
    x.extend((0..(2.0 * rate) as usize).map(|i| {
        let t = i as f64 / rate;
        let fast = (-t / 0.03).exp() * (TAU * 300.0 * t).sin();
        let slow = 0.0316 * (-t / 0.5).exp() * (TAU * 180.0 * t).sin();
        (0.8 * (fast + slow)) as f32
    }));
    let r = describe(
        &Sound::new("two stages", 48_000, x),
        Some(Family::Percussive),
    );
    no_nan(&r);
    let early = value(&r, "decay.early_slope", None);
    let late = value(&r, "decay.late_slope", Some(300.0));
    assert!(early < -200.0, "early {early}");
    assert!((late + 17.37).abs() < 1.0, "late {late}");
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn the_attack_reads_a_known_rise_peak_and_direction() {
    // A 1 kHz tone whose envelope rises linearly over 5 ms, then decays: the RMS envelope's 10–90 %
    // rise is about 0.8 × 5 ms, the peak comes at the top of the ramp, and the first swing is up.
    let rate = 48_000.0;
    let mut x = vec![0.0f32; 240];
    x.extend((0..(1.0 * rate) as usize).map(|i| {
        let t = i as f64 / rate;
        let env = if t < 0.005 {
            t / 0.005
        } else {
            (-(t - 0.005) / 0.1).exp()
        };
        (0.7 * env * (TAU * 1000.0 * t).sin()) as f32
    }));
    let r = describe(&Sound::new("ramp", 48_000, x), None);
    no_nan(&r);
    let rise = value(&r, "attack.rise_time", None);
    assert!((rise - 4.0).abs() < 0.7, "rise {rise}");
    let peak = value(&r, "attack.peak_time", None);
    assert!((peak - 5.0).abs() < 1.0, "peak {peak}");
    let step = value(&r, "attack.first_step", None);
    assert!(step < 0.05, "a ramp starts small: {step}");
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn the_first_swing_s_direction_is_read() {
    let up = damped(48_000.0, 200.0, 0.1, 0.5);
    let down = Sound::new("down", 48_000, up.samples.iter().map(|s| -s).collect());
    assert_eq!(value(&describe(&up, None), "attack.polarity", None), 1.0);
    assert_eq!(value(&describe(&down, None), "attack.polarity", None), -1.0);
}

/// The octave under the rest, 25–50 Hz, read over one of its own periods (40 ms): a kick's lowest
/// push sounds there, below the octave table's old floor of 50 Hz.
#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn the_lowest_octave_finds_a_kicks_sub_bass() {
    let rate = 48_000.0;
    let r = describe(&damped(rate, 35.0, 2.0, 1.5), Some(Family::Percussive));
    no_nan(&r);
    let band = |lo: f64, t: f64| {
        r.sections
            .iter()
            .flat_map(|s| &s.readings)
            .find(|x| {
                x.id == "tone.band_level"
                    && x.band_hz.is_some_and(|b| (b.0 - lo).abs() < 1e-9)
                    && x.window_ms
                        .is_some_and(|w| (0.5 * (w.0 + w.1) - t).abs() < 1e-9)
            })
            .expect("every octave and time has a reading")
    };
    let low = band(25.0, 50.0);
    assert_eq!(low.resolution.map(|r| r.window_ms), Some(40.0));
    // The octave holding the tone reads within a dB and a half of the broadband level, as at 1 kHz.
    let own = low.value.expect("the tone's octave has a level");
    assert!(own > -1.5 && own < 0.5, "own octave {own}");
    let up = band(100.0, 50.0).value;
    assert!(up.is_none_or(|v| v < -25.0), "two octaves up {up:?}");
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn brightness_and_octave_levels_find_a_tone() {
    let rate = 48_000.0;
    let r = describe(&damped(rate, 1000.0, 2.0, 1.5), Some(Family::Percussive));
    no_nan(&r);
    let c = value(&r, "attack.early_centroid", None);
    assert!((c - 1000.0).abs() < 30.0, "early centroid {c}");
    let c = value(&r, "tone.centroid", Some(20.0));
    assert!((c - 1000.0).abs() < 20.0, "centroid {c}");
    // The octave holding the tone reads within a dB of the broadband level; one well away is far below.
    let band = |lo: f64, t: f64| {
        r.sections
            .iter()
            .flat_map(|s| &s.readings)
            .find(|x| {
                x.id == "tone.band_level"
                    && x.band_hz.is_some_and(|b| (b.0 - lo).abs() < 1e-9)
                    && x.window_ms
                        .is_some_and(|w| (0.5 * (w.0 + w.1) - t).abs() < 1e-9)
            })
            .expect("every octave and time has a reading")
            .value
    };
    let own = band(800.0, 50.0).expect("the tone's octave has a level");
    assert!(own > -1.5 && own < 0.5, "own octave {own}");
    // Two octaves down reads far below, or numerically silent (no value) under the numerical floor.
    let down = band(200.0, 50.0);
    assert!(down.is_none_or(|v| v < -40.0), "two octaves down {down:?}");
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn resonance_finds_a_known_peak_and_q() {
    // A Lorentzian power peak at 2 kHz with Q = 10, sampled every 1 Hz; lightly smoothed.
    let (f0, q) = (2000.0, 10.0);
    let power: Vec<f64> = (0..8000)
        .map(|k| {
            let f = k as f64;
            1.0 / (1.0 + ((f - f0) / (f0 / (2.0 * q))).powi(2))
        })
        .collect();
    let s = Spectrum {
        power,
        bin_hz: 1.0,
        window_ms: 1000.0,
    };
    let (peak, got_q) = resonance(&s, 300.0, 7000.0, 0.002).unwrap();
    assert!((peak - f0).abs() <= 2.0, "peak {peak}");
    let got_q = got_q.unwrap();
    assert!((got_q - q).abs() < 0.3, "Q {got_q}");
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn silence_and_broken_audio_are_reported_not_measured() {
    let r = describe(&Sound::new("silence", 48_000, vec![0.0; 48_000]), None);
    assert!(r.sections.is_empty());
    assert!(r.notes.iter().any(|n| n.contains("silent")));
    let mut x = vec![0.1f32; 1000];
    x[500] = f32::NAN;
    let r = describe(&Sound::new("nan", 48_000, x), None);
    assert!(r.sections.is_empty());
    assert!(r.notes.iter().any(|n| n.contains("not a number")));
    let r = describe(&Sound::new("empty", 48_000, Vec::new()), None);
    assert!(r.sections.is_empty());
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_short_file_reports_late_readings_absent() {
    let r = describe(&damped(48_000.0, 300.0, 0.05, 0.2), None);
    no_nan(&r);
    let late = r.find("decay.late_level", Some(800.0)).unwrap();
    assert!(matches!(late.validity, Validity::Absent(_)));
    assert!(
        r.notes
            .iter()
            .any(|n| n.contains("late readings are absent"))
    );
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn reports_serialise_without_nan() {
    let r = describe(&damped(48_000.0, 440.0, 0.3, 1.0), None);
    let json = mxm_listening::report::json::to_json(&r);
    assert!(!json.contains("NaN") && !json.contains("inf"));
    assert!(json.contains("\"decay.t20\""));
    let md = mxm_listening::report::markdown::to_markdown(&r);
    assert!(md.contains("## Decay") && md.contains("Octave levels"));
}

/// A reading made in stages (`plans/plan-mxm-listener-hud.md` §2.3): the first stage holds every
/// part but the perceptual models, and the second adds exactly those — a held note's fluctuation
/// strength in its place, the `perception` section last — leaving every other reading as it was.
#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_reading_in_stages_adds_only_the_perceptual_models() {
    use mxm_listening::describe::{Options, describe_staged};

    let rate = 48_000.0;
    // A held A4 with three harmonics, its level swung at 4 Hz: a fluctuation strength to read.
    let held: Vec<f32> = (0..(3.0 * rate) as usize)
        .map(|i| {
            let t = i as f64 / rate;
            let tone: f64 = (1..=3)
                .map(|k| (TAU * 440.0 * k as f64 * t).sin() / k as f64)
                .sum();
            (0.3 * (1.0 + 0.8 * (TAU * 4.0 * t).sin()) * tone) as f32
        })
        .collect();
    let held = Sound::new("held", rate as u32, held);
    let hit = damped(rate, 180.0, 0.15, 1.0);

    for (sound, family) in [(&hit, None), (&held, Some(Family::Sustained))] {
        let options = Options {
            family,
            ..Options::default()
        };
        let staged = describe_staged(sound, &options);
        let first = staged.report().clone();
        assert!(first.sections.iter().all(|s| s.part != "perception"));
        let full = staged.perceive();
        assert_eq!(full.sections.last().map(|s| s.part), Some("perception"));
        assert_eq!(full.sections.len(), first.sections.len() + 1);
        for (a, b) in first.sections.iter().zip(&full.sections) {
            for (x, y) in a.readings.iter().zip(&b.readings) {
                if x.id != "sustain.fluctuation" {
                    assert_eq!(x, y, "{} changed in the second stage", x.id);
                }
            }
            assert_eq!(a.tables, b.tables);
        }
        no_nan(&full);
    }

    let fluctuation = |r: &Report| r.find("sustain.fluctuation", None).map(|x| x.value);
    let first = describe_staged(
        &held,
        &Options {
            family: Some(Family::Sustained),
            ..Options::default()
        },
    );
    assert_eq!(
        fluctuation(first.report()),
        Some(None),
        "absent before the models"
    );
    let full = first.perceive();
    assert!(
        fluctuation(&full).flatten().is_some_and(|v| v > 0.0),
        "read by the models"
    );
}
