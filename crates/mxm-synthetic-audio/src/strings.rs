//! The bowed and plucked strings: Applied Acoustics Systems' String Studio VS-3, a physical model,
//! rendered offline one note at a time.
//!
//! **A note is tuned before it sounds, never after.** VS-3 can't follow a pitch change exactly once
//! a string sounds (measured 2026-10-19: a bend up waits about 75 ms and then jumps, a bend down
//! glides over about 20 ms), so every note here holds one pitch: its key, and a static pitch bend
//! set before the note starts. The bend is found by rendering the note, measuring its steady pitch
//! (`measure::steady_pitch`) and correcting, until the note sounds steadily within `ACCEPT_CENTS`
//! of the pitch asked for; **it is annotated with the pitch it sounds**, as measured. Its onset and end are read from its own level, rendered alone.
//! Effects that move pitch or smear time (vibrato, unison, the insert effects, the reverbs) are
//! switched off; the body, filter, EQ and compressor stay, as the preset made them.

use crate::measure;
use crate::preset::{self, Preset};
use crate::vst::{Event, Instrument, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const ENGINE: &str = "String Studio VS-3";

/// How far a note's steady pitch may land from the pitch asked for. The bowed model locks onto
/// one of a few pitches near the one its bend asks (measured 2026-10-19: a G4 found steady pitches
/// 4 cents apart, which the bend hops between), so a note is tuned near its target and annotated
/// with the pitch it sounds, measured.
pub const ACCEPT_CENTS: f64 = 12.0;

/// The furthest a note may lock from the pitch asked for when no try lands within
/// `ACCEPT_CENTS` (a violin body's resonance pulls some notes 20 cents sharp, a wolf tone's way).
pub const FALLBACK_CENTS: f64 = 35.0;

/// The most a note may hold away from its harmonics (bow noise, a raucous regime's subharmonics),
/// against what is at them, dB, above 110 Hz: some of the violin preset's low notes are scratchy at
/// -12, and the *Brushed* preset's lowest ones sit near -20 even at their cleanest. Lower and
/// shorter notes are allowed more (`noise_limit`).
pub const MAX_NOISE_DB: f64 = -20.0;

/// How much the steady pitch may differ between the thirds of the note, cents, above 110 Hz: a
/// bowed string wanders a cent or two, as a player's does (each note's spread is in its sidecar).
pub const STEADY_CENTS: f64 = 3.0;

/// A limit at 110 Hz and up, eased to `at_40_hz` by 40 Hz (on a log-frequency line): a low bowed
/// string (the double bass's G#1, 2026-10-19) wobbles 3 to 14 cents and carries 15 to 23 dB of
/// noise, and its pitch is measured over fewer periods.
fn limit(at_110_hz: f64, at_40_hz: f64, f0: f64) -> f64 {
    let x = ((110.0 / f0.max(1.0)).log2() / (110.0f64 / 40.0).log2()).clamp(0.0, 1.0);
    at_110_hz + (at_40_hz - at_110_hz) * x
}

/// The most noise a note may hold, dB: `MAX_NOISE_DB`, eased for low notes (`limit`) and for short
/// ones, whose steady part is little more than their attack (a cello's 0.1 to 0.2 s notes hold 13 to
/// 21 dB of bow noise, 2026-10-19): up to -12 dB at a quarter second.
fn noise_limit(hz: f64, hold: f64) -> f64 {
    let by_pitch = limit(MAX_NOISE_DB, -17.0, hz);
    let short = ((0.5 - hold) / 0.25).clamp(0.0, 1.0);
    by_pitch + (-12.0 - by_pitch).max(0.0) * short
}

/// The weakest a note's own fundamental may be against its loudest harmonic, dB (before set 4
/// shapes it): weaker, and the pitch measured may be an octave below the one sounding.
pub const MIN_FUNDAMENTAL_DB: f64 = -30.0;

/// VS-3's pitch-bend parameter: the controller its `IMidiMapping` gives MIDI pitch bend on channel
/// 1 (found by test: 0.5 is centre, 1 is the top of the bend range).
const PITCH_BEND: u32 = 1271;

/// The bend range, as the preset's `Setup/Bend` value: n is n - 1 semitones (3 is ±2).
const BEND_SETTING: f64 = 3.0;
const BEND_SEMITONES: f64 = 2.0;

/// Where the plugin and its factory library are installed on Windows; `MXM_VS3` and
/// `MXM_VS3_PACK` choose others.
fn default_paths() -> (PathBuf, PathBuf) {
    let bundle = std::env::var_os("MXM_VS3")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(r"C:\Program Files\Common Files\VST3\String Studio VS-3.vst3")
        });
    let pack = std::env::var_os("MXM_VS3_PACK")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                r"C:\Program Files\Applied Acoustics Systems\String Studio VS-3\Factory Library\Factory Library.VS-3 Pack",
            )
        });
    (bundle, pack)
}

/// An instrument here: a factory preset and what is changed in it.
pub struct Voicing {
    pub preset: &'static str,
    /// Parameters set at the start of every render, by name: `(name, CLAP value)`.
    pub settings: &'static [(&'static str, f64)],
    pub plucked: bool,
    /// How far the preset sounds above its key, semitones: the *Upright* presets sound an octave
    /// below it, as a double bass sounds below its written note (measured 2026-10-19).
    pub transpose: f64,
}

/// What every render switches off: whatever moves pitch or smears time.
const DRY: &[(&str, f64)] = &[
    ("A Vibrato: Amount", 0.0),
    ("B Vibrato: Amount", 0.0),
    ("A Unison: On", 0.0),
    ("B Unison: On", 0.0),
    ("A Arpeggiator: On", 0.0),
    ("B Arpeggiator: On", 0.0),
    ("A Effect 1: On", 0.0),
    ("A Effect 2: On", 0.0),
    ("B Effect 1: On", 0.0),
    ("B Effect 2: On", 0.0),
    ("Effect 1: On", 0.0),
    ("Effect 2: On", 0.0),
    ("A Reverb: On", 0.0),
    ("B Reverb: On", 0.0),
    ("Reverb: On", 0.0),
];

pub fn voicing(instrument: &str) -> Voicing {
    match instrument {
        "cello" => Voicing {
            preset: "Baroque Cello",
            settings: &[],
            plucked: false,
            transpose: 0.0,
        },
        "violin" => Voicing {
            preset: "Brushed violin 1",
            settings: &[],
            plucked: false,
            transpose: 0.0,
        },
        // No viola in the library: the cello's with a medium body (the violin's, with a larger body,
        // is scratchy below G3, measured 2026-10-19).
        "viola" => Voicing {
            preset: "Baroque Cello",
            settings: &[("A Body: Size", 0.5)],
            plucked: false,
            transpose: 0.0,
        },
        // No bowed double bass in the library: the cello's, with the largest body.
        "double-bass" => Voicing {
            preset: "Baroque Cello",
            settings: &[("A Body: Size", 1.0)],
            plucked: false,
            transpose: 0.0,
        },
        // Plucked away from the middle, with a little stiffness: *Upright Pluck 2* plucks at the
        // midpoint (no even harmonics) with 61 % inharmonicity, its sixth partial 3.8 % sharp.
        "plucked-bass" => Voicing {
            preset: "Upright Pluck 1",
            settings: &[("A String: Inharmonicity", 0.12)],
            plucked: true,
            transpose: -12.0,
        },
        other => panic!("no String Studio voicing for {other:?}"),
    }
}

/// One note to render.
#[derive(Clone, Debug)]
pub struct StringNote {
    pub hz: f64,
    /// Note-on velocity, 0 to 1.
    pub velocity: f64,
    /// From note-on to the release, s.
    pub hold: f64,
    /// Rendered after the note-off, s.
    pub tail: f64,
    /// Parameter changes during the note, by name: `(name, s after note-on, CLAP value)`.
    pub automation: Vec<(&'static str, f64, f64)>,
    /// The note's release: from `hold` its level falls 40 dB in this many s, by gain, and the
    /// note-off is sent only once it has faded. VS-3's damper bends a string's pitch by tens of
    /// cents as it stops it (measured 2026-10-19), so no note here is heard being damped.
    pub fade: f64,
}

impl StringNote {
    /// When the note-off is sent: after the fade has taken the note below hearing.
    fn note_off(&self) -> f64 {
        self.hold + 2.0 * self.fade + 0.05
    }
}

/// A note as rendered: its samples from note-on (the render's lead removed), and what was read
/// from them.
#[derive(Clone, Debug)]
pub struct Rendered {
    pub audio: Vec<f64>,
    /// Where it begins to sound and where its release has fallen 40 dB, s from note-on.
    pub onset: f64,
    pub end: f64,
    /// Its measured steady pitch, Hz, and how far the pitch asked for is from it, cents.
    pub measured_hz: f64,
    pub error_cents: f64,
    /// The largest difference of a third of the steady part from the whole, cents.
    pub spread_cents: f64,
    /// What is not at a harmonic in the steady part, against what is, dB.
    pub noise_db: f64,
    /// The fundamental against the loudest of the first six harmonics, dB.
    pub fundamental_db: f64,
    pub key: u8,
    pub velocity: f64,
    pub bend_cents: f64,
    pub iterations: usize,
    /// The attack's largest pitch deviation, cents, and when it last leaves 5 cents, ms.
    pub settle_cents: f64,
    pub settle_ms: f64,
    /// From the release to the end: the largest deviation, cents, and when it first passes 5
    /// cents, ms after the release starts.
    pub release_cents: f64,
    pub release_from_ms: Option<f64>,
    pub peak_db: f64,
}

/// How long after its onset a note's settling is watched, s.
pub const SETTLE_WINDOW: f64 = 0.3;

/// The render starts this long before note-on, so the bend is in place first.
const LEAD: f64 = 0.05;

pub struct Strings {
    instrument: Instrument,
    default_state: Vec<u8>,
    pack: String,
    ids: HashMap<String, u32>,
    states: HashMap<&'static str, Vec<u8>>,
    /// Bends found, by (instrument, key, hold in ms, velocity in thousandths, rate), so a note
    /// asked for again starts from its answer.
    bends: HashMap<(String, u8, u64, u64, u64), f64>,
}

impl Strings {
    pub fn load() -> Result<Self> {
        let (bundle, pack) = default_paths();
        Self::load_from(&bundle, &pack)
    }

    pub fn load_from(bundle: &Path, pack: &Path) -> Result<Self> {
        let instrument = Instrument::load(bundle)?;
        let mut instance = instrument.instance(None)?;
        let default_state = instance.save_state()?;
        let mut ids = HashMap::new();
        for param in instance.params()? {
            ids.entry(param.name).or_insert(param.id);
        }
        Ok(Self {
            instrument,
            default_state,
            pack: std::fs::read_to_string(pack)?,
            ids,
            states: HashMap::new(),
            bends: HashMap::new(),
        })
    }

    fn id(&self, name: &str) -> Result<u32> {
        Ok(*self
            .ids
            .get(name)
            .ok_or_else(|| format!("String Studio has no parameter {name:?}"))?)
    }

    fn state(&mut self, voicing: &Voicing) -> Result<Vec<u8>> {
        if let Some(state) = self.states.get(voicing.preset) {
            return Ok(state.clone());
        }
        let mut found: Preset = preset::find(&self.pack, voicing.preset)?;
        found.set("Setup", "Bend", BEND_SETTING)?;
        let state = preset::state_with(&self.default_state, &found)?;
        self.states.insert(voicing.preset, state.clone());
        Ok(state)
    }

    fn render_once(
        &mut self,
        voicing: &Voicing,
        note: &StringNote,
        key: u8,
        bend_cents: f64,
        rate: f64,
    ) -> Result<Vec<f64>> {
        let state = self.state(voicing)?;
        let mut events = Vec::new();
        for (name, value) in DRY.iter().chain(voicing.settings) {
            if let Some(&id) = self.ids.get(*name) {
                events.push(Event::Param {
                    frame: 0,
                    id,
                    value: *value,
                });
            }
        }
        events.push(Event::Param {
            frame: 0,
            id: PITCH_BEND,
            value: (0.5 + bend_cents / 100.0 / (2.0 * BEND_SEMITONES)).clamp(0.0, 1.0),
        });
        let on = (LEAD * rate).round() as u64;
        events.push(Event::NoteOn {
            frame: on,
            key: u16::from(key),
            velocity: note.velocity,
        });
        let off = on + (note.note_off() * rate).round() as u64;
        events.push(Event::NoteOff {
            frame: off,
            key: u16::from(key),
        });
        for &(name, at, value) in &note.automation {
            events.push(Event::Param {
                frame: on + (at * rate).round() as u64,
                id: self.id(name)?,
                value,
            });
        }
        let frames = ((LEAD + note.note_off() + note.tail) * rate).round() as usize;
        let mut instance = self.instrument.instance(Some(&state))?;
        let audio = instance.render(rate, frames, &events)?;
        // For looking at what the model did: the last render, as a WAV.
        if let Some(path) = std::env::var_os("MXM_SYNTH_DUMP") {
            mxm_audio_file::write(
                path,
                &audio,
                1,
                rate as u32,
                mxm_audio_file::Target::WavFloat32,
            )?;
        }
        Ok(audio[on as usize..].iter().map(|&v| f64::from(v)).collect())
    }

    /// The steady pitch `note` sounds at with `bend_cents` on `key`, for exploring the model.
    pub fn pitch_at(
        &mut self,
        instrument: &str,
        note: &StringNote,
        key: u8,
        bend_cents: f64,
        rate: f64,
    ) -> Result<f64> {
        let voicing = voicing(instrument);
        let audio = self.render_once(&voicing, note, key, bend_cents, rate)?;
        let (onset, _, _) = measure::onset_and_end(&audio, rate);
        Ok(measure::steady_pitch(
            &audio,
            rate,
            onset + 0.15,
            note.hold - 0.02,
            note.hz,
            60.0,
            10,
        ))
    }

    /// Renders `note` on `instrument` at `rate`, tuned near `note.hz`: on the nearest key first,
    /// then, if the model locks too far off there, on the keys beside it (bent by up to a
    /// semitone) and with the bow a little slower or faster.
    pub fn note(&mut self, instrument: &str, note: &StringNote, rate: f64) -> Result<Rendered> {
        let voicing = voicing(instrument);
        // The key that sounds the pitch asked for, fractionally.
        let midi = 69.0 + 12.0 * (note.hz / 440.0).log2() - voicing.transpose;
        let nearest = midi.round().clamp(1.0, 126.0);
        let other = if midi >= nearest {
            nearest + 1.0
        } else {
            nearest - 1.0
        };
        let v = note.velocity;
        let tries = [
            (nearest, v),
            (other, v),
            (nearest, (v * 0.9).max(0.05)),
            (nearest, (v * 1.1).min(1.0)),
            (nearest - (other - nearest), v),
        ];
        let mut best: Option<Rendered> = None;
        let mut renders = 0;
        for (n, &(key, velocity)) in tries.iter().enumerate() {
            let key = key as u8;
            let memo = (
                instrument.to_owned(),
                key,
                (note.hold * 1000.0).round() as u64,
                (velocity * 1000.0).round() as u64,
                rate as u64,
            );
            let mut bend = self
                .bends
                .get(&memo)
                .copied()
                .unwrap_or((midi - f64::from(key)) * 100.0);
            let asked = StringNote {
                velocity,
                ..note.clone()
            };
            for _ in 0..if n == 0 { 5 } else { 3 } {
                renders += 1;
                let audio = self.render_once(&voicing, &asked, key, bend, rate)?;
                let found = analyse(audio, rate, &voicing, &asked, key, bend, renders);
                if std::env::var_os("MXM_SYNTH_DEBUG").is_some() {
                    eprintln!(
                        "{instrument} {:.3} Hz key {key} v {velocity:.2} bend {bend:+.3} c: measured {:.4} Hz, off {:+.3} c, spread {:.3} c, noise {:.1} dB, H1 {:.1} dB",
                        note.hz,
                        found.measured_hz,
                        found.error_cents,
                        found.spread_cents,
                        found.noise_db,
                        found.fundamental_db
                    );
                }
                let steady = found.spread_cents <= limit(STEADY_CENTS, 7.0, note.hz)
                    && found.noise_db <= noise_limit(note.hz, note.hold)
                    && found.fundamental_db >= MIN_FUNDAMENTAL_DB;
                let error = found.error_cents;
                if steady
                    && best
                        .as_ref()
                        .is_none_or(|b| error.abs() < b.error_cents.abs())
                {
                    best = Some(found);
                }
                if steady && error.abs() <= ACCEPT_CENTS {
                    self.bends.insert(memo, bend);
                    return Ok(best.expect("just kept"));
                }
                // An unsteady note moves the bend a little to find a steady regime.
                bend += if steady { error } else { error + 0.7 };
                if bend.abs() > 100.0 * BEND_SEMITONES - 5.0 {
                    break;
                }
            }
        }
        match best {
            Some(found) if found.error_cents.abs() <= FALLBACK_CENTS => {
                eprintln!(
                    "note: {instrument} asked {:.3} Hz locks {:+.1} cents off at best; annotated at the {:.3} Hz it sounds",
                    note.hz, -found.error_cents, found.measured_hz
                );
                Ok(found)
            }
            _ => Err(format!(
                "{instrument} at {:.3} Hz: no steady pitch within {FALLBACK_CENTS} cents after {renders} renders",
                note.hz
            )
            .into()),
        }
    }
}

/// What a render of `note` sounds: its onset, end and steady pitch, how steady, how it settles
/// and drifts.
fn analyse(
    mut audio: Vec<f64>,
    rate: f64,
    voicing: &Voicing,
    note: &StringNote,
    key: u8,
    bend: f64,
    iterations: usize,
) -> Rendered {
    // The release, by gain: down 40 dB `fade` s after `hold`, and silent at twice that.
    for (i, v) in audio.iter_mut().enumerate() {
        let after = i as f64 / rate - note.hold;
        if after > 0.0 {
            *v *= if after < 2.0 * note.fade {
                10f64.powf(-2.0 * after / note.fade)
            } else {
                0.0
            };
        }
    }
    let (onset, end, peak_db) = measure::onset_and_end(&audio, rate);
    // The steady part: past the attack, before the release (a plucked note decays, so its early
    // part, and its fundamental alone).
    let (t0, t1, harmonics) = if voicing.plucked {
        (onset + 0.06, (onset + 0.06 + 0.8).min(note.hold), 1)
    } else if note.hold >= 0.4 {
        (onset + 0.15, note.hold - 0.02, 10)
    } else {
        (onset + 0.4 * (note.hold - onset), note.hold, 10)
    };
    let measured = measure::steady_pitch(&audio, rate, t0, t1, note.hz, 60.0, harmonics);
    // The pitch must hold over the note: measured again over each third of the window.
    let spread = if t1 - t0 < 0.15 {
        0.0
    } else {
        let third = (t1 - t0) / 3.0;
        (0..3)
            .map(|k| {
                let a = t0 + k as f64 * third;
                let p =
                    measure::steady_pitch(&audio, rate, a, a + third, measured, 20.0, harmonics);
                (1200.0 * (p / measured).log2()).abs()
            })
            .fold(0.0, f64::max)
    };
    // A plucked string's partials are stretched (stiffness), so only its first six are asked.
    // An octave's guard: a note an octave above its key would show no fundamental at the pitch
    // measured (one an octave below shows energy at half of it, which the share counts as noise).
    let levels = measure::harmonic_levels(&audio, rate, t0, t1, measured, 6);
    let loudest = levels.iter().cloned().fold(f64::MIN, f64::max);
    let fundamental_db = levels[0] - loudest;
    let share = measure::harmonic_share(
        &audio,
        rate,
        t0,
        t1,
        measured,
        if voicing.plucked { 6.5 } else { 20.0 },
    );
    let noise_db = 10.0 * (1.0 - share).max(1e-9).log10() - 10.0 * share.max(1e-9).log10();
    let (settle_cents, settle_ms) = measure::settling(&audio, rate, onset, measured, SETTLE_WINDOW);
    let (release_cents, release_from_ms, _) =
        measure::deviation(&audio, rate, note.hold, end, measured);
    Rendered {
        audio,
        onset,
        end,
        measured_hz: measured,
        error_cents: 1200.0 * (note.hz / measured).log2(),
        spread_cents: spread,
        noise_db,
        fundamental_db,
        key,
        velocity: note.velocity,
        bend_cents: bend,
        iterations,
        settle_cents,
        settle_ms,
        release_cents,
        release_from_ms,
        peak_db,
    }
}
