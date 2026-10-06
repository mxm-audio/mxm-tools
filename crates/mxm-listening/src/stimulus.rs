//! Stimuli with sidecars (the plan's §2 item 10): what an instrument or an effect is played, and a
//! sidecar beside it recording exactly what that was, so `respond` can read the response back against
//! it.
//!
//! A stimulus is rendered from its sidecar alone, deterministically. The sidecar carries a fingerprint
//! of the samples, and `respond` renders the stimulus again and refuses a sidecar whose fingerprint
//! this build does not reproduce: a response is never read against a signal other than the one played.
//! An instrument's or effect's own crate renders a stimulus in-process as a dev-dependency, or the owner
//! plays the file through a host; either way the response is read against the sidecar.
//!
//! The kinds and their sources:
//! - an **impulse**: one sample, for an effect whose response is short and clean;
//! - an **exponential sine sweep** after Farina (AES 108th Convention, 2000): `sin(K·(e^(t/L) − 1))`
//!   with `K = ω₁·L`, `L = T / ln(ω₂/ω₁)`, spending equal time in every octave, faded in and out over
//!   [`FADE_S`];
//! - **tone bursts**: whole cycles of a sine starting at zero phase, repeated;
//! - **level steps**: a sine stepping through levels, for a compressor's static curve and its attack
//!   and release;
//! - a **steady sine**, and **two sines** (SMPTE's 60 Hz and 7 kHz at 4:1 by default) for distortion
//!   and intermodulation;
//! - **white noise** from `mxm-measure`'s seeded generator, never the clock;
//! - **silence**, for an effect's noise and runaway;
//! - a **note** — no audio at all: the key, velocity and timing an instrument is to play.
//!
//! A sidecar may also carry **settings**: what the device was set to when it played the stimulus
//! (`cutoff_hz 1000`, `resonance 0.9`). `respond` reports them, and tabulates readings against them
//! when several responses differ in a setting — a filter's peak against its cutoff, say.

use std::fmt::Write as _;

/// The sidecar's first line; its number changes whenever a kind renders differently.
pub const SIDECAR_HEADER: &str = "# mxm-listening stimulus 1";

/// Every stimulus fades in and out over this long (a raised cosine), so its ends do not click. Not
/// the impulse, the bursts or the steps, whose edges are what is measured.
pub const FADE_S: f64 = 0.005;

/// Which input channels a stimulus drives. `Both` is written as one mono channel, which a host plays
/// on both inputs; `Left` and `Right` as two channels with the other silent — the columns of a true-
/// stereo effect's matrix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Both,
    Left,
    Right,
}

impl Channel {
    fn name(self) -> &'static str {
        match self {
            Channel::Both => "both",
            Channel::Left => "left",
            Channel::Right => "right",
        }
    }

    /// Parses `both`, `left` or `right`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "both" | "mono" => Some(Channel::Both),
            "left" | "l" => Some(Channel::Left),
            "right" | "r" => Some(Channel::Right),
            _ => None,
        }
    }
}

/// What is played.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Impulse,
    /// Farina's exponential sweep from `from_hz` to `to_hz` over the stimulus's `seconds`.
    Sweep {
        from_hz: f64,
        to_hz: f64,
    },
    /// `cycles` whole cycles of a sine at `hz`, starting every `every_s`.
    Bursts {
        hz: f64,
        cycles: f64,
        every_s: f64,
    },
    /// A sine at `hz` stepping through `levels_db`, each relative to the stimulus's level and lasting
    /// an equal share of its `seconds`.
    Steps {
        hz: f64,
        levels_db: Vec<f64>,
    },
    Sine {
        hz: f64,
    },
    /// A sine at `hz` at the stimulus's level, and one at `hz2`, `second_db` against it.
    DualSine {
        hz: f64,
        hz2: f64,
        second_db: f64,
    },
    Noise {
        seed: u64,
    },
    Silence,
    /// A note for an instrument to play: MIDI key and velocity, on at the lead, off `seconds` later.
    Note {
        key: u8,
        velocity: u8,
    },
}

impl Kind {
    /// The kind's name in a sidecar and on the command line.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Kind::Impulse => "impulse",
            Kind::Sweep { .. } => "sweep",
            Kind::Bursts { .. } => "bursts",
            Kind::Steps { .. } => "steps",
            Kind::Sine { .. } => "sine",
            Kind::DualSine { .. } => "dual-sine",
            Kind::Noise { .. } => "noise",
            Kind::Silence => "silence",
            Kind::Note { .. } => "note",
        }
    }

    /// The kind with its defaults: a 20 Hz–20 kHz sweep, 1 kHz sines and bursts of ten cycles every
    /// 250 ms, SMPTE's two tones (60 Hz and 7 kHz, the second 12 dB down), the steps a 30 dB jump up
    /// and back for attack and release and then 6 dB steps from −60 dB to the full level, and middle
    /// C at velocity 100.
    #[must_use]
    pub fn default_of(name: &str) -> Option<Self> {
        Some(match name {
            "impulse" => Kind::Impulse,
            "sweep" => Kind::Sweep {
                from_hz: 20.0,
                to_hz: 20_000.0,
            },
            "bursts" => Kind::Bursts {
                hz: 1000.0,
                cycles: 10.0,
                every_s: 0.25,
            },
            "steps" => Kind::Steps {
                hz: 1000.0,
                levels_db: default_steps(),
            },
            "sine" => Kind::Sine { hz: 1000.0 },
            "dual-sine" | "dual" => Kind::DualSine {
                hz: 60.0,
                hz2: 7000.0,
                second_db: -12.0,
            },
            "noise" => Kind::Noise { seed: 1 },
            "silence" => Kind::Silence,
            "note" => Kind::Note {
                key: 60,
                velocity: 100,
            },
            _ => return None,
        })
    }

    /// How long the signal lasts by default, s.
    #[must_use]
    pub fn default_seconds(&self) -> f64 {
        match self {
            Kind::Impulse => 0.0,
            Kind::Sweep { .. } | Kind::Steps { .. } => 10.0,
            Kind::Noise { .. } | Kind::Silence => 5.0,
            Kind::Note { .. } => 1.0,
            _ => 2.0,
        }
    }
}

/// −30, 0 and −30 dB for a jump each way, then −60 to 0 dB in 6 dB steps.
fn default_steps() -> Vec<f64> {
    let mut v = vec![-30.0, 0.0, -30.0];
    v.extend((0..=10).map(|k| -60.0 + 6.0 * f64::from(k)));
    v
}

/// A stimulus: what is played, how loud, and the silence around it.
#[derive(Clone, Debug, PartialEq)]
pub struct Stimulus {
    pub kind: Kind,
    pub rate: u32,
    /// The signal's peak, dBFS: a sine's amplitude, the sweep's, the impulse's height, the loudest
    /// step's; noise's largest possible sample.
    pub level_dbfs: f64,
    /// Silence before the signal, s.
    pub lead_s: f64,
    /// The signal, s.
    pub seconds: f64,
    /// Silence after it, for a decay to ring out, s.
    pub tail_s: f64,
    pub channel: Channel,
    /// What the device was set to, `(name, value)`, in the order given.
    pub settings: Vec<(String, String)>,
}

impl Stimulus {
    /// A stimulus of `kind` with its default length, at −6 dBFS, 48 kHz, half a second of lead and two
    /// of tail, on both channels.
    #[must_use]
    pub fn new(kind: Kind) -> Self {
        let seconds = kind.default_seconds();
        Self {
            kind,
            rate: 48_000,
            level_dbfs: -6.0,
            lead_s: 0.5,
            seconds,
            tail_s: 2.0,
            channel: Channel::Both,
            settings: Vec::new(),
        }
    }

    fn samples(&self, s: f64) -> usize {
        (s * f64::from(self.rate)).round().max(0.0) as usize
    }

    /// The first sample of the signal.
    #[must_use]
    pub fn start(&self) -> usize {
        self.samples(self.lead_s)
    }

    /// The signal's length in samples; one for an impulse.
    #[must_use]
    pub fn signal_len(&self) -> usize {
        match self.kind {
            Kind::Impulse => 1,
            _ => self.samples(self.seconds),
        }
    }

    /// The whole stimulus's length in samples, lead and tail included.
    #[must_use]
    pub fn len(&self) -> usize {
        self.start() + self.signal_len() + self.samples(self.tail_s)
    }

    /// Whether it lasts no samples at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The step boundaries of a [`Kind::Steps`], as `(first sample, end, level dB re full scale)`.
    #[must_use]
    pub fn steps(&self) -> Vec<(usize, usize, f64)> {
        let Kind::Steps { levels_db, .. } = &self.kind else {
            return Vec::new();
        };
        let n = levels_db.len().max(1);
        let each = self.signal_len() / n;
        levels_db
            .iter()
            .enumerate()
            .map(|(k, db)| {
                (
                    self.start() + k * each,
                    self.start() + (k + 1) * each,
                    self.level_dbfs + db,
                )
            })
            .collect()
    }

    /// The signal as one channel, lead and tail included; silence throughout for a note, which an
    /// instrument plays rather than hears.
    #[must_use]
    pub fn render(&self) -> Vec<f64> {
        let rate = f64::from(self.rate);
        let peak = 10f64.powf(self.level_dbfs / 20.0);
        let mut out = vec![0.0; self.len()];
        let start = self.start();
        let len = self.signal_len();
        let tau = std::f64::consts::TAU;
        let signal = &mut out[start..start + len];
        match &self.kind {
            Kind::Impulse => signal[0] = peak,
            Kind::Sweep { from_hz, to_hz } => {
                let t_total = self.seconds;
                let l = t_total / (to_hz / from_hz).ln();
                let k = tau * from_hz * l;
                for (n, v) in signal.iter_mut().enumerate() {
                    let t = n as f64 / rate;
                    *v = peak * (k * ((t / l).exp() - 1.0)).sin();
                }
                fade(signal, self.samples(FADE_S));
            }
            Kind::Bursts {
                hz,
                cycles,
                every_s,
            } => {
                let every = self.samples(*every_s).max(1);
                let on = ((cycles / hz) * rate).round() as usize;
                for (n, v) in signal.iter_mut().enumerate() {
                    let within = n % every;
                    if within < on {
                        *v = peak * (tau * hz * within as f64 / rate).sin();
                    }
                }
            }
            Kind::Steps { hz, .. } => {
                for (a, b, db) in self.steps() {
                    let amp = 10f64.powf(db / 20.0);
                    for (n, v) in out.iter_mut().enumerate().take(b).skip(a) {
                        *v = amp * (tau * hz * (n - start) as f64 / rate).sin();
                    }
                }
                return out;
            }
            Kind::Sine { hz } => {
                for (n, v) in signal.iter_mut().enumerate() {
                    *v = peak * (tau * hz * n as f64 / rate).sin();
                }
                fade(signal, self.samples(FADE_S));
            }
            Kind::DualSine { hz, hz2, second_db } => {
                // The two amplitudes share the peak, as a two-tone test sets them.
                let ratio = 10f64.powf(second_db / 20.0);
                let (a, b) = (peak / (1.0 + ratio), peak * ratio / (1.0 + ratio));
                for (n, v) in signal.iter_mut().enumerate() {
                    let t = n as f64 / rate;
                    *v = a * (tau * hz * t).sin() + b * (tau * hz2 * t).sin();
                }
                fade(signal, self.samples(FADE_S));
            }
            Kind::Noise { seed } => {
                let noise = mxm_measure::stimulus::noise(len, *seed, 1.0);
                for (v, s) in signal.iter_mut().zip(noise) {
                    *v = peak * f64::from(s);
                }
                fade(signal, self.samples(FADE_S));
            }
            Kind::Silence | Kind::Note { .. } => {}
        }
        out
    }

    /// The signal as it is written: one channel for `Both`, two (left, right) otherwise, as `f32`.
    #[must_use]
    pub fn channels(&self) -> Vec<Vec<f32>> {
        let mono: Vec<f32> = self.render().iter().map(|&v| v as f32).collect();
        let silent = vec![0.0f32; mono.len()];
        match self.channel {
            Channel::Both => vec![mono],
            Channel::Left => vec![mono, silent],
            Channel::Right => vec![silent, mono],
        }
    }

    /// A 64-bit FNV-1a hash of the written samples' bits, frame by frame, as hex.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        let channels = self.channels();
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let frames = channels.first().map_or(0, Vec::len);
        for n in 0..frames {
            for c in &channels {
                for b in c[n].to_bits().to_le_bytes() {
                    h ^= u64::from(b);
                    h = h.wrapping_mul(0x0000_0100_0000_01b3);
                }
            }
        }
        format!("{h:016x}")
    }

    /// The sidecar's text.
    #[must_use]
    pub fn sidecar(&self) -> String {
        let mut s = String::new();
        s.push_str(SIDECAR_HEADER);
        s.push('\n');
        s.push_str(
            "# What was played. `listen respond` reads a response against it and renders it again to\n\
             # check the fingerprint: edit only the settings, what the device was set to.\n",
        );
        let _ = writeln!(s, "kind\t{}", self.kind.name());
        let _ = writeln!(s, "rate\t{}", self.rate);
        let _ = writeln!(s, "level_dbfs\t{}", self.level_dbfs);
        let _ = writeln!(s, "lead_s\t{}", self.lead_s);
        let _ = writeln!(s, "seconds\t{}", self.seconds);
        let _ = writeln!(s, "tail_s\t{}", self.tail_s);
        let _ = writeln!(s, "channel\t{}", self.channel.name());
        match &self.kind {
            Kind::Sweep { from_hz, to_hz } => {
                let _ = writeln!(s, "from_hz\t{from_hz}\nto_hz\t{to_hz}");
            }
            Kind::Bursts {
                hz,
                cycles,
                every_s,
            } => {
                let _ = writeln!(s, "hz\t{hz}\ncycles\t{cycles}\nevery_s\t{every_s}");
            }
            Kind::Steps { hz, levels_db } => {
                let levels: Vec<String> = levels_db.iter().map(f64::to_string).collect();
                let _ = writeln!(s, "hz\t{hz}\nlevels_db\t{}", levels.join(","));
            }
            Kind::Sine { hz } => {
                let _ = writeln!(s, "hz\t{hz}");
            }
            Kind::DualSine { hz, hz2, second_db } => {
                let _ = writeln!(s, "hz\t{hz}\nhz2\t{hz2}\nsecond_db\t{second_db}");
            }
            Kind::Noise { seed } => {
                let _ = writeln!(s, "seed\t{seed}");
            }
            Kind::Note { key, velocity } => {
                let _ = writeln!(s, "key\t{key}\nvelocity\t{velocity}");
            }
            Kind::Impulse | Kind::Silence => {}
        }
        let _ = writeln!(s, "fingerprint\t{}", self.fingerprint());
        for (name, value) in &self.settings {
            let _ = writeln!(s, "setting\t{name}\t{value}");
        }
        s
    }

    /// Reads a sidecar, and renders the stimulus again to check its fingerprint.
    ///
    /// # Errors
    /// A missing or unknown field, a value that does not parse, or a fingerprint this build does not
    /// reproduce.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        if lines.next().map(str::trim_end) != Some(SIDECAR_HEADER) {
            return Err(format!(
                "not a stimulus sidecar: its first line is not `{SIDECAR_HEADER}`"
            ));
        }
        let mut fields: Vec<(String, String)> = Vec::new();
        let mut settings = Vec::new();
        for line in lines {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.splitn(3, '\t');
            let key = parts.next().unwrap_or_default();
            let value = parts
                .next()
                .ok_or_else(|| format!("`{key}` has no value"))?;
            if key == "setting" {
                let v = parts
                    .next()
                    .ok_or_else(|| format!("setting `{value}` has no value"))?;
                settings.push((value.to_string(), v.to_string()));
            } else {
                fields.push((key.to_string(), value.to_string()));
            }
        }
        let get = |k: &str| -> Result<&str, String> {
            fields
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
                .ok_or_else(|| format!("the sidecar has no `{k}`"))
        };
        let num = |k: &str| -> Result<f64, String> {
            let v = get(k)?;
            v.parse::<f64>()
                .ok()
                .filter(|x| x.is_finite())
                .ok_or_else(|| format!("`{k}` is not a number: {v}"))
        };
        let kind_name = get("kind")?;
        let kind = match kind_name {
            "impulse" => Kind::Impulse,
            "sweep" => Kind::Sweep {
                from_hz: num("from_hz")?,
                to_hz: num("to_hz")?,
            },
            "bursts" => Kind::Bursts {
                hz: num("hz")?,
                cycles: num("cycles")?,
                every_s: num("every_s")?,
            },
            "steps" => Kind::Steps {
                hz: num("hz")?,
                levels_db: get("levels_db")?
                    .split(',')
                    .map(|v| {
                        v.trim()
                            .parse::<f64>()
                            .map_err(|_| format!("a level is not a number: {v}"))
                    })
                    .collect::<Result<Vec<f64>, String>>()?,
            },
            "sine" => Kind::Sine { hz: num("hz")? },
            "dual-sine" => Kind::DualSine {
                hz: num("hz")?,
                hz2: num("hz2")?,
                second_db: num("second_db")?,
            },
            "noise" => Kind::Noise {
                seed: get("seed")?
                    .parse()
                    .map_err(|_| "`seed` is not a whole number".to_string())?,
            },
            "silence" => Kind::Silence,
            "note" => Kind::Note {
                key: get("key")?
                    .parse()
                    .map_err(|_| "`key` is not a MIDI key".to_string())?,
                velocity: get("velocity")?
                    .parse()
                    .map_err(|_| "`velocity` is not a MIDI velocity".to_string())?,
            },
            other => return Err(format!("unknown stimulus kind `{other}`")),
        };
        let stimulus = Self {
            kind,
            rate: get("rate")?
                .parse()
                .map_err(|_| "`rate` is not a whole number".to_string())?,
            level_dbfs: num("level_dbfs")?,
            lead_s: num("lead_s")?,
            seconds: num("seconds")?,
            tail_s: num("tail_s")?,
            channel: Channel::parse(get("channel")?)
                .ok_or_else(|| "`channel` is not both, left or right".to_string())?,
            settings,
        };
        stimulus.check()?;
        let written = get("fingerprint")?;
        let now = stimulus.fingerprint();
        if written != now {
            return Err(format!(
                "the sidecar's fingerprint {written} is not this build's rendering of it ({now}): the stimulus was edited, or renders differently now"
            ));
        }
        Ok(stimulus)
    }

    /// Whether the stimulus can be rendered: a known rate, lengths that are not negative, frequencies
    /// inside the band.
    ///
    /// # Errors
    /// Names the first thing wrong.
    pub fn check(&self) -> Result<(), String> {
        let nyquist = f64::from(self.rate) / 2.0;
        if !(8000..=384_000).contains(&self.rate) {
            return Err(format!("a rate of {} Hz is outside 8–384 kHz", self.rate));
        }
        if self.lead_s < 0.0 || self.seconds < 0.0 || self.tail_s < 0.0 || self.seconds > 600.0 {
            return Err("lead, length and tail must be between 0 and 600 s".into());
        }
        if self.level_dbfs > 0.0 {
            return Err("the level is above full scale".into());
        }
        let band = |hz: f64| hz > 0.0 && hz < nyquist;
        let ok = match &self.kind {
            Kind::Sweep { from_hz, to_hz } => band(*from_hz) && band(*to_hz) && from_hz < to_hz,
            Kind::Bursts {
                hz,
                cycles,
                every_s,
            } => band(*hz) && *cycles > 0.0 && *every_s > 0.0,
            Kind::Steps { hz, levels_db } => {
                band(*hz) && !levels_db.is_empty() && levels_db.iter().all(|d| *d <= 0.0)
            }
            Kind::Sine { hz } => band(*hz),
            Kind::DualSine { hz, hz2, .. } => band(*hz) && band(*hz2) && hz != hz2,
            Kind::Note { key, velocity } => *key <= 127 && *velocity <= 127,
            Kind::Impulse | Kind::Noise { .. } | Kind::Silence => true,
        };
        if ok {
            Ok(())
        } else {
            Err(format!(
                "the {} is outside what can be played at {} Hz (a frequency at or above Nyquist, a step above full scale, or an empty list)",
                self.kind.name(),
                self.rate
            ))
        }
    }

    /// The note's key as a frequency, equal temperament at A = 440 Hz; `None` for other kinds.
    #[must_use]
    pub fn note_hz(&self) -> Option<f64> {
        match self.kind {
            Kind::Note { key, .. } => Some(440.0 * 2f64.powf((f64::from(key) - 69.0) / 12.0)),
            _ => None,
        }
    }
}

/// A raised-cosine fade over the first and last `n` samples.
fn fade(x: &mut [f64], n: usize) {
    let n = n.min(x.len() / 2);
    let len = x.len();
    for k in 0..n {
        let g = 0.5 - 0.5 * (std::f64::consts::PI * k as f64 / n as f64).cos();
        x[k] *= g;
        x[len - 1 - k] *= g;
    }
}
