//! Stimuli, their sidecars, and responses read against them: every effect reading proved on a
//! synthetic system whose answer is known in closed form (the plan's §2 items 9 and 10, §6).

use mxm_listening::reading::Report;
use mxm_listening::respond::{against_settings, matrix, respond};
use mxm_listening::stimulus::{Channel, Kind, Stimulus};

const RATE: f64 = 48_000.0;

fn value(r: &Report, id: &str) -> Option<f64> {
    r.find(id, None).and_then(|x| x.value)
}

fn read(r: &Report, id: &str) -> f64 {
    value(r, id).unwrap_or_else(|| panic!("{id} absent in {:#?}", r.find(id, None)))
}

fn close(r: &Report, id: &str, want: f64, tol: f64) {
    let got = read(r, id);
    assert!(
        (got - want).abs() <= tol,
        "{id}: {got} against {want} ± {tol}"
    );
}

fn mono(s: &Stimulus) -> Vec<f64> {
    s.render()
}

fn f32s(x: &[f64]) -> Vec<f32> {
    x.iter().map(|&v| v as f32).collect()
}

fn answer(s: &Stimulus, y: &[f64]) -> Report {
    respond(s, &[f32s(y)], s.rate, "test").expect("the response is read")
}

/// The RBJ cookbook's biquads, run forward.
fn biquad(x: &[f64], b: [f64; 3], a: [f64; 3]) -> Vec<f64> {
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    x.iter()
        .map(|&v| {
            let y = (b[0] * v + b[1] * x1 + b[2] * x2 - a[1] * y1 - a[2] * y2) / a[0];
            (x2, x1, y2, y1) = (x1, v, y1, y);
            y
        })
        .collect()
}

fn low_pass(f0: f64, q: f64) -> ([f64; 3], [f64; 3]) {
    let w = std::f64::consts::TAU * f0 / RATE;
    let alpha = w.sin() / (2.0 * q);
    let c = w.cos();
    (
        [(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0],
        [1.0 + alpha, -2.0 * c, 1.0 - alpha],
    )
}

fn high_pass(f0: f64, q: f64) -> ([f64; 3], [f64; 3]) {
    let w = std::f64::consts::TAU * f0 / RATE;
    let alpha = w.sin() / (2.0 * q);
    let c = w.cos();
    (
        [(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0],
        [1.0 + alpha, -2.0 * c, 1.0 - alpha],
    )
}

fn delayed(x: &[f64], by: usize) -> Vec<f64> {
    let mut y = vec![0.0; x.len()];
    y[by..].copy_from_slice(&x[..x.len() - by]);
    y
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn every_kind_round_trips_through_its_sidecar_and_an_edit_is_refused() {
    for name in [
        "impulse",
        "sweep",
        "bursts",
        "steps",
        "sine",
        "dual-sine",
        "noise",
        "silence",
        "note",
    ] {
        let mut s = Stimulus::new(Kind::default_of(name).unwrap());
        s.settings.push(("cutoff_hz".into(), "1000".into()));
        let text = s.sidecar();
        let back = Stimulus::parse(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(back, s, "{name}");
        assert_eq!(s.render().len(), s.len(), "{name}");
    }
    let s = Stimulus::new(Kind::Sine { hz: 1000.0 });
    let edited = s.sidecar().replace("hz\t1000", "hz\t1001");
    assert!(
        Stimulus::parse(&edited)
            .unwrap_err()
            .contains("fingerprint")
    );
    assert!(Stimulus::parse("kind\tsine\n").is_err());
    let mut high = Stimulus::new(Kind::Sine { hz: 30_000.0 });
    assert!(high.check().is_err());
    high.kind = Kind::Sine { hz: 1000.0 };
    high.level_dbfs = 1.0;
    assert!(high.check().is_err());
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn the_sweep_is_farinas_and_a_note_is_silent() {
    let s = Stimulus::new(Kind::Sweep {
        from_hz: 20.0,
        to_hz: 20_000.0,
    });
    let x = mono(&s);
    let peak = x.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    assert!((20.0 * peak.log10() + 6.0).abs() < 0.01, "{peak}");
    // Instantaneous frequency halfway through, by zero crossings: the geometric mean, 632 Hz.
    let mid = s.start() + s.signal_len() / 2;
    let seg = &x[mid - 480..mid + 480];
    let crossings = seg.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
    assert!(
        (crossings as f64 / 0.02 - 632.5).abs() < 60.0,
        "{crossings}"
    );
    let note = Stimulus::new(Kind::Note {
        key: 69,
        velocity: 100,
    });
    assert!(note.render().iter().all(|&v| v == 0.0));
    assert!((note.note_hz().unwrap() - 440.0).abs() < 1e-9);
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_sweep_through_a_resonant_low_pass_reads_its_peak_q_and_latency() {
    let s = Stimulus::new(Kind::Sweep {
        from_hz: 20.0,
        to_hz: 20_000.0,
    });
    let (b, a) = low_pass(1000.0, 4.0);
    let y = delayed(&biquad(&mono(&s), b, a), 48);
    let r = answer(&s, &y);
    // The RBJ low-pass's gain at its f0 is Q: 12.04 dB.
    close(&r, "response.gain", 12.04, 0.3);
    close(&r, "response.latency", 1.0, 0.25);
    // An analog resonance peaks at f0·√(1 − 1/2Q²) and its width 3 dB down is about f0/Q.
    close(
        &r,
        "response.peak_hz",
        1000.0 * (1.0 - 1.0 / 32.0f64).sqrt(),
        25.0,
    );
    close(&r, "response.q", 4.0, 0.5);
    close(&r, "response.peak_db", 12.1, 0.5);
    assert!(value(&r, "response.low_edge").is_none());
    let edge = read(&r, "response.high_edge");
    assert!(edge > 1300.0 && edge < 1700.0, "{edge}");
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_high_pass_reads_its_edge_and_no_peak() {
    let s = Stimulus::new(Kind::Sweep {
        from_hz: 20.0,
        to_hz: 20_000.0,
    });
    let (b, a) = high_pass(200.0, std::f64::consts::FRAC_1_SQRT_2);
    let r = answer(&s, &biquad(&mono(&s), b, a));
    close(&r, "response.low_edge", 200.0, 6.0);
    close(&r, "response.gain", 0.0, 0.1);
    assert!(value(&r, "response.peak_hz").is_none());
    assert!(value(&r, "response.high_edge").is_none());
}

/// A feedback delay: `y = x + g·y[n − d]`, with a one-pole low-pass in the loop when `pole > 0`.
fn feedback_delay(x: &[f64], d: usize, g: f64, pole: f64) -> Vec<f64> {
    let mut y = vec![0.0; x.len()];
    let mut lp = 0.0;
    for n in 0..x.len() {
        let back = if n >= d { y[n - d] } else { 0.0 };
        lp = (1.0 - pole) * back + pole * lp;
        y[n] = x[n] + g * lp;
    }
    y
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn an_impulse_through_a_feedback_delay_reads_its_time_feedback_and_darkening() {
    let mut s = Stimulus::new(Kind::Impulse);
    s.tail_s = 1.5;
    let clean = answer(&s, &feedback_delay(&mono(&s), 4800, 0.5, 0.0));
    close(&clean, "delay.time", 100.0, 0.1);
    close(&clean, "delay.feedback", -6.02, 0.1);
    close(&clean, "delay.darkening", 0.0, 0.3);
    assert!(read(&clean, "delay.repeats") >= 8.0);
    let dark = answer(&s, &feedback_delay(&mono(&s), 4800, 0.7, 0.5));
    close(&dark, "delay.time", 100.0, 0.5);
    assert!(
        read(&dark, "delay.darkening") < -1.0,
        "{}",
        read(&dark, "delay.darkening")
    );
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn an_exponentially_decaying_noise_reads_as_a_space_with_its_decay_time() {
    let mut s = Stimulus::new(Kind::Impulse);
    s.tail_s = 3.0;
    let x = mono(&s);
    let noise = mxm_measure::stimulus::noise(x.len(), 7, 1.0);
    // An impulse response of T60 = 1.2 s, convolved with the impulse: the response itself, at the lead.
    let y: Vec<f64> = (0..x.len())
        .map(|n| {
            if n < s.start() {
                return 0.0;
            }
            let t = (n - s.start()) as f64 / RATE;
            0.5 * f64::from(noise[n]) * 10f64.powf(-3.0 * t / 1.2)
        })
        .collect();
    let r = answer(&s, &y);
    let t30 = r
        .sections
        .iter()
        .flat_map(|s| &s.readings)
        .find(|x| x.id == "space.t30" && x.band_hz.is_some_and(|b| b.0 < 1000.0 && b.1 > 1000.0))
        .and_then(|x| x.value)
        .expect("a 1 kHz decay time");
    assert!((t30 - 1.2).abs() < 0.08, "{t30}");
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_sine_through_a_polynomial_reads_its_harmonics_and_dc_exactly() {
    let s = Stimulus::new(Kind::Sine { hz: 1000.0 });
    let y: Vec<f64> = mono(&s)
        .iter()
        .map(|&x| x + 0.1 * x * x + 0.05 * x * x * x)
        .collect();
    let r = answer(&s, &y);
    let a = 10f64.powf(-6.0 / 20.0);
    let (h1, h2, h3, dc) = (
        a + 0.05 * 0.75 * a * a * a,
        0.05 * a * a,
        0.0125 * a * a * a,
        0.05 * a * a,
    );
    let db = |v: f64| 20.0 * v.log10();
    close(&r, "effect.gain", db(h1 / a), 0.01);
    close(&r, "effect.h2", db(h2 / h1), 0.05);
    close(&r, "effect.h3", db(h3 / h1), 0.05);
    close(&r, "effect.dc", db(dc), 0.05);
    close(
        &r,
        "effect.thd",
        (h2 * h2 + h3 * h3).sqrt() / h1 * 100.0,
        0.01,
    );
    assert!(value(&r, "effect.pitch_wobble").is_none());
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_tremolo_and_a_vibrato_on_a_sine_read_as_wobble() {
    let s = Stimulus::new(Kind::Sine { hz: 1000.0 });
    let x = mono(&s);
    // Tremolo: ±3 dB at 5 Hz; vibrato by a delay swept ±0.1 ms at 4 Hz: ±2.5 cents·… of 1 kHz.
    let y: Vec<f64> = (0..x.len())
        .map(|n| {
            let t = n as f64 / RATE;
            let gain = 10f64.powf(3.0 * (std::f64::consts::TAU * 5.0 * t).sin() / 20.0);
            let d = 0.0001 * (std::f64::consts::TAU * 4.0 * t).sin() * RATE;
            let at = n as f64 - 10.0 - d;
            let i = at.floor() as usize;
            let f = at - at.floor();
            let v = x.get(i).copied().unwrap_or(0.0) * (1.0 - f)
                + x.get(i + 1).copied().unwrap_or(0.0) * f;
            gain * v
        })
        .collect();
    let r = answer(&s, &y);
    close(&r, "effect.level_wobble_rate", 5.0, 0.3);
    close(&r, "effect.level_wobble", 3.0, 0.4);
    close(&r, "effect.pitch_wobble_rate", 4.0, 0.3);
    // A delay swept by D·sin(2π·fr·t) moves the frequency by the factor 1 − 2π·fr·D·cos(…).
    let cents = 1200.0 * (1.0 + std::f64::consts::TAU * 4.0 * 0.0001).log2();
    close(&r, "effect.pitch_wobble", cents, 0.3 * cents);
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn two_tones_through_a_square_law_read_second_order_intermodulation_exactly() {
    let s = Stimulus::new(Kind::DualSine {
        hz: 60.0,
        hz2: 7000.0,
        second_db: -12.0,
    });
    let k = 0.05;
    let r = answer(
        &s,
        &mono(&s)
            .iter()
            .map(|&x| x + k * x * x)
            .collect::<Vec<f64>>(),
    );
    let peak = 10f64.powf(-6.0 / 20.0);
    let ratio = 10f64.powf(-12.0 / 20.0);
    let (a, b) = (peak / (1.0 + ratio), peak * ratio / (1.0 + ratio));
    // x² holds 2ab·cos·cos: a·b at f2 − f1 and at f2 + f1.
    let product = k * a * b;
    let imd2 = 20.0 * ((2.0 * product * product).sqrt() / (a * a + b * b).sqrt()).log10();
    close(&r, "effect.imd2", imd2, 0.05);
    close(&r, "effect.imd", 2f64.sqrt() * product / b * 100.0, 0.005);
    assert!(read(&r, "effect.imd3") < -120.0);
}

/// A compressor with an ideal level detector: the stimulus's own step level, the gain a one-pole in dB.
fn compressor(s: &Stimulus, threshold: f64, ratio: f64, attack_s: f64, release_s: f64) -> Vec<f64> {
    let x = mono(s);
    let steps = s.steps();
    let (ka, kr) = (
        (-1.0 / (attack_s * RATE)).exp(),
        (-1.0 / (release_s * RATE)).exp(),
    );
    let mut g = 0.0f64;
    (0..x.len())
        .map(|n| {
            let level = steps
                .iter()
                .find(|&&(a, b, _)| n >= a && n < b)
                .map_or(-200.0, |st| st.2);
            let target = -(1.0 - 1.0 / ratio) * (level - threshold).max(0.0);
            let k = if target < g { ka } else { kr };
            g = target + (g - target) * k;
            x[n] * 10f64.powf(g / 20.0)
        })
        .collect()
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn level_steps_through_a_compressor_read_its_curve_and_times() {
    let mut s = Stimulus::new(Kind::default_of("steps").unwrap());
    s.level_dbfs = 0.0;
    let y = delayed(&compressor(&s, -20.0, 4.0, 0.005, 0.1), 24);
    let r = answer(&s, &y);
    close(&r, "effect.threshold", -20.0, 1.0);
    close(&r, "effect.ratio", 4.0, 0.2);
    close(&r, "effect.makeup", 0.0, 0.1);
    close(&r, "effect.attack", 5.0, 0.6);
    close(&r, "effect.release", 100.0, 5.0);
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn silence_reads_a_line_its_level_and_a_runaway() {
    let s = Stimulus::new(Kind::Silence);
    let noise = mxm_measure::stimulus::noise(s.len(), 3, 1.0);
    let line: Vec<f64> = (0..s.len())
        .map(|n| {
            let t = n as f64 / RATE;
            1e-4 * f64::from(noise[n])
                + 10f64.powf(-30.0 / 20.0) * (std::f64::consts::TAU * 1234.5 * t).sin()
        })
        .collect();
    let r = answer(&s, &line);
    close(&r, "effect.line_hz", 1234.5, 0.5);
    close(&r, "effect.line_level", -30.0, 0.2);
    close(&r, "effect.runaway", 0.0, 0.3);
    let growing: Vec<f64> = (0..s.len())
        .map(|n| 1e-3 * f64::from(noise[n]) * 10f64.powf(6.0 * n as f64 / RATE / 20.0))
        .collect();
    let r = answer(&s, &growing);
    close(&r, "effect.runaway", 6.0, 0.3);
    assert!(value(&r, "effect.line_hz").is_none());
}

/// A chorus: `y(n) = x(n − d(n))`, `d` swept sinusoidally, read by linear interpolation.
fn chorus(x: &[f64], mean_ms: f64, depth_ms: f64, hz: f64, phase: f64, dry: f64) -> Vec<f64> {
    (0..x.len())
        .map(|n| {
            let t = n as f64 / RATE;
            let d = (mean_ms + depth_ms * (std::f64::consts::TAU * hz * t + phase).sin()) / 1000.0
                * RATE;
            let at = n as f64 - d;
            if at < 0.0 {
                return dry * x[n];
            }
            let i = at.floor() as usize;
            let f = at - at.floor();
            dry * x[n] + x[i] * (1.0 - f) + x.get(i + 1).copied().unwrap_or(0.0) * f
        })
        .collect()
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn noise_through_a_stereo_chorus_reads_its_sweep_and_the_channels_phase() {
    let s = Stimulus::new(Kind::Noise { seed: 5 });
    let x = mono(&s);
    let left = chorus(&x, 7.0, 2.0, 0.8, 0.0, 1.0);
    let right = chorus(&x, 7.0, 2.0, 0.8, std::f64::consts::FRAC_PI_2, 1.0);
    let r = respond(&s, &[f32s(&left), f32s(&right)], s.rate, "chorus").unwrap();
    close(&r, "effect.mod_rate", 0.8, 0.02);
    close(&r, "effect.mod_depth", 2.0, 0.15);
    close(&r, "effect.mod_delay", 7.0, 0.15);
    close(&r, "effect.mod_phase", 90.0, 8.0);
    assert!(read(&r, "effect.mod_shape") < -25.0);
    assert!(read(&r, "response.coherence") < 0.9);
    assert!(value(&r, "stereo.correlation").is_some());
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn noise_through_a_fixed_filter_is_coherent_and_reads_as_an_impulse_response() {
    let s = Stimulus::new(Kind::Noise { seed: 9 });
    let (b, a) = low_pass(2000.0, std::f64::consts::FRAC_1_SQRT_2);
    let r = answer(&s, &biquad(&mono(&s), b, a));
    assert!(read(&r, "response.coherence") > 0.99);
    close(&r, "response.high_edge", 2000.0, 60.0);
    assert!(value(&r, "effect.mod_rate").is_none());
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn stereo_width_correlation_and_the_true_stereo_matrix() {
    let s = Stimulus::new(Kind::Noise { seed: 2 });
    let x = mono(&s);
    let same = respond(&s, &[f32s(&x), f32s(&x)], s.rate, "mono").unwrap();
    close(&same, "stereo.correlation", 1.0, 1e-9);
    assert!(value(&same, "stereo.width").is_none());
    let flipped: Vec<f64> = x.iter().map(|v| -v).collect();
    let opposed = respond(&s, &[f32s(&x), f32s(&flipped)], s.rate, "opposed").unwrap();
    close(&opposed, "stereo.correlation", -1.0, 1e-9);

    let mut l = Stimulus::new(Kind::Impulse);
    l.channel = Channel::Left;
    let mut rt = l.clone();
    rt.channel = Channel::Right;
    let d = mono(&l);
    let scaled = |k: f64| f32s(&d.iter().map(|v| v * k).collect::<Vec<f64>>());
    let from_left = [scaled(1.0), scaled(0.5)];
    let from_right = [scaled(0.25), scaled(1.0)];
    let m = matrix((&l, &from_left), (&rt, &from_right)).expect("a matrix");
    let get = |id: &str| {
        m.readings
            .iter()
            .find(|r| r.id == id)
            .and_then(|r| r.value)
            .unwrap()
    };
    assert!((get("stereo.cross_left") + 6.02).abs() < 0.01);
    assert!((get("stereo.cross_right") + 12.04).abs() < 0.01);
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn readings_are_tabled_against_a_setting() {
    let mut reports = Vec::new();
    let mut stimuli = Vec::new();
    for cutoff in [2000.0, 500.0, 1000.0] {
        let mut s = Stimulus::new(Kind::Sweep {
            from_hz: 20.0,
            to_hz: 20_000.0,
        });
        s.seconds = 3.0;
        s.settings.push(("cutoff_hz".into(), format!("{cutoff}")));
        let (b, a) = low_pass(cutoff, 3.0);
        reports.push(answer(&s, &biquad(&mono(&s), b, a)));
        stimuli.push(s);
    }
    let pairs: Vec<(&Stimulus, &Report)> = stimuli.iter().zip(&reports).collect();
    let section = against_settings(&pairs).expect("a table");
    let t = &section.tables[0];
    let peak = t
        .columns
        .iter()
        .position(|c| c.0 == "The response's peak")
        .unwrap();
    let rows: Vec<(f64, f64)> = t
        .rows
        .iter()
        .map(|r| (r[0].unwrap(), r[peak].unwrap()))
        .collect();
    assert_eq!(
        rows.iter().map(|r| r.0).collect::<Vec<_>>(),
        vec![500.0, 1000.0, 2000.0]
    );
    for (cutoff, peak) in rows {
        assert!((peak / cutoff - 0.97).abs() < 0.04, "{cutoff}: {peak}");
    }
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn bursts_through_a_plain_gain_read_the_gain_and_a_short_hangover() {
    let s = Stimulus::new(Kind::default_of("bursts").unwrap());
    let r = answer(&s, &mono(&s).iter().map(|v| 0.5 * v).collect::<Vec<f64>>());
    close(&r, "effect.burst_gain", -6.02, 0.05);
    close(&r, "effect.burst_first", 0.0, 0.05);
    assert!(read(&r, "effect.hangover") <= 1.0);
    let echo = feedback_delay(&mono(&s), 2400, 0.5, 0.0);
    let r = answer(&s, &echo);
    assert!(value(&r, "effect.hangover").is_none_or(|h| h > 100.0));
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_response_at_another_rate_or_with_a_nan_is_refused() {
    let s = Stimulus::new(Kind::Sine { hz: 1000.0 });
    let y = f32s(&mono(&s));
    assert!(respond(&s, std::slice::from_ref(&y), 44_100, "x").is_err());
    let mut bad = y;
    bad[100] = f32::NAN;
    assert!(respond(&s, &[bad], 48_000, "x").is_err());
}

/// The plan's §6 invariants over every stimulus and a voice, at every rate the crate is scored on:
/// never NaN, absent rather than a value, and identical output for identical input.
#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn every_stimulus_and_a_voice_keep_the_invariants_at_every_rate() {
    fn check(r: &Report) {
        for s in &r.sections {
            for x in &s.readings {
                if let Some(v) = x.value {
                    assert!(v.is_finite(), "{}: {} is {v}", r.name, x.id);
                }
                if matches!(x.validity, mxm_listening::reading::Validity::Absent(_)) {
                    assert!(
                        x.value.is_none(),
                        "{}: {} absent with a value",
                        r.name,
                        x.id
                    );
                }
            }
            for t in &s.tables {
                for row in &t.rows {
                    assert!(
                        row.iter().flatten().all(|v| v.is_finite()),
                        "{}: {}",
                        r.name,
                        t.id
                    );
                }
            }
        }
    }
    for rate in mxm_measure::RATES {
        for name in [
            "impulse",
            "sweep",
            "bursts",
            "steps",
            "sine",
            "dual-sine",
            "noise",
            "silence",
        ] {
            let mut s = Stimulus::new(Kind::default_of(name).unwrap());
            s.rate = rate as u32;
            s.seconds = s.seconds.min(2.0);
            s.tail_s = 0.5;
            // A gentle system: a one-pole low-pass and a quiet echo 20 ms later.
            let x = s.render();
            let d = (0.02 * rate) as usize;
            let mut lp = 0.0;
            let y: Vec<f32> = (0..x.len())
                .map(|n| {
                    lp = 0.7 * lp + 0.3 * x[n];
                    (lp + if n >= d { 0.2 * x[n - d] } else { 0.0 }) as f32
                })
                .collect();
            let a = respond(&s, std::slice::from_ref(&y), s.rate, name).unwrap();
            let b = respond(&s, std::slice::from_ref(&y), s.rate, name).unwrap();
            check(&a);
            assert_eq!(a, b, "{name} at {rate}: not identical");
        }
        let mut note = Stimulus::new(Kind::Note {
            key: 69,
            velocity: 100,
        });
        note.rate = rate as u32;
        note.seconds = 0.6;
        note.tail_s = 0.4;
        let (on, off) = (note.start(), note.start() + (note.seconds * rate) as usize);
        let y: Vec<f32> = (0..note.len())
            .map(|n| {
                if n < on || n >= off {
                    return 0.0;
                }
                let t = (n - on) as f64 / rate;
                ((t / 0.01).min(1.0) * 0.3 * (std::f64::consts::TAU * 440.0 * t).sin()) as f32
            })
            .collect();
        let a = respond(&note, std::slice::from_ref(&y), note.rate, "voice").unwrap();
        check(&a);
        assert_eq!(
            a,
            respond(&note, &[y], note.rate, "voice").unwrap(),
            "voice at {rate}"
        );
    }
}
