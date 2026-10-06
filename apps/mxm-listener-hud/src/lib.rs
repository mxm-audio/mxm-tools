//! mxm-listener-hud: drop a sound and see what the listener hears in it.
//!
//! Three layers, one direction (`plans/plan-mxm-listener-hud.md` §2.1, in the private archive): the
//! listener interprets, `scene` decides what is shown, and the painters draw it — `scope` the
//! pitch, `glyphs` the parts and their zoom, `timeline` the sound through time, `hud` the window
//! around them. **Decoration may be invented; numbers may not**: every number on screen is the
//! listener's.

pub mod analysis;
pub mod figures;
pub mod glyphs;
pub mod hud;
pub mod list;
pub mod playback;
pub mod plot;
pub mod scene;
pub mod scope;
pub mod theme;
pub mod timeline;
pub mod worker;

pub use hud::Hud;
