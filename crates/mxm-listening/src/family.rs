//! What kind of sound this is. The family picks the analysers and, from L2, the vocabulary.
//!
//! L0 knows the **percussive hit**, the owner's first priority (the plan's revision 2). L5a adds the
//! **pitched note** — struck or plucked, decaying: tuned percussion, bells, strings — which is
//! **declared, never detected**: a struck note's envelope is a hit's (a xylophone note passes every
//! test a drum does), so detecting notes would move drums out of the percussive family and change
//! their readings. A **synth voice** and an **impulse response** are declared too, and an **effect response** comes only from
//! `respond`, which knows the stimulus it answers.
//! A **sustained note** — held by a bow or a breath — is declared the same way. Anything the detector
//! cannot place is `Unknown`, analysed with the percussive parts and flagged,
//! so a report never pretends to know more than it does.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// One struck event: a fast rise to a single peak, then a decay.
    Percussive,
    /// A pitched note, struck or plucked, left to decay: its partials and the laws they follow.
    Note,
    /// A pitched note held by a bow or a breath: a note's partials, and how its pitch holds.
    Sustained,
    /// A synth voice's note: a held note's readings and a voice's own — aliasing, envelope stages,
    /// clicks, zipper, a unison's beating. Declared, as a note is.
    Voice,
    /// An impulse response — a recorded one, or one `respond` deconvolved from a sweep, an impulse
    /// or noise: its frequency response, taps and space.
    Impulse,
    /// An effect's response to a stimulus other than an impulse, read against the stimulus's sidecar
    /// (`respond`): never declared on a file alone, which cannot say what was played.
    Effect,
    Unknown,
}

impl Family {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Family::Percussive => "percussive hit",
            Family::Note => "pitched note",
            Family::Sustained => "sustained note",
            Family::Voice => "synth voice",
            Family::Impulse => "impulse response",
            Family::Effect => "effect response",
            Family::Unknown => "unknown",
        }
    }

    /// Parses a caller's declaration (`percussive`, `note`).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "percussive" | "percussion" | "hit" => Some(Family::Percussive),
            "note" | "pitched" | "pitched-note" => Some(Family::Note),
            "sustained" | "bowed" | "blown" => Some(Family::Sustained),
            "voice" | "synth" | "synth-voice" => Some(Family::Voice),
            "impulse" | "ir" | "impulse-response" => Some(Family::Impulse),
            _ => None,
        }
    }
}

impl fmt::Display for Family {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The family, and how it was decided.
#[derive(Clone, Debug, PartialEq)]
pub struct Detected {
    pub family: Family,
    /// `declared` by the caller, or `detected` with the evidence.
    pub how: String,
}

/// A percussive hit reaches its peak within this long after the onset.
const PEAK_WITHIN_MS: f64 = 60.0;
/// …and falls at least this far below its loudest 10 ms by 500 ms after the peak (or by its end).
const FALL_DB: f64 = 10.0;

/// Detects the family of a mono sound from its onset, peak time and fall. **Chosen** thresholds:
/// every hit in the drum corpus peaks inside 15 ms, and even a long-ringing drum (T60 near 1.5 s)
/// falls more than 10 dB in half a second, while a held note does neither. A first version asked 20 dB
/// and turned a 1.4 s ring away.
#[must_use]
pub fn detect(x: &[f64], rate: f64, onset: Option<usize>) -> Detected {
    let Some(onset) = onset else {
        return Detected {
            family: Family::Unknown,
            how: "detected: silent".into(),
        };
    };
    let (peak_at, _) =
        x.iter().enumerate().fold(
            (0, 0.0f64),
            |(i, m), (j, s)| if s.abs() > m { (j, s.abs()) } else { (i, m) },
        );
    let peak_ms = (peak_at.saturating_sub(onset)) as f64 / rate * 1000.0;
    let w = ((0.01 * rate) as usize).max(1);
    let level = |from: usize| -> Option<f64> {
        let seg = x.get(from..(from + w).min(x.len()))?;
        if seg.is_empty() {
            return None;
        }
        Some((seg.iter().map(|s| s * s).sum::<f64>() / seg.len() as f64).sqrt())
    };
    let loud = level(peak_at.saturating_sub(w / 2)).unwrap_or(0.0);
    let later = peak_at + (0.5 * rate) as usize;
    let late = level(later.min(x.len().saturating_sub(w))).unwrap_or(0.0);
    let fall_db = if late > 0.0 && loud > 0.0 {
        20.0 * (loud / late).log10()
    } else {
        f64::INFINITY
    };
    let percussive = peak_ms <= PEAK_WITHIN_MS && fall_db >= FALL_DB;
    Detected {
        family: if percussive {
            Family::Percussive
        } else {
            Family::Unknown
        },
        how: format!(
            "detected: peak {peak_ms:.1} ms after the onset, {} dB down 500 ms after the peak",
            if fall_db.is_finite() {
                format!("{fall_db:.0}")
            } else {
                "all the way".into()
            }
        ),
    }
}
