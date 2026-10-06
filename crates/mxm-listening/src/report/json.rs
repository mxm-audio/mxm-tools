//! A report as JSON, written by hand: the crate takes no serialisation dependency (the plan's §1).
//! Absent values are `null`, never zero; the validity says why.

use std::fmt::Write as _;

use crate::reading::{Reading, Report, Table};

/// The report as a JSON document.
#[must_use]
pub fn to_json(r: &Report) -> String {
    let mut s = String::new();
    s.push_str("{\n");
    let _ = writeln!(s, "  \"name\": {},", string(&r.name));
    let _ = writeln!(s, "  \"rate\": {},", r.rate);
    let _ = writeln!(s, "  \"duration_s\": {},", number(Some(r.duration_s)));
    let _ = writeln!(
        s,
        "  \"family\": {{\"family\": {}, \"how\": {}}},",
        string(r.family.family.name()),
        string(&r.family.how)
    );
    let _ = writeln!(s, "  \"onset_s\": {},", number(r.onset_s));
    let notes: Vec<String> = r.notes.iter().map(|n| string(n)).collect();
    let _ = writeln!(s, "  \"notes\": [{}],", notes.join(", "));
    s.push_str("  \"sections\": [\n");
    for (i, section) in r.sections.iter().enumerate() {
        let _ = writeln!(
            s,
            "    {{\"part\": {}, \"title\": {}, \"readings\": [",
            string(section.part),
            string(section.title)
        );
        for (j, reading) in section.readings.iter().enumerate() {
            let comma = if j + 1 < section.readings.len() {
                ","
            } else {
                ""
            };
            let _ = writeln!(s, "      {}{comma}", reading_json(reading));
        }
        s.push_str("    ], \"tables\": [");
        let tables: Vec<String> = section.tables.iter().map(table_json).collect();
        s.push_str(&tables.join(", "));
        let comma = if i + 1 < r.sections.len() { "," } else { "" };
        let _ = writeln!(s, "]}}{comma}");
    }
    s.push_str("  ]\n}\n");
    s
}

/// A comparison as JSON: every finding, most audible first.
#[must_use]
pub fn comparison_to_json(c: &crate::compare::Comparison) -> String {
    let pair = |p: Option<(f64, f64)>| match p {
        Some((a, b)) => format!("[{}, {}]", number(Some(a)), number(Some(b))),
        None => "null".into(),
    };
    let findings: Vec<String> = c
        .findings
        .iter()
        .map(|f| {
            format!(
                "{{\"id\": {}, \"part\": {}, \"label\": {}, \"phrase\": {}, \"window_ms\": {}, \"band_hz\": {}, \"unit\": {}, \"reference\": {}, \"candidate\": {}, \"threshold\": {}, \"threshold_kind\": {}, \"threshold_source\": {}, \"units\": {}, \"salience\": {}, \"eq_reachable\": {}, \"sample\": {}, \"support\": {}, \"spread\": {}, \"established\": {}, \"score\": {}, \"detail\": {}}}",
                string(f.id),
                string(f.part),
                string(f.label),
                f.phrase.as_deref().map_or("null".into(), string),
                pair(f.window_ms),
                pair(f.band_hz),
                string(f.unit.symbol()),
                number(Some(f.reference)),
                number(Some(f.candidate)),
                number(Some(f.threshold.value)),
                string(match f.threshold.kind {
                    crate::audibility::Kind::Absolute => "absolute",
                    crate::audibility::Kind::Relative => "relative",
                }),
                string(&f.threshold.source),
                number(Some(f.units)),
                number(Some(f.salience)),
                f.eq_reachable,
                f.sample,
                f.support.map_or("null".into(), |(a, n)| format!("[{a}, {n}]")),
                number(f.spread),
                f.established.map_or("null".into(), |e| e.to_string()),
                number(Some(f.score)),
                f.detail.as_deref().map_or("null".into(), string)
            )
        })
        .collect();
    let notes: Vec<String> = c.notes.iter().map(|n| string(n)).collect();
    let regions: Vec<String> = c
        .regions
        .iter()
        .map(|r| {
            format!(
                "{{\"window_ms\": [{}, {}], \"band_hz\": [{}, {}], \"difference_db\": {}, \"where_ms\": [{}, {}], \"explained\": {}}}",
                number(Some(r.window_ms.0)),
                number(Some(r.window_ms.1)),
                number(Some(r.band_hz.0)),
                number(Some(r.band_hz.1)),
                number(Some(r.difference_db)),
                number(Some(r.where_ms.0)),
                number(Some(r.where_ms.1)),
                r.explained
            )
        })
        .collect();
    format!(
        "{{\n\"reference\": {},\n\"candidate\": {},\n\"notes\": [{}],\n\"findings\": [\n  {}\n],\n\"regions\": [\n  {}\n]\n}}\n",
        string(&c.reference),
        string(&c.candidate),
        notes.join(", "),
        findings.join(",\n  "),
        regions.join(",\n  ")
    )
}

/// A set's summary as JSON: every reading's statistics per group.
#[must_use]
pub fn set_to_json(r: &crate::set::SetReport) -> String {
    let groups: Vec<String> = r
        .groups
        .iter()
        .map(|g| {
            let files: Vec<String> = g.files.iter().map(|f| string(f)).collect();
            let stats: Vec<String> = g
                .stats
                .iter()
                .map(|st| {
                    let pair = |p: Option<(f64, f64)>| match p {
                        Some((a, b)) => format!("[{}, {}]", number(Some(a)), number(Some(b))),
                        None => "null".into(),
                    };
                    format!(
                        "{{\"id\": {}, \"unit\": {}, \"window_ms\": {}, \"band_hz\": {}, \"mean\": {}, \"spread\": {}, \"min\": {}, \"max\": {}, \"n\": {}, \"of\": {}}}",
                        string(st.id),
                        string(st.unit.symbol()),
                        pair(st.window_ms),
                        pair(st.band_hz),
                        number(Some(st.mean)),
                        number(Some(st.spread)),
                        number(Some(st.min)),
                        number(Some(st.max)),
                        st.n,
                        st.of
                    )
                })
                .collect();
            format!(
                "{{\"name\": {}, \"files\": [{}], \"stats\": [\n    {}\n  ]}}",
                string(&g.name),
                files.join(", "),
                stats.join(",\n    ")
            )
        })
        .collect();
    let velocity: Vec<String> = r
        .velocity_map
        .iter()
        .map(|(g, d)| format!("{{\"group\": {}, \"db\": {}}}", string(g), number(Some(*d))))
        .collect();
    let lines: Vec<String> = r.room_lines.iter().map(|v| number(Some(*v))).collect();
    format!(
        "{{\n\"groups\": [\n  {}\n],\n\"velocity_map\": [{}],\n\"room_lines_hz\": [{}]\n}}\n",
        groups.join(",\n  "),
        velocity.join(", "),
        lines.join(", ")
    )
}

fn table_json(t: &Table) -> String {
    let columns: Vec<String> = t
        .columns
        .iter()
        .map(|(label, unit)| {
            format!(
                "{{\"label\": {}, \"unit\": {}}}",
                string(label),
                string(unit.symbol())
            )
        })
        .collect();
    let rows: Vec<String> = t
        .rows
        .iter()
        .map(|row| {
            format!(
                "[{}]",
                row.iter()
                    .map(|v| number(*v))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect();
    let window = match t.window_ms {
        Some((a, b)) => format!("[{}, {}]", number(Some(a)), number(Some(b))),
        None => "null".into(),
    };
    format!(
        "{{\"id\": {}, \"title\": {}, \"window_ms\": {}, \"columns\": [{}], \"rows\": [{}], \"source\": {}}}",
        string(t.id),
        string(&t.title),
        window,
        columns.join(", "),
        rows.join(", "),
        string(t.source)
    )
}

fn reading_json(r: &Reading) -> String {
    let pair = |p: Option<(f64, f64)>| match p {
        Some((a, b)) => format!("[{}, {}]", number(Some(a)), number(Some(b))),
        None => "null".into(),
    };
    let resolution = match r.resolution {
        Some(res) => format!(
            "{{\"window_ms\": {}, \"bin_hz\": {}, \"span\": {}}}",
            number(Some(res.window_ms)),
            number(res.bin_hz),
            number(res.span)
        ),
        None => "null".into(),
    };
    format!(
        "{{\"id\": {}, \"label\": {}, \"value\": {}, \"unit\": {}, \"window_ms\": {}, \"band_hz\": {}, \"resolution\": {}, \"validity\": {}, \"source\": {}}}",
        string(r.id),
        string(r.label),
        number(r.value),
        string(r.unit.symbol()),
        pair(r.window_ms),
        pair(r.band_hz),
        resolution,
        string(&super::validity_text(&r.validity)),
        string(r.source)
    )
}

/// A finite number, or `null`.
fn number(v: Option<f64>) -> String {
    match v {
        Some(x) if x.is_finite() => {
            let text = format!("{x}");
            if text.contains('.') || text.contains('e') || text.contains("inf") {
                text
            } else {
                format!("{text}.0")
            }
        }
        _ => "null".into(),
    }
}

/// A JSON string literal.
fn string(v: &str) -> String {
    let mut out = String::with_capacity(v.len() + 2);
    out.push('"');
    for ch in v.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
