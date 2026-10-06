//! Audibility: how large a difference in each reading must be before it is worth reporting, and the
//! owner's words for each difference.
//!
//! Both are data, not code (the plan's §3–§4): `data/thresholds.tsv` and `data/vocabulary.tsv`, compiled
//! in so the command works anywhere. The owner's own thresholds from calibration (L3) are loaded over the
//! literature's and stay local.

use crate::reading::{Reading, Unit};

/// How a threshold is applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// In the reading's unit.
    Absolute,
    /// As a fraction of the reference's value.
    Relative,
}

/// One threshold and where it comes from.
#[derive(Clone, Debug, PartialEq)]
pub struct Threshold {
    pub kind: Kind,
    pub value: f64,
    pub source: String,
    /// For a relative threshold: the smallest value that is perceived at all (roughness's 0.07 asper);
    /// values under it compare as it, so a change between two imperceptible values is none.
    pub floor: Option<f64>,
}

impl Threshold {
    /// The size of the difference between a reference and a candidate value, in thresholds.
    #[must_use]
    pub fn units(&self, reference: f64, candidate: f64) -> f64 {
        match self.kind {
            Kind::Absolute => (candidate - reference).abs() / self.value,
            Kind::Relative => {
                if let Some(f) = self.floor.filter(|f| *f > 0.0) {
                    if reference >= 0.0 && candidate >= 0.0 {
                        return (candidate.max(f) / reference.max(f)).ln().abs()
                            / (1.0 + self.value).ln();
                    }
                }
                if reference > 0.0 && candidate > 0.0 {
                    (candidate / reference).ln().abs() / (1.0 + self.value).ln()
                } else {
                    // Negative quantities (decay rates in dB/s): a fraction of the reference's size.
                    let scale = reference.abs().max(1e-12);
                    (candidate - reference).abs() / (self.value * scale)
                }
            }
        }
    }
}

/// The thresholds table: rows of id, an optional context (a sound family's name), and the threshold.
#[derive(Clone, Debug, Default)]
pub struct Thresholds {
    by_id: Vec<(String, Option<String>, Threshold)>,
}

const LITERATURE: &str = include_str!("../data/thresholds.tsv");
const VOCABULARY: &str = include_str!("../data/vocabulary.tsv");

impl Thresholds {
    /// The literature's thresholds (`data/thresholds.tsv`).
    #[must_use]
    pub fn literature() -> Self {
        Self::parse(LITERATURE)
    }

    /// Parses a thresholds table (`id`, `abs` or `rel`, value, source, and optionally a context and a
    /// floor); malformed lines are skipped.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let by_id = text
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                if f.len() < 3 {
                    return None;
                }
                let kind = match f[1].trim() {
                    "abs" => Kind::Absolute,
                    "rel" => Kind::Relative,
                    _ => return None,
                };
                let value: f64 = f[2].trim().parse().ok().filter(|v: &f64| *v > 0.0)?;
                Some((
                    f[0].trim().to_string(),
                    f.get(4)
                        .map(|c| c.trim().to_string())
                        .filter(|c| !c.is_empty()),
                    Threshold {
                        kind,
                        value,
                        source: f.get(3).map_or(String::new(), |s| s.trim().to_string()),
                        floor: f
                            .get(5)
                            .and_then(|v| v.trim().parse::<f64>().ok())
                            .filter(|v| *v > 0.0),
                    },
                ))
            })
            .collect();
        Self { by_id }
    }

    /// Overlays another table: its rows win (the owner's over the literature's), each replacing the
    /// row with its id and context.
    #[must_use]
    pub fn overlaid(mut self, over: Self) -> Self {
        for (id, context, t) in over.by_id {
            self.by_id.retain(|(i, c, _)| !(i == &id && c == &context));
            self.by_id.push((id, context, t));
        }
        self
    }

    /// The literature's thresholds with the owner's from `dir/owner-thresholds.tsv` laid over them
    /// when that local table exists (the plan's §5 precedence), and a note saying so. `dir` is the
    /// local state folder, `.listening` by default; the table never leaves the owner's machine.
    #[must_use]
    pub fn with_owner(dir: &std::path::Path) -> (Self, Option<String>) {
        let literature = Self::literature();
        match std::fs::read_to_string(dir.join("owner-thresholds.tsv")) {
            Ok(text) => {
                let owner = Self::parse(&text);
                let n = owner.len();
                (
                    literature.overlaid(owner),
                    Some(format!(
                        "the owner's own thresholds ({n} readings, from calibration) are used where they exist"
                    )),
                )
            }
            Err(_) => (literature, None),
        }
    }

    /// How many rows the table has.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// The threshold for a reading in any context: its own row, else its unit's.
    #[must_use]
    pub fn for_reading(&self, id: &str, unit: Unit) -> Option<&Threshold> {
        let by_unit = format!("*:{}", unit.symbol());
        self.by_id
            .iter()
            .find(|(i, c, _)| i == id && c.is_none())
            .or_else(|| {
                self.by_id
                    .iter()
                    .find(|(i, c, _)| *i == by_unit && c.is_none())
            })
            .map(|(_, _, t)| t)
    }

    /// The threshold for a reading of a sound in `context` (its family's name): a row for that
    /// context first, then [`Thresholds::for_reading`] — the plan's §5 precedence, the owner's own in
    /// the same context, then the owner's in any, then the literature's.
    #[must_use]
    pub fn for_reading_in(
        &self,
        id: &str,
        unit: Unit,
        context: Option<&str>,
    ) -> Option<&Threshold> {
        context
            .and_then(|ctx| {
                self.by_id
                    .iter()
                    .find(|(i, c, _)| i == id && c.as_deref() == Some(ctx))
                    .map(|(_, _, t)| t)
            })
            .or_else(|| self.for_reading(id, unit))
    }
}

/// One vocabulary rule.
#[derive(Clone, Debug, PartialEq)]
struct Rule {
    id: String,
    window: Option<f64>,
    band: Option<f64>,
    /// +1, −1, or 0 for either direction.
    direction: i8,
    phrase: String,
}

/// What a phrase has meant: a vocabulary row that holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct Meaning {
    pub id: String,
    /// The window's start (ms) and the band's lower edge (Hz) the row is tied to, where it is.
    pub window: Option<f64>,
    pub band: Option<f64>,
    /// +1 the candidate higher, −1 lower, 0 either.
    pub direction: i8,
    pub phrase: String,
}

/// The owner's vocabulary.
#[derive(Clone, Debug, Default)]
pub struct Vocabulary {
    rules: Vec<Rule>,
}

impl Vocabulary {
    /// The vocabulary (`data/vocabulary.tsv`).
    #[must_use]
    pub fn owner() -> Self {
        Self::parse(VOCABULARY)
    }

    #[must_use]
    pub fn parse(text: &str) -> Self {
        let rules = text
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                if f.len() < 5 {
                    return None;
                }
                let any = |s: &str| -> Option<Option<f64>> {
                    match s.trim() {
                        "*" => Some(None),
                        v => v.parse().ok().map(Some),
                    }
                };
                Some(Rule {
                    id: f[0].trim().to_string(),
                    window: any(f[1])?,
                    band: any(f[2])?,
                    direction: match f[3].trim() {
                        "+" => 1,
                        "-" => -1,
                        _ => 0,
                    },
                    phrase: f[4].trim().to_string(),
                })
            })
            .collect();
        Self { rules }
    }

    /// Every row whose phrase holds each word of `query`, case aside (the plan's §4: phrase to
    /// measurements) — what the owner's words have meant.
    #[must_use]
    pub fn explain(&self, query: &str) -> Vec<Meaning> {
        let words = query_words(query);
        if words.is_empty() {
            return Vec::new();
        }
        self.rules
            .iter()
            .filter(|r| {
                let phrase = r.phrase.to_lowercase();
                words.iter().all(|w| phrase.contains(w.as_str()))
            })
            .map(|r| Meaning {
                id: r.id.clone(),
                window: r.window,
                band: r.band,
                direction: r.direction,
                phrase: r.phrase.clone(),
            })
            .collect()
    }

    /// Every row for the reading `id`, whatever its window, band or direction: the owner's words for
    /// a change in it.
    #[must_use]
    pub fn for_id(&self, id: &str) -> Vec<Meaning> {
        self.rules
            .iter()
            .filter(|r| r.id == id)
            .map(|r| Meaning {
                id: r.id.clone(),
                window: r.window,
                band: r.band,
                direction: r.direction,
                phrase: r.phrase.clone(),
            })
            .collect()
    }

    /// The phrase for a difference in a reading, most specific rule first.
    #[must_use]
    pub fn phrase(&self, reading: &Reading, difference: f64) -> Option<&str> {
        let direction = if difference > 0.0 { 1 } else { -1 };
        let close = |a: f64, b: f64| (a - b).abs() < 1e-6;
        self.rules
            .iter()
            .filter(|r| r.id == reading.id)
            .filter(|r| r.direction == 0 || r.direction == direction)
            .filter(|r| {
                r.window
                    .is_none_or(|w| reading.window_ms.is_some_and(|(a, _)| close(a, w)))
            })
            .filter(|r| {
                r.band
                    .is_none_or(|b| reading.band_hz.is_some_and(|(lo, _)| close(lo, b)))
            })
            .max_by_key(|r| u8::from(r.window.is_some()) + u8::from(r.band.is_some()))
            .map(|r| r.phrase.as_str())
    }
}

/// A query's words, lower case, letters and digits only.
#[must_use]
pub fn query_words(query: &str) -> Vec<String> {
    query
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}
