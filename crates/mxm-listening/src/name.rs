//! What a file's name claims about its sound, and whether the listener hears the same.
//!
//! The owner (2026-09-28): the name *"is often useful. Though it can be misleading"*. So a name gives
//! **claims, never readings**. A claim is shown beside what the listener hears and never replaces it,
//! and [`check`] says whether the two agree. A name that turns out wrong is worth knowing too: the drum
//! A/B mapping found a set whose "tom" and "conga" names were likely swapped, and a reference an octave
//! from its model (`docs/drum-model-fitting.md` §1, in mxm-drum-machine).
//!
//! A claim is read from the name's words. A note is a letter, an optional sharp or flat, and an octave
//! (`C4`, `F#3`, `Bb2`, `Cs4`), equal-tempered with A4 = 440 Hz; two notes run together are a run's
//! range (`C4B4`, as `split` reads one). A word may suggest a family (`kick`, `piano`, `violin`,
//! `pad`, `IR`); others give a tempo (`120bpm`), a dynamic (`pp` to `fff`) or say the file is a loop.

use crate::family::Family;
use crate::reading::Report;

/// A note a name holds.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteClaim {
    /// As the listener writes it: `C#4`, `Bb2`.
    pub name: String,
    /// Equal-tempered, A4 = 440 Hz.
    pub hz: f64,
}

/// Everything a name claims. Every field is a claim, not a reading.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Claims {
    pub note: Option<NoteClaim>,
    /// A run's range: its first and last note.
    pub run: Option<(NoteClaim, NoteClaim)>,
    /// A word that suggests a family, as written, and the family.
    pub family: Option<(String, Family)>,
    pub tempo_bpm: Option<f64>,
    /// `ppp` to `fff`.
    pub dynamic: Option<String>,
    /// The name says the file is a loop: several events, which the listener reads as one sound.
    pub looped: bool,
}

impl Claims {
    /// Whether the name claims anything at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Claims::default()
    }
}

/// Words that suggest a family, matched whole and with trailing digits dropped (`kick01`, `hats`).
/// Chosen: the words sample sets and the research's recordings use; a word missing here claims
/// nothing, which is safe.
const FAMILY_WORDS: [(&[&str], Family); 5] = [
    (
        &[
            "kick",
            "bd",
            "bassdrum",
            "snare",
            "sd",
            "rim",
            "rimshot",
            "clap",
            "hat",
            "hats",
            "hh",
            "hihat",
            "cymbal",
            "crash",
            "ride",
            "tom",
            "toms",
            "conga",
            "bongo",
            "cowbell",
            "clave",
            "shaker",
            "tamb",
            "tambourine",
            "perc",
            "drum",
            "drums",
            "cajon",
            "woodblock",
        ],
        Family::Percussive,
    ),
    (
        &[
            "piano",
            "marimba",
            "xylophone",
            "vibraphone",
            "vibes",
            "glockenspiel",
            "celesta",
            "bell",
            "bells",
            "harp",
            "guitar",
            "pluck",
            "kalimba",
            "mallet",
            "rhodes",
            "epiano",
            "crotale",
            "crotales",
            "tubular",
            "pizz",
            "pizzicato",
        ],
        Family::Note,
    ),
    (
        &[
            "violin",
            "viola",
            "cello",
            "flute",
            "clarinet",
            "oboe",
            "bassoon",
            "sax",
            "saxophone",
            "trumpet",
            "horn",
            "trombone",
            "tuba",
            "bowed",
            "arco",
            "organ",
        ],
        Family::Sustained,
    ),
    (
        &[
            "synth", "saw", "sawtooth", "square", "pad", "lead", "osc", "supersaw", "pwm",
        ],
        Family::Voice,
    ),
    (&["ir", "impulse"], Family::Impulse),
];

const DYNAMICS: [&str; 6] = ["ppp", "pp", "mp", "mf", "ff", "fff"];

/// What `stem` — a file name without its extension — claims.
#[must_use]
pub fn claims(stem: &str) -> Claims {
    let tokens: Vec<&str> = stem
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '#'))
        .filter(|t| !t.is_empty())
        .collect();
    let mut out = Claims::default();
    for (i, token) in tokens.iter().enumerate() {
        let lower = token.to_ascii_lowercase();
        if out.note.is_none() && out.run.is_none() {
            match notes_in(token).as_slice() {
                [one] => out.note = Some(one.clone()),
                [first, last] => out.run = Some((first.clone(), last.clone())),
                _ => {}
            }
        }
        if out.family.is_none() {
            let word = lower.trim_end_matches(|c: char| c.is_ascii_digit());
            if let Some((_, family)) = FAMILY_WORDS.iter().find(|(words, _)| words.contains(&word))
            {
                out.family = Some(((*token).to_string(), *family));
            }
        }
        if out.tempo_bpm.is_none() {
            let number = if lower == "bpm" {
                i.checked_sub(1).and_then(|j| tokens[j].parse::<f64>().ok())
            } else if let Some(n) = lower.strip_suffix("bpm") {
                n.parse::<f64>().ok()
            } else if let Some(n) = lower.strip_prefix("bpm") {
                n.parse::<f64>().ok()
            } else {
                None
            };
            out.tempo_bpm = number.filter(|b| (30.0..=300.0).contains(b));
        }
        if out.dynamic.is_none() && DYNAMICS.contains(&lower.as_str()) {
            out.dynamic = Some(lower.clone());
        }
        if lower.trim_end_matches(|c: char| c.is_ascii_digit()) == "loop" || lower == "loops" {
            out.looped = true;
        }
    }
    out
}

/// The notes a token is made of: one (`A4`), two run together (`C4B4`), or none.
fn notes_in(token: &str) -> Vec<NoteClaim> {
    let mut out = Vec::new();
    let mut rest = token;
    while !rest.is_empty() {
        let Some((note, used)) = note_at(rest) else {
            return Vec::new();
        };
        out.push(note);
        rest = &rest[used..];
        if out.len() > 2 {
            return Vec::new();
        }
    }
    out
}

/// A note at the start of `s` — letter, accidental, octave digit — and the bytes it took.
fn note_at(s: &str) -> Option<(NoteClaim, usize)> {
    let bytes = s.as_bytes();
    let letter = bytes.first()?.to_ascii_uppercase();
    let class = match letter {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => return None,
    };
    let (shift, accidental, used) = match bytes.get(1) {
        Some(b'#' | b's') => (1, "#", 2),
        Some(b'b') if bytes.get(2).is_some_and(u8::is_ascii_digit) => (-1, "b", 2),
        _ => (0, "", 1),
    };
    let octave = bytes.get(used).filter(|b| b.is_ascii_digit())?;
    let octave = i32::from(octave - b'0');
    let midi = 12 * (octave + 1) + class + shift;
    let name = format!("{}{accidental}{octave}", letter as char);
    let hz = 440.0 * 2f64.powf(f64::from(midi - 69) / 12.0);
    Some((NoteClaim { name, hz }, used + 1))
}

/// Whether a claim and the listener agree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Agreement {
    Agrees,
    Differs,
    /// The name suggests something only a caller can declare: the listener never detects a note, a
    /// held note, a voice or an impulse response, so the claim asks to be declared.
    Suggests,
    /// The listener could not read what the claim is about.
    Unmeasured,
}

/// One claim held against the report.
#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    /// `note` or `family`.
    pub what: &'static str,
    pub agreement: Agreement,
    /// In words, with the numbers.
    pub detail: String,
}

/// A note claim agrees within half a semitone: `split`'s tolerance for a note's name.
pub const NOTE_TOLERANCE_CENTS: f64 = crate::split::NAME_TOLERANCE_CENTS;

/// Each claim held against what the listener heard in `report`. A tempo, a dynamic and a loop are not
/// checked: the listener reads no tempo and no dynamic marking.
#[must_use]
pub fn check(claims: &Claims, report: &Report) -> Vec<Check> {
    let mut out = Vec::new();
    let family = report.family.family;
    let declared = report.family.how == "declared";
    if let Some((word, claimed)) = &claims.family {
        let only_declared = matches!(
            claimed,
            Family::Note | Family::Sustained | Family::Voice | Family::Impulse
        );
        let (agreement, detail) = if *claimed == family {
            (
                Agreement::Agrees,
                format!("\"{word}\": read as a {}", family.name()),
            )
        } else if only_declared && !declared {
            (
                Agreement::Suggests,
                format!(
                    "\"{word}\" suggests a {}, which is declared, never detected; read as a {}",
                    claimed.name(),
                    family.name()
                ),
            )
        } else {
            (
                Agreement::Differs,
                format!(
                    "\"{word}\" suggests a {}; read as a {} ({})",
                    claimed.name(),
                    family.name(),
                    report.family.how
                ),
            )
        };
        out.push(Check {
            what: "family",
            agreement,
            detail,
        });
    }
    if let Some(note) = &claims.note {
        out.push(check_note(note, report));
    }
    out
}

fn check_note(note: &NoteClaim, report: &Report) -> Check {
    let hz = |id: &str| report.find(id, None).and_then(|r| r.value);
    let heard: Vec<(&str, f64)> = match report.family.family {
        Family::Note | Family::Sustained | Family::Voice => [
            ("fundamental", hz("note.fundamental")),
            ("virtual pitch", hz("note.virtual_pitch")),
        ]
        .into_iter()
        .filter_map(|(what, v)| v.map(|v| (what, v)))
        .collect(),
        _ => hz("pitch.rest")
            .map(|v| vec![("rest pitch", v)])
            .unwrap_or_default(),
    };
    let cents = |v: f64| 1200.0 * (v / note.hz).log2();
    let Some(&(what, value)) = heard
        .iter()
        .min_by(|a, b| cents(a.1).abs().total_cmp(&cents(b.1).abs()))
    else {
        return Check {
            what: "note",
            agreement: Agreement::Unmeasured,
            detail: format!(
                "{} ({:.1} Hz): no pitch was read to hold it against",
                note.name, note.hz
            ),
        };
    };
    let c = cents(value);
    let octaves = (c / 1200.0).round();
    let (agreement, detail) = if c.abs() <= NOTE_TOLERANCE_CENTS {
        (
            Agreement::Agrees,
            format!("{}: the {what} is {value:.1} Hz, {c:+.0} c", note.name),
        )
    } else if octaves != 0.0 && (c - 1200.0 * octaves).abs() <= NOTE_TOLERANCE_CENTS {
        (
            Agreement::Differs,
            format!(
                "{}: the {what} is {value:.1} Hz, {octaves:+.0} octave{} from the name",
                note.name,
                if octaves.abs() > 1.0 { "s" } else { "" }
            ),
        )
    } else {
        (
            Agreement::Differs,
            format!(
                "{}: the {what} is {value:.1} Hz, {c:+.0} c from the name",
                note.name
            ),
        )
    };
    Check {
        what: "note",
        agreement,
        detail,
    }
}
