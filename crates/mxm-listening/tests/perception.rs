//! The perceptual models against their standard's own calibration points (ECMA-418-2): a 1 kHz tone
//! at 40 dB SPL is 1 sone_HMS and 1 tu_HMS; noise has no tonality; loudness grows with level.

use mxm_listening::Sound;
use mxm_listening::perception::{hearing, sottek};

const RATE: f64 = 48_000.0;

/// A sine at `db` dB SPL under the owner's level (a full-scale sine is 94 dB SPL), `seconds` long.
fn tone(hz: f64, db: f64, seconds: f64) -> Sound {
    let amplitude = 10f64.powf((db - 94.0) / 20.0);
    let x = (0..(seconds * RATE) as usize)
        .map(|i| (amplitude * (std::f64::consts::TAU * hz * i as f64 / RATE).sin()) as f32)
        .collect();
    Sound::new("tone", RATE as u32, x)
}

fn run(s: &Sound) -> sottek::Sottek {
    sottek::analyse(&hearing::pressure(s)).expect("an analysis")
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_1khz_tone_at_40_db_is_one_sone_and_one_tonality_unit() {
    let r = run(&tone(1000.0, 40.0, 2.0));
    let n = r.single_loudness().unwrap();
    let t = r.single_tonality().unwrap();
    assert!((n - 1.0).abs() < 0.02, "loudness {n} sone_HMS");
    assert!((t - 1.0).abs() < 0.02, "tonality {t} tu_HMS");
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn noise_has_little_tonality_and_loudness_grows_with_level() {
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let noise: Vec<f32> = (0..(2.0 * RATE) as usize)
        .map(|_| {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            (((state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 52) as f64 - 1.0)
                * 0.05) as f32
        })
        .collect();
    let r = run(&Sound::new("noise", RATE as u32, noise));
    let t = r.single_tonality().unwrap();
    assert!(t < 0.1, "noise tonality {t}");
    let quiet = run(&tone(1000.0, 50.0, 1.5)).single_loudness().unwrap();
    let loud = run(&tone(1000.0, 70.0, 1.5)).single_loudness().unwrap();
    // One band's specific loudness grows about ×1.4 per 10 dB there (the nonlinearity's Table 2), and
    // a louder tone reaches more bands: the whole grows ×1.4–2 per 10 dB, around the classical
    // doubling. Measured: 1.72 → 4.50 sone_HMS.
    assert!(loud / quiet > 2.0 && loud / quiet < 4.0, "{quiet} → {loud}");
}

/// An amplitude-modulated sine: carrier `hz` at `db` dB SPL, depth `m` at `rate` Hz.
fn am(hz: f64, db: f64, m: f64, rate: f64, seconds: f64) -> Sound {
    let amplitude = 10f64.powf((db - 94.0) / 20.0);
    let x = (0..(seconds * RATE) as usize)
        .map(|i| {
            let t = i as f64 / RATE;
            (amplitude
                * (1.0 + m * (std::f64::consts::TAU * rate * t).sin())
                * (std::f64::consts::TAU * hz * t).sin()) as f32
        })
        .collect();
    Sound::new("am", RATE as u32, x)
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_1khz_tone_fully_modulated_at_70_hz_and_60_db_is_one_asper() {
    use mxm_listening::perception::roughness;
    let r = roughness::analyse(&hearing::pressure(&am(1000.0, 60.0, 1.0, 70.0, 2.0))).unwrap();
    let v = r.single().unwrap();
    // This implementation reads 1.04 asper here, where the standard lets an implementation adjust its
    // calibration by 0.25 % only: a 4 % deviation not yet found, recorded rather than calibrated away,
    // and a quarter of roughness's 17 % threshold.
    assert!((v - 1.0).abs() < 0.05, "roughness {v} asper");
    // An unmodulated tone is not rough.
    let steady = roughness::analyse(&hearing::pressure(&tone(1000.0, 60.0, 2.0))).unwrap();
    assert!(
        steady.single().unwrap() < 0.05,
        "steady {}",
        steady.single().unwrap()
    );
}

/// Band noise from `lo` to `hi` Hz at `db` dB SPL, 3 s.
fn band_noise(lo: f64, hi: f64, db: f64) -> Sound {
    let mut st = 0x2545_F491_4F6C_DD1Du64;
    let white: Vec<f64> = (0..(3.0 * RATE) as usize)
        .map(|_| {
            st ^= st >> 12;
            st ^= st << 25;
            st ^= st >> 27;
            (st.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 52) as f64 - 1.0
        })
        .collect();
    let nb = mxm_listening::repr::bands::band(&white, RATE, lo, hi).unwrap();
    let rms = (nb.iter().map(|v| v * v).sum::<f64>() / nb.len() as f64).sqrt();
    let target = 10f64.powf((db - 94.0) / 20.0) / std::f64::consts::SQRT_2;
    Sound::new(
        "noise",
        RATE as u32,
        nb.iter().map(|v| (v * target / rms) as f32).collect(),
    )
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn the_acum_reference_reads_one_acum_and_a_higher_band_reads_sharper() {
    use mxm_listening::perception;
    let mean_sharpness = |s: &Sound| {
        let a = sottek::analyse(&hearing::pressure(s)).unwrap();
        let v: Vec<f64> = a
            .basis
            .iter()
            .skip(sottek::SETTLE)
            .filter_map(perception::sharpness)
            .collect();
        v.iter().sum::<f64>() / v.len() as f64
    };
    let reference = mean_sharpness(&band_noise(950.0, 1050.0, 60.0));
    assert!((reference - 1.0).abs() < 0.02, "reference {reference} acum");
    let high = mean_sharpness(&band_noise(3800.0, 4200.0, 60.0));
    assert!(high > 1.8, "4 kHz band {high} acum");
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_gain_alone_changes_no_perceptual_reading() {
    use mxm_listening::describe::describe;
    // A drum: a ring and a noise burst, 1 s; then the same 6 dB down.
    let mut st = 0x9E37_79B9_7F4A_7C15u64;
    let x: Vec<f32> = (0..RATE as usize)
        .map(|i| {
            let t = i as f64 / RATE;
            st ^= st >> 12;
            st ^= st << 25;
            st ^= st >> 27;
            let white =
                (st.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 52) as f64 - 1.0;
            (0.5 * (-t / 0.2).exp() * (std::f64::consts::TAU * 190.0 * t).sin()
                + 0.2 * (-t / 0.03).exp() * white) as f32
        })
        .collect();
    let quiet: Vec<f32> = x.iter().map(|v| v * 0.5).collect();
    let (a, b) = (
        describe(
            &Sound::new("drum", RATE as u32, x),
            Some(mxm_listening::Family::Percussive),
        ),
        describe(
            &Sound::new("drum", RATE as u32, quiet),
            Some(mxm_listening::Family::Percussive),
        ),
    );
    let section = |r: &mxm_listening::Report| {
        r.sections
            .iter()
            .find(|s| s.part == "perception")
            .expect("the perception section")
            .readings
            .clone()
    };
    for (ra, rb) in section(&a).iter().zip(section(&b)) {
        match (ra.value, rb.value) {
            (Some(u), Some(v)) => assert!(
                (u - v).abs() <= 1e-3 * u.abs().max(1e-3),
                "{} {:?}: {u} against {v}",
                ra.id,
                ra.window_ms
            ),
            (u, v) => assert_eq!(u.is_some(), v.is_some(), "{}", ra.id),
        }
    }
}

#[test]
#[ignore = "release gate (`scripts/merge_gate.py --full`): over 30 s in the merge gate"]
fn a_1khz_tone_fully_modulated_at_4_hz_and_60_db_is_one_vacil() {
    use mxm_listening::perception::fluctuation;
    let f = fluctuation::analyse(&hearing::pressure(&am(1000.0, 60.0, 1.0, 4.0, 6.0))).unwrap();
    let v = f.single().unwrap();
    // This implementation reads 1.025 vacil here (the standard lets an implementation adjust its
    // calibration by 0.25 % only): recorded rather than calibrated away.
    assert!((v - 1.0).abs() < 0.05, "fluctuation strength {v} vacil");
    // A band-pass in modulation rate, greatest near 4 Hz, and less for a shallower modulation.
    let at = |rate: f64, depth: f64| {
        fluctuation::analyse(&hearing::pressure(&am(1000.0, 60.0, depth, rate, 6.0)))
            .and_then(|f| f.single())
            .unwrap()
    };
    let (slow, fast, shallow) = (at(1.0, 1.0), at(16.0, 1.0), at(4.0, 0.5));
    assert!(
        slow < 0.8 * v && fast < 0.8 * v,
        "1 Hz {slow}, 4 Hz {v}, 16 Hz {fast}"
    );
    assert!(shallow < 0.8 * v, "half the depth {shallow}");
    // An unmodulated tone does not fluctuate.
    let steady = fluctuation::analyse(&hearing::pressure(&tone(1000.0, 60.0, 6.0))).unwrap();
    assert!(
        steady.single().unwrap() < 0.05,
        "steady {}",
        steady.single().unwrap()
    );
}
