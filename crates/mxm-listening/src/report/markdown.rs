//! A report as Markdown, for reading: each part as a short table, the octave levels as a band × time
//! grid. A value below the floor is marked `~`, a single render's sample `†`, an absent one `—`.

use std::fmt::Write as _;

use crate::reading::{Reading, Report, Section, Table, Validity};

/// The report as Markdown.
#[must_use]
pub fn to_markdown(r: &Report) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# {}\n", r.name);
    let _ = writeln!(
        s,
        "{} Hz, {:.2} s. Family: **{}** ({}).{}",
        r.rate,
        r.duration_s,
        r.family.family,
        r.family.how,
        r.onset_s
            .map(|o| format!(
                " Onset at {:.1} ms; every window is measured from it.",
                o * 1000.0
            ))
            .unwrap_or_default()
    );
    if !r.notes.is_empty() {
        s.push('\n');
        for n in &r.notes {
            let _ = writeln!(s, "- {n}");
        }
    }
    for section in &r.sections {
        let _ = writeln!(s, "\n## {}\n", section.title);
        if section.part == "tone" {
            band_grid(&mut s, section);
            let rest: Vec<&Reading> = section
                .readings
                .iter()
                .filter(|x| x.id != "tone.band_level")
                .collect();
            table(&mut s, &rest);
        } else {
            let all: Vec<&Reading> = section.readings.iter().collect();
            table(&mut s, &all);
        }
        for t in &section.tables {
            data_table(&mut s, t);
        }
    }
    s.push_str("\n`~` below the floor (more than 45 dB under the peak), `†` a single render's sample, `≤` at the resolution limit, `—` absent.\n");
    s
}

/// The readings a set's Markdown summary shows, by id and window start: the ones the owner's phrases
/// have pointed at. The JSON carries every reading.
pub const SET_SUMMARY: [(&str, Option<f64>); 16] = [
    ("level.body_loudness", None),
    ("attack.peak_time", None),
    ("attack.rise_time", None),
    ("attack.early_centroid", None),
    ("decay.t20", None),
    ("decay.t40", None),
    ("decay.late_level", Some(300.0)),
    ("decay.late_level", Some(800.0)),
    ("decay.late_slope", Some(300.0)),
    ("pitch.rest", None),
    ("pitch.salience", Some(80.0)),
    ("tone.centroid", Some(20.0)),
    ("texture.buzz_share", None),
    ("texture.late_wire_level", Some(450.0)),
    ("modulation.ring_wobble", None),
    ("modulation.ring_share", Some(100.0)),
];

/// A set's summary as Markdown: the chosen readings per group, mean ± spread (n of files).
#[must_use]
pub fn set_to_markdown(r: &crate::set::SetReport) -> String {
    let mut s = String::new();
    s.push_str("# Set\n\n");
    let names: Vec<String> = r
        .groups
        .iter()
        .map(|g| format!("{} ({} files)", g.name, g.files.len()))
        .collect();
    let _ = writeln!(s, "Groups: {}.", names.join(", "));
    if !r.velocity_map.is_empty() {
        let v: Vec<String> = r
            .velocity_map
            .iter()
            .map(|(g, d)| format!("{g} {d:+.1} dB"))
            .collect();
        let _ = writeln!(
            s,
            "\nBody loudness against the middle group (the velocity map): {}.",
            v.join(", ")
        );
    }
    if r.room_lines.is_empty() {
        s.push_str("\nNo late line recurs across the set.\n");
    } else {
        let v: Vec<String> = r.room_lines.iter().map(|hz| format!("{hz:.1}")).collect();
        let _ = writeln!(
            s,
            "\nLate lines recurring across the set — the room or the kit, not the drum: {} Hz.",
            v.join(", ")
        );
    }
    let _ = write!(s, "\n| Reading | Where |");
    for g in &r.groups {
        let _ = write!(s, " {} |", g.name);
    }
    let _ = writeln!(s, "\n|---|---|{}", "---|".repeat(r.groups.len()));
    for (id, window) in SET_SUMMARY {
        let Some(first) = r.groups.iter().find_map(|g| g.stat(id, window)) else {
            continue;
        };
        let place = match first.window_ms {
            Some((a, b)) => format!("{a:.0}–{b:.0} ms"),
            None => String::new(),
        };
        let _ = write!(s, "| {} | {} |", first.label, place);
        for g in &r.groups {
            let cell = match g.stat(id, window) {
                Some(st) => format!(
                    "{:.1} ± {:.1} {}{}",
                    st.mean,
                    st.spread,
                    st.unit.symbol(),
                    if st.n < st.of {
                        format!(" ({}/{})", st.n, st.of)
                    } else {
                        String::new()
                    }
                ),
                None => "—".into(),
            };
            let _ = write!(s, " {cell} |");
        }
        s.push('\n');
    }
    s
}

/// A comparison as Markdown: the audible differences, most audible first, each with its measurements,
/// its size in thresholds and what it rests on; then how many readings stayed within threshold.
#[must_use]
pub fn comparison_to_markdown(c: &crate::compare::Comparison) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# {} against {}\n", c.candidate, c.reference);
    for n in &c.notes {
        let _ = writeln!(s, "- {n}");
    }
    let audible: Vec<&crate::compare::Finding> = c.audible().collect();
    if audible.is_empty() {
        s.push_str("\nNo audible difference.\n");
    } else {
        s.push_str("\n## The differences, most audible first\n\n");
        for (i, group) in c.headline().iter().take(HEADLINE).enumerate() {
            let f = group[0];
            let headline = f.phrase.clone().unwrap_or_else(|| f.label.to_string());
            let mut tags = vec![format!("{:.1}×", f.units)];
            if let Some((agree, of)) = f.support {
                tags.push(format!("{agree}/{of} takes"));
            }
            if f.eq_reachable {
                tags.push("EQ or fader".into());
            }
            if f.sample {
                tags.push("sample".into());
            }
            let elsewhere = if group.len() > 1 {
                let windows: Vec<String> = group[1..]
                    .iter()
                    .filter(|g| g.id == f.id)
                    .filter_map(|g| g.window_ms.map(|(a, _)| format!("{a:.0}")))
                    .collect();
                // Other readings in the group share its cause.
                let mut with: Vec<String> = Vec::new();
                for g in &group[1..] {
                    let label = g.label.to_lowercase();
                    if g.id != f.id && !with.contains(&label) {
                        with.push(label);
                    }
                }
                let mut tail = String::new();
                if !windows.is_empty() {
                    tail.push_str(&format!("; also at {} ms", windows.join(", ")));
                }
                if !with.is_empty() {
                    tail.push_str(&format!("; with it: {}", with.join(", ")));
                }
                tail
            } else {
                String::new()
            };
            let _ = writeln!(
                s,
                "{}. **{}**: {}{} {} → {} {} ({}){}",
                i + 1,
                headline,
                f.label.to_lowercase(),
                where_text(f.window_ms, f.band_hz),
                short(f.reference),
                short(f.candidate),
                f.unit.symbol(),
                tags.join(", "),
                elsewhere
            );
            if let Some(d) = &f.detail {
                let _ = writeln!(s, "   - {d}");
            }
        }
        let groups = c.headline().len();
        if groups > HEADLINE {
            let _ = writeln!(
                s,
                "\n…and {} more kinds of difference below.",
                groups - HEADLINE
            );
        }
        s.push_str("\n## Every audible difference\n\n");
        for (i, f) in audible.iter().enumerate() {
            let mut place = Vec::new();
            if let Some((a, b)) = f.window_ms {
                place.push(if (a - b).abs() < 1e-9 {
                    format!("at {a:.0} ms")
                } else {
                    format!("{a:.0}–{b:.0} ms")
                });
            }
            if let Some((lo, hi)) = f.band_hz {
                place.push(format!("{lo:.0}–{hi:.0} Hz"));
            }
            let mut tags = vec![format!(
                "{:.1}× threshold ({})",
                f.units,
                crate::compare::threshold_text(&f.threshold, f.unit)
            )];
            if let Some((agree, of)) = f.support {
                tags.push(format!("holds against {agree}/{of} takes"));
            }
            if f.eq_reachable {
                tags.push("an EQ or a fader could do this".into());
            }
            if f.sample {
                tags.push("a single render's sample".into());
            }
            let headline = f.phrase.clone().unwrap_or_else(|| f.label.to_string());
            let unit = f.unit.symbol();
            let _ = writeln!(
                s,
                "{}. **{}** — {}{}: {} → {} {} ({})",
                i + 1,
                headline,
                f.label,
                if place.is_empty() {
                    String::new()
                } else {
                    format!(", {}", place.join(", "))
                },
                short(f.reference),
                short(f.candidate),
                unit,
                tags.join("; ")
            );
            if let Some(d) = &f.detail {
                let _ = writeln!(s, "   - {d}");
            }
        }
    }
    let unexplained: Vec<&crate::unexplained::Region> = c.unexplained().collect();
    if !c.regions.is_empty() {
        if unexplained.is_empty() {
            s.push_str("\nThe auditory map finds no difference the readings do not explain.\n");
        } else {
            s.push_str("\n## Unexplained: the auditory map differs where no reading does\n\n");
            for r in unexplained {
                let _ = writeln!(
                    s,
                    "- {:.0}–{:.0} ms, {:.0}–{:.0} Hz: {:+.1} dB (the frames differ from {:.0} to {:.0} ms)",
                    r.window_ms.0,
                    r.window_ms.1,
                    r.band_hz.0,
                    r.band_hz.1,
                    r.difference_db,
                    r.where_ms.0,
                    r.where_ms.1
                );
            }
        }
    }
    let files: Vec<&crate::compare::Finding> = c.file_differences().collect();
    if !files.is_empty() {
        s.push_str("\n## The files differ (not the sound)\n\n");
        for f in files {
            let _ = writeln!(
                s,
                "- {}{}: {} → {} {}",
                f.label,
                where_text(f.window_ms, f.band_hz),
                short(f.reference),
                short(f.candidate),
                f.unit.symbol()
            );
        }
    }
    let within = c.findings.iter().filter(|f| f.units < 1.0).count();
    let unsettled = c
        .findings
        .iter()
        .filter(|f| f.units >= 1.0 && f.established == Some(false))
        .count();
    let _ = writeln!(
        s,
        "\n{within} readings differ by less than their threshold{}.",
        if unsettled > 0 {
            format!("; {unsettled} more pass it but are not established against the takes")
        } else {
            String::new()
        }
    );
    s
}

/// How many kinds of difference the headline shows.
pub const HEADLINE: usize = 12;

/// One finding in a line: its phrase, the reading, where, and both values in threshold units.
#[must_use]
pub fn finding_line(f: &crate::compare::Finding) -> String {
    format!(
        "**{}**: {}{} {} → {} {} ({:.1}×)",
        f.phrase.clone().unwrap_or_else(|| f.label.to_string()),
        f.label.to_lowercase(),
        where_text(f.window_ms, f.band_hz),
        short(f.reference),
        short(f.candidate),
        f.unit.symbol(),
        f.units
    )
}

fn where_text(window: Option<(f64, f64)>, band: Option<(f64, f64)>) -> String {
    let mut p = Vec::new();
    if let Some((a, b)) = window {
        p.push(if (a - b).abs() < 1e-9 {
            format!("at {a:.0} ms")
        } else {
            format!("{a:.0}–{b:.0} ms")
        });
    }
    if let Some((lo, hi)) = band {
        p.push(format!("{lo:.0}–{hi:.0} Hz"));
    }
    if p.is_empty() {
        String::new()
    } else {
        format!(" ({})", p.join(", "))
    }
}

fn short(x: f64) -> String {
    if x.abs() >= 100.0 {
        format!("{x:.0}")
    } else if x.abs() >= 10.0 {
        format!("{x:.1}")
    } else {
        format!("{x:.2}")
    }
}

/// A table of numbers, one row per item; at most 16 rows are printed.
fn data_table(s: &mut String, t: &Table) {
    let _ = writeln!(
        s,
        "
{}:
",
        t.title
    );
    if t.rows.is_empty() {
        s.push_str(
            "(none found)
",
        );
        return;
    }
    let header: Vec<String> = t
        .columns
        .iter()
        .map(|(label, unit)| {
            if unit.symbol().is_empty() {
                (*label).to_string()
            } else {
                format!("{label} ({})", unit.symbol())
            }
        })
        .collect();
    let _ = writeln!(s, "| {} |", header.join(" | "));
    let _ = writeln!(s, "|{}", "---|".repeat(t.columns.len()));
    for row in t.rows.iter().take(16) {
        let cells: Vec<String> = row
            .iter()
            .map(|v| match v {
                Some(x) if x.abs() >= 100.0 => format!("{x:.0}"),
                Some(x) if x.abs() >= 10.0 => format!("{x:.1}"),
                Some(x) => format!("{x:.2}"),
                None => "—".into(),
            })
            .collect();
        let _ = writeln!(s, "| {} |", cells.join(" | "));
    }
    if t.rows.len() > 16 {
        let _ = writeln!(
            s,
            "
…and {} more.",
            t.rows.len() - 16
        );
    }
}

fn value(r: &Reading) -> String {
    match (&r.validity, r.value) {
        (Validity::Absent(why), _) => format!("— ({why})"),
        (_, None) => "—".into(),
        (_, Some(x)) if r.unit == crate::reading::Unit::Sign => {
            if x > 0.0 {
                "+1 (up)".into()
            } else {
                "−1 (down)".into()
            }
        }
        (v, Some(x)) => {
            let mark = match v {
                Validity::BelowFloor => " ~",
                Validity::Sample => " †",
                Validity::Unresolved => " ≤",
                _ => "",
            };
            let digits = if x.abs() >= 100.0 {
                0
            } else if x.abs() >= 10.0 {
                1
            } else {
                2
            };
            let unit = r.unit.symbol();
            let space = if unit.is_empty() || unit == "×" || unit == "±" || unit == "%" {
                ""
            } else {
                " "
            };
            format!("{x:.digits$}{space}{unit}{mark}")
        }
    }
}

fn place(r: &Reading) -> String {
    let mut p = Vec::new();
    if let Some((a, b)) = r.window_ms {
        if (a - b).abs() < 1e-9 {
            p.push(format!("at {a:.1} ms"));
        } else {
            p.push(format!("{a:.0}–{b:.0} ms"));
        }
    }
    if let Some((lo, hi)) = r.band_hz {
        p.push(format!("{lo:.0}–{hi:.0} Hz"));
    }
    p.join(", ")
}

fn table(s: &mut String, readings: &[&Reading]) {
    if readings.is_empty() {
        return;
    }
    s.push_str("| Measure | Where | Value |\n|---|---|---|\n");
    for r in readings {
        let _ = writeln!(s, "| {} | {} | {} |", r.label, place(r), value(r));
    }
}

/// The octave levels as a grid: one row per band, one column per time.
fn band_grid(s: &mut String, section: &Section) {
    let levels: Vec<&Reading> = section
        .readings
        .iter()
        .filter(|r| r.id == "tone.band_level")
        .collect();
    if levels.is_empty() {
        return;
    }
    let mut times: Vec<f64> = Vec::new();
    let mut bands: Vec<(f64, f64)> = Vec::new();
    for r in &levels {
        if let Some((a, b)) = r.window_ms {
            let t = 0.5 * (a + b);
            if !times.iter().any(|&x| (x - t).abs() < 1e-9) {
                times.push(t);
            }
        }
        if let Some(band) = r.band_hz {
            if !bands.contains(&band) {
                bands.push(band);
            }
        }
    }
    s.push_str("Octave levels, dB against the loudest 10 ms:\n\n| Band | ");
    let header: Vec<String> = times.iter().map(|t| format!("{t:.0} ms")).collect();
    let _ = writeln!(s, "{} |", header.join(" | "));
    let _ = writeln!(s, "|---|{}", "---|".repeat(times.len()));
    for band in &bands {
        let _ = write!(s, "| {:.0}–{:.0} Hz |", band.0, band.1);
        for t in &times {
            let cell = levels
                .iter()
                .find(|r| {
                    r.band_hz == Some(*band)
                        && r.window_ms
                            .is_some_and(|(a, b)| (0.5 * (a + b) - t).abs() < 1e-9)
                })
                .map_or("—".to_string(), |r| match (&r.validity, r.value) {
                    (Validity::Absent(_), _) | (_, None) => "—".into(),
                    (Validity::BelowFloor, Some(v)) => format!("{v:.0}~"),
                    (_, Some(v)) => format!("{v:.0}"),
                });
            let _ = write!(s, " {cell} |");
        }
        s.push('\n');
    }
    s.push('\n');
}
