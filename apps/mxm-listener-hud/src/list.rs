//! The LIST view: every reading of the report at once, as H0 showed it — plain and complete, each row
//! explained on hover. The HUD's zoom shows the same readings a part at a time.

use mxm_listening::glossary;
use mxm_listening::prep::{Codec, Container};
use mxm_listening::reading::{Reading, Report, Section, Table, Validity};

use crate::scene::number;

/// Every part, every reading and every table.
pub fn report_list(ui: &mut egui::Ui, report: &Report) {
    ui.label(format!(
        "Family: {} ({})",
        report.family.family.name(),
        report.family.how
    ));
    for note in &report.notes {
        ui.label(format!("Note: {note}"));
    }
    for (i, section) in report.sections.iter().enumerate() {
        egui::CollapsingHeader::new(format!(
            "{} — {} readings",
            section.title,
            section.readings.len()
        ))
        .id_salt(("section", i, section.part))
        .default_open(true)
        .show(ui, |ui| {
            readings_grid(ui, i, section);
            for (j, table) in section.tables.iter().enumerate() {
                table_grid(ui, (i, j), table);
            }
        });
    }
}

fn readings_grid(ui: &mut egui::Ui, i: usize, section: &Section) {
    if section.readings.is_empty() {
        return;
    }
    egui::Grid::new(("readings", i))
        .striped(true)
        .num_columns(7)
        .show(ui, |ui| {
            for heading in [
                "Reading",
                "Value",
                "Where",
                "Level",
                "Resolution",
                "Validity",
                "Source",
            ] {
                ui.strong(heading);
            }
            ui.end_row();
            for reading in &section.readings {
                ui.label(reading.label).on_hover_text(explained(reading.id));
                ui.label(value(reading));
                ui.label(place(reading));
                ui.label(
                    reading
                        .level_db
                        .map(|db| format!("{} dB", number(db)))
                        .unwrap_or_default(),
                );
                ui.label(resolution(reading));
                ui.label(validity(&reading.validity));
                ui.weak(reading.source);
                ui.end_row();
            }
        });
}

fn table_grid(ui: &mut egui::Ui, id: (usize, usize), table: &Table) {
    let window = table
        .window_ms
        .map(|(a, b)| format!(" · {}–{} ms", number(a), number(b)))
        .unwrap_or_default();
    ui.label(format!("{}{window}", table.title))
        .on_hover_text(explained(table.id));
    egui::Grid::new(("table", id.0, id.1))
        .striped(true)
        .num_columns(table.columns.len())
        .show(ui, |ui| {
            for (name, unit) in &table.columns {
                let symbol = unit.symbol();
                if symbol.is_empty() {
                    ui.strong(*name);
                } else {
                    ui.strong(format!("{name} ({symbol})"));
                }
            }
            ui.end_row();
            for row in &table.rows {
                for cell in row {
                    ui.label(cell.map(number).unwrap_or_else(|| "—".into()));
                }
                ui.end_row();
            }
        });
    ui.weak(table.source);
}

/// What an id means, from the listener's glossary, and the id itself.
fn explained(id: &str) -> String {
    match glossary::meaning(id) {
        Some(meaning) => format!("{meaning}\n\n{id}"),
        None => id.to_string(),
    }
}

fn value(r: &Reading) -> String {
    match r.value {
        Some(v) => format!("{} {}", number(v), r.unit.symbol())
            .trim_end()
            .to_string(),
        None => "—".into(),
    }
}

fn place(r: &Reading) -> String {
    let window = r
        .window_ms
        .map(|(a, b)| format!("{}–{} ms", number(a), number(b)));
    let band = r
        .band_hz
        .map(|(a, b)| format!("{}–{} Hz", number(a), number(b)));
    [window, band]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
}

fn resolution(r: &Reading) -> String {
    let Some(res) = r.resolution else {
        return String::new();
    };
    let mut parts = vec![format!("{} ms window", number(res.window_ms))];
    if let Some(bin) = res.bin_hz {
        parts.push(format!("{} Hz bins", number(bin)));
    }
    if let Some(span) = res.span {
        parts.push(format!("±{}", number(span)));
    }
    parts.join(" · ")
}

fn validity(v: &Validity) -> String {
    match v {
        Validity::Valid => String::new(),
        Validity::BelowFloor => "below the floor".into(),
        Validity::Sample => "one draw".into(),
        Validity::Unresolved => "unresolved".into(),
        Validity::Absent(why) => format!("absent: {why}"),
    }
}

#[must_use]
pub fn codec_name(codec: Codec) -> &'static str {
    match codec {
        Codec::Pcm => "PCM",
        Codec::Flac => "FLAC",
        Codec::Alac => "ALAC",
        Codec::Mp3 => "MP3 (lossy)",
        Codec::Aac => "AAC (lossy)",
        Codec::Vorbis => "Vorbis (lossy)",
    }
}

#[must_use]
pub fn container_name(container: Container) -> &'static str {
    match container {
        Container::Wav => "WAV",
        Container::Aiff => "AIFF",
        Container::Flac => "FLAC",
        Container::Mp4 => "MP4",
        Container::Ogg => "Ogg",
        Container::Mp3 => "MP3",
        Container::Other(name) => name,
    }
}
