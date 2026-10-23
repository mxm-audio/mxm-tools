//! The screen (`screen`) on synthetic hits whose differences are known by construction: a gain is no
//! difference, a burst the reference lacks is a patch, a tail cut short stops short, the reference's
//! hiss left out is no fault, the hiss read from a recording's end is its floor and not a ring still
//! falling, and the takes' own difference is the yardstick.

use mxm_listening::Sound;
use mxm_listening::screen::{
    BANDS, HISS_S, NoiseCeiling, OFF_DB, PATCH_OFF, Patch, SHARE_OFF, Screen, TAIL_OFF_DB, band_hz,
    hiss, recording_noise, screen, verdict, window_ms,
};

const RATE: u32 = 48_000;

/// A deterministic noise in −1..1 (a linear congruential generator; any white-ish noise will do).
fn noise(seed: u64, n: usize) -> Vec<f64> {
    let mut x = seed;
    (0..n)
        .map(|_| {
            x = x
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (x >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
        })
        .collect()
}

/// A decaying amplitude reaching −60 dB after `t60` seconds.
fn decay(t: f64, t60: f64) -> f64 {
    10f64.powf(-3.0 * t / t60)
}

/// A drum-like hit: 10 ms of silence, then a 60 Hz body, a 400 Hz ring and a noise strike with a
/// quieter noise tail, `seconds` long; `shape` multiplies each sample at `t` seconds after the onset.
fn hit(seconds: f64, shape: impl Fn(f64) -> f64) -> Vec<f64> {
    let n = (seconds * f64::from(RATE)) as usize;
    let lead = RATE as usize / 100;
    let hiss = noise(7, n);
    (0..n)
        .map(|i| {
            if i < lead {
                return 0.0;
            }
            let t = (i - lead) as f64 / f64::from(RATE);
            let tau = std::f64::consts::TAU;
            let body = 0.5 * (tau * 60.0 * t).sin() * decay(t, 0.6);
            let ring = 0.2 * (tau * 400.0 * t).sin() * decay(t, 1.0);
            let strike = 0.4 * hiss[i] * decay(t, 0.08);
            let tail = 0.03 * hiss[i] * decay(t, 0.8);
            (body + ring + strike + tail) * shape(t)
        })
        .collect()
}

fn sound(name: &str, x: &[f64]) -> Sound {
    Sound::new(name, RATE, x.iter().map(|&v| v as f32).collect())
}

#[test]
fn the_bands_and_their_windows() {
    // Third octaves from 20 Hz: each 2^(1/3) wide, the last centred at 20·2^(29/3) Hz.
    let (lo, hi) = band_hz(0);
    assert!((hi / lo - 2f64.powf(1.0 / 3.0)).abs() < 1e-12);
    assert!(
        (band_hz(BANDS - 1).1 / 2f64.powf(1.0 / 6.0) - 20.0 * 2f64.powf(29.0 / 3.0)).abs() < 1e-9
    );
    // Four periods of a band's lowest frequency: 160 ms at the bottom, 5 ms from 800 Hz up.
    assert_eq!(window_ms(band_hz(0).0), 160.0);
    assert_eq!(window_ms(800.0), 5.0);
    assert_eq!(window_ms(100.0), 40.0);
}

#[test]
fn a_gain_is_no_difference() {
    let r = hit(1.5, |_| 1.0);
    let c: Vec<f64> = r.iter().map(|v| v * 0.3).collect();
    let s = screen(
        &sound("reference", &r),
        &sound("candidate", &c),
        NoiseCeiling::default(),
    )
    .expect("screened");
    // Matched in body loudness, the two are the same sound to the last cell.
    assert!(s.off_share < 1e-9, "{s:?}");
    assert!(s.patch.is_none(), "{s:?}");
    assert!(s.tail_db.is_some_and(|t| t.abs() < 1e-6), "{s:?}");
    assert!(verdict(&s, None).is_empty());
}

#[test]
fn a_burst_the_reference_lacks_is_a_patch() {
    let r = hit(1.5, |_| 1.0);
    let lead = RATE as usize / 100;
    let mut c = r.clone();
    // A 2 kHz tone from 50 to 250 ms, as loud as the body: nothing in the reference stands there.
    for (i, v) in c.iter_mut().enumerate().skip(lead) {
        let t = (i - lead) as f64 / f64::from(RATE);
        if (0.05..0.25).contains(&t) {
            let w = (std::f64::consts::PI * (t - 0.05) / 0.2).sin().powi(2);
            *v += 0.5 * w * (std::f64::consts::TAU * 2000.0 * t).sin();
        }
    }
    let s = screen(
        &sound("reference", &r),
        &sound("candidate", &c),
        NoiseCeiling::default(),
    )
    .expect("screened");
    let p: &Patch = s.patch.as_ref().expect("a patch");
    assert!(p.too_loud && p.mean_db > OFF_DB, "{p:?}");
    // It sits at 2 kHz, within the burst's span (each cell reads 20 ms around it, its window
    // 5 ms more): its edges bound 2 kHz and overlap 50-250 ms.
    assert!(p.band_hz.0 < 2000.0 && p.band_hz.1 > 2000.0, "{p:?}");
    assert!(p.window_ms.0 < 250.0 && p.window_ms.1 > 50.0, "{p:?}");
    assert!(p.loudness > PATCH_OFF, "{p:?}");
    let why = verdict(&s, None);
    assert!(why.iter().any(|w| w.starts_with("too loud")), "{why:?}");
}

#[test]
fn a_tail_cut_short_stops_short() {
    let r = hit(1.5, |_| 1.0);
    // The same hit, falling a further 60 dB every 100 ms from 150 ms on.
    let c = hit(1.5, |t| if t < 0.15 { 1.0 } else { decay(t - 0.15, 0.1) });
    let s = screen(
        &sound("reference", &r),
        &sound("candidate", &c),
        NoiseCeiling::default(),
    )
    .expect("screened");
    let tail = s.tail_db.expect("a tail");
    assert!(tail < -TAIL_OFF_DB, "{s:?}");
    let why = verdict(&s, None);
    assert!(why.iter().any(|w| w.contains("stops short")), "{why:?}");
}

#[test]
fn the_references_hiss_left_out_is_no_fault() {
    let n = (1.5 * f64::from(RATE)) as usize;
    let floor = noise(99, n);
    // The reference carries a recording's hiss 66 dB under its peak; the candidate is clean.
    let c = hit(1.5, |_| 1.0);
    let r: Vec<f64> = c.iter().zip(&floor).map(|(v, h)| v + 5e-4 * h).collect();
    let s = screen(
        &sound("reference", &r),
        &sound("candidate", &c),
        NoiseCeiling::default(),
    )
    .expect("screened");
    assert!(s.hiss_from_end, "{s:?}");
    assert!(s.off_share < SHARE_OFF / 3.0, "{s:?}");
    assert!(
        s.patch
            .as_ref()
            .is_none_or(|p| p.loudness < PATCH_OFF / 3.0),
        "{s:?}"
    );
}

#[test]
fn the_takes_are_the_yardstick() {
    let screened = |off_share: f64, tail_db: f64| Screen {
        off_share,
        patch: None,
        tail_db: Some(tail_db),
        length_ms: 1000.0,
        hiss_from_end: true,
        off_cells: Vec::new(),
    };
    // 20 % off is off alone, and not where two takes differ by 10 % (under twice theirs and 5 %).
    assert!(!verdict(&screened(0.20, 0.0), None).is_empty());
    assert!(verdict(&screened(0.20, 0.0), Some(&screened(0.10, 0.0))).is_empty());
    // A tail 10 dB short is off alone, and not where two takes differ by 3 dB.
    assert!(!verdict(&screened(0.0, -10.0), None).is_empty());
    assert!(verdict(&screened(0.0, -10.0), Some(&screened(0.0, 3.0))).is_empty());
    // Under the share threshold nothing counts, whatever the takes.
    assert!(verdict(&screened(SHARE_OFF - 0.01, 0.0), Some(&screened(0.0, 0.0))).is_empty());
}

#[test]
fn the_hiss_read_from_the_end_is_the_floor() {
    let rate = f64::from(RATE);
    let n = (2.0 * rate) as usize;
    let level = 1e-3;
    // A hit whose ring is gone by its last half second over a white floor: uniform in ±level, its
    // power level²/3 spread evenly to rate/2.
    let x: Vec<f64> = hit(2.0, |t| decay(t, 0.3).max(0.0))
        .iter()
        .zip(noise(3, n))
        .map(|(v, h)| v + level * h)
        .collect();
    let x = &x[RATE as usize / 100..];
    let floor = hiss(x, rate).expect("a stretch after the strike");
    for k in [12, 18, 24] {
        let (lo, hi) = band_hz(k);
        let want = level * level / 3.0 * (hi - lo) / (rate / 2.0);
        let got = 10.0 * (floor[k] / want).log10();
        // Within 1.5 dB: five 100 ms frames of a band at least 50 Hz wide scatter by under 1 dB, and
        // the zero-phase Butterworth's skirts let in about what its −6 dB edges lose.
        assert!(
            got.abs() < 1.5,
            "band {k} ({lo:.0}-{hi:.0} Hz): {got:+.1} dB"
        );
    }
}

#[test]
fn a_ring_still_falling_at_the_end_is_not_the_floor() {
    let rate = f64::from(RATE);
    let n = (2.0 * rate) as usize;
    // A 1 kHz ring falling 30 dB a second to the file's end, over a floor 80 dB under it.
    let x: Vec<f64> = (0..n)
        .map(|i| {
            let t = i as f64 / rate;
            0.5 * (std::f64::consts::TAU * 1000.0 * t).sin() * decay(t, 2.0)
        })
        .zip(noise(5, n))
        .map(|(v, h)| v + 5e-5 * h)
        .collect();
    let floor = hiss(&x, rate).expect("a stretch after the strike");
    let k = (0..BANDS)
        .find(|&k| band_hz(k).0 < 1000.0 && band_hz(k).1 > 1000.0)
        .unwrap();
    let last = &x[n - ((HISS_S + 0.01) * rate) as usize..n - (0.01 * rate) as usize];
    let stretch = last.iter().map(|v| v * v).sum::<f64>() / last.len() as f64;
    // Falling 30 dB a second, less twice its frames' scatter, the floor lies about 12 dB under the
    // stretch's mean: at least 10.
    assert!(
        10.0 * (floor[k] / stretch).log10() < -10.0,
        "{:.1}",
        10.0 * (floor[k] / stretch).log10()
    );
}

#[test]
fn a_noise_faded_under_the_dither_is_read_where_it_holds_still() {
    let rate = f64::from(RATE);
    let n = (3.0 * rate) as usize;
    // A hit over a noise 40 dB over a dither, the noise fading out over a second as a take's export
    // fades, gone 0.7 s before the end: its end holds the dither alone.
    let (noise_level, dither) = (1e-3, 1e-5);
    let fade = |i: usize| ((n as f64 - 0.7 * rate - i as f64) / rate).clamp(0.0, 1.0);
    let x: Vec<f64> = hit(3.0, |t| decay(t, 0.3))
        .iter()
        .zip(noise(11, n).iter().zip(noise(12, n)))
        .enumerate()
        .map(|(i, (v, (a, b)))| v + noise_level * a * fade(i) + dither * b)
        .collect();
    let x = &x[RATE as usize / 100..];
    let k = 21;
    let (lo, hi) = band_hz(k);
    let want = noise_level * noise_level / 3.0 * (hi - lo) / (rate / 2.0);
    // Read from the end, the floor is the dither's, 40 dB under the noise.
    let end = hiss(x, rate).expect("a stretch after the strike")[k];
    assert!(
        10.0 * (end / want).log10() < -30.0,
        "{:.1}",
        10.0 * (end / want).log10()
    );
    // Read where the band holds still under a ceiling over the noise, it is the noise.
    let still = recording_noise(x, rate, 10.0 * want.log10() + 10.0)[k].expect("a still stretch");
    assert!(
        (10.0 * (still / want).log10()).abs() < 1.5,
        "{:.1}",
        10.0 * (still / want).log10()
    );
    // Under a ceiling beneath it, the noise is taken for a ring holding still and passed over: what
    // holds still under that ceiling is the dither at the end, 40 dB under the noise.
    let under = recording_noise(x, rate, 10.0 * want.log10() - 10.0)[k].expect("the dither");
    assert!(
        10.0 * (under / want).log10() < -30.0,
        "{:.1}",
        10.0 * (under / want).log10()
    );
}
