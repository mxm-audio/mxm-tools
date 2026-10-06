//! Simulated rooms, generated impulse responses — offline, as plain Rust.
//!
//! The crate's contracts are in its `AGENTS.md`; the method, its sources and its evidence are
//! `research:effects/room-acoustics-simulation.md`. Every algorithm here is implemented from a
//! paper's or a standard's equations. No room-acoustics software source was opened, and no
//! software material library was used.
//!
//! # What exists
//!
//! Five milestones: the scene (R1), the geometric hybrid (R2), the wave solver with its boundary
//! models and edge diffraction (R3), the BRAS benchmark (R4, an example) and the catalogue (R5).
//!
//! - [`boundary`]: positive-real boundary models (panel absorber, Miki porous layer, or fitted to
//!   band absorption through Paris' integral), shared by the wave solver and the image sources.
//! - [`fdtd`]: the rectilinear leapfrog scheme at its stability limit, with staircased
//!   positive-real boundaries, a calibrated point source and pressure–velocity probes.
//! - [`diffraction`]: first-order edge diffraction by the Biot–Tolstoy–Medwin line integral.
//!
//! - [`geometry`]: closed polygonal rooms, validated as watertight and inward-facing, ray casting,
//!   and parametric generators: a box, an extruded floor plan, regions with openings at one height
//!   or stepped heights, and a box holding prisms.
//! - [`material`] and [`bands`]: per-octave random-incidence absorption and scattering.
//! - [`air`]: ISO 9613-1:1993 atmospheric absorption and the speed of sound.
//! - [`ism`]: image sources in arbitrary polyhedra, after Borish, *Extension of the image model to
//!   arbitrary polyhedra*, JASA 75(6), 1984, with explicit validity and visibility tests. For a
//!   rigid box it reproduces Allen & Berkley, *Image method for efficiently simulating small-room
//!   acoustics*, JASA 65(4), 1979, image for image.
//! - [`rays`]: stochastic ray tracing with scattering (after Krokstad et al. 1968 and Vorländer 1989,
//!   read as abstracts), a receiver sphere and diffuse rain, and discovery of ray-assisted image
//!   sources in non-diffuse scenes.
//! - [`late`]: the histogram becomes Poisson events at the reflection density `4πc³t²/V`, heard by
//!   every capsule as plane waves.
//! - [`directivity`]: source and capsule patterns, and arrays from mono to first-order ambisonics.
//! - [`render`]: each arrival becomes a band-limited, fractionally delayed impulse through a
//!   minimum-phase filter built from its band gains. The filter is made by real-cepstrum folding,
//!   after Oppenheim & Schafer, *Discrete-Time Signal Processing*, 3rd ed., §13.5.3. A set of
//!   sources shares time zero, lead and length.
//! - [`simulation`]: the hybrid for one scene, with its seam by image-source order.
//! - [`analysis`]: ISO 3382 decay parameters from backward integration (Schroeder, JASA 37(3),
//!   1965), echo density (Abel & Huang 2006) and band correlation. It is trusted only because its
//!   tests reproduce known synthetic decays.
//! - [`wav`] and [`sidecar`]: 32-bit float WAV writing, PCM and float WAV reading, and the JSON sidecar
//!   that records how a file was made.
//! - [`catalogue`]: the 21 generic archetypes, their surfaces blended from published rows, and the
//!   checks the `catalogue` binary's release runs (class ranges, the collection level, the
//!   consumer's WAV rules).
//!
//! # Chosen, not read
//!
//! Each of these says so where it is defined:
//! - the octave band set and how material data outside 125 Hz–4 kHz is extended ([`bands`]);
//! - the fractional-delay kernel's width and window, and the minimum-phase design grid ([`render`]);
//! - ray counts, bins, radii, floors and the continuation rule ([`rays`]);
//! - the late event rate, smoothing and floor ([`late`]);
//! - the analyser's band filters ([`analysis`]);
//! - the speed-of-sound formula and the humidity conversion ([`air`]).

#![forbid(unsafe_code)]

pub mod air;
pub mod analysis;
pub mod bands;
pub mod boundary;
pub mod catalogue;
pub mod complex;
pub mod diffraction;
pub mod directivity;
pub mod error;
pub mod fdtd;
mod fft;
pub mod geometry;
pub mod ism;
pub mod late;
pub mod material;
mod par;
pub mod rays;
pub mod render;
pub mod rng;
pub mod scene;
pub mod sidecar;
pub mod simulation;
pub mod wav;

pub use air::Air;
pub use bands::{Bands, NUM_BANDS};
pub use error::Error;
pub use geometry::{Region, Room, Vec3};
pub use ism::{Arrival, ImageSourceOptions};
pub use material::Material;
pub use render::{RenderOptions, Rendered};
pub use scene::Scene;
