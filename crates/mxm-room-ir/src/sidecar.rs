//! The JSON sidecar written beside every WAV: how the file was made, and what it measures.
//!
//! Hand-written JSON, because the shape is small and fixed. A non-finite number and a missing
//! metric are both written as `null`, never as a token a JSON reader would reject.

use std::fmt::Write;

use crate::analysis::{Analysis, DecayMetrics};
use crate::bands::NOMINAL_CENTRES_HZ;
use crate::directivity::PlacedCapsule;
use crate::render::{RenderOptions, Rendered};
use crate::simulation::Simulation;

/// A JSON object written field by field, in insertion order.
#[derive(Debug, Default)]
pub struct JsonObject {
    fields: Vec<(String, String)>,
}

impl JsonObject {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn string(mut self, key: &str, value: &str) -> Self {
        self.fields.push((key.into(), string(value)));
        self
    }

    pub fn number(mut self, key: &str, value: f64) -> Self {
        self.fields.push((key.into(), number(value)));
        self
    }

    pub fn optional(mut self, key: &str, value: Option<f64>) -> Self {
        self.fields
            .push((key.into(), value.map_or("null".into(), number)));
        self
    }

    pub fn integer(mut self, key: &str, value: u64) -> Self {
        self.fields.push((key.into(), value.to_string()));
        self
    }

    pub fn boolean(mut self, key: &str, value: bool) -> Self {
        self.fields.push((key.into(), value.to_string()));
        self
    }

    /// A value that is already JSON.
    pub fn raw(mut self, key: &str, json: String) -> Self {
        self.fields.push((key.into(), json));
        self
    }

    pub fn strings(self, key: &str, values: &[&str]) -> Self {
        let items: Vec<String> = values.iter().map(|v| string(v)).collect();
        self.raw(key, format!("[{}]", items.join(", ")))
    }

    pub fn finish(&self) -> String {
        let mut s = String::from("{");
        for (i, (k, v)) in self.fields.iter().enumerate() {
            let _ = write!(s, "{}{}: {}", if i == 0 { "" } else { ", " }, string(k), v);
        }
        s.push('}');
        s
    }

    /// The object with one field per line at the top level, for reading in a diff.
    pub fn pretty(&self) -> String {
        let mut s = String::from("{\n");
        for (i, (k, v)) in self.fields.iter().enumerate() {
            let comma = if i + 1 == self.fields.len() { "" } else { "," };
            let _ = writeln!(s, "  {}: {v}{comma}", string(k));
        }
        s.push_str("}\n");
        s
    }
}

/// Everything a sidecar records about one rendered file.
pub struct Sidecar<'a> {
    pub simulation: &'a Simulation,
    pub array_name: &'a str,
    pub capsules: &'a [PlacedCapsule],
    pub render: &'a RenderOptions,
    pub rendered: &'a Rendered,
    /// Metrics of the full-response render (direct path included, uncapped), never of the file
    /// itself when it is direct-free or capped.
    pub full_response_analysis: Option<&'a Analysis>,
    /// Further top-level fields, already JSON (the catalogue's position, the wave solver's seam).
    pub extra: Vec<(String, String)>,
    pub notes: &'a [&'a str],
}

impl Sidecar<'_> {
    pub fn to_json(&self) -> String {
        let sim = self.simulation;
        let scene = &sim.scene;
        let rendered = self.rendered;
        let peak = rendered
            .channels
            .iter()
            .flat_map(|c| c.iter())
            .fold(0.0f32, |m, v| m.max(v.abs()));
        let capsules: Vec<String> = self
            .capsules
            .iter()
            .map(|c| {
                JsonObject::new()
                    .string("name", &c.name)
                    .raw(
                        "offset_m",
                        format!(
                            "[{}, {}, {}]",
                            number(c.offset.x),
                            number(c.offset.y),
                            number(c.offset.z)
                        ),
                    )
                    .string("directivity", &c.directivity.canonical_text())
                    .finish()
            })
            .collect();
        let names: Vec<&str> = rendered.channel_names.iter().map(String::as_str).collect();
        let rays = &sim.trace;
        let mut object = JsonObject::new()
            .string(
                "generator",
                concat!("mxm-room-ir ", env!("CARGO_PKG_VERSION")),
            )
            .string("scene", &scene.name)
            .string("scene_hash_fnv1a64", &scene.hash_hex())
            .integer("sample_rate_hz", u64::from(rendered.sample_rate))
            .integer("channels", rendered.channels.len() as u64)
            .strings("channel_names", &names)
            .integer("frames", rendered.samples().len() as u64)
            .number("duration_s", rendered.duration_s())
            .string("array", self.array_name)
            .raw("capsules", format!("[{}]", capsules.join(", ")))
            .string(
                "source_directivity",
                &scene.source_directivity.canonical_text(),
            )
            .boolean("non_diffuse", scene.non_diffuse)
            .boolean("direct_included", rendered.direct_included)
            .boolean("air_absorption", self.render.air_absorption)
            .raw(
                "air",
                JsonObject::new()
                    .number("temperature_c", scene.air.temperature_c)
                    .number("relative_humidity_pct", scene.air.relative_humidity_pct)
                    .number("pressure_kpa", scene.air.pressure_kpa)
                    .finish(),
            )
            .number("room_volume_m3", scene.room.volume())
            .number("room_surface_m2", scene.room.surface_area())
            .string("time_zero", "earliest arrival over the set, on a sample")
            .integer("lead_samples", rendered.lead_samples as u64)
            .number("trimmed_propagation_delay_s", rendered.trimmed_delay_s)
            .number(
                "reference_distance_m",
                self.render.reference_distance_m * rendered.gain,
            )
            .optional("normalized_peak_dbfs", self.render.normalize_peak_dbfs)
            .string(
                "level",
                "pressure 1 is the direct sound at the reference distance, which carries a normalized file's gain",
            )
            .number("peak_abs", f64::from(peak))
            .number("speed_of_sound_m_per_s", scene.air.speed_of_sound())
            .integer(
                "image_source_max_order",
                sim.options.image_sources.max_order as u64,
            )
            .integer("image_source_arrivals", sim.image_source_arrivals as u64)
            .integer("ray_assisted_arrivals", sim.ray_assisted_arrivals as u64)
            .integer("arrivals_rendered", rendered.arrivals_rendered as u64)
            .raw(
                "rays",
                JsonObject::new()
                    .integer("count", rays.rays as u64)
                    .integer("seed", rays.seed)
                    .number("receiver_radius_m", rays.receiver_radius_m)
                    .number("discovery_radius_m", rays.discovery_radius_m)
                    .number("bin_width_s", sim.options.rays.bin_width_s)
                    .integer("leaked", rays.leaked as u64)
                    .integer("hits", rays.hits)
                    .finish(),
            )
            .raw(
                "late",
                JsonObject::new()
                    .integer("events", rendered.late_events as u64)
                    .integer("seed", self.render.late.seed)
                    .number("min_rate_hz", self.render.late.min_rate_hz)
                    .number("max_rate_hz", self.render.late.max_rate_hz)
                    .finish(),
            )
            .raw(
                "crossover_hz",
                rendered.crossover_hz.map_or("null".into(), |(lo, hi)| {
                    format!("[{}, {}]", number(lo), number(hi))
                }),
            )
            .raw(
                "cap",
                rendered.capped.map_or("null".into(), |c| {
                    JsonObject::new()
                        .number("length_s", c.length_s)
                        .number("fade_s", c.fade_s)
                        .finish()
                }),
            );
        for (k, v) in &self.extra {
            object = object.raw(k, v.clone());
        }
        let analysis = self
            .full_response_analysis
            .map_or("null".to_string(), analysis_json);
        object
            .raw("full_response_analysis", analysis)
            .strings("notes", self.notes)
            .pretty()
    }
}

/// An analysis as JSON: onset, broadband and per-octave ISO 3382 parameters.
pub fn analysis_json(a: &Analysis) -> String {
    let bands: Vec<String> = a
        .bands
        .iter()
        .zip(NOMINAL_CENTRES_HZ)
        .map(|(m, f)| metrics(m).number("centre_hz", f).finish())
        .collect();
    JsonObject::new()
        .integer("onset_sample", a.onset_sample as u64)
        .raw("broadband", metrics(&a.broadband).finish())
        .raw("octaves", format!("[{}]", bands.join(", ")))
        .finish()
}

fn metrics(m: &DecayMetrics) -> JsonObject {
    JsonObject::new()
        .optional("edt_s", m.edt_s)
        .optional("t20_s", m.t20_s)
        .optional("t30_s", m.t30_s)
        .optional("c50_db", m.c50_db)
        .optional("c80_db", m.c80_db)
        .optional("d50", m.d50)
        .optional("ts_s", m.ts_s)
}

fn number(v: f64) -> String {
    if v.is_finite() {
        format!("{v}")
    } else {
        "null".to_string()
    }
}

fn string(v: &str) -> String {
    let mut out = String::from("\"");
    for c in v.chars() {
        match c {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_and_non_finite() {
        assert_eq!(string("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
        assert_eq!(number(f64::INFINITY), "null");
        assert_eq!(number(0.25), "0.25");
        let o = JsonObject::new()
            .string("a", "x")
            .optional("b", None)
            .integer("c", 3);
        assert_eq!(o.finish(), "{\"a\": \"x\", \"b\": null, \"c\": 3}");
    }
}
