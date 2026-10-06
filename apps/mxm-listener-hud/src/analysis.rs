//! What the window asks of the listener, in stages, and what it says about what was read.
//!
//! The stages are the worker's cancellation points (the plan's §2.2): the load, the listener's first
//! stage — every part but the perceptual models, which cost about as much as everything else
//! together — the curves the window draws, and the listener's second stage, which adds the models
//! (`describe_staged`, `curves`, `Staged::perceive`) — and last the self-test's rebuild, whose parts
//! the window plays (`resynth::rebuild_with`). Every number
//! here comes from the listener; this module only carries it and words the facts the load kept.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mxm_listening::curves::{Curves, curves};
use mxm_listening::describe::{Options, describe_staged};
use mxm_listening::family::Family;
use mxm_listening::prep::{HIT_MAX_FRAMES, Kept};
use mxm_listening::resynth::rebuild_with;
use mxm_listening::{Report, Sound};

use crate::worker::Emit;

/// One dropped sound to read. The path goes to the loader and nowhere else (the no-path rule).
#[derive(Clone, Debug)]
pub struct Request {
    pub path: PathBuf,
    pub name: String,
    /// The family the caller declares; `None` lets the listener detect it.
    pub family: Option<Family>,
    /// The note the name claims, Hz, when the owner trusts it: a pitched note's fundamental is then
    /// the partial within a semitone of it (the listener's `--expect`).
    pub expect_hz: Option<f64>,
}

/// What the listener read of the file before it listened.
#[derive(Clone, Debug, PartialEq)]
pub struct Loaded {
    pub rate: u32,
    pub duration_s: f64,
    pub kept: Kept,
    /// The listened sound: the mono buffer the listener read, cut at its limit.
    pub samples: Arc<[f32]>,
}

/// The self-test's rebuild of the sound from its readings, in parts, sample for sample with the
/// listened sound: its modes, its noise and the whole.
#[derive(Clone, Debug, PartialEq)]
pub struct Rebuilt {
    pub rate: u32,
    pub modes: Arc<[f32]>,
    pub noise: Arc<[f32]>,
    pub whole: Arc<[f32]>,
}

/// One stage's result.
#[derive(Clone, Debug)]
pub enum Stage {
    Loaded(Loaded),
    /// The decoder refused the file, in its own words.
    Refused(String),
    /// What the window draws, from the report's onset.
    Curves(Box<Curves>),
    /// The rebuild's parts, to play; `None` for a sound the rebuild does not read (an impulse
    /// response) or cannot measure.
    Rebuilt(Option<Box<Rebuilt>>),
    /// A reading of the sound — first without the perceptual models, then with them — and how long
    /// it took from the drop.
    Read {
        report: Box<Report>,
        perception: bool,
        took: Duration,
    },
}

/// The name a sound is shown and reported by: its file's stem, as `listen` names one — never the
/// path, which may point into a private folder.
#[must_use]
pub fn display_name(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "sound".into())
}

/// Reads one request, stage by stage, stopping when `emit` says a newer drop has superseded it.
pub fn analyse(request: &Request, emit: &mut Emit<'_, Stage>) {
    let started = Instant::now();
    let (sound, kept) = match Sound::load_kept(&request.path, request.name.clone()) {
        Ok(loaded) => loaded,
        Err(reason) => {
            emit(Stage::Refused(reason));
            return;
        }
    };
    let loaded = Loaded {
        rate: sound.rate,
        duration_s: sound.duration_s(),
        kept,
        samples: sound.samples.clone().into(),
    };
    if !emit(Stage::Loaded(loaded)) {
        return;
    }
    let options = Options {
        family: request.family,
        expect_hz: request.expect_hz,
        ..Options::default()
    };
    let staged = describe_staged(&sound, &options);
    let quick = Stage::Read {
        report: Box::new(staged.report().clone()),
        perception: false,
        took: started.elapsed(),
    };
    if !emit(quick) {
        return;
    }
    let drawn = curves(&sound, staged.report().onset_s);
    if !emit(Stage::Curves(Box::new(drawn))) {
        return;
    }
    let full = staged.perceive();
    let family = full.family.family;
    if !emit(Stage::Read {
        report: Box::new(full),
        perception: true,
        took: started.elapsed(),
    }) {
        return;
    }
    // The rebuild reads the sound as its family does: a note from a note's analysis, anything else
    // as a hit. An impulse response is not rebuilt.
    let rebuilt = (family != Family::Impulse)
        .then(|| rebuild_with(&sound, &[], Some(family)))
        .flatten()
        .map(|r| {
            let f32s = |x: &[f64]| -> Arc<[f32]> { x.iter().map(|&v| v as f32).collect() };
            Box::new(Rebuilt {
                rate: sound.rate,
                modes: f32s(&r.modal),
                noise: f32s(&r.noise),
                whole: r.sound.samples.into(),
            })
        });
    emit(Stage::Rebuilt(rebuilt));
}

/// What the listener did not read, or read differently from the file, in words (the plan's §2.4).
/// Built from the load's facts and the report only, so no path can reach it.
#[must_use]
pub fn notices(loaded: &Loaded, report: Option<&Report>) -> Vec<String> {
    let mut out = Vec::new();
    let kept = &loaded.kept;
    if kept.source_channels > 1 {
        out.push(format!(
            "{} channels, averaged to one: the listener reads a sound in mono.",
            kept.source_channels
        ));
    }
    if kept.cut {
        out.push(format!(
            "Longer than the listener keeps: read up to {HIT_MAX_FRAMES} frames, {:.1} s at {} Hz.",
            HIT_MAX_FRAMES as f64 / f64::from(loaded.rate.max(1)),
            loaded.rate
        ));
    }
    if let Some(report) = report {
        match report.onset_s {
            _ if report.family.family == Family::Impulse => out.push(
                "Read as an impulse response from the file's first sample, whose delay is its \
                 latency."
                    .into(),
            ),
            Some(onset) => out.push(format!(
                "Read as one sound from its first onset at {onset:.3} s, with no event taken apart: \
                 each reading covers its own window from there. The sound lasts {:.2} s.",
                loaded.duration_s
            )),
            None => out.push("Nothing was measured; the listener's notes say why.".into()),
        }
    }
    out
}
