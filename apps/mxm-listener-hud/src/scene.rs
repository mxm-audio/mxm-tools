//! What the HUD shows, decided without drawing (the plan's §2.1): the pitch the crosshair locks on,
//! the partials and modes as blips, the parts as glyphs with their key readings, the headline and the
//! stages the timeline brackets. **Every number here is a reading's or a table cell's**, carried with
//! the id it came from; the scene picks and groups, it never computes a number about the sound.

use mxm_listening::Family;
use mxm_listening::parts::note::nearest_note;
use mxm_listening::reading::{Reading, Report, Table, Unit, Validity};

/// The group a part belongs to. Each has one colour and one shape, fixed everywhere, so a colour is
/// never the only cue (the owner's working preference: readable without red/green discrimination,
/// because the owner is red-green colour-blind).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// The attack, the level and the fall: triangles.
    Time,
    /// Pitch, partials and lines: circles.
    Ring,
    /// Texture and modulation: diamonds.
    Noise,
    /// How it sounds — tone, the words, the hearing model, a space: hexagons.
    Character,
    /// The recording chain, and anything else: squares.
    Chain,
}

impl Role {
    #[must_use]
    pub fn of(part: &str) -> Role {
        match part {
            "attack" | "level" | "decay" | "delay" => Role::Time,
            "pitch" | "note" | "sustain" | "voice" | "tonality" | "response" => Role::Ring,
            "texture" | "modulation" => Role::Noise,
            "tone" | "words" | "perception" | "space" => Role::Character,
            _ => Role::Chain,
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Role::Time => "TIME",
            Role::Ring => "RING",
            Role::Noise => "NOISE",
            Role::Character => "CHARACTER",
            Role::Chain => "CHAIN",
        }
    }
}

/// A reading as shown: its value and everything that says where it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub id: &'static str,
    pub label: &'static str,
    pub value: Option<f64>,
    pub unit: Unit,
    pub validity: Validity,
    pub window_ms: Option<(f64, f64)>,
    pub band_hz: Option<(f64, f64)>,
}

impl Shown {
    #[must_use]
    pub fn of(r: &Reading) -> Self {
        Self {
            id: r.id,
            label: r.label,
            value: r.value,
            unit: r.unit,
            validity: r.validity.clone(),
            window_ms: r.window_ms,
            band_hz: r.band_hz,
        }
    }
}

/// One part of the sound, as a glyph.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub part: &'static str,
    pub title: &'static str,
    pub role: Role,
    /// Its most telling readings, in order; the first is the glyph's number.
    pub key: Vec<Shown>,
    pub readings: usize,
    /// Readings holding a value.
    pub present: usize,
    pub tables: usize,
    /// Its index in the report's sections.
    pub section: usize,
}

/// What the crosshair locks on.
#[derive(Clone, Debug, PartialEq)]
pub enum Lock {
    Locked {
        /// The reading the pitch is.
        id: &'static str,
        /// `rest pitch`, `fundamental`, …
        what: &'static str,
        hz: f64,
        /// The nearest equal-tempered note and the distance from it — the listener's own conversion.
        note: String,
        cents: f64,
    },
    NoLock {
        why: String,
    },
}

/// A partial or a mode, as a blip on the ring.
#[derive(Clone, Debug, PartialEq)]
pub struct Blip {
    pub hz: f64,
    /// Its level, dB, against the table's reference (the fundamental, or the window's start).
    pub level_db: Option<f64>,
    /// How long it rings (T60), s.
    pub ring_s: Option<f64>,
    /// The table it is a row of.
    pub table: &'static str,
}

/// Where the timeline's brackets fall, ms from the onset, each a reading's value or absent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stages {
    /// The attack ends at the peak (`attack.peak_time`).
    pub attack_ms: Option<f64>,
    /// The body ends 20 dB down (`attack.peak_time` + `decay.t20`).
    pub body_ms: Option<f64>,
    /// The tail ends 40 dB down (`attack.peak_time` + `decay.t40`).
    pub tail_ms: Option<f64>,
}

/// The whole scene.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub family: Family,
    /// `declared`, or how it was detected.
    pub how: String,
    pub lock: Lock,
    pub blips: Vec<Blip>,
    pub parts: Vec<Part>,
    /// A few readings across the top.
    pub headline: Vec<Shown>,
    pub stages: Stages,
    pub duration_ms: f64,
    pub onset_ms: Option<f64>,
    pub readings: usize,
    pub notes: Vec<String>,
}

/// Each part's most telling readings, in order. A part not named here shows its first readings.
const KEYS: &[(&str, &[&str])] = &[
    (
        "attack",
        &[
            "attack.rise_time",
            "attack.peak_time",
            "attack.early_centroid",
        ],
    ),
    (
        "level",
        &["level.peak", "level.body_loudness", "level.crest"],
    ),
    ("decay", &["decay.t40", "decay.t20", "decay.late_slope"]),
    (
        "pitch",
        &["pitch.rest", "pitch.attack_against_rest", "pitch.salience"],
    ),
    ("tone", &["tone.centroid", "tone.median", "tone.resonance"]),
    ("tonality", &["tonality.tonal_share", "tonality.peaks"]),
    (
        "words",
        &["words.brightness", "words.warmth", "words.hardness"],
    ),
    (
        "texture",
        &["texture.rattle", "texture.buzz_share", "texture.slap_crest"],
    ),
    (
        "modulation",
        &[
            "modulation.ring_wobble",
            "modulation.roughness",
            "modulation.ring_share",
        ],
    ),
    (
        "artefacts",
        &[
            "artefacts.noise_floor",
            "artefacts.click",
            "artefacts.pre_onset",
        ],
    ),
    (
        "perception",
        &[
            "perception.loudness_peak",
            "perception.sharpness",
            "perception.roughness",
        ],
    ),
    (
        "note",
        &["note.fundamental", "note.t60", "note.inharmonicity"],
    ),
    (
        "sustain",
        &[
            "sustain.vibrato_rate",
            "sustain.vibrato_depth",
            "sustain.hnr",
        ],
    ),
    (
        "voice",
        &["voice.attack", "voice.aliasing", "voice.peak_hz"],
    ),
    (
        "response",
        &[
            "response.peak_hz",
            "response.low_edge",
            "response.high_edge",
        ],
    ),
    ("delay", &["delay.time", "delay.feedback", "delay.first"]),
    ("space", &["space.t30", "space.edt", "space.drr"]),
];

/// The first reading with `id` that holds a value, else the first with `id`.
fn first<'a>(report: &'a Report, id: &str) -> Option<&'a Reading> {
    let all = || {
        report
            .sections
            .iter()
            .flat_map(|s| &s.readings)
            .filter(move |r| r.id == id)
    };
    all().find(|r| r.value.is_some()).or_else(|| all().next())
}

fn value(report: &Report, id: &str) -> Option<f64> {
    first(report, id).and_then(|r| r.value)
}

/// The scene of one report.
#[must_use]
pub fn build(report: &Report) -> Scene {
    let parts = report
        .sections
        .iter()
        .enumerate()
        .map(|(section, s)| {
            let named = KEYS
                .iter()
                .find(|(p, _)| *p == s.part)
                .map(|(_, ids)| *ids)
                .unwrap_or(&[]);
            let mut key: Vec<Shown> = named
                .iter()
                .filter_map(|id| {
                    let all = || s.readings.iter().filter(|r| r.id == *id);
                    all().find(|r| r.value.is_some()).or_else(|| all().next())
                })
                .map(Shown::of)
                .collect();
            if key.is_empty() {
                key = s.readings.iter().take(3).map(Shown::of).collect();
            }
            Part {
                part: s.part,
                title: s.title,
                role: Role::of(s.part),
                key,
                readings: s.readings.len(),
                present: s.readings.iter().filter(|r| r.value.is_some()).count(),
                tables: s.tables.len(),
                section,
            }
        })
        .collect();

    let peak = value(report, "attack.peak_time");
    let after_peak = |id| value(report, id).map(|v| peak.unwrap_or(0.0) + v);
    let headline_ids: &[&str] = match report.family.family {
        Family::Impulse => &["space.t30", "space.drr", "response.peak_hz"],
        Family::Note | Family::Sustained | Family::Voice => &[
            "note.t60",
            "perception.loudness_peak",
            "words.brightness",
            "words.hardness",
        ],
        _ => &[
            "decay.t40",
            "perception.loudness_peak",
            "words.brightness",
            "words.hardness",
        ],
    };
    Scene {
        family: report.family.family,
        how: report.family.how.clone(),
        lock: lock(report),
        blips: blips(report),
        parts,
        headline: headline_ids
            .iter()
            .filter_map(|id| first(report, id))
            .map(Shown::of)
            .collect(),
        stages: Stages {
            attack_ms: peak,
            body_ms: after_peak("decay.t20"),
            tail_ms: after_peak("decay.t40"),
        },
        duration_ms: report.duration_s * 1000.0,
        onset_ms: report.onset_s.map(|s| s * 1000.0),
        readings: report.sections.iter().map(|s| s.readings.len()).sum(),
        notes: report.notes.clone(),
    }
}

/// The family's pitch: a note's fundamental, a hit's rest pitch; none for an impulse response.
fn lock(report: &Report) -> Lock {
    let (id, what) = match report.family.family {
        Family::Impulse => {
            return Lock::NoLock {
                why: "an impulse response has no pitch: its response and its space are read".into(),
            };
        }
        Family::Note | Family::Sustained | Family::Voice => ("note.fundamental", "fundamental"),
        _ => ("pitch.rest", "rest pitch"),
    };
    let Some(reading) = first(report, id) else {
        return Lock::NoLock {
            why: "nothing was measured".into(),
        };
    };
    match (reading.value, &reading.validity) {
        (Some(hz), _) => match nearest_note(hz) {
            Some((note, cents)) => Lock::Locked {
                id,
                what,
                hz,
                note,
                cents,
            },
            None => Lock::NoLock {
                why: format!("the {what} is not a frequency"),
            },
        },
        (None, Validity::Absent(why)) => Lock::NoLock {
            why: format!("no {what}: {why}"),
        },
        (None, _) => Lock::NoLock {
            why: format!("no {what}"),
        },
    }
}

/// The column of `table` whose name starts with `prefix`.
fn column(table: &Table, prefix: &str) -> Option<usize> {
    table
        .columns
        .iter()
        .position(|(n, _)| n.starts_with(prefix))
}

/// The partials of a note, or the modes of a hit's first windows.
fn blips(report: &Report) -> Vec<Blip> {
    let tables: Vec<&Table> = report
        .sections
        .iter()
        .flat_map(|s| &s.tables)
        .filter(|t| t.id == "note.partials")
        .collect();
    let tables = if tables.is_empty() {
        report
            .sections
            .iter()
            .flat_map(|s| &s.tables)
            .filter(|t| t.id == "pitch.modes")
            .take(3)
            .collect()
    } else {
        tables
    };
    let mut out = Vec::new();
    for t in tables {
        let Some(f) = column(t, "Frequency") else {
            continue;
        };
        let level = column(t, "Strike level").or_else(|| column(t, "Level"));
        let t60 = column(t, "T60");
        let seconds = t60.map(|c| t.columns[c].1 == Unit::Seconds);
        for row in &t.rows {
            let Some(hz) = row.get(f).copied().flatten().filter(|hz| *hz > 0.0) else {
                continue;
            };
            out.push(Blip {
                hz,
                level_db: level.and_then(|c| row.get(c).copied().flatten()),
                ring_s: t60
                    .and_then(|c| row.get(c).copied().flatten())
                    .map(|v| if seconds == Some(true) { v } else { v / 1000.0 }),
                table: t.id,
            });
        }
    }
    out
}

/// The display form of a value: about four significant figures and the unit. Display only.
#[must_use]
pub fn number(v: f64) -> String {
    let a = v.abs();
    if a == 0.0 {
        "0".into()
    } else if a >= 1000.0 {
        format!("{v:.0}")
    } else if a >= 100.0 {
        format!("{v:.1}")
    } else if a >= 1.0 {
        format!("{v:.2}")
    } else if a >= 0.01 {
        format!("{v:.3}")
    } else {
        format!("{v:.2e}")
    }
}

/// A shown value with its unit, or what stands for its absence.
#[must_use]
pub fn with_unit(s: &Shown) -> String {
    match s.value {
        Some(v) => format!("{} {}", number(v), s.unit.symbol())
            .trim_end()
            .to_string(),
        None => "NO SIGNAL".into(),
    }
}
