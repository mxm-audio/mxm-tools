//! The parts of a sound, each an analyser with its own report section (the plan's §2).
//!
//! L0 reads a percussive hit's **attack**, **level**, **decay** and **tone over time**. Every window is
//! measured from the onset (`prep::onset`), and every level against the sound's own peak unless a
//! reading says otherwise.

pub mod artefacts;
pub mod attack;
pub mod decay;
pub mod effect;
pub mod impulse;
pub mod level;
pub mod modulation;
pub mod note;
pub mod perception;
pub mod pitch;
pub mod sustain;
pub mod texture;
pub mod tonality;
pub mod tone;
pub mod voice;
pub mod words;

use crate::repr::envelope;

/// One sound, prepared for the analysers.
#[derive(Clone, Debug)]
pub struct Context {
    /// The whole file, mono, in `f64`.
    pub x: Vec<f64>,
    /// The same samples as decoded, for the page-defined measures that work in `f32`.
    pub raw: Vec<f32>,
    pub rate: f64,
    /// The onset sample.
    pub onset: usize,
    /// The largest absolute sample, and where it is.
    pub peak: f64,
    pub peak_at: usize,
}

impl Context {
    /// `None` for silence, or for a buffer holding a non-finite sample (a DSP failure is never
    /// laundered into a measurement).
    #[must_use]
    pub fn new(samples: &[f32], rate: u32) -> Option<Self> {
        let peak = mxm_measure::level::peak(samples)?;
        if peak.is_nan() || peak <= 0.0 {
            return None;
        }
        let onset = crate::prep::onset(samples)?;
        let x: Vec<f64> = samples.iter().map(|&s| f64::from(s)).collect();
        let peak_at = x
            .iter()
            .enumerate()
            .fold(
                (0, 0.0f64),
                |(i, m), (j, s)| if s.abs() > m { (j, s.abs()) } else { (i, m) },
            )
            .0;
        Some(Self {
            x,
            raw: samples.to_vec(),
            rate: f64::from(rate),
            onset,
            peak: f64::from(peak),
            peak_at,
        })
    }

    /// The sample `ms` milliseconds after the onset.
    #[must_use]
    pub fn at(&self, ms: f64) -> usize {
        self.onset + envelope::samples(ms / 1000.0, self.rate)
    }

    /// The samples from `from_ms` to `to_ms` after the onset; `None` if the window starts past the end
    /// or would be empty. A window running past the end is cut there.
    #[must_use]
    pub fn window(&self, from_ms: f64, to_ms: f64) -> Option<&[f64]> {
        let a = self.at(from_ms);
        let b = self.at(to_ms).min(self.x.len());
        (a < b).then(|| &self.x[a..b])
    }

    /// The window is wholly inside the file.
    #[must_use]
    pub fn covers(&self, to_ms: f64) -> bool {
        self.at(to_ms) <= self.x.len()
    }

    /// The RMS level of a window against the peak, dB.
    #[must_use]
    pub fn level_db(&self, from_ms: f64, to_ms: f64) -> Option<f64> {
        let w = self.window(from_ms, to_ms)?;
        envelope::db(envelope::rms(w)?, self.peak)
    }

    /// The samples from the onset to the end.
    #[must_use]
    pub fn from_onset(&self) -> &[f64] {
        &self.x[self.onset..]
    }

    /// Milliseconds per sample.
    #[must_use]
    pub fn ms(&self, samples: usize) -> f64 {
        samples as f64 / self.rate * 1000.0
    }
}
