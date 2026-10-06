//! A synth voice's readings, each on a synthetic voice whose answer is known by construction: the
//! voice is played a note stimulus and read back through `respond`, as an instrument's crate would.

use mxm_listening::reading::Report;
use mxm_listening::respond::respond;
use mxm_listening::stimulus::{Kind, Stimulus};

const RATE: f64 = 48_000.0;
const TAU: f64 = std::f64::consts::TAU;

fn value(r: &Report, id: &str) -> Option<f64> {
    r.find(id, None).and_then(|x| x.value)
}

fn read(r: &Report, id: &str) -> f64 {
    value(r, id).unwrap_or_else(|| panic!("{id} absent: {:#?}", r.find(id, None)))
}

fn close(r: &Report, id: &str, want: f64, tol: f64) {
    let got = read(r, id);
    assert!(
        (got - want).abs() <= tol,
        "{id}: {got} against {want} ± {tol}"
    );
}

fn note(key: u8, held: f64) -> Stimulus {
    let mut s = Stimulus::new(Kind::Note { key, velocity: 100 });
    s.seconds = held;
    s.tail_s = 1.5;
    s
}

/// An ADSR: a linear attack, an exponential decay to the sustain, an exponential release.
#[derive(Clone, Copy)]
struct Adsr {
    attack: f64,
    decay: f64,
    sustain: f64,
    release: f64,
}

const HELD: Adsr = Adsr {
    attack: 0.005,
    decay: 0.1,
    sustain: 1.0,
    release: 0.05,
};

/// The voice: `osc(t)` shaped by `env`, from the note's on to its off and its release.
fn play(s: &Stimulus, env: Adsr, osc: impl Fn(f64) -> f64) -> Vec<f64> {
    let (on, off) = (s.lead_s, s.lead_s + s.seconds);
    let level = |t: f64| {
        if t < env.attack {
            t / env.attack
        } else {
            env.sustain + (1.0 - env.sustain) * (-(t - env.attack) / env.decay).exp()
        }
    };
    (0..s.len())
        .map(|n| {
            let t = n as f64 / RATE;
            let e = if t < on {
                0.0
            } else if t < off {
                level(t - on)
            } else if env.release > 0.0 {
                level(off - on) * (-(t - off) / env.release).exp()
            } else {
                0.0
            };
            0.5 * e * osc(t - on)
        })
        .collect()
}

fn hear(s: &Stimulus, y: &[f64]) -> Report {
    let y: Vec<f32> = y.iter().map(|&v| v as f32).collect();
    respond(s, &[y], s.rate, "voice").expect("the voice is read")
}

fn additive_saw(f0: f64) -> impl Fn(f64) -> f64 {
    let top = (0.5 * RATE / f0).floor() as usize;
    move |t| {
        (1..=top)
            .map(|k| (TAU * k as f64 * f0 * t).sin() / k as f64)
            .sum::<f64>()
            * (2.0 / std::f64::consts::PI)
            * 0.5
    }
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_naive_saw_aliases_audibly_and_an_additive_one_does_not() {
    let s = note(81, 1.0);
    let f0 = 880.0;
    let naive = hear(&s, &play(&s, HELD, |t| 2.0 * (f0 * t).fract() - 1.0));
    let clean = hear(&s, &play(&s, HELD, additive_saw(f0)));
    let (a, b) = (
        read(&naive, "voice.aliasing"),
        read(&clean, "voice.aliasing"),
    );
    assert!(a > -30.0, "naive {a}");
    assert!(b < -70.0, "additive {b}");
    assert!(read(&naive, "voice.alias_audible") > 10.0);
    assert!(read(&clean, "voice.alias_audible") < 0.0);
    // A clean saw has nothing off its series: no zipper, no line.
    assert!(value(&clean, "voice.zipper_hz").is_none());
    assert!(value(&clean, "voice.line_hz").is_none());
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn an_adsr_reads_its_stages() {
    let s = note(69, 1.5);
    let env = Adsr {
        attack: 0.02,
        decay: 0.15,
        sustain: 0.5,
        release: 0.1,
    };
    let r = hear(&s, &play(&s, env, |t| (TAU * 440.0 * t).sin()));
    // The attack ends 0.5 dB under the peak: 0.944 of a 20 ms ramp.
    close(&r, "voice.attack", 18.9, 2.5);
    close(&r, "voice.decay", 150.0, 15.0);
    close(&r, "voice.sustain", -6.02, 0.3);
    // An exponential release of 100 ms falls 60 dB in 6.9 of them.
    close(&r, "voice.release", 690.8, 35.0);
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_hard_note_off_clicks_and_a_released_one_does_not() {
    // Released a quarter cycle into the last period, where the sine is at its peak.
    let s = note(69, 1.0 + 0.25 / 440.0);
    let hard = Adsr {
        release: 0.0,
        ..HELD
    };
    let soft = Adsr {
        attack: 0.02,
        release: 0.1,
        ..HELD
    };
    let r = hear(&s, &play(&s, hard, |t| (TAU * 440.0 * t).sin()));
    assert!(
        read(&r, "voice.off_click") > 10.0,
        "{}",
        read(&r, "voice.off_click")
    );
    let r = hear(&s, &play(&s, soft, |t| (TAU * 440.0 * t).sin()));
    assert!(
        read(&r, "voice.off_click") < 3.0,
        "{}",
        read(&r, "voice.off_click")
    );
    assert!(
        read(&r, "voice.on_click") < 3.0,
        "{}",
        read(&r, "voice.on_click")
    );
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_gain_stepped_every_64_samples_reads_as_zipper_at_750_hz() {
    let s = note(95, 1.5);
    let f0 = 440.0 * 2f64.powf(26.0 / 12.0);
    let held = s.seconds * RATE;
    let stepped = |t: f64| {
        let n = (t * RATE).max(0.0);
        let step = (n / 64.0).floor() * 64.0;
        (1.0 - 0.7 * (step / held).min(1.0)) * (TAU * f0 * t).sin()
    };
    let r = hear(&s, &play(&s, HELD, stepped));
    close(&r, "voice.zipper_hz", 750.0, 3.0);
    let smooth = |t: f64| (1.0 - 0.7 * (t * RATE / held).min(1.0)) * (TAU * f0 * t).sin();
    let r = hear(&s, &play(&s, HELD, smooth));
    assert!(value(&r, "voice.zipper_hz").is_none());
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_detuned_unison_beats_twice_as_fast_on_its_second_harmonic() {
    let s = note(57, 3.0);
    let (up, down) = (
        220.0 * 2f64.powf(5.0 / 1200.0),
        220.0 * 2f64.powf(-5.0 / 1200.0),
    );
    let (a, b) = (additive_saw(up), additive_saw(down));
    let r = hear(&s, &play(&s, HELD, |t| 0.5 * (a(t) + b(t))));
    close(&r, "voice.unison_beat", up - down, 0.1);
    close(&r, "voice.unison_detune", 10.0, 1.0);
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_resonant_filter_peaks_on_the_series_and_a_singing_filter_is_a_line_off_it() {
    let s = note(45, 1.0);
    let saw = additive_saw(110.0);
    let raw = play(&s, HELD, &saw);
    // An RBJ low-pass, 2 kHz, Q 6.
    let w = TAU * 2000.0 / RATE;
    let (alpha, c) = (w.sin() / 12.0, w.cos());
    let (b, a) = (
        [(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0],
        [1.0 + alpha, -2.0 * c, 1.0 - alpha],
    );
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    let filtered: Vec<f64> = raw
        .iter()
        .map(|&v| {
            let y = (b[0] * v + b[1] * x1 + b[2] * x2 - a[1] * y1 - a[2] * y2) / a[0];
            (x2, x1, y2, y1) = (x1, v, y1, y);
            y
        })
        .collect();
    let r = hear(&s, &filtered);
    close(&r, "voice.peak_hz", 2000.0, 120.0);
    assert!(read(&r, "voice.peak_db") > 6.0);
    let singing = play(&s, HELD, |t| saw(t) + 0.3 * (TAU * 1234.0 * t).sin());
    let r = hear(&s, &singing);
    close(&r, "voice.line_hz", 1234.0, 1.0);
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_bent_sine_reads_its_distortion_and_an_offset_its_dc() {
    let s = note(69, 1.0);
    let clean = play(&s, HELD, |t| (TAU * 440.0 * t).sin());
    let (on, off) = (s.start(), s.start() + (s.seconds * RATE) as usize);
    let bent: Vec<f64> = clean
        .iter()
        .enumerate()
        .map(|(n, &x)| x + 0.1 * x * x + if n >= on && n < off { 0.01 } else { 0.0 })
        .collect();
    let r = hear(&s, &bent);
    // x = 0.5·sin: the square law's second harmonic is 0.1·0.25/2 against 0.5.
    close(&r, "voice.thd", 0.1 * 0.25 / 2.0 / 0.5 * 100.0, 0.05);
    // The offset and the square law's own DC, 0.1·0.25/2.
    close(&r, "voice.dc", 20.0 * (0.01 + 0.0125f64).log10(), 0.2);
}
