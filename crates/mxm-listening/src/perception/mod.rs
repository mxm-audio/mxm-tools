//! Perceptual models: how loud, how sharp, how tonal, how rough a sound is to a listener, after
//! ECMA-418-2 (the Sottek hearing model), the one perceptual standard published in full text
//! (`research:listening/perceptual-measures.md` §7). They take sound pressure, so every reading here
//! assumes the owner's fixed level, a full-scale sine at 94 dB SPL, heard diotically in a free field
//! (the plan's revision 8), and says so.
//!
//! **For a hit**, the standard's single values do not apply: each discards the first 300 ms as its
//! filters' transient (§7.7 of the research page). What a hit has is the time-dependent values, and
//! those are what this module returns and the reports quote, window by window from the onset.
//!
//! **Sharpness** is the specific basis loudness's weighted first moment along the critical-band axis with
//! DIN 45692's weighting, `g(z) = 1` to 15.8 Bark and `0.15·e^(0.42(z − 15.8)) + 0.85` above — the form
//! the research page marks unverified, as DIN 45692 was not read — here on the Sottek model's specific
//! loudness and Bark_HMS, and calibrated on this model to the acum's definition: a narrow band of noise
//! at 1 kHz and 60 dB SPL reads 1 acum (Swift & Gee report DIN 45692 working on another model's
//! specific loudness).

pub mod fluctuation;
pub mod hearing;
pub mod roughness;
pub mod sottek;

use crate::sound::Sound;
use hearing::BANDS;

/// Sharpness's calibration on this model: 1 acum for 1 kHz narrow-band noise at 60 dB SPL. The
/// textbook form's 0.11 read 1.043 acum here.
pub const SHARPNESS_K: f64 = 0.1055;

/// DIN 45692's weighting at critical-band rate `z` (unverified form).
#[must_use]
pub fn sharpness_weight(z: f64) -> f64 {
    if z <= 15.8 {
        1.0
    } else {
        0.15 * (0.42 * (z - 15.8)).exp() + 0.85
    }
}

/// Sharpness of one specific-loudness pattern, acum; `None` where it is silent.
#[must_use]
pub fn sharpness(specific: &[f64; BANDS]) -> Option<f64> {
    let total: f64 = specific.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let moment: f64 = specific
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let z = hearing::z(i);
            n * sharpness_weight(z) * z
        })
        .sum();
    Some(SHARPNESS_K * moment / total)
}

/// A sound's perceptual curves from its first sample.
#[derive(Clone, Debug, Default)]
pub struct Perception {
    /// Loudness N(l), sone_HMS, every 1/187.5 s (Clause 8: tonal and noise loudness weighted, both
    /// smoothed at 3.5 Hz, so it lags and smears a hit).
    pub loudness: Vec<f64>,
    /// Basis loudness N_basis(l), sone_HMS (Clause 5): unsmoothed but for its blocks, so it keeps a
    /// hit's timing; noise reads louder in it than in `loudness`.
    pub basis: Vec<f64>,
    /// Sharpness per time step from the basis specific loudness, acum (`None` where silent).
    pub sharpness: Vec<Option<f64>>,
    /// Tonality T(l), tu_HMS, and its frequency, Hz.
    pub tonality: Vec<(f64, f64)>,
    /// Roughness R(l₅₀), asper, every 1/50 s.
    pub roughness: Vec<f64>,
    /// The share of the basis loudness in the bands centred under [`BOOM_HZ`], per time step: what
    /// boominess rests on (Hatano & Hashimoto's booming index, JSAE 2000, reads the loudness under
    /// 280 Hz). Zero where silent.
    pub low_share: Vec<f64>,
}

/// Boominess reads the loudness under this, Hz (Hatano & Hashimoto 2000).
pub const BOOM_HZ: f64 = 280.0;

impl Perception {
    /// The loudness, sharpness and tonality time step, s.
    pub const STEP_S: f64 = 1.0 / sottek::RATE_TIME;
    /// The roughness time step, s.
    pub const ROUGHNESS_STEP_S: f64 = 1.0 / roughness::RATE_OUT;
}

/// The perceptual curves of `sound`, its first `seconds` only (the models cost a few seconds per
/// second of sound); `None` for an empty sound.
#[must_use]
pub fn analyse(sound: &Sound, seconds: f64) -> Option<Perception> {
    let mut p = hearing::pressure(sound);
    p.truncate((seconds * hearing::RATE) as usize);
    let s = sottek::analyse(&p)?;
    let r = roughness::analyse(&p)?;
    Some(Perception {
        loudness: s.total_loudness(),
        basis: s.total_basis(),
        sharpness: s.basis.iter().map(sharpness).collect(),
        tonality: s.total_tonality(),
        roughness: r.total(),
        low_share: s
            .basis
            .iter()
            .map(|b| {
                let total: f64 = b.iter().sum();
                let low: f64 = b
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| hearing::centre_hz(hearing::z(*i)) < BOOM_HZ)
                    .map(|(_, v)| v)
                    .sum();
                if total > 0.0 { low / total } else { 0.0 }
            })
            .collect(),
    })
}
