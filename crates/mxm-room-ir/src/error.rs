//! The one error type: every failure is a rejected input, never a half-made result.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// A material coefficient outside 0..=1 or not finite.
    InvalidMaterial(String),
    /// Geometry that is not a closed, inward-facing, planar polygonal room.
    InvalidGeometry(String),
    /// A source or receiver that is not strictly inside the room.
    PointOutsideRoom(String),
    /// The image-source tree exceeded its node budget before reaching the requested order.
    TooManyImages(usize),
    /// A render or analysis option that cannot produce a meaningful result.
    InvalidOption(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidMaterial(m) => write!(f, "invalid material: {m}"),
            Error::InvalidGeometry(m) => write!(f, "invalid geometry: {m}"),
            Error::PointOutsideRoom(m) => write!(f, "point outside room: {m}"),
            Error::TooManyImages(n) => write!(f, "image-source tree exceeded {n} nodes"),
            Error::InvalidOption(m) => write!(f, "invalid option: {m}"),
        }
    }
}

impl std::error::Error for Error {}
