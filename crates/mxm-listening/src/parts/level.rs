//! The level: the peak, and the K-weighted body loudness two sounds are matched on
//! (`docs/drum-model-fitting.md` §2 in mxm-drum-machine, `prep::body_rms`).

use super::Context;
use crate::reading::{Reading, Section, Unit};
use crate::repr::envelope;

#[must_use]
pub fn read(c: &Context) -> Section {
    let peak_dbfs = envelope::db(c.peak, 1.0);
    // The body loudness is taken from 5 ms before the onset, where the page's trim starts.
    let start = c.onset.saturating_sub(c.rate as usize / 200);
    let body = f64::from(crate::prep::body_rms(&c.raw[start..], c.rate as u32));
    let body_db = envelope::db(body, 1.0);
    let crest = envelope::db(c.peak, body);
    Section::new(
        "level",
        "Level",
        vec![
            Reading::new(
                "level.peak",
                "Peak",
                peak_dbfs,
                Unit::DecibelsFullScale,
                "the largest absolute sample",
            ),
            Reading::new(
                "level.body_loudness",
                "Body loudness (K-weighted RMS)",
                body_db,
                Unit::DecibelsFullScale,
                "BS.1770 K-weighting over the loud body; guide §2 (`prep::body_rms`)",
            ),
            Reading::new(
                "level.crest",
                "Peak over body loudness",
                crest,
                Unit::Decibels,
                "level.peak − level.body_loudness",
            ),
        ],
    )
}
