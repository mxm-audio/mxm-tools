//! A sound as the listener reads it.

use std::path::Path;

/// Mono samples, their rate, and a logical name — never a path, because a report may be kept and a
/// path into a private folder must not travel with it.
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    pub name: String,
    pub rate: u32,
    pub samples: Vec<f32>,
}

impl Sound {
    #[must_use]
    pub fn new(name: impl Into<String>, rate: u32, samples: Vec<f32>) -> Self {
        Self {
            name: name.into(),
            rate,
            samples,
        }
    }

    /// Decodes a file to mono by the A/B page's definition ([`crate::prep::decode_mono`]).
    pub fn load(path: &Path, name: impl Into<String>) -> Result<Self, String> {
        let (samples, rate) = crate::prep::decode_mono(path)?;
        Ok(Self::new(name, rate, samples))
    }

    /// [`Sound::load`], sample for sample, with what the decode kept of the file
    /// ([`crate::prep::Kept`]): its channels before they were averaged, and whether it was cut at the
    /// limit. A window shows what the listener read, not only what it made of it.
    ///
    /// # Errors
    /// The decoder's own sentence when the file cannot be decoded; it never names the path.
    pub fn load_kept(
        path: &Path,
        name: impl Into<String>,
    ) -> Result<(Self, crate::prep::Kept), String> {
        let (samples, rate, kept) = crate::prep::decode_mono_kept(path)?;
        Ok((Self::new(name, rate, samples), kept))
    }

    /// Decodes a whole file to mono, however long ([`crate::prep::decode_mono_whole`]): a run of notes
    /// to be cut, not a hit to be measured.
    ///
    /// # Errors
    /// When the file cannot be decoded or is longer than the whole-file limit.
    pub fn load_whole(path: &Path, name: impl Into<String>) -> Result<Self, String> {
        let (samples, rate) = crate::prep::decode_mono_whole(path)?;
        Ok(Self::new(name, rate, samples))
    }

    #[must_use]
    pub fn duration_s(&self) -> f64 {
        self.samples.len() as f64 / f64::from(self.rate.max(1))
    }
}
