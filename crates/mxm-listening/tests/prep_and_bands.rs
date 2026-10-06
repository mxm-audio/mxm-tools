//! Preparation and the band filter against closed forms.

use mxm_listening::prep::{body_rms, k_weighting, onset, trim};
use mxm_listening::repr::bands::{butterworth_band_pass, filter_zero_phase, response};

#[test]
fn k_weighting_matches_the_standard_at_48_khz() {
    // ITU-R BS.1770-4, Table 1 (the pre-filter) and Table 2 (the RLB high-pass), 48 kHz.
    let [(b1, a1), (b2, a2)] = k_weighting(48_000);
    let want_b1 = [1.53512485958697, -2.69169618940638, 1.19839281085285];
    let want_a1 = [-1.69065929318241, 0.73248077421585];
    let want_a2 = [-1.99004745483398, 0.99007225036621];
    for (g, w) in b1.iter().zip(want_b1) {
        assert!((g - w).abs() < 1e-12, "{g} vs {w}");
    }
    for (g, w) in a1.iter().zip(want_a1) {
        assert!((g - w).abs() < 1e-12);
    }
    assert_eq!(b2, [1.0, -2.0, 1.0]);
    for (g, w) in a2.iter().zip(want_a2) {
        assert!((g - w).abs() < 1e-12);
    }
}

#[test]
fn body_loudness_of_a_steady_tone_is_its_weighted_rms() {
    // A 1 kHz sine's body loudness is its RMS times the K-weighting's gain at 1 kHz.
    for rate in mxm_measure::RATES {
        let rate_u = rate as u32;
        let x: Vec<f32> = (0..rate_u)
            .map(|i| (0.5 * (std::f64::consts::TAU * 1000.0 * i as f64 / rate).sin()) as f32)
            .collect();
        let omega = std::f64::consts::TAU * 1000.0 / rate;
        let gain: f64 = k_weighting(rate_u)
            .iter()
            .map(|(b, a)| {
                let s = mxm_listening::repr::bands::Section { b: *b, a: *a };
                response(&[s], omega).abs()
            })
            .product();
        let want = 0.5 / 2f64.sqrt() * gain;
        let got = f64::from(body_rms(&x, rate_u));
        assert!((got / want - 1.0).abs() < 0.005, "{rate}: {got} vs {want}");
    }
}

#[test]
fn onset_and_trim_follow_the_page() {
    let rate = 48_000;
    let mut x = vec![0.0f32; 4800];
    x.extend((0..48_000).map(|i| (-(i as f32) / 4800.0).exp() * 0.8));
    assert_eq!(onset(&x), Some(4800));
    let t = trim(&x, rate);
    // 5 ms (240 samples) before the onset survive; the first sample is faded, so it starts at zero.
    assert_eq!(t[..240].iter().filter(|s| **s != 0.0).count(), 0);
    assert!(t[240] > 0.0);
    assert_eq!(onset(&[0.0; 16]), None);
}

/// The load that keeps its facts reads what `Sound::load` reads, sample for sample, and says what it
/// kept: a stereo file's two channels folded to one, a file longer than the limit cut there. A file
/// it refuses gets the decoder's sentence, and no part of the path travels with it.
#[test]
fn a_load_keeps_what_it_read_and_never_the_path() {
    use mxm_audio_file::{Target, write};
    use mxm_listening::Sound;
    use mxm_listening::prep::{Codec, Container, HIT_MAX_FRAMES};

    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("prep-kept");
    let rate = 48_000;
    let tone =
        |i: usize, hz: f64| (0.5 * (std::f64::consts::TAU * hz * i as f64 / 48_000.0).sin()) as f32;

    // Stereo: the left at 440 Hz, the right at 660 Hz; the mono is their average.
    let frames = 4_800;
    let stereo: Vec<f32> = (0..frames)
        .flat_map(|i| [tone(i, 440.0), tone(i, 660.0)])
        .collect();
    let path = dir.join("stereo.wav");
    write(&path, &stereo, 2, rate, Target::WavFloat32).unwrap();
    let plain = Sound::load(&path, "stereo").unwrap();
    let (sound, kept) = Sound::load_kept(&path, "stereo").unwrap();
    assert_eq!(sound, plain);
    assert_eq!(
        (kept.source_channels, kept.channels, kept.cut),
        (2, 2, false)
    );
    assert_eq!((kept.codec, kept.container), (Codec::Pcm, Container::Wav));

    // Longer than the limit: kept up to it, and said to be cut.
    let long: Vec<f32> = (0..HIT_MAX_FRAMES + 480).map(|i| tone(i, 220.0)).collect();
    let path = dir.join("long.wav");
    write(&path, &long, 1, rate, Target::WavFloat32).unwrap();
    let (sound, kept) = Sound::load_kept(&path, "long").unwrap();
    assert_eq!(sound.samples.len(), HIT_MAX_FRAMES);
    assert!(kept.cut);
    assert_eq!(Sound::load(&path, "long").unwrap(), sound);

    // Not audio, in a folder whose name must not travel.
    let path = dir.join("private-folder-name").join("not-audio.wav");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"this is text, not a sound").unwrap();
    let error = Sound::load_kept(&path, "not-audio").unwrap_err();
    assert!(!error.contains("private-folder-name"), "{error}");
    assert!(!error.is_empty());
}

#[test]
fn band_pass_has_unit_centre_gain_and_half_power_edges() {
    for rate in mxm_measure::RATES {
        for (lo, hi) in [(50.0, 100.0), (800.0, 1600.0), (6400.0, 12_800.0)] {
            let s = butterworth_band_pass(4, lo, hi, rate).unwrap();
            let at = |hz: f64| response(&s, std::f64::consts::TAU * hz / rate).abs();
            // The centre after prewarping sits between the edges; the edges are −3 dB for one pass.
            let centre = {
                let w1 = (std::f64::consts::PI * lo / rate).tan();
                let w2 = (std::f64::consts::PI * hi / rate).tan();
                (w1 * w2).sqrt().atan() * rate / std::f64::consts::PI
            };
            assert!(
                (at(centre) - 1.0).abs() < 1e-9,
                "{rate} {lo}-{hi} centre {}",
                at(centre)
            );
            assert!(
                (at(lo) - 0.5f64.sqrt()).abs() < 1e-6,
                "{rate} {lo} edge {}",
                at(lo)
            );
            assert!(
                (at(hi) - 0.5f64.sqrt()).abs() < 1e-6,
                "{rate} {hi} edge {}",
                at(hi)
            );
            // An octave outside the band, a fourth-order band-pass is well down.
            assert!(at(lo / 2.0) < 0.1 && at((hi * 2.0).min(0.49 * rate)) < 0.5);
        }
    }
    assert!(butterworth_band_pass(4, 100.0, 50.0, 48_000.0).is_none());
    assert!(butterworth_band_pass(4, 20_000.0, 30_000.0, 48_000.0).is_none());
}

#[test]
fn zero_phase_filtering_keeps_an_in_band_sine_in_place() {
    let rate = 48_000.0;
    let s = butterworth_band_pass(4, 800.0, 1600.0, rate).unwrap();
    let hz = (800.0f64 * 1600.0).sqrt();
    let x: Vec<f64> = (0..48_000)
        .map(|i| (std::f64::consts::TAU * hz * i as f64 / rate).sin())
        .collect();
    let y = filter_zero_phase(&s, &x);
    // Away from the ends: same amplitude, no delay (the prewarped centre is a hair off `hz`, so a
    // few thousandths of amplitude).
    for i in 10_000..38_000 {
        assert!(
            (y[i] - x[i]).abs() < 0.01,
            "sample {i}: {} vs {}",
            y[i],
            x[i]
        );
    }
}
