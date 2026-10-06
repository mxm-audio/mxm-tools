//! `mxm-listening` listens to a sound and says what it does, part by part — and, from its L2 slice,
//! how two sounds differ, ranked by how audible each difference is, in the owner's words. It exists
//! because the owner hears a difference at once and the agents that tune the collection cannot
//! listen; every measure here is one the owner's ear had to point at first. Ships nothing.
//!
//! The plan is `plans/plan-mxm-listening.md` (in the private archive); the contract is this crate's
//! `AGENTS.md`. **L0**, this slice: preparation (decode, onset, trim and K-weighted body loudness —
//! the drum A/B page's own definitions, `prep`), the numerics (`numeric`), the percussive family
//! (`family`), and `describe` for a hit's attack, level, decay and tone over time (`parts`),
//! reported as JSON and Markdown.
//!
//! ```text
//! cargo run -p mxm-listening --release --bin listen -- describe <file> [--name NAME] [--family percussive] [--json OUT] [--md OUT]
//! ```

pub mod audibility;
pub mod compare;
pub mod curves;
pub mod describe;
pub mod family;
pub mod glossary;
pub mod golden;
pub mod keymap;
pub mod name;
pub mod numeric;
pub mod objects;
pub mod parts;
pub mod perception;
pub mod perturb;
pub mod prep;
pub mod reading;
pub mod report;
pub mod repr;
pub mod respond;
pub mod resynth;
pub mod session;
pub mod set;
pub mod sound;
pub mod split;
pub mod staircase;
pub mod stimulus;
pub mod unexplained;

pub use describe::describe;
pub use family::Family;
pub use reading::{Reading, Report, Section, Unit, Validity};
pub use sound::Sound;
