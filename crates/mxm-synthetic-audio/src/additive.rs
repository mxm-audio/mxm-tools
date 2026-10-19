//! Additive synthesis: an instrument as a harmonic spectrum shaped by its resonances, its noise
//! and its envelopes, played along per-sample f0 and amplitude curves.
//!
//! The pitch truth is exact by construction: every partial's phase is the integral of `k · f0(t)`
//! (times a string's stretch), so the f0 written beside a file is the f0 the samples were made
//! from. Spectra are approximations from the instrument-acoustics literature: the string bodies'
//! main resonances and bridge hill, the wind instruments' formants and the clarinet's weak even
//! partials after J. Meyer, *Acoustics and the Performance of Music* (5th ed., Springer 2009); the
//! sung vowels' formants, the singer's formant and a soprano's first formant tuned to f0 after
//! J. Sundberg, *The Science of the Singing Voice* (1987); a stiff string's partials
//! `k·f·√(1 + B·k²)` after Fletcher, *Normal vibration frequencies of a stiff piano string*, JASA
//! 36 (1964).

use crate::dsp::{Biquad, Rng, db_to_gain, white};

/// One resonance of a spectral envelope: centre Hz, width in octaves (a Gaussian's sigma on a
/// log-frequency axis), gain dB.
#[derive(Clone, Copy, Debug)]
pub struct Resonance(pub f64, pub f64, pub f64);

#[derive(Clone, Copy, Debug)]
pub struct Profile {
    pub name: &'static str,
    /// The source's fall per octave of harmonic number, dB, played softly and loudly.
    pub tilt_soft: f64,
    pub tilt_loud: f64,
    pub resonances: &'static [Resonance],
    /// Above this, a further fall of `rolloff_db_oct` per octave (raised by brightness).
    pub rolloff_hz: f64,
    pub rolloff_db_oct: f64,
    /// Added to even harmonics below `even_until_hz` (a clarinet's weak evens), dB.
    pub even_db: f64,
    pub even_until_hz: f64,
    /// The first resonance follows f0 when f0 is above it (a soprano's formant tuning).
    pub formant_tuning: bool,
    /// Noise under the tone, dB (RMS against the tone's RMS), and its band, Hz.
    pub noise_db: f64,
    pub noise_band: (f64, f64),
    /// A stiff string's inharmonicity coefficient B (0 for bowed strings, winds and voices).
    pub inharmonicity: f64,
    /// Slow random level wobble of each partial, dB RMS.
    pub shimmer_db: f64,
    /// Typical attack (to full level) and release (to -40 dB), s.
    pub attack: f64,
    pub release: f64,
}

const VIOLIN: &[Resonance] = &[
    Resonance(275.0, 0.18, 6.0),
    Resonance(460.0, 0.15, 8.0),
    Resonance(550.0, 0.15, 6.0),
    Resonance(1000.0, 0.4, -3.0),
    Resonance(2600.0, 0.5, 7.0),
];
const VIOLA: &[Resonance] = &[
    Resonance(230.0, 0.2, 6.0),
    Resonance(350.0, 0.16, 7.0),
    Resonance(440.0, 0.16, 5.0),
    Resonance(900.0, 0.4, -3.0),
    Resonance(2100.0, 0.5, 6.0),
];
const CELLO: &[Resonance] = &[
    Resonance(100.0, 0.25, 5.0),
    Resonance(190.0, 0.18, 6.0),
    Resonance(225.0, 0.16, 5.0),
    Resonance(600.0, 0.4, 2.0),
    Resonance(1500.0, 0.5, 5.0),
];
const DOUBLE_BASS: &[Resonance] = &[
    Resonance(60.0, 0.3, 4.0),
    Resonance(110.0, 0.25, 6.0),
    Resonance(250.0, 0.4, 3.0),
    Resonance(1000.0, 0.6, 3.0),
];
/// /a/, a mezzo's.
const VOICE: &[Resonance] = &[
    Resonance(800.0, 0.14, 14.0),
    Resonance(1150.0, 0.12, 10.0),
    Resonance(2800.0, 0.12, 5.0),
    Resonance(3500.0, 0.12, 2.0),
];
/// /a/, a soprano's; the first formant is tuned to f0 when f0 rises past it.
const SOPRANO: &[Resonance] = &[
    Resonance(900.0, 0.14, 14.0),
    Resonance(1300.0, 0.12, 9.0),
    Resonance(2900.0, 0.12, 4.0),
    Resonance(3800.0, 0.12, 2.0),
];
/// /a/, a tenor's, with the singer's formant clustered near 3 kHz.
const TENOR: &[Resonance] = &[
    Resonance(650.0, 0.14, 13.0),
    Resonance(1080.0, 0.12, 9.0),
    Resonance(2700.0, 0.08, 9.0),
    Resonance(3100.0, 0.08, 9.0),
];
const BASSOON: &[Resonance] = &[
    Resonance(470.0, 0.2, 9.0),
    Resonance(1200.0, 0.2, 6.0),
    Resonance(2500.0, 0.3, 2.0),
];
const TROMBONE: &[Resonance] = &[Resonance(520.0, 0.45, 8.0), Resonance(1200.0, 0.5, 3.0)];
const CLARINET: &[Resonance] = &[Resonance(1500.0, 0.4, 3.0), Resonance(3000.0, 0.4, 1.0)];
const SAXOPHONE: &[Resonance] = &[
    Resonance(500.0, 0.3, 6.0),
    Resonance(1600.0, 0.3, 6.0),
    Resonance(3000.0, 0.3, 2.0),
];
const HORN: &[Resonance] = &[Resonance(340.0, 0.5, 8.0), Resonance(750.0, 0.5, 2.0)];
const FLUTE: &[Resonance] = &[Resonance(2000.0, 0.8, -2.0)];

pub fn profile(name: &str) -> Profile {
    let base = Profile {
        name: "",
        tilt_soft: 7.0,
        tilt_loud: 4.0,
        resonances: &[],
        rolloff_hz: 4000.0,
        rolloff_db_oct: 12.0,
        even_db: 0.0,
        even_until_hz: 0.0,
        formant_tuning: false,
        noise_db: -38.0,
        noise_band: (2000.0, 8000.0),
        inharmonicity: 0.0,
        shimmer_db: 0.6,
        attack: 0.06,
        release: 0.25,
    };
    match name {
        "violin" => Profile {
            name: "violin",
            resonances: VIOLIN,
            rolloff_hz: 4500.0,
            ..base
        },
        "viola" => Profile {
            name: "viola",
            resonances: VIOLA,
            rolloff_hz: 3800.0,
            tilt_soft: 7.5,
            tilt_loud: 4.5,
            ..base
        },
        "cello" => Profile {
            name: "cello",
            resonances: CELLO,
            rolloff_hz: 3000.0,
            tilt_soft: 8.0,
            tilt_loud: 5.0,
            ..base
        },
        "double-bass" => Profile {
            name: "double-bass",
            resonances: DOUBLE_BASS,
            rolloff_hz: 1800.0,
            tilt_soft: 8.0,
            tilt_loud: 5.0,
            noise_band: (1000.0, 6000.0),
            attack: 0.08,
            release: 0.35,
            ..base
        },
        "plucked-bass" => Profile {
            name: "plucked-bass",
            resonances: DOUBLE_BASS,
            rolloff_hz: 1500.0,
            tilt_soft: 9.0,
            tilt_loud: 6.0,
            noise_db: -42.0,
            noise_band: (1500.0, 6000.0),
            inharmonicity: 0.0002,
            shimmer_db: 0.2,
            attack: 0.004,
            release: 1.2,
            ..base
        },
        "voice" => Profile {
            name: "voice",
            resonances: VOICE,
            tilt_soft: 12.0,
            tilt_loud: 8.0,
            rolloff_hz: 5000.0,
            noise_db: -34.0,
            noise_band: (400.0, 6000.0),
            shimmer_db: 0.8,
            attack: 0.08,
            release: 0.15,
            ..base
        },
        "soprano" => Profile {
            name: "soprano",
            resonances: SOPRANO,
            tilt_soft: 11.0,
            tilt_loud: 7.0,
            rolloff_hz: 5000.0,
            formant_tuning: true,
            noise_db: -36.0,
            noise_band: (500.0, 7000.0),
            shimmer_db: 0.8,
            attack: 0.08,
            release: 0.15,
            ..base
        },
        "tenor" => Profile {
            name: "tenor",
            resonances: TENOR,
            tilt_soft: 11.0,
            tilt_loud: 7.0,
            rolloff_hz: 4500.0,
            noise_db: -36.0,
            noise_band: (400.0, 6000.0),
            shimmer_db: 0.8,
            attack: 0.08,
            release: 0.15,
            ..base
        },
        "bassoon" => Profile {
            name: "bassoon",
            resonances: BASSOON,
            tilt_soft: 5.0,
            tilt_loud: 3.0,
            rolloff_hz: 3000.0,
            noise_db: -40.0,
            noise_band: (1000.0, 6000.0),
            attack: 0.05,
            release: 0.12,
            ..base
        },
        "trombone" => Profile {
            name: "trombone",
            resonances: TROMBONE,
            tilt_soft: 9.0,
            tilt_loud: 2.5,
            rolloff_hz: 1800.0,
            rolloff_db_oct: 15.0,
            noise_db: -42.0,
            noise_band: (1000.0, 5000.0),
            shimmer_db: 0.4,
            attack: 0.05,
            release: 0.15,
            ..base
        },
        "clarinet" => Profile {
            name: "clarinet",
            resonances: CLARINET,
            tilt_soft: 4.0,
            tilt_loud: 2.5,
            rolloff_hz: 3000.0,
            even_db: -22.0,
            even_until_hz: 1500.0,
            noise_db: -40.0,
            noise_band: (1500.0, 7000.0),
            shimmer_db: 0.4,
            attack: 0.04,
            release: 0.12,
            ..base
        },
        "saxophone" => Profile {
            name: "saxophone",
            resonances: SAXOPHONE,
            tilt_soft: 5.0,
            tilt_loud: 2.5,
            rolloff_hz: 4000.0,
            noise_db: -34.0,
            noise_band: (1500.0, 8000.0),
            attack: 0.04,
            release: 0.12,
            ..base
        },
        "horn" => Profile {
            name: "horn",
            resonances: HORN,
            tilt_soft: 8.0,
            tilt_loud: 3.0,
            rolloff_hz: 1200.0,
            rolloff_db_oct: 18.0,
            noise_db: -44.0,
            noise_band: (800.0, 4000.0),
            shimmer_db: 0.4,
            attack: 0.06,
            release: 0.2,
            ..base
        },
        "flute" => Profile {
            name: "flute",
            resonances: FLUTE,
            tilt_soft: 14.0,
            tilt_loud: 9.0,
            rolloff_hz: 5000.0,
            noise_db: -30.0,
            noise_band: (1500.0, 9000.0),
            shimmer_db: 0.5,
            attack: 0.05,
            release: 0.12,
            ..base
        },
        other => panic!("no additive profile {other:?}"),
    }
}

impl Profile {
    /// The envelope's level at `f` Hz for f0 `f0` and a brightness, dB.
    pub fn envelope_db_at(&self, f: f64, f0: f64, bright: f64) -> f64 {
        self.envelope_db(f, f0, bright)
    }

    /// The envelope's level at `f` Hz for f0 `f0`, dB.
    fn envelope_db(&self, f: f64, f0: f64, bright: f64) -> f64 {
        let mut db = 0.0;
        for (i, &Resonance(centre, width, gain)) in self.resonances.iter().enumerate() {
            let centre = if i == 0 && self.formant_tuning {
                centre.max(f0 * 1.05)
            } else {
                centre
            };
            let x = (f / centre).log2() / width;
            db += gain * (-0.5 * x * x).exp();
        }
        // Brighter playing lifts the roll-off point by up to an octave.
        let knee = self.rolloff_hz * (1.0 + bright);
        if f > knee {
            db -= self.rolloff_db_oct * (f / knee).log2();
        }
        db
    }
}

/// What one continuous line plays, per sample: f0 Hz, the tone's amplitude, the noise's amplitude
/// (1 = the profile's level under a full tone), and brightness from 0 (soft) to 1 (loud).
pub struct Line {
    pub hz: Vec<f64>,
    pub amp: Vec<f64>,
    pub noise: Vec<f64>,
    pub bright: Vec<f64>,
    /// Harmonic number and a level set against the envelope's, dB (a weak fundamental).
    pub harmonic_db: Vec<(usize, f64)>,
    /// Only these harmonics sound, when given (an even-harmonics-only control).
    pub only: Option<fn(usize) -> bool>,
    /// When given, each moment's partials are scaled so the tone's power is what it would be at
    /// this f0 (a vibrato's midline): the resonances still colour the partials as the pitch moves,
    /// but the level follows `amp` alone.
    pub centre_hz: Option<Vec<f64>>,
}

impl Line {
    /// A line from a function of time, s, to `(hz, amp, noise, bright)`.
    pub fn from_fn(rate: f64, duration: f64, f: impl Fn(f64) -> (f64, f64, f64, f64)) -> Self {
        let n = (duration * rate).round() as usize;
        let mut line = Line {
            hz: Vec::with_capacity(n),
            amp: Vec::with_capacity(n),
            noise: Vec::with_capacity(n),
            bright: Vec::with_capacity(n),
            harmonic_db: Vec::new(),
            only: None,
            centre_hz: None,
        };
        for i in 0..n {
            let (hz, amp, noise, bright) = f(i as f64 / rate);
            line.hz.push(hz);
            line.amp.push(amp);
            line.noise.push(noise);
            line.bright.push(bright);
        }
        line
    }
}

/// Partials are updated every `CONTROL` samples and ramped between.
const CONTROL: usize = 32;
const MAX_HARMONICS: usize = 80;

/// Renders `line` with `profile` at `rate`. `rng` seeds the partials' wobble and the noise.
pub fn render(profile: &Profile, line: &Line, rate: f64, rng: &mut Rng) -> Vec<f64> {
    let n = line.hz.len();
    let mut out = vec![0.0; n];
    if n == 0 {
        return out;
    }
    let nyquist_guard = 0.42 * rate;
    let b = profile.inharmonicity;
    let stretch: Vec<f64> = (1..=MAX_HARMONICS)
        .map(|k| {
            let k = k as f64;
            ((1.0 + b * k * k) / (1.0 + b)).sqrt()
        })
        .collect();
    // Each partial's slow level wobble, at the control rate.
    let blocks = n / CONTROL + 2;
    let control_rate = rate / CONTROL as f64;
    let wobble: Vec<Vec<f64>> = (0..MAX_HARMONICS)
        .map(|_| crate::dsp::slow_noise(rng, blocks, control_rate, 3.0))
        .collect();
    let mut phase = vec![0.0f64; MAX_HARMONICS];
    // Random starting phases, as a real tone has.
    for p in phase.iter_mut() {
        *p = rng.range(0.0, std::f64::consts::TAU);
    }
    let mut current = vec![0.0f64; MAX_HARMONICS];
    let override_db = |k: usize| -> f64 {
        line.harmonic_db
            .iter()
            .filter(|(h, _)| *h == k)
            .map(|(_, db)| *db)
            .sum()
    };
    let mut start = 0;
    let mut block = 0;
    while start < n {
        let end = (start + CONTROL).min(n);
        let f0 = line.hz[start].max(1.0);
        let bright = line.bright[start].clamp(0.0, 1.0);
        let tilt = profile.tilt_soft + (profile.tilt_loud - profile.tilt_soft) * bright;
        let partials = |f0: f64| -> Vec<f64> {
            let mut target = vec![0.0f64; MAX_HARMONICS];
            for k in 1..=MAX_HARMONICS {
                let f = k as f64 * f0 * stretch[k - 1];
                if f > nyquist_guard {
                    break;
                }
                if let Some(only) = line.only
                    && !only(k)
                {
                    continue;
                }
                let mut db = -tilt * (k as f64).log2() + profile.envelope_db(f, f0, bright);
                if k % 2 == 0 && f < profile.even_until_hz {
                    db += profile.even_db;
                }
                db += override_db(k);
                db += profile.shimmer_db * wobble[k - 1][block];
                // A partial near the guard fades out rather than stopping.
                let fade = ((nyquist_guard - f) / (0.05 * rate)).clamp(0.0, 1.0);
                target[k - 1] = db_to_gain(db) * fade;
            }
            target
        };
        let mut target = partials(f0);
        if let Some(centre) = &line.centre_hz {
            let reference: f64 = partials(centre[start].max(1.0)).iter().map(|a| a * a).sum();
            let actual: f64 = target.iter().map(|a| a * a).sum();
            if actual > 0.0 {
                let scale = (reference / actual).sqrt();
                for a in target.iter_mut() {
                    *a *= scale;
                }
            }
        }
        let len = end - start;
        for k in 0..MAX_HARMONICS {
            let (a0, a1) = (current[k], target[k]);
            if a0 == 0.0 && a1 == 0.0 {
                // Keep the phase moving so a partial that returns stays continuous.
                for i in start..end {
                    phase[k] +=
                        std::f64::consts::TAU * (k + 1) as f64 * line.hz[i] * stretch[k] / rate;
                }
                phase[k] %= std::f64::consts::TAU;
                continue;
            }
            for (j, i) in (start..end).enumerate() {
                let a = a0 + (a1 - a0) * (j as f64 / len as f64);
                phase[k] += std::f64::consts::TAU * (k + 1) as f64 * line.hz[i] * stretch[k] / rate;
                out[i] += a * line.amp[i] * phase[k].sin();
            }
            phase[k] %= std::f64::consts::TAU;
            current[k] = a1;
        }
        start = end;
        block += 1;
    }
    // Noise: white through the profile's band, set against the tone's RMS where it sounds fully.
    let peak_amp = line.amp.iter().fold(0.0f64, |p, &a| p.max(a));
    if peak_amp > 0.0 && line.noise.iter().any(|&v| v > 0.0) {
        let full: Vec<usize> = (0..n).filter(|&i| line.amp[i] > 0.7 * peak_amp).collect();
        let tone_rms = if full.is_empty() {
            1.0
        } else {
            (full
                .iter()
                .map(|&i| (out[i] / line.amp[i] * peak_amp).powi(2))
                .sum::<f64>()
                / full.len() as f64)
                .sqrt()
        };
        let mut noise = white(rng, n);
        let (lo, hi) = profile.noise_band;
        for filter in [
            Biquad::highpass(rate, lo, 0.707),
            Biquad::highpass(rate, lo, 0.707),
            Biquad::lowpass(rate, hi.min(0.45 * rate), 0.707),
            Biquad::lowpass(rate, hi.min(0.45 * rate), 0.707),
        ] {
            let mut f = filter;
            f.run_all(&mut noise);
        }
        let noise_rms = (noise.iter().map(|v| v * v).sum::<f64>() / n as f64)
            .sqrt()
            .max(1e-300);
        let gain = tone_rms / peak_amp * db_to_gain(profile.noise_db) / noise_rms;
        for i in 0..n {
            out[i] += noise[i] * gain * line.noise[i];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The f0 a rendered tone has is the one it was made from: measured by the zero crossings of
    /// its fundamental alone.
    #[test]
    fn a_tone_sounds_at_its_f0() {
        let rate = 48_000.0;
        let line = Line::from_fn(rate, 1.0, |_| (220.0, 1.0, 0.0, 0.5));
        let mut only_fundamental = line;
        only_fundamental.only = Some(|k| k == 1);
        let out = render(&profile("cello"), &only_fundamental, rate, &mut Rng::new(1));
        let crossings: Vec<f64> = (1..out.len())
            .filter(|&i| out[i - 1] < 0.0 && out[i] >= 0.0)
            .map(|i| (i - 1) as f64 + out[i - 1] / (out[i - 1] - out[i]))
            .collect();
        let period = (crossings[crossings.len() - 1] - crossings[0]) / (crossings.len() - 1) as f64;
        assert!((rate / period - 220.0).abs() < 0.01, "{}", rate / period);
    }

    #[test]
    fn rendering_is_seeded() {
        let rate = 48_000.0;
        let line = Line::from_fn(rate, 0.3, |t| (300.0 + 50.0 * t, 0.5, 1.0, 0.5));
        let a = render(&profile("voice"), &line, rate, &mut Rng::new(3));
        let b = render(&profile("voice"), &line, rate, &mut Rng::new(3));
        assert_eq!(a, b);
    }
}
