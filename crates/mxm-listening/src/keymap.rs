//! Key tracking (the plan's §2 item 11, "across all of them"): how a note's readings move across an
//! instrument's compass. AAS scales every parameter across the keyboard from middle C
//! (`research:physical-modelling/aas-modeling-collection.md`), so each reading is fitted as a line
//! against octaves from middle C over a set of notes, as a set's velocity layers are read against
//! loudness: its value at middle C and its change for each octave up. A decay time is fitted on a log
//! scale — it halves or doubles by octaves — and its change is a factor.
//!
//! The line is Theil–Sen's (the median of every pair of notes' slopes, and the median intercept
//! under it; Sen, *JASA* 1968, after Theil 1950) and its spread the residuals' median absolute
//! deviation scaled to a standard deviation: one note read wrong — a missed partial, a line in
//! sympathy — moved a least-squares line through Iowa's vibraphone at mf by 15 dB an octave in its
//! tilt.

use crate::reading::{Report, Unit};

/// C4, Hz.
pub const MIDDLE_C_HZ: f64 = 261.625_565_300_598_6;

/// The readings tracked across the keys, and whether each is fitted on a log scale.
pub const TRACKED: [(&str, bool); 7] = [
    ("note.tuning", false),
    ("note.t60", true),
    ("note.decay_law", false),
    ("note.tilt", false),
    ("note.partials", false),
    ("note.ideal_cents", false),
    ("level.body_loudness", false),
];

/// One reading across the keys.
#[derive(Clone, Debug, PartialEq)]
pub struct Trend {
    pub id: &'static str,
    pub label: &'static str,
    pub unit: Unit,
    /// Fitted on a log scale: `at_middle_c` is then the value, `per_octave` a factor.
    pub log: bool,
    /// The line's value at middle C (extrapolated when the compass does not reach it).
    pub at_middle_c: f64,
    /// Its change for each octave up: added, or on a log scale multiplied.
    pub per_octave: f64,
    /// How far the notes sit from the line, the residuals' median absolute deviation scaled to one
    /// standard deviation (×1.4826): in the unit, or on a log scale as a factor.
    pub spread: f64,
    pub notes: usize,
    /// The lowest and highest fundamental the line was fitted over, octaves from middle C.
    pub compass: (f64, f64),
}

fn value(report: &Report, id: &str) -> Option<(f64, &'static str, Unit)> {
    let r = report.find(id, None)?;
    match r.validity {
        crate::reading::Validity::Valid | crate::reading::Validity::Sample => {
            r.value.map(|v| (v, r.label, r.unit))
        }
        _ => None,
    }
}

fn median(v: &mut [f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        0.5 * (v[n / 2 - 1] + v[n / 2])
    })
}

/// Every tracked reading's trend over `reports` (pitched notes: each needs `note.fundamental`), for
/// the readings at least three notes spanning half an octave hold.
#[must_use]
pub fn trends(reports: &[&Report]) -> Vec<Trend> {
    TRACKED
        .iter()
        .filter_map(|&(id, log)| {
            let mut label = "";
            let mut unit = Unit::Plain;
            let points: Vec<(f64, f64)> = reports
                .iter()
                .filter_map(|r| {
                    let (f1, _, _) = value(r, "note.fundamental")?;
                    let (v, l, u) = value(r, id)?;
                    if log && v <= 0.0 {
                        return None;
                    }
                    (label, unit) = (l, u);
                    Some(((f1 / MIDDLE_C_HZ).log2(), if log { v.ln() } else { v }))
                })
                .collect();
            let lo = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
            let hi = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
            if points.len() < 3 || hi - lo < 0.5 {
                return None;
            }
            let mut slopes: Vec<f64> = Vec::new();
            for (i, a) in points.iter().enumerate() {
                for b in &points[i + 1..] {
                    if (b.0 - a.0).abs() > 1e-9 {
                        slopes.push((b.1 - a.1) / (b.0 - a.0));
                    }
                }
            }
            let slope = median(&mut slopes)?;
            let mut intercepts: Vec<f64> = points.iter().map(|p| p.1 - slope * p.0).collect();
            let at = median(&mut intercepts)?;
            let mut residuals: Vec<f64> = points
                .iter()
                .map(|p| (p.1 - at - slope * p.0).abs())
                .collect();
            let sd = 1.4826 * median(&mut residuals)?;
            Some(Trend {
                id,
                label,
                unit,
                log,
                at_middle_c: if log { at.exp() } else { at },
                per_octave: if log { slope.exp() } else { slope },
                spread: if log { sd.exp() } else { sd },
                notes: points.len(),
                compass: (lo, hi),
            })
        })
        .collect()
}

/// A body's response across a set: third-octave bands of absolute frequency, each with the median
/// of the partials' levels that fall in it, every level taken against its own note's tilt (a line
/// through that note's strike levels against octaves above its fundamental). A note's own laws move
/// with its pitch; what stays at the same frequency across the compass is the body, the soundboard or
/// the resonator (the plan's §2 item 11: the body's fixed lines, found across a set spanning the
/// keyboard). `(band centre Hz, median dB, partials, notes)` for bands at least [`BODY_MIN_NOTES`]
/// notes reach.
#[must_use]
pub fn body(reports: &[&Report]) -> Vec<(f64, f64, usize, usize)> {
    let mut pooled: Vec<(f64, f64, usize)> = Vec::new();
    for (k, r) in reports.iter().enumerate() {
        let Some(table) = r
            .sections
            .iter()
            .flat_map(|s| &s.tables)
            .find(|t| t.id == "note.partials")
        else {
            continue;
        };
        let rows: Vec<(f64, f64, f64)> = table
            .rows
            .iter()
            .filter_map(|row| {
                let (hz, ratio, level) = (
                    row.first()?.as_ref()?,
                    row.get(1)?.as_ref()?,
                    row.get(2)?.as_ref()?,
                );
                (*ratio >= 0.98).then_some((*hz, ratio.log2(), *level))
            })
            .collect();
        if rows.len() < 3 {
            continue;
        }
        let n = rows.len() as f64;
        let mx = rows.iter().map(|r| r.1).sum::<f64>() / n;
        let my = rows.iter().map(|r| r.2).sum::<f64>() / n;
        let sxx: f64 = rows.iter().map(|r| (r.1 - mx).powi(2)).sum();
        if sxx <= 0.0 {
            continue;
        }
        let slope = rows.iter().map(|r| (r.1 - mx) * (r.2 - my)).sum::<f64>() / sxx;
        for &(hz, octaves, level) in &rows {
            pooled.push((hz, level - my - slope * (octaves - mx), k));
        }
    }
    let mut out = Vec::new();
    let mut centre = 31.5f64;
    while centre < 20_000.0 {
        let (lo, hi) = (
            centre * 2f64.powf(-1.0 / 6.0),
            centre * 2f64.powf(1.0 / 6.0),
        );
        let mut here: Vec<f64> = pooled
            .iter()
            .filter(|p| p.0 >= lo && p.0 < hi)
            .map(|p| p.1)
            .collect();
        let mut notes: Vec<usize> = pooled
            .iter()
            .filter(|p| p.0 >= lo && p.0 < hi)
            .map(|p| p.2)
            .collect();
        notes.sort_unstable();
        notes.dedup();
        if notes.len() >= BODY_MIN_NOTES {
            let count = here.len();
            if let Some(m) = median(&mut here) {
                out.push((centre, m, count, notes.len()));
            }
        }
        centre *= 2f64.powf(1.0 / 3.0);
    }
    out
}

/// Lines within this share of each other in different notes are one line.
pub const RECUR_SHARE: f64 = 0.015;

/// Lines at one frequency under at least half of a set's notes: a guitar's or a cello's body
/// resonance, or the room. A note's own partials move with its pitch, so what stays put across a set
/// spanning a compass is not any note's (the plan's §2 item 11: the body's fixed lines, found across a
/// set as a drum set's room lines are). Each note counts a line once; a line is reported at the median
/// of its sightings.
#[must_use]
pub fn recurring_lines(reports: &[&Report]) -> Vec<f64> {
    let per_note: Vec<Vec<f64>> = reports
        .iter()
        .map(|r| {
            r.sections
                .iter()
                .flat_map(|s| &s.tables)
                .filter(|t| t.id == "note.partials")
                .flat_map(|t| {
                    t.rows
                        .iter()
                        .filter_map(|row| row.first().copied().flatten())
                })
                .collect()
        })
        .filter(|v: &Vec<f64>| !v.is_empty())
        .collect();
    if per_note.len() < 4 {
        return Vec::new();
    }
    let near = |a: f64, b: f64| (a / b - 1.0).abs() <= RECUR_SHARE;
    let mut found: Vec<f64> = Vec::new();
    for lines in &per_note {
        for &hz in lines {
            if found.iter().any(|&f| near(f, hz)) {
                continue;
            }
            let mut sightings: Vec<f64> = per_note
                .iter()
                .filter_map(|other| {
                    other
                        .iter()
                        .copied()
                        .filter(|&o| near(o, hz))
                        .min_by(|a, b| (a - hz).abs().total_cmp(&(b - hz).abs()))
                })
                .collect();
            if 2 * sightings.len() >= per_note.len() {
                if let Some(m) = median(&mut sightings) {
                    found.push(m);
                }
            }
        }
    }
    found.sort_by(f64::total_cmp);
    found
}

/// A body band is reported when this many notes put a partial in it.
pub const BODY_MIN_NOTES: usize = 3;

/// The trends as a Markdown table.
#[must_use]
pub fn markdown(
    title: &str,
    notes: usize,
    trends: &[Trend],
    body: &[(f64, f64, usize, usize)],
    recurring: &[f64],
) -> String {
    let mut out = format!(
        "# {title}\n\n{notes} notes. Each reading is a line through its values against octaves from middle C (C4, {MIDDLE_C_HZ:.2} Hz); a decay time is fitted on a log scale, so it changes by a factor.\n\n"
    );
    if !recurring.is_empty() {
        let lines: Vec<String> = recurring.iter().map(|hz| format!("{hz:.1}")).collect();
        out.push_str(&format!(
            "Lines under at least half the notes, kept out of every note's partials (the body or the room): {} Hz.\n\n",
            lines.join(", ")
        ));
    }
    out.push_str("| Reading | At middle C | For each octave up | Spread around the line | Notes | Compass (octaves from C4) |\n|---|---|---|---|---|---|\n");
    for t in trends {
        let unit = t.unit.symbol();
        let (at, per, spread) = if t.log {
            (
                format!("{:.3} {unit}", t.at_middle_c),
                format!("×{:.2}", t.per_octave),
                format!("×{:.2}", t.spread),
            )
        } else {
            (
                format!("{:.2} {unit}", t.at_middle_c),
                format!("{:+.2} {unit}", t.per_octave),
                format!("±{:.2} {unit}", t.spread),
            )
        };
        out.push_str(&format!(
            "| {} | {at} | {per} | {spread} | {} | {:+.1} to {:+.1} |\n",
            t.label, t.notes, t.compass.0, t.compass.1
        ));
    }
    if !body.is_empty() {
        out.push_str("
## The body

Partial levels by third octave of frequency, each against its own note's tilt: what stays at one frequency across the notes is the body, the soundboard or the resonator.

| Band (Hz) | Median against the note's tilt | Partials | Notes |
|---|---|---|---|
");
        for (centre, db, partials, n) in body {
            out.push_str(&format!(
                "| {centre:.0} | {db:+.1} dB | {partials} | {n} |
"
            ));
        }
    }
    out
}
