//! Comparing two sounds: every reading of the candidate against the reference's, sized in audibility
//! thresholds, weighted by how loud its window is, tagged when an EQ or a fader could produce it,
//! phrased in the owner's words and ranked, most audible first (the plan's §3).
//!
//! Against a **set** (the reference's round robins), a difference counts as established only when it
//! exceeds both its threshold and the set's spread and its sign holds against most of the takes: one
//! strike per layer is noise (`docs/drum-model-fitting.md` §7).

use crate::audibility::{Kind, Threshold, Thresholds, Vocabulary};
use crate::reading::{Reading, Report, Unit, Validity};

/// One difference between the candidate and the reference.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub part: &'static str,
    pub id: &'static str,
    pub label: &'static str,
    pub window_ms: Option<(f64, f64)>,
    pub band_hz: Option<(f64, f64)>,
    pub unit: Unit,
    /// The reference's value (a set's mean), and the candidate's.
    pub reference: f64,
    pub candidate: f64,
    pub threshold: Threshold,
    /// The difference in thresholds.
    pub units: f64,
    /// How loud the window is, 0.1–1: a difference in a quiet window counts for less.
    pub salience: f64,
    /// An EQ or a fader could make this difference: ranked lower, never hidden (the owner's rule).
    pub eq_reachable: bool,
    /// Either reading is a single render's sample of a random quantity.
    pub sample: bool,
    /// Against a set: the takes the difference's sign holds against, of the takes with a value; and
    /// the set's spread.
    pub support: Option<(usize, usize)>,
    pub spread: Option<f64>,
    /// Against a set: whether the difference is established.
    pub established: Option<bool>,
    pub phrase: Option<String>,
    /// A property of the file rather than the sound (its end, its floor, hum): listed apart.
    pub file_property: bool,
    /// The ranking score: thresholds (capped at [`OBVIOUS`]) × salience, halved when an EQ could do it.
    pub score: f64,
    /// Why the difference is what it is, where the listener can say: a rule reading that chose
    /// different things in the two sounds, and the margin it chose by.
    pub detail: Option<String>,
}

/// Readings taken at the rest pitch: when the two sounds' rest pitches are different modes, these
/// differ because of it.
pub const AT_REST_PITCH: [&str; 9] = [
    "pitch.rest_t60",
    "pitch.attack_against_rest",
    "pitch.glide",
    "pitch.salience",
    "pitch.head_share",
    "modulation.ring_wobble",
    "modulation.ring_width_3",
    "modulation.ring_width_10",
    "modulation.ring_share",
];

/// Beyond this many thresholds a difference is simply obvious; the ranking then goes by how loud its
/// window is, so one categorical reading cannot bury the rest (**chosen**).
pub const OBVIOUS: f64 = 20.0;

/// Readings not compared: `tone.window_level` repeats `attack.window_level` and the decay's levels,
/// the attack's strongest mode is a category (which mode leads), compared as its interval to the rest
/// pitch instead, and a note's fundamental and virtual pitch are compared as their tunings.
/// The word attributes (`parts::words`) are read, never compared: each restates readings compared in
/// their own right (brightness the tone's bands, hardness the attack), and comparing both would count
/// one difference twice.
pub const WORDS: &str = "words.";

pub const NOT_COMPARED: [&str; 4] = [
    "tone.window_level",
    "pitch.attack",
    "note.fundamental",
    "note.virtual_pitch",
];

/// Readings that describe the file, not the sound.
fn file_property(part: &str, id: &str) -> bool {
    part == "artefacts" || matches!(id, "decay.end_level" | "decay.digital_silence")
}

/// A comparison of two sounds, or of a sound with a set.
#[derive(Clone, Debug, PartialEq)]
pub struct Comparison {
    pub reference: String,
    pub candidate: String,
    /// Every difference, most audible first; `units` below 1 are within threshold.
    pub findings: Vec<Finding>,
    pub notes: Vec<String>,
    /// Regions where the sounds differ on the auditory map; empty until [`Comparison::map`] runs.
    pub regions: Vec<crate::unexplained::Region>,
}

impl Comparison {
    /// The audible findings about the sound: at least one threshold, established where a set could
    /// say, and not a property of the file.
    pub fn audible(&self) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(|f| f.units >= 1.0 && f.established != Some(false) && !f.file_property)
    }

    /// Adds the auditory map's regions, each marked explained or not by the audible findings.
    pub fn map(&mut self, reference: &crate::Sound, candidate: &crate::Sound) {
        let audible: Vec<&Finding> = self.audible().collect();
        self.regions = crate::unexplained::regions(reference, candidate, &audible);
    }

    /// The regions no audible finding explains.
    pub fn unexplained(&self) -> impl Iterator<Item = &crate::unexplained::Region> {
        self.regions.iter().filter(|r| !r.explained)
    }

    /// The files' own differences past threshold.
    pub fn file_differences(&self) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(|f| f.units >= 1.0 && f.file_property)
    }

    /// The audible findings grouped by kind — one reading, or one reading in one band — strongest
    /// first: the headline an agent or the owner reads first. When the two sounds' rest pitches are
    /// different modes, the readings taken at the rest pitch are one cause with it and join its group,
    /// led by the rest pitch.
    #[must_use]
    pub fn headline(&self) -> Vec<Vec<&Finding>> {
        let flipped = self
            .audible()
            .any(|f| f.id == "pitch.rest" && f.detail.is_some());
        let key = |g: &Finding| {
            if flipped && g.detail.is_some() && AT_REST_PITCH.contains(&g.id) {
                return ("pitch.rest", None);
            }
            (
                g.id,
                if g.id == "tone.band_level" {
                    g.band_hz.map(|b| b.0.to_bits())
                } else {
                    None
                },
            )
        };
        let mut groups: Vec<Vec<&Finding>> = Vec::new();
        for f in self.audible() {
            match groups.iter_mut().find(|g| key(g[0]) == key(f)) {
                Some(g) => g.push(f),
                None => groups.push(vec![f]),
            }
        }
        for g in &mut groups {
            if let Some(i) = g.iter().position(|f| f.id == "pitch.rest") {
                let rest = g.remove(i);
                g.insert(0, rest);
            }
        }
        groups
    }
}

/// A reading that can be compared: present, and neither below the floor nor unresolved.
fn usable(r: &Reading) -> Option<f64> {
    match r.validity {
        Validity::Valid | Validity::Sample => r.value,
        _ => None,
    }
}

fn same(a: Option<(f64, f64)>, b: Option<(f64, f64)>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => (x.0 - y.0).abs() < 1e-9 && (x.1 - y.1).abs() < 1e-9,
        _ => false,
    }
}

/// An octave level as the spectrum's shape at its moment: the band against the total of every band
/// at the same time, dB. A change of decay or of gain moves every band together and leaves the shape;
/// a change of colour moves it.
fn band_shape(report: &Report, r: &Reading) -> Option<f64> {
    let level = usable(r)?;
    let window = r.window_ms?;
    let total: f64 = report
        .sections
        .iter()
        .flat_map(|s| &s.readings)
        .filter(|x| x.id == "tone.band_level" && same(x.window_ms, Some(window)))
        .filter_map(usable)
        .map(|l| 10f64.powf(l / 10.0))
        .sum();
    (total > 0.0).then(|| level - 10.0 * total.log10())
}

/// The value a comparison uses for a reading: its own, or, for an octave level, its shape.
fn compared(report: &Report, r: &Reading) -> Option<f64> {
    if r.id == "tone.band_level" {
        band_shape(report, r)
    } else {
        usable(r)
    }
}

fn find<'a>(report: &'a Report, like: &Reading) -> Option<&'a Reading> {
    report.sections.iter().flat_map(|s| &s.readings).find(|r| {
        r.id == like.id && same(r.window_ms, like.window_ms) && same(r.band_hz, like.band_hz)
    })
}

/// How much a window counts: full above −20 dB against the peak, down to 0.1 at −60 dB.
fn salience(r: &Reading) -> f64 {
    let level = if r.id == "tone.band_level" {
        r.value
    } else {
        r.level_db
    };
    level.map_or(1.0, |l| ((l + 60.0) / 40.0).clamp(0.1, 1.0))
}

/// Octave bands whose difference is the same at every time: a static colour an EQ could give.
fn static_bands(reference: &[&Report], candidate: &Report) -> Vec<(f64, f64)> {
    let mut bands: Vec<(f64, f64)> = Vec::new();
    let levels: Vec<&Reading> = candidate
        .sections
        .iter()
        .flat_map(|s| &s.readings)
        .filter(|r| r.id == "tone.band_level")
        .collect();
    for r in &levels {
        if let Some(b) = r.band_hz {
            if !bands.contains(&b) {
                bands.push(b);
            }
        }
    }
    bands
        .into_iter()
        .filter(|&band| {
            let diffs: Vec<f64> = levels
                .iter()
                .filter(|r| r.band_hz == Some(band))
                .filter_map(|r| {
                    let c = compared(candidate, r)?;
                    let refs: Vec<f64> = reference
                        .iter()
                        .filter_map(|rep| find(rep, r).and_then(|x| compared(rep, x)))
                        .collect();
                    if refs.is_empty() {
                        return None;
                    }
                    Some(c - refs.iter().sum::<f64>() / refs.len() as f64)
                })
                .collect();
            if diffs.len() < 4 {
                return false;
            }
            let mean = diffs.iter().sum::<f64>() / diffs.len() as f64;
            let sd =
                (diffs.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / diffs.len() as f64).sqrt();
            mean.abs() >= 1.0 && sd < 1.5
        })
        .collect()
}

/// Compares a candidate with one reference.
#[must_use]
pub fn compare(
    reference: &Report,
    candidate: &Report,
    thresholds: &Thresholds,
    vocabulary: &Vocabulary,
) -> Comparison {
    compare_to_set(
        &[reference],
        candidate,
        thresholds,
        vocabulary,
        &reference.name,
    )
}

/// Compares a candidate with a set of references (one or more takes of the reference sound).
#[must_use]
pub fn compare_to_set(
    references: &[&Report],
    candidate: &Report,
    thresholds: &Thresholds,
    vocabulary: &Vocabulary,
    reference_name: &str,
) -> Comparison {
    let mut notes = Vec::new();
    if references.len() > 1 {
        notes.push(format!(
            "against {} takes: a difference is established when it exceeds both its threshold and the takes' spread, and holds against at least two thirds of them",
            references.len()
        ));
    }
    // A note's partials become readings keyed by the reference's partials, so they compare as any
    // reading does; a drum has no partial table and compares exactly as before.
    let matched = note_partials(references, candidate);
    let (references, candidate): (Vec<&Report>, &Report) = match &matched {
        Some((refs, cand, unmatched)) => {
            notes.extend(unmatched.iter().cloned());
            (refs.iter().collect(), cand)
        }
        None => (references.to_vec(), candidate),
    };
    let references = references.as_slice();
    let static_colour = static_bands(references, candidate);
    let mut findings = Vec::new();
    for section in &candidate.sections {
        for r in &section.readings {
            if NOT_COMPARED.contains(&r.id) || r.id.starts_with(WORDS) {
                continue;
            }
            let Some(c) = compared(candidate, r) else {
                continue;
            };
            let refs: Vec<f64> = references
                .iter()
                .filter_map(|rep| find(rep, r).and_then(|x| compared(rep, x)))
                .collect();
            if refs.is_empty() {
                continue;
            }
            let context = Some(candidate.family.family.name());
            let Some(threshold) = thresholds.for_reading_in(r.id, r.unit, context).cloned() else {
                continue;
            };
            let n = refs.len() as f64;
            let mean = refs.iter().sum::<f64>() / n;
            let spread = (refs.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n).sqrt();
            let units = threshold.units(mean, c);
            if !units.is_finite() {
                continue;
            }
            let difference = c - mean;
            let (support, established, spread_out) = if refs.len() > 1 {
                let agree = refs.iter().filter(|v| (c - **v) * difference > 0.0).count();
                let spread_units = threshold.units(mean, mean + spread.copysign(difference));
                let ok = units >= 1.0 && units > spread_units && 3 * agree >= 2 * refs.len();
                (Some((agree, refs.len())), Some(ok), Some(spread))
            } else {
                (None, None, None)
            };
            let fader = matches!(r.id, "level.body_loudness" | "level.peak");
            let colour = matches!(
                r.id,
                "tone.band_level"
                    | "tone.centroid"
                    | "attack.early_centroid"
                    | "attack.early_rolloff"
            ) && match r.band_hz {
                Some(b) if r.id == "tone.band_level" => static_colour.contains(&b),
                _ => {
                    r.id != "tone.band_level"
                        && !static_colour.is_empty()
                        && static_colour.len() * 2 >= 8
                }
            };
            let eq_reachable = fader || colour;
            let reference_reading = references.iter().find_map(|rep| find(rep, r));
            let s = reference_reading.map_or_else(|| salience(r), salience);
            let sample = r.validity == Validity::Sample
                || reference_reading.is_some_and(|x| x.validity == Validity::Sample);
            let is_file = file_property(section.part, r.id);
            let score = if is_file {
                0.0
            } else {
                units.min(OBVIOUS) * s * if eq_reachable { 0.5 } else { 1.0 }
            };
            findings.push(Finding {
                part: section.part,
                id: r.id,
                label: r.label,
                window_ms: r.window_ms,
                band_hz: r.band_hz,
                unit: r.unit,
                reference: mean,
                candidate: c,
                threshold,
                units,
                salience: s,
                eq_reachable,
                sample,
                support,
                spread: spread_out,
                established,
                phrase: vocabulary.phrase(r, difference).map(str::to_string),
                file_property: is_file,
                score,
                detail: None,
            });
        }
    }
    if let Some(why) = references.first().and_then(|r| rest_rule(r, candidate)) {
        for f in &mut findings {
            if f.id == "pitch.rest" {
                f.detail = Some(why.clone());
            } else if AT_REST_PITCH.contains(&f.id) {
                f.detail = Some(
                    "read at the rest pitch, which the two sounds take from different modes (see the rest pitch)"
                        .to_string(),
                );
            }
        }
    }
    // Two notes whose virtual pitches are different notes are heard as different notes; their tunings
    // against those notes are no difference in cents (the virtual pitch of a bar can take an upper
    // partial's pitch where the fundamental names the note).
    let heard = |r: &Report| r.find("note.virtual_pitch", None).and_then(|x| x.value);
    if let (Some(a), Some(b)) = (references.first().and_then(|r| heard(r)), heard(candidate)) {
        if (1200.0 * (b / a).log2()).abs() > 50.0 {
            findings.retain(|f| f.id != "note.virtual_tuning");
            let name =
                |hz: f64| crate::parts::note::nearest_note(hz).map_or(String::new(), |n| n.0);
            notes.push(format!(
                "heard as different notes: the virtual pitch takes {} in the reference and {} in the candidate",
                name(a),
                name(b)
            ));
        }
    }
    findings.sort_by(|a, b| b.score.total_cmp(&a.score));
    Comparison {
        reference: reference_name.to_string(),
        candidate: candidate.name.clone(),
        findings,
        notes,
        regions: Vec::new(),
    }
}

/// Two partials of two notes are the same partial when their ratios to their own fundamentals are
/// within this share of each other (about half a semitone).
pub const PARTIAL_MATCH: f64 = 0.03;

/// One row of a note's partial table: Hz, ratio to the fundamental, strike level against it (dB), T60 (s).
type PartialRow = (f64, f64, Option<f64>, Option<f64>);

/// A note's partial table as `(Hz, ratio to the fundamental, strike level against it dB, T60 s)`,
/// the partials above the fundamental (its own readings are `note.t60` and `note.tuning`).
fn partial_rows(report: &Report) -> Option<Vec<PartialRow>> {
    let table = report
        .sections
        .iter()
        .flat_map(|s| &s.tables)
        .find(|t| t.id == "note.partials")?;
    Some(
        table
            .rows
            .iter()
            .filter_map(|row| {
                let (hz, ratio) = (
                    row.first().copied().flatten()?,
                    row.get(1).copied().flatten()?,
                );
                (ratio > 1.0 + PARTIAL_MATCH).then(|| {
                    (
                        hz,
                        ratio,
                        row.get(2).copied().flatten(),
                        row.get(3).copied().flatten(),
                    )
                })
            })
            .collect(),
    )
}

/// The first reference's partials above its fundamental as slots, and every report — the references and the candidate —
/// with a section of readings for the partial it holds in each slot: its level against its
/// fundamental, its T60, and its tuning against the slot's ratio, each keyed by the slot's band
/// (±[`PARTIAL_MATCH`] around the first reference's partial). With notes on the partials only one side
/// holds. `None` unless the first reference and the candidate both carry a partial table.
fn note_partials(
    references: &[&Report],
    candidate: &Report,
) -> Option<(Vec<Report>, Report, Vec<String>)> {
    use crate::reading::Section;
    let slots = partial_rows(references.first()?)?;
    partial_rows(candidate)?;
    let src = "the note's partial table, matched by ratio to each note's fundamental within ±3 %";
    let with = |report: &Report| -> (Report, Vec<f64>) {
        let rows = partial_rows(report).unwrap_or_default();
        let mut used = vec![false; rows.len()];
        let mut readings = Vec::new();
        for &(hz, ratio, _, _) in &slots {
            let band = (hz * (1.0 - PARTIAL_MATCH), hz * (1.0 + PARTIAL_MATCH));
            let Some((i, row)) = rows
                .iter()
                .enumerate()
                .filter(|(i, r)| !used[*i] && (r.1 / ratio - 1.0).abs() <= PARTIAL_MATCH)
                .min_by(|a, b| {
                    (a.1.1 / ratio)
                        .ln()
                        .abs()
                        .total_cmp(&(b.1.1 / ratio).ln().abs())
                })
            else {
                continue;
            };
            used[i] = true;
            let level = row.2;
            let mut r = Reading::new(
                "note.partial_level",
                "A partial's level at the strike, against the fundamental",
                level,
                Unit::Decibels,
                src,
            )
            .band(band.0, band.1);
            r.level_db = level;
            readings.push(r);
            let mut r = Reading::new(
                "note.partial_t60",
                "A partial's decay (T60)",
                row.3,
                Unit::Seconds,
                src,
            )
            .band(band.0, band.1);
            r.level_db = level;
            readings.push(r);
            let mut r = Reading::new(
                "note.partial_tuning",
                "A partial's ratio to the fundamental, against the reference's",
                Some(1200.0 * (row.1 / ratio).log2()),
                Unit::Cents,
                src,
            )
            .band(band.0, band.1);
            r.level_db = level;
            readings.push(r);
        }
        let extra: Vec<f64> = rows
            .iter()
            .zip(&used)
            .filter(|(_, u)| !**u)
            .map(|(r, _)| r.1)
            .collect();
        let mut out = report.clone();
        out.sections.push(Section::new(
            "note",
            "The note's partials, matched",
            readings,
        ));
        (out, extra)
    };
    let refs: Vec<Report> = references.iter().map(|r| with(r).0).collect();
    let (cand, extra) = with(candidate);
    let held: Vec<f64> = slots
        .iter()
        .filter(|s| {
            !cand.sections.last().is_some_and(|sec| {
                sec.readings
                    .iter()
                    .any(|r| r.band_hz.is_some_and(|b| b.0 <= s.0 && s.0 <= b.1))
            })
        })
        .map(|s| s.1)
        .collect();
    let list = |v: &[f64]| {
        v.iter()
            .map(|r| format!("{r:.2}×"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut notes = Vec::new();
    if !held.is_empty() {
        notes.push(format!(
            "partials the reference holds and the candidate does not (ratio to the fundamental): {}",
            list(&held)
        ));
    }
    if !extra.is_empty() {
        notes.push(format!(
            "partials the candidate holds and the reference does not: {}",
            list(&extra)
        ));
    }
    Some((refs, cand, notes))
}

/// When the two rest pitches are different modes, the rest-pitch rule's margin for each in both
/// sounds: a lower mode near the rule's 12 dB line moves the reading by an interval for a change of a
/// dB or two, and the owner should hear which.
fn rest_rule(reference: &Report, candidate: &Report) -> Option<String> {
    use crate::parts::pitch::{PERSISTS, REST_WITHIN_DB};
    let rest = |r: &Report| {
        r.sections
            .iter()
            .flat_map(|s| &s.readings)
            .find(|x| x.id == "pitch.rest")
            .and_then(|x| x.value.map(|v| (v, x.window_ms)))
    };
    let ((a, wa), (b, wb)) = (rest(reference)?, rest(candidate)?);
    if (b / a - 1.0).abs() <= PERSISTS {
        return None;
    }
    // Each sound's modes in its ring window: (Hz, level, seen in other windows), room lines left out.
    let modes = |r: &Report, w: Option<(f64, f64)>| -> Vec<(f64, f64, f64)> {
        r.sections
            .iter()
            .flat_map(|s| &s.tables)
            .find(|t| t.id == "pitch.modes" && t.window_ms == w)
            .map(|t| {
                t.rows
                    .iter()
                    .filter(|row| row.get(5).copied().flatten() != Some(1.0))
                    .filter_map(|row| {
                        Some((
                            row[0]?,
                            row[2]?,
                            row.get(4).copied().flatten().unwrap_or(0.0),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let (ma, mb) = (modes(reference, wa), modes(candidate, wb));
    let under = |m: &[(f64, f64, f64)], hz: f64| -> String {
        let top = m.iter().fold(f64::NEG_INFINITY, |x, v| x.max(v.1));
        match m
            .iter()
            .filter(|v| (v.0 / hz - 1.0).abs() <= PERSISTS)
            .min_by(|p, q| (p.0 - hz).abs().total_cmp(&(q.0 - hz).abs()))
        {
            Some(v) if v.1 >= top => "the strongest".to_string(),
            Some(v) => format!(
                "{:.1} dB under the strongest{}",
                top - v.1,
                if v.2 < 1.0 {
                    ", seen in no other window"
                } else {
                    ""
                }
            ),
            None => "not found".to_string(),
        }
    };
    let (low, high) = if a < b { (a, b) } else { (b, a) };
    Some(format!(
        "the rule takes the lowest believed mode within {REST_WITHIN_DB:.0} dB of the strongest, and chose different modes: {low:.0} Hz is {} in the reference and {} in the candidate; {high:.0} Hz is {} and {}",
        under(&ma, low),
        under(&mb, low),
        under(&ma, high),
        under(&mb, high)
    ))
}

/// A threshold in words.
#[must_use]
pub fn threshold_text(t: &Threshold, unit: Unit) -> String {
    match t.kind {
        Kind::Absolute => format!("{} {}", t.value, unit.symbol()),
        Kind::Relative => format!("{:.1} %", 100.0 * t.value),
    }
}
