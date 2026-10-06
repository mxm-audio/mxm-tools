//! The curves a window draws, on sounds whose answers are known by construction: a damped sine's
//! envelope falls at its closed-form rate, its spectrum and its spectrogram peak at its frequency, and
//! silence or a non-finite sample gives no curve, as it gives no reading.

use mxm_listening::Sound;
use mxm_listening::curves::{Curves, curves};
use mxm_listening::repr::envelope::line_fit;

const TAU: f64 = std::f64::consts::TAU;

/// 5 ms of silence, then `0.8·e^{−t/τ}·sin(2πft)`.
fn damped(rate: f64, hz: f64, tau: f64, seconds: f64) -> Sound {
    let pre = (0.005 * rate) as usize;
    let mut x = vec![0.0f32; pre];
    x.extend((0..(seconds * rate) as usize).map(|i| {
        let t = i as f64 / rate;
        (0.8 * (-t / tau).exp() * (TAU * hz * t).sin()) as f32
    }));
    Sound::new("damped", rate as u32, x)
}

fn finite_or_absent(c: &Curves) {
    for curve in [&c.envelope, &c.spectrum].into_iter().flatten() {
        assert!(curve.points.iter().all(|p| p.0.is_finite()));
        assert!(curve.points.iter().filter_map(|p| p.1).all(f64::is_finite));
    }
    if let Some(s) = &c.spectrogram {
        assert!(s.cells_db.iter().flatten().flatten().all(|v| v.is_finite()));
    }
}

#[test]
fn a_damped_sine_draws_its_decay_and_its_frequency() {
    for rate in [44_100.0, 48_000.0, 96_000.0] {
        let (hz, tau) = (440.0, 0.1);
        let c = curves(&damped(rate, hz, tau, 1.0), None);
        finite_or_absent(&c);

        // The envelope: loudest at the start, falling 20·log10(e)/τ dB a second.
        let env = c.envelope.expect("an envelope");
        assert_eq!(env.id, "curve.envelope");
        let top = env.points[0].1.expect("the start is heard");
        assert!(top > -1.0, "{rate}: starts at {top} dB");
        let points: Vec<(f64, f64)> = env
            .points
            .iter()
            .filter(|p| (50.0..300.0).contains(&p.0))
            .filter_map(|p| p.1.map(|v| (p.0 / 1000.0, v)))
            .collect();
        let (slope, _) = line_fit(&points).unwrap();
        let want = -20.0 * std::f64::consts::E.log10() / tau;
        // A one-period RMS window follows an exponential exactly up to its ripple: 2 %.
        assert!(
            (slope / want - 1.0).abs() < 0.02,
            "{rate}: {slope} vs {want} dB/s"
        );

        // The spectrum: its loudest band holds the sine.
        let spectrum = c.spectrum.expect("a spectrum");
        let (peak_hz, _) = spectrum
            .points
            .iter()
            .filter_map(|p| p.1.map(|v| (p.0, v)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        // Within a band's half-width: a 24th of an octave is 50 c, half of it 25.
        assert!(
            (1200.0 * (peak_hz / hz).log2()).abs() <= 25.0,
            "{rate}: the spectrum peaks at {peak_hz}"
        );

        // The spectrogram: early frames peak in the band holding the sine; times rise from the onset.
        let s = c.spectrogram.expect("a spectrogram");
        assert!(s.times_ms.windows(2).all(|w| w[1] > w[0]));
        // The first frame is centred half a frame after the onset.
        assert!((s.times_ms[0] - s.resolution.window_ms / 2.0).abs() < 1e-9);
        let band = s
            .bands_hz
            .iter()
            .position(|&(lo, hi)| lo <= hz && hz < hi)
            .unwrap();
        for frame in &s.cells_db[..10] {
            let loudest = (0..frame.len())
                .max_by(|&a, &b| {
                    frame[a]
                        .unwrap_or(f64::MIN)
                        .total_cmp(&frame[b].unwrap_or(f64::MIN))
                })
                .unwrap();
            assert_eq!(loudest, band, "{rate}");
        }
    }
}

#[test]
fn the_curves_start_where_the_report_starts() {
    let rate = 48_000.0;
    let sound = damped(rate, 220.0, 0.2, 0.6);
    let own = curves(&sound, None);
    // A caller's onset 100 ms later: the envelope, one point a millisecond, holds 100 fewer, and the
    // spectrogram fewer frames. Each is against its own loudest moment, so neither starts lower.
    let later = curves(&sound, Some(0.105));
    let points = |c: &Curves| c.envelope.as_ref().unwrap().points.len();
    assert_eq!(points(&own) - points(&later), 100);
    let frames = |c: &Curves| c.spectrogram.as_ref().unwrap().times_ms.len();
    assert!(frames(&later) < frames(&own));
    // An onset past the end is ignored.
    assert_eq!(curves(&sound, Some(60.0)), own);
}

#[test]
fn silence_and_a_non_finite_sample_draw_nothing() {
    assert_eq!(
        curves(&Sound::new("silent", 48_000, vec![0.0; 48_000]), None),
        Curves::default()
    );
    let mut broken = damped(48_000.0, 440.0, 0.1, 0.5).samples;
    broken[1000] = f32::NAN;
    assert_eq!(
        curves(&Sound::new("broken", 48_000, broken), None),
        Curves::default()
    );
}
