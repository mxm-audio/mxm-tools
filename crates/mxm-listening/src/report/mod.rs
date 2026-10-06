//! Reports as JSON (for tools and later comparison) and as Markdown (for reading).

pub mod json;
pub mod markdown;

use crate::reading::Validity;

/// A validity in words.
#[must_use]
pub fn validity_text(v: &Validity) -> String {
    match v {
        Validity::Valid => "valid".into(),
        Validity::BelowFloor => "below floor".into(),
        Validity::Sample => "sample".into(),
        Validity::Unresolved => "at the resolution limit".into(),
        Validity::Absent(why) => format!("absent: {why}"),
    }
}
