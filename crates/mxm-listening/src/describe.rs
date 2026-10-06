//! `describe`: everything the listener hears in one sound, part by part.

use crate::family::{self, Detected, Family};
use crate::parts::{self, Context};
use crate::reading::Report;
use crate::sound::Sound;

/// What the caller knows about a sound before listening to it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Options {
    /// The family, overriding detection.
    pub family: Option<Family>,
    /// Lines the room or the kit adds (from a set's recurring late lines, `set::room_lines`), Hz: kept
    /// out of the rest pitch and the ring's measures.
    pub room_lines: Vec<f64>,
    /// Leaves out the perceptual models, which cost about as much as everything else together: for a
    /// caller that reads other parts only (a calibration's operators, most tests).
    pub without_perception: bool,
    /// The note a pitched note is meant to be, Hz: its fundamental is then the partial within a
    /// semitone of it. A kalimba's other tines ring in sympathy and outlast a high tine, and level and
    /// decay cannot tell them from the note's own weak fundamental (a crotale's); the caller can.
    pub expect_hz: Option<f64>,
    /// When a synth voice's note was released, s from the start of the file: its release and its
    /// note-off click are read from there.
    pub note_off_s: Option<f64>,
}

/// Describes one sound. `declared` overrides the family detection.
#[must_use]
pub fn describe(sound: &Sound, declared: Option<Family>) -> Report {
    describe_with(
        sound,
        &Options {
            family: declared,
            ..Options::default()
        },
    )
}

/// Describes one sound with what the caller knows about it: [`describe_staged`] and, unless the
/// options leave them out, [`Staged::perceive`].
#[must_use]
pub fn describe_with(sound: &Sound, options: &Options) -> Report {
    let staged = describe_staged(sound, options);
    if options.without_perception {
        staged.into_report()
    } else {
        staged.perceive()
    }
}

/// A reading made in two stages (`plans/plan-mxm-listener-hud.md` §2.3): every part but the
/// perceptual models first, then the models, which cost about as much as everything else together.
/// A window shows the first stage while the second runs; `describe_with` runs both, so a reading
/// made in stages is the reading made at once, reading for reading.
#[derive(Clone, Debug)]
pub struct Staged {
    report: Report,
    /// What the perceptual models read; `None` where nothing was measured, or for an impulse
    /// response, which takes none.
    context: Option<Context>,
}

impl Staged {
    fn done(report: Report) -> Self {
        Self {
            report,
            context: None,
        }
    }

    /// The first stage's report: every part but the perceptual models.
    #[must_use]
    pub fn report(&self) -> &Report {
        &self.report
    }

    /// The first stage's report alone.
    #[must_use]
    pub fn into_report(self) -> Report {
        self.report
    }

    /// Adds the perceptual models: a held note's fluctuation strength, in its place in the sustain
    /// section, and the `perception` section last.
    #[must_use]
    pub fn perceive(mut self) -> Report {
        if let Some(context) = &self.context {
            if let Some(reading) = self
                .report
                .sections
                .iter_mut()
                .flat_map(|s| s.readings.iter_mut())
                .find(|r| r.id == "sustain.fluctuation")
            {
                *reading = parts::sustain::fluctuation_reading(context, true);
            }
            self.report.sections.push(parts::perception::read(context));
        }
        self.report
    }
}

/// The first stage of [`describe_with`]: every part but the perceptual models, whatever
/// `options.without_perception` says.
#[must_use]
pub fn describe_staged(sound: &Sound, options: &Options) -> Staged {
    let declared = options.family;
    let mut notes = Vec::new();
    let rate = sound.rate;
    let duration_s = sound.duration_s();
    let empty = |family: Detected, notes: Vec<String>| {
        Staged::done(Report {
            name: sound.name.clone(),
            rate,
            duration_s,
            family,
            onset_s: None,
            sections: Vec::new(),
            notes,
        })
    };
    if rate == 0 {
        notes.push("the sample rate is zero; nothing was measured".into());
        return empty(unknown("no sample rate"), notes);
    }
    if let Some((at, _)) = sound
        .samples
        .iter()
        .enumerate()
        .find(|(_, s)| !s.is_finite())
    {
        notes.push(format!(
            "sample {at} is not a number or infinite; nothing was measured (a DSP failure is not measured)"
        ));
        return empty(unknown("non-finite samples"), notes);
    }
    let Some(mut context) = Context::new(&sound.samples, rate) else {
        notes.push("the sound is silent; nothing was measured".into());
        return empty(unknown("silent"), notes);
    };

    let detected = match declared {
        Some(family) => Detected {
            family,
            how: "declared".into(),
        },
        None => family::detect(&context.x, context.rate, Some(context.onset)),
    };
    if detected.family == Family::Impulse {
        // A recorded impulse response: read from the file's first sample, whose delay is its latency.
        let top = (0.45 * context.rate).min(20_000.0);
        let (sections, more) =
            parts::impulse::read(&[context.x.clone()], context.rate, (20.0, top), 0);
        notes.extend(more);
        return Staged::done(Report {
            name: sound.name.clone(),
            rate,
            duration_s,
            family: detected,
            onset_s: Some(0.0),
            sections,
            notes,
        });
    }
    if detected.family == Family::Unknown {
        notes.push(
            "the family is unknown: read with the percussive parts, which may not fit this sound"
                .into(),
        );
    }
    let voice = detected.family == Family::Voice;
    let sustained = detected.family == Family::Sustained || voice;
    let note = detected.family == Family::Note || sustained;
    if note {
        // A note cut from a run starts with the one before it still ringing (`parts::note::onset`).
        if let Some(at) = parts::note::onset(&context.raw, rate) {
            context.onset = at;
        }
    }
    warts(&context, &mut notes);

    let windows = if note {
        parts::pitch::analyse_note(&context)
    } else {
        parts::pitch::analyse(&context)
    };
    let room = &options.room_lines;
    if !room.is_empty() {
        let lines: Vec<String> = room.iter().map(|hz| format!("{hz:.1}")).collect();
        notes.push(format!(
            "room lines kept out of the pitch and the ring: {} Hz",
            lines.join(", ")
        ));
    }
    let ring_hz = parts::pitch::rest(&windows, room).map(|m| m.hz);
    let mut sections = vec![
        parts::attack::read(&context),
        parts::level::read(&context),
        parts::decay::read(&context),
        parts::pitch::section(&context, &windows, room),
        parts::tone::read(&context),
        parts::tonality::read(&context),
        parts::words::read(&context),
    ];
    // A snare's wires and slaps mean nothing on a bar: a note's texture waits for the instruments
    // that have one (a sitar's buzz, a prepared string).
    if !note {
        sections.push(parts::texture::read(&context));
    }
    sections.push(parts::modulation::read(&context, ring_hz, &windows));
    sections.push(parts::artefacts::read(&context));
    if note {
        let mut partials = parts::note::read(
            &context,
            &windows,
            ring_hz,
            room,
            options.expect_hz,
            sustained,
        );
        if sustained {
            let f0 = partials
                .readings
                .iter()
                .find(|r| r.id == "note.fundamental")
                .and_then(|r| r.value);
            // A held note's tuning is its held pitch's (`parts::sustain::held_pitch`).
            let held = f0.and_then(|f| parts::sustain::held_pitch(&context, f));
            if let (Some(hz), Some(r)) = (
                held,
                partials.readings.iter_mut().find(|r| r.id == "note.tuning"),
            ) {
                r.value = parts::note::nearest_note(hz).map(|n| n.1);
                r.source = "a held note's pitch against equal temperament (A = 440 Hz): the median of its pitch track over the held stretch, which a vibrato swings around";
            }
            // Its fluctuation strength is perceptual: `Staged::perceive` fills it in.
            sections.push(parts::sustain::read(&context, f0, false));
            if voice {
                let onset_s = context.onset as f64 / context.rate;
                let off = options.note_off_s.map(|o| o - onset_s).filter(|o| *o > 0.0);
                sections.push(parts::voice::read(&context, f0.or(options.expect_hz), off));
            }
        }
        sections.push(partials);
    }
    let onset_s = Some(context.onset as f64 / context.rate);
    Staged {
        report: Report {
            name: sound.name.clone(),
            rate,
            duration_s,
            family: detected,
            onset_s,
            sections,
            notes,
        },
        context: Some(context),
    }
}

fn unknown(why: &str) -> Detected {
    Detected {
        family: Family::Unknown,
        how: format!("not detected: {why}"),
    }
}

/// Recording-chain warts, reported as the file's and never as the sound's (the plan's §2 item 1).
fn warts(c: &Context, notes: &mut Vec<String>) {
    let clipped = c.x.iter().filter(|s| s.abs() >= 0.999).count();
    if clipped > 1 {
        notes.push(format!(
            "{clipped} samples at or above −0.01 dBFS: the file may be clipped"
        ));
    }
    let mean = c.x.iter().sum::<f64>() / c.x.len() as f64;
    if mean.abs() > 0.01 * c.peak {
        notes.push(format!(
            "a DC offset of {:.1} dB against the peak",
            20.0 * (mean.abs() / c.peak).log10()
        ));
    }
    let pre_roll = c.onset as f64 / c.rate * 1000.0;
    if pre_roll > 50.0 {
        notes.push(format!(
            "{pre_roll:.0} ms before the onset: every window is measured from the onset"
        ));
    }
    let after = (c.x.len() - c.onset) as f64 / c.rate;
    if after < 0.5 {
        notes.push(format!(
            "only {:.0} ms after the onset: late readings are absent",
            after * 1000.0
        ));
    }
}
