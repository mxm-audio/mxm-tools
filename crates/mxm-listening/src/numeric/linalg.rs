//! Dense real linear algebra: a row-major matrix, the singular value decomposition, the eigenvalues of
//! a real square matrix, and linear least squares.
//!
//! Techniques, all written here from their published descriptions (no implementation was opened):
//! - **SVD:** the one-sided Jacobi method of Hestenes (1958), as described in Demmel & Veselić,
//!   "Jacobi's method is more accurate than QR" (SIAM J. Matrix Anal. Appl., 1992): rotate column
//!   pairs until every pair is orthogonal; the column norms are the singular values.
//! - **Eigenvalues:** Householder reduction to upper Hessenberg form, then Francis's implicit
//!   double-shift QR iteration with deflation on small subdiagonals (Golub & Van Loan, *Matrix
//!   Computations*, 4th ed., §7.4–7.5, Algorithms 7.4.2 and 7.5.1–7.5.2). Only eigenvalues are
//!   computed, so each transformation is applied to the active block alone.
//! - **Least squares:** Householder QR and back substitution (Golub & Van Loan §5.2–5.3).
//!
//! Every function reports absence (`None`) rather than a wrong answer: a non-finite input, a shape
//! that does not fit, an iteration that does not converge, or a rank-deficient least-squares system.

// Index loops are kept: they mirror the textbook algorithms line by line, which is what a reader
// checks them against.
#![allow(clippy::needless_range_loop)]

use super::Complex;

/// A dense matrix, row-major.
#[derive(Clone, Debug, PartialEq)]
pub struct Matrix {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<f64>,
}

impl Matrix {
    #[must_use]
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            data: vec![0.0; rows * cols],
        }
    }

    #[must_use]
    pub fn identity(n: usize) -> Self {
        let mut m = Self::zeros(n, n);
        for i in 0..n {
            m.data[i * n + i] = 1.0;
        }
        m
    }

    /// Build from rows; `None` if they are ragged.
    #[must_use]
    pub fn from_rows(rows: &[Vec<f64>]) -> Option<Self> {
        let cols = rows.first().map_or(0, Vec::len);
        if rows.iter().any(|r| r.len() != cols) {
            return None;
        }
        Some(Self {
            rows: rows.len(),
            cols,
            data: rows.concat(),
        })
    }

    #[must_use]
    pub fn get(&self, r: usize, c: usize) -> f64 {
        self.data[r * self.cols + c]
    }

    pub fn set(&mut self, r: usize, c: usize, v: f64) {
        self.data[r * self.cols + c] = v;
    }

    #[must_use]
    pub fn transpose(&self) -> Self {
        let mut t = Self::zeros(self.cols, self.rows);
        for r in 0..self.rows {
            for c in 0..self.cols {
                t.data[c * self.rows + r] = self.data[r * self.cols + c];
            }
        }
        t
    }

    /// The product `self · other`; `None` if the shapes do not fit.
    #[must_use]
    pub fn mul(&self, other: &Self) -> Option<Self> {
        if self.cols != other.rows {
            return None;
        }
        let mut out = Self::zeros(self.rows, other.cols);
        for r in 0..self.rows {
            for k in 0..self.cols {
                let a = self.data[r * self.cols + k];
                if a == 0.0 {
                    continue;
                }
                for c in 0..other.cols {
                    out.data[r * other.cols + c] += a * other.data[k * other.cols + c];
                }
            }
        }
        Some(out)
    }

    fn is_finite(&self) -> bool {
        self.data.iter().all(|v| v.is_finite())
    }
}

/// A thin singular value decomposition `A = U·diag(s)·Vᵀ`: for an `m × n` matrix with `m ≥ n`, `U` is
/// `m × n` with orthonormal columns, `s` holds `n` values in descending order, and `V` is `n × n`
/// orthogonal. A column of `U` whose singular value is zero is left zero.
#[derive(Clone, Debug)]
pub struct Svd {
    pub u: Matrix,
    pub s: Vec<f64>,
    pub v: Matrix,
}

/// Sweeps before the Jacobi iteration is declared not to converge. Quadratic convergence makes a
/// dozen typical; the bound only stops a pathological input from spinning.
const JACOBI_SWEEPS: usize = 80;

/// The singular value decomposition of an `m × n` matrix with `m ≥ n` (one-sided Jacobi). For
/// `m < n`, decompose the transpose. `None` for a non-finite entry, an empty matrix, `m < n`, or no
/// convergence.
#[must_use]
pub fn svd(a: &Matrix) -> Option<Svd> {
    let (m, n) = (a.rows, a.cols);
    if m == 0 || n == 0 || m < n || !a.is_finite() {
        return None;
    }
    // Work on columns: `u` holds A's columns, rotated in place.
    let mut u: Vec<Vec<f64>> = (0..n)
        .map(|c| (0..m).map(|r| a.get(r, c)).collect())
        .collect();
    let mut v: Vec<Vec<f64>> = (0..n)
        .map(|c| (0..n).map(|r| if r == c { 1.0 } else { 0.0 }).collect())
        .collect();
    let mut converged = false;
    for _ in 0..JACOBI_SWEEPS {
        let mut rotated = false;
        for p in 0..n {
            for q in p + 1..n {
                let (mut alpha, mut beta, mut gamma) = (0.0, 0.0, 0.0);
                for i in 0..m {
                    alpha += u[p][i] * u[p][i];
                    beta += u[q][i] * u[q][i];
                    gamma += u[p][i] * u[q][i];
                }
                // Columns already orthogonal to working precision.
                if gamma == 0.0 || gamma.abs() <= f64::EPSILON * (alpha * beta).sqrt() {
                    continue;
                }
                rotated = true;
                let zeta = (beta - alpha) / (2.0 * gamma);
                let t = zeta.signum() / (zeta.abs() + (1.0 + zeta * zeta).sqrt());
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = c * t;
                for i in 0..m {
                    let (up, uq) = (u[p][i], u[q][i]);
                    u[p][i] = c * up - s * uq;
                    u[q][i] = s * up + c * uq;
                }
                for i in 0..n {
                    let (vp, vq) = (v[p][i], v[q][i]);
                    v[p][i] = c * vp - s * vq;
                    v[q][i] = s * vp + c * vq;
                }
            }
        }
        if !rotated {
            converged = true;
            break;
        }
    }
    if !converged {
        return None;
    }
    let mut order: Vec<(f64, usize)> = u
        .iter()
        .enumerate()
        .map(|(c, col)| (col.iter().map(|x| x * x).sum::<f64>().sqrt(), c))
        .collect();
    order.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut out_u = Matrix::zeros(m, n);
    let mut out_v = Matrix::zeros(n, n);
    let mut s = Vec::with_capacity(n);
    for (k, &(sigma, c)) in order.iter().enumerate() {
        s.push(sigma);
        for i in 0..m {
            let value = if sigma > 0.0 { u[c][i] / sigma } else { 0.0 };
            out_u.set(i, k, value);
        }
        for i in 0..n {
            out_v.set(i, k, v[c][i]);
        }
    }
    Some(Svd {
        u: out_u,
        s,
        v: out_v,
    })
}

/// Iterations allowed per eigenvalue before the QR iteration is declared not to converge; an
/// exceptional shift is taken every tenth.
const QR_ITERATIONS: usize = 60;

/// The eigenvalues of a real square matrix, complex pairs adjacent, in no promised order. `None` for
/// a non-square or non-finite matrix, or if the QR iteration does not converge.
#[must_use]
pub fn eigenvalues(a: &Matrix) -> Option<Vec<Complex>> {
    let n = a.rows;
    if n != a.cols || !a.is_finite() {
        return None;
    }
    if n == 0 {
        return Some(Vec::new());
    }
    let mut h: Vec<Vec<f64>> = (0..n)
        .map(|r| (0..n).map(|c| a.get(r, c)).collect())
        .collect();
    hessenberg(&mut h);
    let norm: f64 = h
        .iter()
        .flatten()
        .map(|v| v.abs())
        .sum::<f64>()
        .max(f64::MIN_POSITIVE);
    let mut values = Vec::with_capacity(n);
    let mut hi = n as isize - 1;
    let mut iterations = 0usize;
    while hi >= 0 {
        let top = hi as usize;
        // The lowest row of the active block: the first small subdiagonal above `top`.
        let mut low = top;
        while low > 0 {
            let s = h[low - 1][low - 1].abs() + h[low][low].abs();
            let s = if s == 0.0 { norm } else { s };
            if h[low][low - 1].abs() <= f64::EPSILON * s {
                h[low][low - 1] = 0.0;
                break;
            }
            low -= 1;
        }
        if low == top {
            values.push(Complex::new(h[top][top], 0.0));
            hi -= 1;
            iterations = 0;
            continue;
        }
        if low + 1 == top {
            let (p, q) = two_by_two(
                h[top - 1][top - 1],
                h[top - 1][top],
                h[top][top - 1],
                h[top][top],
            );
            values.push(p);
            values.push(q);
            hi -= 2;
            iterations = 0;
            continue;
        }
        iterations += 1;
        if iterations > QR_ITERATIONS {
            return None;
        }
        francis_step(&mut h, low, top, iterations.is_multiple_of(10));
    }
    Some(values)
}

/// Householder reduction to upper Hessenberg form, in place (similarity: eigenvalues unchanged).
fn hessenberg(h: &mut [Vec<f64>]) {
    let n = h.len();
    for k in 0..n.saturating_sub(2) {
        let x: Vec<f64> = (k + 1..n).map(|r| h[r][k]).collect();
        let Some((v, beta)) = reflector(&x) else {
            continue;
        };
        apply_left(h, &v, beta, k + 1, 0, n);
        apply_right(h, &v, beta, k + 1, 0, n);
    }
}

/// The Householder vector `v` and `β = 2/(vᵀv)` with `(I − β v vᵀ)·x = ∓‖x‖·e₁`; `None` for a zero `x`.
fn reflector(x: &[f64]) -> Option<(Vec<f64>, f64)> {
    let norm = x.iter().map(|v| v * v).sum::<f64>().sqrt();
    if norm == 0.0 {
        return None;
    }
    let alpha = if x[0] >= 0.0 { -norm } else { norm };
    let mut v = x.to_vec();
    v[0] -= alpha;
    let vv: f64 = v.iter().map(|a| a * a).sum();
    if vv == 0.0 {
        return None;
    }
    Some((v, 2.0 / vv))
}

/// `H[r0.., c0..c1] ← (I − β v vᵀ)·H[r0.., c0..c1]`, with `v` spanning rows `r0..r0+len(v)`.
fn apply_left(h: &mut [Vec<f64>], v: &[f64], beta: f64, r0: usize, c0: usize, c1: usize) {
    for c in c0..c1 {
        let s: f64 = v.iter().enumerate().map(|(i, vi)| vi * h[r0 + i][c]).sum();
        let s = beta * s;
        for (i, vi) in v.iter().enumerate() {
            h[r0 + i][c] -= s * vi;
        }
    }
}

/// `H[r0..r1, c0..] ← H[r0..r1, c0..]·(I − β v vᵀ)`, with `v` spanning columns `c0..c0+len(v)`.
fn apply_right(h: &mut [Vec<f64>], v: &[f64], beta: f64, c0: usize, r0: usize, r1: usize) {
    for row in h.iter_mut().take(r1).skip(r0) {
        let s: f64 = v.iter().enumerate().map(|(j, vj)| row[c0 + j] * vj).sum();
        let s = beta * s;
        for (j, vj) in v.iter().enumerate() {
            row[c0 + j] -= s * vj;
        }
    }
}

/// One implicit double-shift QR step on the active block `low..=top` of a Hessenberg matrix
/// (Golub & Van Loan Algorithm 7.5.1), transforming that block only. The shifts are the eigenvalues
/// of the block's trailing 2 × 2; an exceptional shift breaks a cycle.
fn francis_step(h: &mut [Vec<f64>], low: usize, top: usize, exceptional: bool) {
    let (s, t) = if exceptional {
        let e = h[top][top - 1].abs() + h[top - 1][top - 2].abs();
        (1.5 * e, e * e)
    } else {
        (
            h[top - 1][top - 1] + h[top][top],
            h[top - 1][top - 1] * h[top][top] - h[top - 1][top] * h[top][top - 1],
        )
    };
    // The first column of (H − σ₁I)(H − σ₂I) = H² − sH + tI.
    let mut x = h[low][low] * h[low][low] + h[low][low + 1] * h[low + 1][low] - s * h[low][low] + t;
    let mut y = h[low + 1][low] * (h[low][low] + h[low + 1][low + 1] - s);
    let mut z = h[low + 1][low] * h[low + 2][low + 1];
    for k in low..=top - 2 {
        if let Some((v, beta)) = reflector(&[x, y, z]) {
            let first_col = if k > low { k - 1 } else { low };
            apply_left(h, &v, beta, k, first_col, top + 1);
            let last_row = (k + 3).min(top) + 1;
            apply_right(h, &v, beta, k, low, last_row);
        }
        x = h[k + 1][k];
        y = h[k + 2][k];
        if k + 3 <= top {
            z = h[k + 3][k];
        }
    }
    if let Some((v, beta)) = reflector(&[x, y]) {
        apply_left(h, &v, beta, top - 1, top - 2, top + 1);
        apply_right(h, &v, beta, top - 1, low, top + 1);
    }
}

/// The eigenvalues of `[[a, b], [c, d]]`, computed so that neither root loses precision to
/// cancellation.
fn two_by_two(a: f64, b: f64, c: f64, d: f64) -> (Complex, Complex) {
    let mean = 0.5 * (a + d);
    let half_gap = 0.5 * (a - d);
    let disc = half_gap * half_gap + b * c;
    if disc >= 0.0 {
        let root = disc.sqrt();
        // The larger-magnitude root first, the other from the product (the determinant).
        let big = if mean >= 0.0 {
            mean + root
        } else {
            mean - root
        };
        let det = a * d - b * c;
        let small = if big != 0.0 { det / big } else { mean - root };
        (Complex::new(big, 0.0), Complex::new(small, 0.0))
    } else {
        let root = (-disc).sqrt();
        (Complex::new(mean, root), Complex::new(mean, -root))
    }
}

/// A diagonal of `R` this small against the largest means the columns are dependent. Rounding in
/// exactly dependent columns leaves a few ε, not zero; `n·ε` was too tight to see it (**chosen**).
pub const RANK_TOLERANCE: f64 = 1e-12;

/// The least-squares solution of `A·x ≈ b` for an `m × n` matrix with `m ≥ n`, by Householder QR.
/// `None` for a shape that does not fit, a non-finite input, or a rank-deficient `A` (a diagonal of
/// `R` smaller than [`RANK_TOLERANCE`] times the largest): a system without a unique answer is
/// reported absent, not solved with an invented regularisation.
#[must_use]
pub fn least_squares(a: &Matrix, b: &[f64]) -> Option<Vec<f64>> {
    let (m, n) = (a.rows, a.cols);
    if m < n || n == 0 || b.len() != m || !a.is_finite() || b.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let mut r: Vec<Vec<f64>> = (0..m)
        .map(|i| (0..n).map(|j| a.get(i, j)).collect())
        .collect();
    let mut y = b.to_vec();
    for k in 0..n {
        let x: Vec<f64> = (k..m).map(|i| r[i][k]).collect();
        let Some((v, beta)) = reflector(&x) else {
            continue;
        };
        apply_left(&mut r, &v, beta, k, k, n);
        let s: f64 = v
            .iter()
            .enumerate()
            .map(|(i, vi)| vi * y[k + i])
            .sum::<f64>()
            * beta;
        for (i, vi) in v.iter().enumerate() {
            y[k + i] -= s * vi;
        }
    }
    let largest = (0..n).map(|k| r[k][k].abs()).fold(0.0f64, f64::max);
    if largest == 0.0 || (0..n).any(|k| r[k][k].abs() <= RANK_TOLERANCE * largest) {
        return None;
    }
    let mut x = vec![0.0; n];
    for k in (0..n).rev() {
        let s: f64 = (k + 1..n).map(|j| r[k][j] * x[j]).sum();
        x[k] = (y[k] - s) / r[k][k];
    }
    Some(x)
}
