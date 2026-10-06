//! Modulation spike: what the LFO and the envelope actually do.
//!
//! The companion to `osc_spike.rs`, and the evidence source for
//! [`docs/modulation/`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/modulation/README.md)
//! in mxm-kit. It measures the **shipped** `lfo.rs` and `envelope.rs` (mxm-mono-01's) through
//! their public API.
//!
//! One thing here is *not* shipped code and is labelled wherever it appears: the parameter
//! smoothing in §4. Smoothing belongs to the plugin (the plugin conventions own parameters,
//! ranges and smoothing: once `plugins/AGENTS.md`, now mxm-kit's `docs/plugin-conventions.md`;
//! `crates/mxm-mono-01-dsp/AGENTS.md` in mxm-mono-01 explicitly disclaims them), and this
//! crate has no dependencies, so an example here cannot reach the shipped path. §4 measures a
//! generic one-pole model of it. Treat those numbers as being about the technique, never as
//! validation of mxm-mono-01.
//!
//! Method is the same as the oscillator spike: exactly-periodic frequencies in a power-of-two
//! window, so no window function is needed and alias energy separates exactly.
//!
//! Run with: `cargo run -p dsp-lab --release --example mod_spike`

use mxm_measure::spectrum::{N, fft, periods_for};
use mxm_mono_01_dsp::envelope::Adsr;
use mxm_mono_01_dsp::lfo::{Lfo, LfoShape};
use std::f64::consts::PI;

const FS: f64 = 44_100.0;

/// Energy off the harmonic grid of `p` over energy on it, in dB.
fn alias_db(x: &[f64], p: usize) -> f64 {
    let mut re = x.to_vec();
    let mut im = vec![0.0f64; x.len()];
    fft(&mut re, &mut im).expect("the harness allocates power-of-two buffers");
    let (mut want, mut other) = (0.0f64, 0.0f64);
    for bin in 1..N / 2 {
        let e = re[bin] * re[bin] + im[bin] * im[bin];
        if bin % p == 0 {
            want += e;
        } else {
            other += e;
        }
    }
    10.0 * (other / want.max(1e-30)).max(1e-30).log10()
}

/// Energy above `above_hz` relative to the carrier bin, in dB.
///
/// The metric §3 and §4 need. A sine whose amplitude is being modulated has legitimate
/// sidebands close to the carrier, and an alias-style measure counts those as error — which
/// makes a slow, clean envelope look *worse* than an instant one. A click is broadband, so
/// measuring far from the carrier separates the two.
fn splatter_above_db(x: &[f64], carrier_bin: usize, above_hz: f64) -> f64 {
    let mut re = x.to_vec();
    let mut im = vec![0.0f64; x.len()];
    fft(&mut re, &mut im).expect("the harness allocates power-of-two buffers");
    let first = (above_hz * N as f64 / FS) as usize;
    let mut high = 0.0f64;
    for bin in first..N / 2 {
        high += re[bin] * re[bin] + im[bin] * im[bin];
    }
    let c = re[carrier_bin] * re[carrier_bin] + im[carrier_bin] * im[carrier_bin];
    10.0 * (high / c.max(1e-30)).max(1e-30).log10()
}

fn main() {
    println!("mxm-mono-01 modulation spike\n");
    println!(
        "Analysis: N = {N}, fs = {FS} Hz, exactly-periodic rates, rectangular window.\n\
         Sections 1-3 measure the shipped `lfo.rs` and `envelope.rs`. Section 4 measures a\n\
         generic smoothing model, NOT the plugin's — see the module docs.\n"
    );

    // ---------------------------------------------------------------- 1. LFO spectra
    println!("1. The LFO's own spectrum. `lfo.rs` states it is not band limited; this is what");
    println!("   that costs, measured as energy off the LFO's own harmonic grid.\n");
    println!(
        "      {:>10}  {:>10}  {:>12}  {:>12}",
        "shape", "rate (Hz)", "alias dB", "harmonics"
    );
    for shape in [
        LfoShape::Triangle,
        LfoShape::Square,
        LfoShape::SawUp,
        LfoShape::Random,
    ] {
        // Below about 5 Hz the analysis window holds too few cycles for the harmonic
        // grid to be meaningful — at one period per window every bin is a harmonic and
        // the metric degenerates to -300 dB. So the sweep starts where it can measure.
        for target in [7.0f64, 15.0, 30.0] {
            let p = periods_for(target, FS, N);
            let rate = p as f64 * FS / N as f64;
            let mut lfo = Lfo::new();
            let x: Vec<f64> = (0..N)
                .map(|_| lfo.process(rate as f32, shape, FS as f32) as f64)
                .collect();
            if shape == LfoShape::Random {
                // Sample and hold has no harmonic structure to be off, so the alias
                // measure means nothing for it. Said rather than printed as a number.
                println!(
                    "      {:>10}  {:>10.2}  {:>12}  {:>12}",
                    format!("{shape:?}"),
                    rate,
                    "n/a",
                    "noise"
                );
            } else {
                println!(
                    "      {:>10}  {:>10.2}  {:>12.1}  {:>12}",
                    format!("{shape:?}"),
                    rate,
                    alias_db(&x, p),
                    (0.45 * FS / rate) as usize
                );
            }
        }
    }

    // ------------------------------------------------- 2. what it does to the audio
    println!(
        "\n2. Tremolo: a 1 kHz sine multiplied by the LFO, so whatever is in the LFO lands in"
    );
    println!("   the audio band where it can be measured against the carrier.\n");
    println!(
        "      {:>10}  {:>10}  {:>14}  {:>14}",
        "shape", "rate (Hz)", "alias dB", "vs LFO alone"
    );
    for shape in [LfoShape::Triangle, LfoShape::Square, LfoShape::SawUp] {
        for target in [7.0f64, 30.0] {
            let p = periods_for(target, FS, N);
            let rate = p as f64 * FS / N as f64;

            let mut bare = Lfo::new();
            let bare_x: Vec<f64> = (0..N)
                .map(|_| bare.process(rate as f32, shape, FS as f32) as f64)
                .collect();
            let bare_db = alias_db(&bare_x, p);

            // Carrier at an exact multiple of the LFO rate, so the product stays periodic.
            let mult = (1000.0 / rate).round().max(1.0) as usize;
            let mut lfo = Lfo::new();
            let x: Vec<f64> = (0..N)
                .map(|i| {
                    let m = lfo.process(rate as f32, shape, FS as f32) as f64;
                    let carrier = (2.0 * PI * (mult * p) as f64 * i as f64 / N as f64).sin();
                    carrier * (1.0 + 0.5 * m)
                })
                .collect();
            let trem_db = alias_db(&x, p);
            println!(
                "      {:>10}  {:>10.2}  {:>14.1}  {:>14.1}",
                format!("{shape:?}"),
                rate,
                trem_db,
                trem_db - bare_db
            );
        }
    }

    // ------------------------------------------------------------- 3. the envelope
    println!("\n3. Envelope attack: how short is too short. A 1 kHz sine gated by the shipped");
    println!("   ADSR, measured as energy away from the carrier bin — the click.\n");
    println!(
        "      {:>12}  {:>18}  {:>14}",
        "attack (ms)", "click >4 kHz dB", "peak"
    );
    for attack_ms in [0.0f64, 0.5, 1.0, 5.0, 20.0] {
        let p = periods_for(1000.0, FS, N);
        let mut env = Adsr::new();
        env.set_sample_rate(FS as f32);
        env.trigger();
        let mut peak = 0.0f64;
        let x: Vec<f64> = (0..N)
            .map(|i| {
                let e = env.process(attack_ms as f32 / 1000.0, 1.0, 1.0, 0.1) as f64;
                let c = (2.0 * PI * p as f64 * i as f64 / N as f64).sin();
                let y = c * e;
                peak = peak.max(y.abs());
                y
            })
            .collect();

        println!(
            "      {:>12.1}  {:>18.1}  {:>14.3}",
            attack_ms,
            splatter_above_db(&x, p, 4000.0),
            peak
        );
    }

    // ------------------------------------------- 4. smoothing (a generic model, not ours)
    println!("\n4. Parameter smoothing. **A generic one-pole model, not the plugin's** — see the");
    println!("   module docs. A control stepping between two values every 10 ms, applied as a");
    println!("   gain on a 1 kHz sine.\n");
    println!(
        "      {:>16}  {:>18}  {:>14}",
        "smoothing (ms)", "zipper >4 kHz dB", "audible step"
    );
    for smooth_ms in [0.0f64, 0.2, 1.0, 5.0, 20.0] {
        let p = periods_for(1000.0, FS, N);
        let coeff = if smooth_ms <= 0.0 {
            1.0
        } else {
            1.0 - (-1.0 / (smooth_ms * 0.001 * FS)).exp()
        };
        let step_period = (FS * 0.010) as usize;
        let mut state = 0.5f64;
        let x: Vec<f64> = (0..N)
            .map(|i| {
                let target = if (i / step_period).is_multiple_of(2) {
                    0.25
                } else {
                    0.75
                };
                state += coeff * (target - state);
                let c = (2.0 * PI * p as f64 * i as f64 / N as f64).sin();
                c * state
            })
            .collect();

        println!(
            "      {:>16.1}  {:>18.1}  {:>14}",
            smooth_ms,
            splatter_above_db(&x, p, 4000.0),
            if smooth_ms < 1.0 { "yes" } else { "no" }
        );
    }
}
