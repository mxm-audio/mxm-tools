//! Cutting a run of struck notes: twelve strikes at irregular spacing come back as exactly twelve
//! notes, named from the run's range — through a vibraphone's deep, slow tremolo, and past the two
//! things that broke the first cuts on the Iowa runs: a pianissimo strike beside a loud low knock, and
//! a second strike of the note before.

use mxm_listening::Sound;
use mxm_listening::split::{attacks, check_cents, note_hz, notes, range_names};

const RATE: f64 = 48_000.0;
const STARTS: [f64; 12] = [
    0.5, 3.1, 4.9, 8.2, 9.6, 12.9, 14.1, 17.7, 19.0, 22.4, 24.8, 26.1,
];
const TOTAL_S: f64 = 29.0;

/// Twelve notes up from C4, each struck at its start with amplitude `level(k)` and decaying over 1.5 s,
/// with a 5 Hz tremolo `depth` deep.
fn run(level: impl Fn(usize) -> f64, depth: f64) -> Vec<f64> {
    let mut x = vec![0.0f64; (TOTAL_S * RATE) as usize];
    for (k, &s) in STARTS.iter().enumerate() {
        strike(
            &mut x,
            s,
            261.63 * 2f64.powf(k as f64 / 12.0),
            level(k),
            depth,
        );
    }
    x
}

fn strike(x: &mut [f64], at_s: f64, hz: f64, level: f64, depth: f64) {
    for (i, v) in x.iter_mut().enumerate().skip((at_s * RATE) as usize) {
        let t = i as f64 / RATE - at_s;
        let trem = 1.0 - depth * (0.5 - 0.5 * (std::f64::consts::TAU * 5.0 * t).cos());
        *v += level * (-t / 1.5).exp() * trem * (std::f64::consts::TAU * hz * t).sin();
    }
}

fn hz(names: &[String]) -> Vec<f64> {
    names.iter().filter_map(|n| note_hz(n)).collect()
}

fn assert_cut_at_starts(at: &[usize]) {
    assert_eq!(at.len(), 12);
    for (a, s) in at.iter().zip(STARTS) {
        let err = (*a as f64 / RATE - s).abs();
        assert!(err < 0.025, "strike at {s} s cut at {} s", *a as f64 / RATE);
    }
}

#[test]
fn a_range_in_the_name_names_the_notes() {
    assert_eq!(range_names("Marimba.yarn.mf.C4B4").unwrap().len(), 12);
    assert_eq!(
        range_names("bells.brass.C8E8").unwrap(),
        ["C8", "Db8", "D8", "Eb8", "E8"]
    );
    assert_eq!(range_names("xylophone.rosewood.pp.F4B4").unwrap()[0], "F4");
    assert_eq!(range_names("Marimba.yarn.pp.C7").unwrap(), ["C7"]);
    assert!(range_names("snare hit").is_none());
    assert!((note_hz("A4").unwrap() - 440.0).abs() < 1e-9);
    assert!((note_hz("C4").unwrap() - 261.626).abs() < 1e-3);
}

#[test]
fn a_run_with_a_tremolo_is_cut_at_its_strikes() {
    // A 5 Hz tremolo 90 % deep: the level swings by about 20 dB, slowly.
    let x = run(|k| 0.3 + 0.05 * (k % 3) as f64, 0.9);
    let run = Sound::new(
        "Vibraphone.sustain.mf.C4B4",
        RATE as u32,
        x.iter().map(|&v| v as f32).collect(),
    );
    let names = range_names(&run.name).unwrap();
    assert_cut_at_starts(&attacks(&run.samples, run.rate, &hz(&names)));
    let cut = notes(&run).unwrap();
    assert_eq!(cut.len(), 12);
    assert_eq!(cut[0].name, "Vibraphone.sustain.mf.C4B4 C4");
    assert_eq!(cut[11].name, "Vibraphone.sustain.mf.C4B4 B4");
    for (note, name) in cut.iter().zip(&names) {
        let c = check_cents(note, note_hz(name).unwrap()).unwrap();
        assert!(c.abs() < 10.0, "{name} reads {c:+.0} c");
    }
}

#[test]
fn a_quiet_strike_beats_a_loud_knock_and_a_second_strike() {
    // E4 (the fifth note) 30 dB under the rest; a knock at 50 Hz, loud and sudden, late in the run;
    // and C4 struck again at 1.9 s, as loud as the first time.
    let mut x = run(|k| if k == 4 { 0.01 } else { 0.3 }, 0.0);
    for (i, v) in x.iter_mut().enumerate().skip((27.6 * RATE) as usize) {
        let t = i as f64 / RATE - 27.6;
        *v += 0.8 * (-t / 0.08).exp() * (std::f64::consts::TAU * 50.0 * t).sin();
    }
    strike(&mut x, 1.9, 261.63, 0.3, 0.0);
    let samples: Vec<f32> = x.iter().map(|&v| v as f32).collect();
    let names = range_names("Marimba.yarn.pp.C4B4").unwrap();
    assert_cut_at_starts(&attacks(&samples, RATE as u32, &hz(&names)));
}

#[test]
fn a_run_of_held_notes_is_cut_where_each_starts() {
    // Twelve bowed-like notes: an 80 ms soft start, held for 1.6 s, each fading under the next, the
    // fundamental 20 dB under the second harmonic.
    let starts: Vec<f64> = (0..12).map(|k| 0.5 + 2.0 * f64::from(k)).collect();
    let mut x = vec![0.0f64; (25.0 * RATE) as usize];
    for (k, &s) in starts.iter().enumerate() {
        let f0 = 196.0 * 2f64.powf(k as f64 / 12.0);
        for (i, v) in x.iter_mut().enumerate().skip((s * RATE) as usize) {
            let t = i as f64 / RATE - s;
            if t > 2.2 {
                break;
            }
            let env = (t / 0.08).min(1.0) * ((2.2 - t) / 0.4).clamp(0.0, 1.0);
            let tone: f64 = (1..=6)
                .map(|n| {
                    let a = if n == 1 { 0.1 } else { 1.0 / f64::from(n) };
                    a * (std::f64::consts::TAU * f64::from(n) * f0 * t).sin()
                })
                .sum();
            *v += 0.2 * env * tone;
        }
    }
    let samples: Vec<f32> = x.iter().map(|&v| v as f32).collect();
    let names = range_names("Cello.arco.mf.sulG.G3Gb4").unwrap();
    let hz: Vec<f64> = names.iter().filter_map(|n| note_hz(n)).collect();
    let at = mxm_listening::split::attacks_held(&samples, RATE as u32, &hz);
    assert_eq!(at.len(), 12);
    for (a, s) in at.iter().zip(&starts) {
        let err = *a as f64 / RATE - s;
        assert!(
            (-0.06..0.12).contains(&err),
            "a note starting at {s} s cut at {} s",
            *a as f64 / RATE
        );
    }
}
