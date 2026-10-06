//! The Sottek hearing model, ECMA-418-2 Clause 5: sound pressure through the outer and middle/inner
//! ear, an auditory filter bank of 53 bands half a Bark_HMS apart, half-wave rectification, block RMS,
//! the compressive nonlinearity and the threshold in quiet. Written from the standard's text (4th
//! edition, June 2025; `research:listening/perceptual-measures.md` §7.2); no implementation opened.

use crate::sound::Sound;

/// The model's sampling rate, Hz (§5.1.1): anything else is resampled to it.
pub const RATE: f64 = 48_000.0;
/// Critical-band filters (CBF): z = 0.5 … 26.5 Bark_HMS in steps of [`DZ`].
pub const BANDS: usize = 53;
pub const DZ: f64 = 0.5;
/// Reference pressure, Pa.
pub const P0: f64 = 20e-6;
/// The pressure of a full-scale sample, Pa RMS per unit RMS: the owner's fixed listening level, a
/// full-scale sine at 94 dB SPL (the plan's revision 8) — amplitude 1 is 1 Pa RMS, so a sample value
/// is multiplied by √2.
pub const PASCAL_PER_UNIT: f64 = std::f64::consts::SQRT_2;

/// Δf(f = 0) and c of Formulae (9)–(10).
const DF0: f64 = 81.9289;
const C: f64 = 0.1618;

/// The critical-band rate of band `i` (0-based), Bark_HMS.
#[must_use]
pub fn z(i: usize) -> f64 {
    DZ * (i + 1) as f64
}

/// Centre frequency F(z), Hz (Formula (9)).
#[must_use]
pub fn centre_hz(z: f64) -> f64 {
    DF0 / C * (C * z).sinh()
}

/// Bandwidth Δf(z), Hz (Formula (10)).
#[must_use]
pub fn bandwidth_hz(z: f64) -> f64 {
    (DF0 * DF0 + (C * centre_hz(z)).powi(2)).sqrt()
}

/// Table 1: the outer and middle/inner ear as eight second-order sections, free field; `b0 b1 b2 a1 a2`.
/// A diffuse field uses sections 3–8.
const EAR: [[f64; 5]; 8] = [
    [1.015896, -1.925299, 0.922118, -1.925299, 0.938014],
    [0.958943, -1.806088, 0.876439, -1.806088, 0.835382],
    [0.961372, -1.763632, 0.821788, -1.763632, 0.783160],
    [2.225804, -1.434650, -0.498204, -1.434650, 0.727599],
    [0.471735, -0.366092, 0.244145, -0.366092, -0.284120],
    [0.115267, 0.000000, -0.115267, -1.796003, 0.805838],
    [0.988029, -1.912434, 0.926132, -1.912434, 0.914161],
    [1.952238, 0.162320, -0.667994, 0.162320, 0.284244],
];

/// Table 2: the nonlinearity's thresholds (dB re 20 µPa) and exponents; ν₀ = 1.
const THRESHOLDS_DB: [f64; 8] = [15.0, 25.0, 35.0, 45.0, 55.0, 65.0, 75.0, 85.0];
const EXPONENTS: [f64; 8] = [
    0.6602, 0.0864, 0.6384, 0.0328, 0.4068, 0.2082, 0.3994, 0.6434,
];
/// α of Formula (23).
const ALPHA: f64 = 1.5;
/// c_N, sone_HMS per Bark_HMS: a 1 kHz tone at 40 dB SPL is 1 sone_HMS (Clause 8.1).
pub const C_N: f64 = 0.0211964;

/// Table 3: the specific loudness threshold LTQ(z), z = 0.5 … 26.5.
const LTQ: [f64; BANDS] = [
    0.3310, 0.1625, 0.1051, 0.0757, 0.0576, 0.0453, 0.0365, 0.0298, 0.0247, 0.0207, 0.0176, 0.0151,
    0.0131, 0.0115, 0.0103, 0.0093, 0.0086, 0.0081, 0.0077, 0.0074, 0.0073, 0.0072, 0.0071, 0.0072,
    0.0073, 0.0074, 0.0076, 0.0079, 0.0082, 0.0086, 0.0092, 0.0100, 0.0109, 0.0122, 0.0138, 0.0157,
    0.0172, 0.0180, 0.0180, 0.0177, 0.0176, 0.0177, 0.0182, 0.0190, 0.0202, 0.0217, 0.0237, 0.0263,
    0.0296, 0.0339, 0.0398, 0.0485, 0.0622,
];

/// The sound as pressure at 48 kHz, Pa, at the owner's fixed level, with the 5 ms fade-in of
/// Formula (1).
#[must_use]
pub fn pressure(sound: &Sound) -> Vec<f64> {
    let x: Vec<f64> = sound
        .samples
        .iter()
        .map(|&v| f64::from(v) * PASCAL_PER_UNIT)
        .collect();
    let mut p = if (f64::from(sound.rate) - RATE).abs() < 0.5 {
        x
    } else {
        crate::repr::resample::rate(&x, f64::from(sound.rate), RATE)
    };
    let fade = (0.005 * RATE) as usize;
    for (n, v) in p.iter_mut().take(fade).enumerate() {
        *v *= 0.5 - 0.5 * (std::f64::consts::PI * n as f64 / fade as f64).cos();
    }
    p
}

/// The outer and middle/inner ear, free field (§5.1.3), in place.
pub fn ear(p: &mut [f64]) {
    for [b0, b1, b2, a1, a2] in EAR {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        for v in p.iter_mut() {
            let x0 = *v;
            let y0 = b0 * x0 + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
            x2 = x1;
            x1 = x0;
            y2 = y1;
            y1 = y0;
            *v = y0;
        }
    }
}

/// Band `i` of the auditory filter bank (§5.1.4): the order-5 low-pass of Formula (11), modulated to
/// F(z) (Formulae (16)–(17)), run on complex numbers in double precision; the band-pass signal is twice
/// the real part.
#[must_use]
pub fn band(p: &[f64], i: usize) -> Vec<f64> {
    const K: usize = 5;
    const E: [f64; K] = [0.0, 1.0, 11.0, 11.0, 1.0];
    let zi = z(i);
    let (f, df) = (centre_hz(zi), bandwidth_hz(zi));
    // Formula (8): τ = C(2k−2, k−1) / (2^(2k−1) Δf) — for k = 5, 70 / 512.
    let tau = 70.0 / 512.0 / df;
    let d = (-1.0 / (RATE * tau)).exp();
    let norm: f64 = (1..K).map(|j| E[j] * d.powi(j as i32)).sum();
    let gain = (1.0 - d).powi(K as i32) / norm;
    let w = std::f64::consts::TAU * f / RATE;
    // b′_m for m = 0..k−1 and a′_m for m = 1..k, as (re, im).
    let b: Vec<(f64, f64)> = (0..K)
        .map(|m| {
            let v = gain * d.powi(m as i32) * E[m];
            (v * (w * m as f64).cos(), v * (w * m as f64).sin())
        })
        .collect();
    let a: Vec<(f64, f64)> = (1..=K)
        .map(|m| {
            let v = (-d).powi(m as i32) * binomial(K, m);
            (v * (w * m as f64).cos(), v * (w * m as f64).sin())
        })
        .collect();
    let mut xs = [0.0f64; K];
    let mut ys = [(0.0f64, 0.0f64); K];
    let mut out = Vec::with_capacity(p.len());
    for &x0 in p {
        xs.rotate_right(1);
        xs[0] = x0;
        let (mut re, mut im) = (0.0, 0.0);
        for m in 0..K {
            re += b[m].0 * xs[m];
            im += b[m].1 * xs[m];
        }
        for m in 0..K {
            let (ar, ai) = a[m];
            let (yr, yi) = ys[m];
            re -= ar * yr - ai * yi;
            im -= ar * yi + ai * yr;
        }
        ys.rotate_right(1);
        ys[0] = (re, im);
        out.push(2.0 * re);
    }
    out
}

fn binomial(n: usize, k: usize) -> f64 {
    (0..k).fold(1.0, |acc, j| acc * (n - j) as f64 / (j + 1) as f64)
}

/// The specific loudness of a band's RMS pressure `p` (Pa), before the threshold in quiet (Formula (23)).
#[must_use]
pub fn nonlinearity(p: f64) -> f64 {
    let mut out = C_N * p / P0;
    let mut previous = 1.0;
    for (db, v) in THRESHOLDS_DB.iter().zip(EXPONENTS) {
        let pt = P0 * 10f64.powf(db / 20.0);
        out *= (1.0 + (p / pt).powf(ALPHA)).powf((v - previous) / ALPHA);
        previous = v;
    }
    out
}

/// The specific basis loudness of band `i` from its block RMS pressure (Formula (25)).
#[must_use]
pub fn basis_loudness(i: usize, p: f64) -> f64 {
    (nonlinearity(p) - LTQ[i]).max(0.0)
}

/// Block RMS of the half-wave rectified block, with the factor 2 of Formula (22).
#[must_use]
pub fn rectified_rms(block: &[f64]) -> f64 {
    if block.is_empty() {
        return 0.0;
    }
    let e: f64 = block
        .iter()
        .map(|&v| if v > 0.0 { v * v } else { 0.0 })
        .sum();
    (2.0 * e / block.len() as f64).sqrt()
}
