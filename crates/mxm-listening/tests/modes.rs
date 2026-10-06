//! The mode analysis against sums of damped sinusoids whose modes are known by construction
//! (`research:listening/modal-estimation.md` §10's checks, at the collection's rates).

use mxm_listening::repr::modes::{Mode, analyse_band};

const TAU: f64 = std::f64::consts::TAU;

/// `Σ A·e^{−decay·t}·cos(2π f t + φ)` for `seconds` at `rate`, plus white noise at `snr_db` below the
/// signal's mean power (none for `None`), from a fixed seed.
fn modes(rate: f64, seconds: f64, spec: &[(f64, f64, f64, f64)], snr_db: Option<f64>) -> Vec<f64> {
    let n = (seconds * rate) as usize;
    let mut x: Vec<f64> = (0..n)
        .map(|i| {
            let t = i as f64 / rate;
            spec.iter()
                .map(|&(f, d, a, p)| a * (-d * t).exp() * (TAU * f * t + p).cos())
                .sum()
        })
        .collect();
    if let Some(snr) = snr_db {
        let power = x.iter().map(|v| v * v).sum::<f64>() / n as f64;
        let sigma = (power / 10f64.powf(snr / 10.0)).sqrt();
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut gauss = || {
            // Box–Muller from xorshift64*.
            let mut uniform = || {
                state ^= state >> 12;
                state ^= state << 25;
                state ^= state >> 27;
                ((state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 + 0.5)
                    / (1u64 << 53) as f64
            };
            let (a, b) = (uniform(), uniform());
            (-2.0 * a.ln()).sqrt() * (TAU * b).cos()
        };
        for v in &mut x {
            *v += sigma * gauss();
        }
    }
    x
}

fn nearest(found: &[Mode], hz: f64) -> Mode {
    *found
        .iter()
        .min_by(|a, b| (a.hz - hz).abs().total_cmp(&(b.hz - hz).abs()))
        .expect("a mode")
}

#[test]
fn one_mode_is_recovered_exactly() {
    for rate in mxm_measure::RATES {
        let x = modes(rate, 1.0, &[(440.0, 9.21, 0.5, 0.4)], None);
        let found = analyse_band(&x, rate, 0, (0.3 * rate) as usize, 300.0, 600.0).unwrap();
        let m = nearest(&found.modes, 440.0);
        assert!((m.hz - 440.0).abs() < 0.01, "{rate}: {} Hz", m.hz);
        assert!((m.decay - 9.21).abs() < 0.05, "{rate}: decay {}", m.decay);
        assert!(
            (m.amplitude - 0.5).abs() < 0.005,
            "{rate}: amplitude {}",
            m.amplitude
        );
        assert!((m.phase - 0.4).abs() < 0.01, "{rate}: phase {}", m.phase);
        assert!((m.t60() - 0.75).abs() < 0.01);
    }
}

#[test]
fn two_modes_three_hertz_apart_are_resolved() {
    // The research page's check A: 200 Hz (20/s, 1.0) and 203 Hz (25/s, 0.7) in 100 ms, where a
    // spectrogram of the whole 100 ms shows one peak. Noise-free, then at 60 dB. The window holds its
    // own filters (`FILTER_SHARE`), so less than all of it is analysed.
    let rate = 48_000.0;
    let spec = [(200.0, 20.0, 1.0, 0.3), (203.0, 25.0, 0.7, -1.1)];
    for (snr, hz_tol, decay_tol) in [(None, 0.01, 0.1), (Some(60.0), 0.1, 1.0)] {
        let x = modes(rate, 0.5, &spec, snr);
        let found = analyse_band(&x, rate, 0, (0.1 * rate) as usize, 141.0, 283.0).unwrap();
        let a = nearest(&found.modes, 200.0);
        let b = nearest(&found.modes, 203.0);
        assert!(
            (a.hz - 200.0).abs() < hz_tol && (b.hz - 203.0).abs() < hz_tol,
            "{snr:?}: {} {}",
            a.hz,
            b.hz
        );
        assert!(
            (a.decay - 20.0).abs() < decay_tol && (b.decay - 25.0).abs() < decay_tol,
            "{snr:?}: {} {}",
            a.decay,
            b.decay
        );
        assert!(
            (a.amplitude - 1.0).abs() < 0.05 && (b.amplitude - 0.7).abs() < 0.05,
            "{snr:?}: {} {}",
            a.amplitude,
            b.amplitude
        );
    }
}

#[test]
fn a_window_starting_late_reads_the_amplitude_there() {
    // The amplitude is the mode's at the window's start: 0.8·e^{−10·0.2} at 200 ms.
    let rate = 48_000.0;
    let x = modes(rate, 1.0, &[(330.0, 10.0, 0.8, 0.0)], None);
    let found = analyse_band(
        &x,
        rate,
        (0.2 * rate) as usize,
        (0.3 * rate) as usize,
        250.0,
        450.0,
    )
    .unwrap();
    let m = nearest(&found.modes, 330.0);
    let want = 0.8 * (-2.0f64).exp();
    assert!(
        (m.amplitude - want).abs() < 0.002,
        "{} vs {want}",
        m.amplitude
    );
}

#[test]
fn noise_alone_yields_no_confident_modes() {
    let rate = 48_000.0;
    let x = modes(rate, 0.5, &[(300.0, 1000.0, 1e-9, 0.0)], Some(-160.0));
    let found = analyse_band(&x, rate, 0, (0.2 * rate) as usize, 200.0, 400.0);
    // Either nothing passes ESTER, or what does is a handful of weak candidates, never a strong line.
    if let Some(found) = found {
        assert!(found.modes.len() <= 12);
    }
}
