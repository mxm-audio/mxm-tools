//! Measurement harnesses for the `docs/` references, which are mxm-kit's since the split.
//!
//! This crate ships nothing. It exists so the evidence behind
//! [`docs/oscillators/`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/oscillators/README.md)
//! and
//! [`docs/modulation/`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/modulation/README.md)
//! (both in mxm-kit) lives in a repository and can be reproduced from a clone, rather than sitting
//! in a scratch project on one machine.
//!
//! # There is nothing here any more, and that is the arrangement
//!
//! The harnesses are in `examples/`. The numeric core they share — the transform, its inverse,
//! `princarg`, the analysis length `N`, the exactly-periodic frequency choice `periods_for`, and the
//! two spectral weightings `a_weight` and `spectral_flatness_db` — **moved to
//! [`mxm_measure::spectrum`]**, because none of it was ever specific to these two references and the
//! rest of the repository had been re-implementing pieces of it by hand.
//!
//! That move is why this crate takes `mxm-measure` as a **normal** dependency while every shipped
//! crate takes it only as a dev-dependency: `dsp-lab` has no shipped graph to protect, and its
//! harnesses *are* measurement.
//!
//! **Measurement policy stayed here.** Deciding which bins count as wanted *is* the experiment, and
//! each harness decides it differently — `analyse`, `analyse_multi`, `harmonic_deviation`, `alias_db`
//! and `splatter_above_db` are in the harnesses that use them, even where two of them look alike. The
//! test for moving something down into `mxm-measure` is that it has no opinion about the measurement:
//! a transform, a window length, a weighting curve. A helper that encodes what a result *means* does
//! not qualify, and neither does a threshold.
//!
//! A change to `mxm-measure`'s spectrum module is a change to both references' evidence. Re-run both
//! harnesses and diff against the recorded runs.
