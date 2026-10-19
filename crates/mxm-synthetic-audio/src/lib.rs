//! Synthetic recordings with exact pitch ground truth, for the pitch editor's analysis.
//!
//! Two engines: additive synthesis (`additive`), whose f0 is the one it was made from, and
//! String Studio VS-3 (`strings`), a physical model rendered as a VST3 one note at a time, each note
//! tuned before it sounds and measured. The sets are in `sets`; a file's truth is written by
//! `truth`.

pub mod additive;
pub mod dsp;
pub mod json;
pub mod measure;
pub mod preset;
pub mod reverb;
pub mod sets;
pub mod strings;
pub mod truth;
pub mod vst;

/// What wrote a file, for its sidecar.
pub const GENERATOR: &str = concat!("mxm-synthetic-audio ", env!("CARGO_PKG_VERSION"));
