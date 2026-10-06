//! Perturbations of a real sound: the calibration's trials (the plan's §5), and a known change for any
//! test that wants one. Each operator changes one thing by an amount in its own parameter, names the
//! reading that change moves, and — except the level operator itself — leaves the body loudness where
//! it was, so a louder candidate cannot be picked by loudness alone.
//!
//! The parameter grows with the difference: a staircase walks it on a log scale, and the owner's
//! threshold in the reading's own unit is read afterwards by measuring the candidate at the
//! staircase's answer (`session`), so an operator never has to know its reading's formula.
//!
//! **Techniques:** band-limited resampling (`repr::resample`); an octave's level moved by adding its
//! own zero-phase band (`repr::bands`); noise added in the wires' band under the sound's own envelope
//! there; amplitude modulation of the ring's band; a raised-cosine fade over the attack; a decay
//! changed by an exponential gain from the onset. For a **note** (the plan's revision 29, part C): a
//! zero-phase gain rising by octaves above the fundamental (its tilt); a gain falling with time and
//! frequency, frame by frame over a short-time Fourier transform (its decay law); each harmonic's band,
//! as an analytic signal, shifted in frequency by the stiff-string law (its inharmonicity); and a
//! sinusoidally swept delay read by cubic interpolation (a vibrato).

use crate::describe::{Options, describe_with};
use crate::family::Family;
use crate::prep;
use crate::repr::{bands, envelope};
use crate::sound::Sound;

/// What a perturbation changes. The parameter `p` is always positive, and larger is more different.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Operator {
    /// Louder by `p` dB. Reading: `level.body_loudness`.
    Level,
    /// Higher by `p` cents, everything read faster (a drum retuned). Reading: `pitch.rest`.
    Pitch,
    /// The decay shorter: its rate raised by `p` % from the onset on. Reading: `pitch.rest_t60`.
    DecayShorter,
    /// One band up by `p` dB. Reading: `tone.band_level` in the octave starting at `lo`.
    Band { lo: f64, hi: f64 },
    /// A softer attack: a raised-cosine fade over the first `p` ms. Reading: `attack.rise_time`.
    AttackSofter,
    /// More noise in 2–8 kHz, under the sound's own envelope there: that band's energy up by `p` dB.
    /// Reading: `texture.buzz_share`.
    Noise,
    /// The ring wobbling at 20 Hz, `p` % deep. Reading: `modulation.ring_wobble`.
    Wobble,
    /// A one-sample click at the onset, `p` % of the peak. Reading: `attack.first_step`.
    Click,
    /// A note's overtones up by `p` dB for each octave above its fundamental. Reading: `note.tilt`.
    Tilt,
    /// A note's upper partials dying sooner: `M` in T60 ∝ (f/f₁)^M lowered by `p`/100. Reading:
    /// `note.decay_law`.
    DecayLaw,
    /// A harmonic note stretched as a stiffer string: each harmonic `n` moved by √(1 + B·n²), B set so
    /// the tenth moves `p` cents. Reading: `note.inharmonicity`.
    Inharmonicity,
    /// A vibrato of `p` cents at [`VIBRATO_HZ`]. Reading: `sustain.vibrato_depth`.
    Vibrato,
}

impl Operator {
    /// A stable name for logs and the command line.
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            Self::Level => "level".into(),
            Self::Pitch => "pitch".into(),
            Self::DecayShorter => "decay".into(),
            Self::Band { lo, hi } => format!("band-{lo:.0}-{hi:.0}"),
            Self::AttackSofter => "attack".into(),
            Self::Noise => "noise".into(),
            Self::Wobble => "wobble".into(),
            Self::Click => "click".into(),
            Self::Tilt => "tilt".into(),
            Self::DecayLaw => "decay-law".into(),
            Self::Inharmonicity => "inharmonicity".into(),
            Self::Vibrato => "vibrato".into(),
        }
    }

    /// Parses [`Operator::name`]'s form.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "level" => Self::Level,
            "pitch" => Self::Pitch,
            "decay" => Self::DecayShorter,
            "attack" => Self::AttackSofter,
            "noise" => Self::Noise,
            "wobble" => Self::Wobble,
            "click" => Self::Click,
            "tilt" => Self::Tilt,
            "decay-law" => Self::DecayLaw,
            "inharmonicity" => Self::Inharmonicity,
            "vibrato" => Self::Vibrato,
            other => {
                let rest = other.strip_prefix("band-")?;
                let (lo, hi) = rest.split_once('-')?;
                Self::Band {
                    lo: lo.parse().ok()?,
                    hi: hi.parse().ok()?,
                }
            }
        })
    }

    /// The reading this operator moves, and the band's lower edge where the reading has bands.
    #[must_use]
    pub fn reading(&self) -> (&'static str, Option<f64>) {
        match self {
            Self::Level => ("level.body_loudness", None),
            Self::Pitch => ("pitch.rest", None),
            Self::DecayShorter => ("pitch.rest_t60", None),
            Self::Band { lo, .. } => ("tone.band_level", Some(*lo)),
            Self::AttackSofter => ("attack.rise_time", None),
            Self::Noise => ("texture.buzz_share", None),
            Self::Wobble => ("modulation.ring_wobble", None),
            Self::Click => ("attack.first_step", None),
            Self::Tilt => ("note.tilt", None),
            Self::DecayLaw => ("note.decay_law", None),
            Self::Inharmonicity => ("note.inharmonicity", None),
            Self::Vibrato => ("sustain.vibrato_depth", None),
        }
    }

    /// Where a staircase starts, and the range it may walk: clearly audible to begin with.
    #[must_use]
    pub fn range(&self) -> (f64, f64, f64) {
        match self {
            Self::Level => (4.0, 0.05, 12.0),
            Self::Pitch => (60.0, 0.5, 400.0),
            Self::DecayShorter => (80.0, 1.0, 400.0),
            Self::Band { .. } => (8.0, 0.1, 24.0),
            Self::AttackSofter => (8.0, 0.1, 40.0),
            Self::Noise => (8.0, 0.1, 24.0),
            Self::Wobble => (30.0, 0.3, 100.0),
            Self::Click => (30.0, 0.1, 100.0),
            Self::Tilt => (6.0, 0.05, 24.0),
            Self::DecayLaw => (60.0, 1.0, 300.0),
            Self::Inharmonicity => (30.0, 0.3, 300.0),
            Self::Vibrato => (30.0, 0.5, 150.0),
        }
    }
}

/// A sound prepared for perturbation: what the operators need to know about it, measured once.
#[derive(Clone, Debug)]
pub struct Subject {
    pub sound: Sound,
    pub onset: usize,
    /// The ring's frequency (the rest pitch, else the strongest mode), Hz.
    pub ring_hz: Option<f64>,
    /// The ring's amplitude decay rate, 1/s.
    pub decay_per_s: Option<f64>,
    /// The body loudness the candidates are matched to.
    pub body: f32,
    pub family: Option<Family>,
    /// A note's fundamental, Hz, where the sound was read as a note.
    pub f0: Option<f64>,
    /// A note's fundamental's T60, s, and its decay law `M`.
    pub note_t60: Option<f64>,
    pub decay_law: Option<f64>,
    /// Whether the note has a harmonic series to stretch.
    pub series: bool,
}

impl Subject {
    /// Measures what the operators need.
    #[must_use]
    pub fn new(sound: Sound, family: Option<Family>) -> Self {
        let report = describe_with(
            &sound,
            &Options {
                family,
                without_perception: true,
                ..Options::default()
            },
        );
        let value = |id: &str| {
            report
                .sections
                .iter()
                .flat_map(|s| &s.readings)
                .find(|r| r.id == id)
                .and_then(|r| r.value)
        };
        let ring_hz = value("pitch.rest").or_else(|| value("pitch.strongest"));
        let decay_per_s = value("pitch.rest_t60")
            .filter(|t| *t > 0.0)
            .map(|t60_ms| 6.91 / (t60_ms / 1000.0))
            .or_else(|| {
                value("decay.late_slope")
                    .filter(|s| *s < 0.0)
                    .map(|db_per_s| -db_per_s * std::f64::consts::LN_10 / 20.0)
            });
        let onset = prep::onset(&sound.samples).unwrap_or(0);
        let body = prep::body_rms(&sound.samples, sound.rate);
        Self {
            onset,
            ring_hz,
            decay_per_s,
            body,
            family: Some(report.family.family),
            f0: value("note.fundamental"),
            note_t60: value("note.t60"),
            decay_law: value("note.decay_law"),
            series: value("note.inharmonicity").is_some(),
            sound,
        }
    }

    /// The sound changed by `op` at size `p`; `None` where the sound lacks what the operator needs
    /// (a ring to wobble, a decay to shorten).
    #[must_use]
    pub fn perturbed(&self, op: Operator, p: f64) -> Option<Sound> {
        let rate = f64::from(self.sound.rate);
        let x: Vec<f64> = self.sound.samples.iter().map(|&v| f64::from(v)).collect();
        let t = |j: usize| (j as f64 - self.onset as f64) / rate;
        let y: Vec<f64> = match op {
            Operator::Level => {
                let g = 10f64.powf(p / 20.0);
                return Some(self.named(x.iter().map(|v| v * g).collect(), op, p, false));
            }
            Operator::Pitch => crate::repr::resample::about(&x, self.onset, 2f64.powf(p / 1200.0)),
            Operator::DecayShorter => {
                let d = self.decay_per_s?;
                let extra = d * p / 100.0;
                x.iter()
                    .enumerate()
                    .map(|(j, v)| v * (-extra * t(j).max(0.0)).exp())
                    .collect()
            }
            Operator::Band { lo, hi } => {
                let b = bands::band(&x, rate, lo, hi.min(0.45 * rate))?;
                let g = 10f64.powf(p / 20.0) - 1.0;
                x.iter().zip(&b).map(|(v, w)| v + g * w).collect()
            }
            Operator::AttackSofter => {
                let fade = (p / 1000.0 * rate).max(1.0);
                x.iter()
                    .enumerate()
                    .map(|(j, v)| {
                        let u = ((j as f64 - self.onset as f64) / fade).clamp(0.0, 1.0);
                        v * (0.5 - 0.5 * (std::f64::consts::PI * u).cos())
                    })
                    .collect()
            }
            Operator::Noise => {
                let hi = 8000.0f64.min(0.45 * rate);
                let own = bands::band(&x, rate, 2000.0, hi)?;
                let noise = bands::band(&white(x.len()), rate, 2000.0, hi)?;
                let w = envelope::samples(0.005, rate).max(1);
                let (e_own, e_noise) = (
                    envelope::windowed_rms(&own, w),
                    envelope::windowed_rms(&noise, w),
                );
                let g = (10f64.powf(p / 10.0) - 1.0).sqrt();
                x.iter()
                    .enumerate()
                    .map(|(j, v)| {
                        let k = j / w;
                        let (a, b) = (
                            e_own.get(k).copied().unwrap_or(0.0),
                            e_noise.get(k).copied().unwrap_or(0.0),
                        );
                        v + if b > 0.0 { g * a / b * noise[j] } else { 0.0 }
                    })
                    .collect()
            }
            Operator::Wobble => {
                let f = self.ring_hz?;
                let ring = bands::band(&x, rate, 0.85 * f, (1.15 * f).min(0.45 * rate))?;
                let m = p / 100.0;
                x.iter()
                    .zip(&ring)
                    .enumerate()
                    .map(|(j, (v, r))| v + m * (std::f64::consts::TAU * WOBBLE_HZ * t(j)).sin() * r)
                    .collect()
            }
            Operator::Click => {
                let peak = f64::from(prep::peak(&self.sound.samples));
                let mut y = x.clone();
                if let Some(v) = y.get_mut(self.onset) {
                    *v += p / 100.0 * peak;
                }
                y
            }
            Operator::Tilt => {
                let f0 = self.f0?;
                let slope = p / 20.0;
                spectral_gain(&x, rate, |f| {
                    if f > f0 {
                        10f64.powf(slope * (f / f0).log2())
                    } else {
                        1.0
                    }
                })?
            }
            Operator::DecayLaw => {
                let (f0, t60, m) = (self.f0?, self.note_t60?, self.decay_law?);
                let sigma1 = 6.91 / t60;
                let change = p / 100.0;
                let onset = self.onset;
                timed_gain(&x, rate, |f, j| {
                    if f <= f0 {
                        return 1.0;
                    }
                    let r = f / f0;
                    let sigma = sigma1 * r.powf(-m);
                    let extra = sigma * (r.powf(change) - 1.0);
                    let t = (j as f64 - onset as f64).max(0.0) / rate;
                    (-extra * t).exp()
                })?
            }
            Operator::Inharmonicity => {
                let f0 = self.f0.filter(|_| self.series)?;
                let b = (2f64.powf(p / 600.0) - 1.0) / 100.0;
                stretch(&x, rate, f0, b)?
            }
            Operator::Vibrato => {
                let depth = (2f64.powf(p / 1200.0) - 1.0) / (std::f64::consts::TAU * VIBRATO_HZ);
                let d = depth * rate;
                (0..x.len())
                    .map(|j| {
                        let s = (std::f64::consts::TAU * VIBRATO_HZ * t(j)).sin();
                        cubic(&x, j as f64 - d * (1.0 + s))
                    })
                    .collect()
            }
        };
        Some(self.named(y, op, p, true))
    }

    /// The candidate as a sound, loudness-matched to the subject when `matched`.
    fn named(&self, y: Vec<f64>, op: Operator, p: f64, matched: bool) -> Sound {
        let mut s: Vec<f32> = y.iter().map(|&v| v as f32).collect();
        if matched {
            let b = prep::body_rms(&s, self.sound.rate);
            if b > 0.0 && self.body > 0.0 {
                let g = self.body / b;
                for v in &mut s {
                    *v *= g;
                }
            }
        }
        Sound::new(
            format!("{} ({} {p:.3})", self.sound.name, op.name()),
            self.sound.rate,
            s,
        )
    }
}

/// The wobble's rate, Hz: inside the ring's ±15 % band at any drum pitch above 130 Hz, which is all
/// the wobble reading hears.
pub const WOBBLE_HZ: f64 = 20.0;

/// A vibrato's rate, Hz: the middle of a player's 4.5 to 6.5.
pub const VIBRATO_HZ: f64 = 5.0;

/// `x` through a zero-phase gain `gain(hz)`, applied to its whole spectrum (zero-padded to twice its
/// length, so nothing wraps).
fn spectral_gain(x: &[f64], rate: f64, gain: impl Fn(f64) -> f64) -> Option<Vec<f64>> {
    let n = (2 * x.len()).next_power_of_two();
    let mut re = vec![0.0; n];
    let mut im = vec![0.0; n];
    re[..x.len()].copy_from_slice(x);
    mxm_measure::spectrum::fft(&mut re, &mut im)?;
    for k in 0..n {
        let f = k.min(n - k) as f64 * rate / n as f64;
        let g = gain(f);
        re[k] *= g;
        im[k] *= g;
    }
    mxm_measure::spectrum::ifft(&mut re, &mut im)?;
    re.truncate(x.len());
    Some(re)
}

/// `x` through a gain `gain(hz, sample)` changing with time: a short-time Fourier transform over
/// Hann frames of about 40 ms every quarter frame, each frame's spectrum scaled at its centre sample
/// and overlap-added, divided by the windows' sum (so a gain of one gives `x` back).
fn timed_gain(x: &[f64], rate: f64, gain: impl Fn(f64, usize) -> f64) -> Option<Vec<f64>> {
    let n = ((0.04 * rate) as usize).next_power_of_two();
    let hop = n / 4;
    let w: Vec<f64> = (0..n)
        .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos())
        .collect();
    let mut out = vec![0.0; x.len() + n];
    let mut sum = vec![0.0; x.len() + n];
    let mut start: isize = -(n as isize);
    while start < x.len() as isize {
        let mut re = vec![0.0; n];
        let mut im = vec![0.0; n];
        for i in 0..n {
            let j = start + i as isize;
            if j >= 0 && (j as usize) < x.len() {
                re[i] = x[j as usize] * w[i];
            }
        }
        mxm_measure::spectrum::fft(&mut re, &mut im)?;
        let centre = (start + n as isize / 2).max(0) as usize;
        for k in 0..n {
            let g = gain(k.min(n - k) as f64 * rate / n as f64, centre);
            re[k] *= g;
            im[k] *= g;
        }
        mxm_measure::spectrum::ifft(&mut re, &mut im)?;
        for i in 0..n {
            let j = start + i as isize;
            if j >= 0 {
                out[j as usize] += re[i];
                sum[j as usize] += w[i];
            }
        }
        start += hop as isize;
    }
    Some(
        (0..x.len())
            .map(|j| if sum[j] > 1e-9 { out[j] / sum[j] } else { x[j] })
            .collect(),
    )
}

/// A harmonic note stretched: each harmonic `n ≥ 2`'s band, `(n ± ½)·f0`, taken as an analytic signal
/// and shifted by `n·f0·(√(1 + b·n²) − 1)`. The bands tile the spectrum, so a stretch of zero gives
/// `x` back.
fn stretch(x: &[f64], rate: f64, f0: f64, b: f64) -> Option<Vec<f64>> {
    let n = (2 * x.len()).next_power_of_two();
    let mut xr = vec![0.0; n];
    let mut xi = vec![0.0; n];
    xr[..x.len()].copy_from_slice(x);
    mxm_measure::spectrum::fft(&mut xr, &mut xi)?;
    let bin = rate / n as f64;
    let mut y = x.to_vec();
    let mut h = 2usize;
    while (h as f64 + 0.5) * f0 < 0.45 * rate {
        let hf = h as f64;
        let (lo, hi) = ((hf - 0.5) * f0, (hf + 0.5) * f0);
        let (a, z) = (
            (lo / bin).ceil() as usize,
            ((hi / bin).ceil() as usize).min(n / 2),
        );
        let mut re = vec![0.0; n];
        let mut im = vec![0.0; n];
        for k in a..z {
            re[k] = 2.0 * xr[k];
            im[k] = 2.0 * xi[k];
        }
        mxm_measure::spectrum::ifft(&mut re, &mut im)?;
        let shift = hf * f0 * ((1.0 + b * hf * hf).sqrt() - 1.0);
        let step = std::f64::consts::TAU * shift / rate;
        for (j, v) in y.iter_mut().enumerate() {
            let (s, c) = (step * j as f64).sin_cos();
            *v += (re[j] * c - im[j] * s) - re[j];
        }
        h += 1;
    }
    Some(y)
}

/// `x` at a fractional position, by Catmull–Rom's cubic; silence outside it.
fn cubic(x: &[f64], at: f64) -> f64 {
    let i = at.floor();
    let f = at - i;
    let get = |k: f64| {
        let k = k as isize;
        if k < 0 || k as usize >= x.len() {
            0.0
        } else {
            x[k as usize]
        }
    };
    let (p0, p1, p2, p3) = (get(i - 1.0), get(i), get(i + 1.0), get(i + 2.0));
    p1 + 0.5
        * f
        * (p2 - p0 + f * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3 + f * (3.0 * (p1 - p2) + p3 - p0)))
}

/// Deterministic white noise, uniform in −1..1.
fn white(n: usize) -> Vec<f64> {
    let mut s = 0x9E37_79B9_7F4A_7C15u64;
    (0..n)
        .map(|_| {
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            (s.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 52) as f64 - 1.0
        })
        .collect()
}
