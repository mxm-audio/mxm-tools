//! Tonal or noise: a decaying tone is a line; the same peak made by filtering noise through a resonance
//! is noise; a sound with both keeps them apart by band.

use mxm_listening::describe::{Options, describe_with};
use mxm_listening::repr::bands;
use mxm_listening::{Family, Report, Sound};

const TAU: f64 = std::f64::consts::TAU;
const RATE: f64 = 48_000.0;

fn noise(n: usize, seed: u64) -> Vec<f64> {
    let mut s = seed;
    (0..n)
        .map(|_| {
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            (s.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 52) as f64 - 1.0
        })
        .collect()
}

/// White noise through a resonance at `hz` with quality `q` (a second-order band-pass, twice).
fn resonant_noise(n: usize, hz: f64, q: f64, seed: u64) -> Vec<f64> {
    let bw = hz / q;
    let s = bands::butterworth_band_pass(1, hz - bw / 2.0, hz + bw / 2.0, RATE).unwrap();
    bands::filter_zero_phase(&s, &noise(n, seed))
}

fn describe(x: Vec<f64>) -> Report {
    let peak = x.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let mut samples = vec![0.0f32; 240];
    samples.extend(x.iter().map(|v| (0.8 * v / peak) as f32));
    describe_with(
        &Sound::new("s", RATE as u32, samples),
        &Options {
            family: Some(Family::Percussive),
            without_perception: true,
            ..Options::default()
        },
    )
}

fn share(r: &Report, band_lo: f64, window_from: f64) -> f64 {
    r.sections
        .iter()
        .flat_map(|s| &s.readings)
        .find(|x| {
            x.id == "tonality.tonal_share"
                && x.band_hz.is_some_and(|b| (b.0 - band_lo).abs() < 1e-9)
                && x.window_ms
                    .is_some_and(|w| (w.0 - window_from).abs() < 1e-9)
        })
        .and_then(|x| x.value)
        .expect("a tonal share")
}

fn envelope(_n: usize, tau: f64) -> impl Fn(usize) -> f64 {
    move |i| {
        let t = i as f64 / RATE;
        (t / 0.001).min(1.0) * (-t / tau).exp()
    }
}

#[test]
fn a_decaying_tone_is_a_line_and_resonant_noise_is_not() {
    let n = RATE as usize;
    let env = envelope(n, 0.3);
    let tone: Vec<f64> = (0..n)
        .map(|i| env(i) * (TAU * 1000.0 * i as f64 / RATE).sin())
        .collect();
    let noisy: Vec<f64> = resonant_noise(n, 1000.0, 20.0, 7)
        .iter()
        .enumerate()
        .map(|(i, v)| env(i) * v)
        .collect();
    let a = share(&describe(tone), 800.0, 20.0);
    let b = share(&describe(noisy), 800.0, 20.0);
    assert!(a > 80.0, "tone: {a} % tonal");
    assert!(b < 30.0, "resonant noise at Q 20: {b} % tonal");
}

#[test]
fn a_tone_over_a_noise_band_is_kept_apart_by_band() {
    // A 250 Hz ring and a resonant noise band at 4 kHz, like a drum's head and a snare's wires.
    let n = RATE as usize;
    let env = envelope(n, 0.25);
    let hiss = resonant_noise(n, 4000.0, 4.0, 11);
    let x: Vec<f64> = (0..n)
        .map(|i| env(i) * ((TAU * 250.0 * i as f64 / RATE).sin() + 0.3 * hiss[i] / 0.05))
        .collect();
    let r = describe(x);
    let low = share(&r, 200.0, 20.0);
    let high = share(&r, 3200.0, 20.0);
    assert!(low > 80.0, "the ring's band: {low} % tonal");
    assert!(high < 30.0, "the noise band: {high} % tonal");
    let table = r
        .sections
        .iter()
        .flat_map(|s| &s.tables)
        .find(|t| t.id == "tonality.peaks")
        .unwrap();
    let ring = table
        .rows
        .iter()
        .find(|row| row[0].is_some_and(|hz| (hz - 250.0).abs() < 5.0))
        .expect("the ring's peak");
    assert_eq!(ring[4], Some(1.0), "the ring is a line: {ring:?}");
}
