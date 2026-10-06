//! The numerics the listener needs and the workspace does not have: a complex number, and dense
//! linear algebra small enough to write and check here — a one-sided Jacobi SVD, the eigenvalues of a
//! real matrix by Hessenberg reduction and Francis's double-shift QR, and least squares by Householder
//! QR. The owner chose hand-rolled numerics over a numeric crate (the plan's revision 1); every
//! routine is scored against a closed form in `tests/numeric.rs`.
//!
//! The FFT is `mxm-measure`'s: one transform in the repository's measurement code, not a fourth.

pub mod complex;
pub mod linalg;

pub use complex::Complex;
pub use linalg::{Matrix, Svd, eigenvalues, least_squares, svd};
