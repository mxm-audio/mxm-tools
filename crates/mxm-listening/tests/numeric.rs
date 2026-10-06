//! The numerics against closed forms: matrices built from known factors, polynomials with known
//! roots, least-squares systems with known solutions.

use mxm_listening::numeric::{Complex, Matrix, eigenvalues, least_squares, svd};

fn rotation(n: usize, i: usize, j: usize, theta: f64) -> Matrix {
    let mut r = Matrix::identity(n);
    let (s, c) = theta.sin_cos();
    r.set(i, i, c);
    r.set(j, j, c);
    r.set(i, j, -s);
    r.set(j, i, s);
    r
}

fn orthogonal(n: usize, seed: f64) -> Matrix {
    let mut q = Matrix::identity(n);
    for i in 0..n {
        for j in i + 1..n {
            let theta = seed * (1.0 + i as f64 * 0.7 + j as f64 * 1.3);
            q = q.mul(&rotation(n, i, j, theta)).unwrap();
        }
    }
    q
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol * (1.0 + b.abs())
}

#[test]
fn svd_recovers_known_singular_values_and_factors() {
    // A = Q1 · diag(5, 3, 2, 0.5) · Q2ᵀ, padded to 6 rows.
    let q1 = orthogonal(6, 0.37);
    let q2 = orthogonal(4, 0.91);
    let sigma = [5.0, 3.0, 2.0, 0.5];
    let mut d = Matrix::zeros(6, 4);
    for (k, s) in sigma.iter().enumerate() {
        d.set(k, k, *s);
    }
    let a = q1.mul(&d).unwrap().mul(&q2.transpose()).unwrap();
    let out = svd(&a).expect("converges");
    for (got, want) in out.s.iter().zip(sigma) {
        assert!(close(*got, want, 1e-12), "{got} vs {want}");
    }
    // U·diag(s)·Vᵀ reproduces A, and U and V have orthonormal columns.
    let mut us = out.u.clone();
    for r in 0..6 {
        for c in 0..4 {
            us.set(r, c, out.u.get(r, c) * out.s[c]);
        }
    }
    let back = us.mul(&out.v.transpose()).unwrap();
    for (x, y) in back.data.iter().zip(&a.data) {
        assert!((x - y).abs() < 1e-12);
    }
    let utu = out.u.transpose().mul(&out.u).unwrap();
    let vtv = out.v.transpose().mul(&out.v).unwrap();
    for i in 0..4 {
        for j in 0..4 {
            let want = if i == j { 1.0 } else { 0.0 };
            assert!((utu.get(i, j) - want).abs() < 1e-12);
            assert!((vtv.get(i, j) - want).abs() < 1e-12);
        }
    }
}

#[test]
fn svd_reports_absence_rather_than_a_wrong_answer() {
    assert!(
        svd(&Matrix::zeros(2, 3)).is_none(),
        "fewer rows than columns"
    );
    let mut a = Matrix::identity(3);
    a.set(1, 1, f64::NAN);
    assert!(svd(&a).is_none(), "non-finite");
    // A rank-deficient matrix still decomposes, with a zero singular value.
    let a = Matrix::from_rows(&[vec![1.0, 2.0], vec![2.0, 4.0], vec![3.0, 6.0]]).unwrap();
    let out = svd(&a).unwrap();
    assert!(out.s[1].abs() < 1e-12);
    assert!(close(out.s[0], (14.0f64 * 5.0).sqrt(), 1e-12));
}

/// The companion matrix of the monic polynomial with these roots.
fn companion(roots: &[Complex]) -> Matrix {
    // Expand ∏(z − r) with complex arithmetic; the roots come in conjugate pairs, so the result is real.
    let mut coef = vec![Complex::ONE];
    for &r in roots {
        let mut next = vec![Complex::ZERO; coef.len() + 1];
        for (i, &c) in coef.iter().enumerate() {
            next[i] = next[i] + c;
            next[i + 1] = next[i + 1] - c * r;
        }
        coef = next;
    }
    let n = roots.len();
    let mut m = Matrix::zeros(n, n);
    for j in 0..n {
        m.set(0, j, -coef[j + 1].re);
    }
    for i in 1..n {
        m.set(i, i - 1, 1.0);
    }
    m
}

fn assert_same_set(got: &[Complex], want: &[Complex], tol: f64) {
    assert_eq!(got.len(), want.len());
    let mut used = vec![false; got.len()];
    for w in want {
        let (i, d) = got
            .iter()
            .enumerate()
            .filter(|(i, _)| !used[*i])
            .map(|(i, g)| (i, (*g - *w).abs()))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        assert!(d < tol, "root {w:?} missed by {d}");
        used[i] = true;
    }
}

#[test]
fn eigenvalues_find_real_and_complex_roots() {
    let roots = [
        Complex::new(1.0, 0.0),
        Complex::new(2.0, 0.0),
        Complex::new(-3.0, 0.0),
        Complex::new(0.0, 1.0),
        Complex::new(0.0, -1.0),
    ];
    let got = eigenvalues(&companion(&roots)).expect("converges");
    assert_same_set(&got, &roots, 1e-9);
}

#[test]
fn eigenvalues_find_close_damped_sinusoid_poles() {
    // The poles a decaying drum's modes leave in a signal model, r·e^{±jθ}, with two modes 6 Hz apart
    // near 180 Hz — what the high-resolution mode analysis will ask of this solver. Built as a rotated
    // block diagonal (each 2 × 2 block a rotation-scaling with eigenvalues r·e^{±jθ}), which is how a
    // subspace method meets them. A companion matrix of the same roots is too ill-conditioned near
    // z = 1 to test a solver with: its coefficients alone lose the roots.
    let rate = 48_000.0;
    let n = 8;
    let mut b = Matrix::zeros(n, n);
    let mut roots = Vec::new();
    for (k, (hz, t60)) in [(180.0, 0.9), (186.0, 0.4), (410.0, 0.3), (1200.0, 0.1)]
        .into_iter()
        .enumerate()
    {
        let theta = std::f64::consts::TAU * hz / rate;
        let r = (-6.907_755_278_982_137 / (t60 * rate)).exp();
        let (s, c) = theta.sin_cos();
        let i = 2 * k;
        b.set(i, i, r * c);
        b.set(i, i + 1, -r * s);
        b.set(i + 1, i, r * s);
        b.set(i + 1, i + 1, r * c);
        roots.push(Complex::polar(r, theta));
        roots.push(Complex::polar(r, -theta));
    }
    let q = orthogonal(n, 0.29);
    let a = q.mul(&b).unwrap().mul(&q.transpose()).unwrap();
    let got = eigenvalues(&a).expect("converges");
    assert_same_set(&got, &roots, 1e-12);
}

#[test]
fn eigenvalues_of_a_rotation_similarity() {
    // Q·B·Qᵀ with B block-diagonal: a 2 × 2 rotation-scaling block (0.5 ± 0.8j) and reals 0.3, −0.7.
    let mut b = Matrix::zeros(4, 4);
    b.set(0, 0, 0.5);
    b.set(0, 1, -0.8);
    b.set(1, 0, 0.8);
    b.set(1, 1, 0.5);
    b.set(2, 2, 0.3);
    b.set(3, 3, -0.7);
    let q = orthogonal(4, 0.53);
    let a = q.mul(&b).unwrap().mul(&q.transpose()).unwrap();
    let got = eigenvalues(&a).unwrap();
    let want = [
        Complex::new(0.5, 0.8),
        Complex::new(0.5, -0.8),
        Complex::new(0.3, 0.0),
        Complex::new(-0.7, 0.0),
    ];
    assert_same_set(&got, &want, 1e-12);
}

#[test]
fn eigenvalues_report_absence_for_a_bad_matrix() {
    assert!(eigenvalues(&Matrix::zeros(2, 3)).is_none());
    let mut a = Matrix::identity(3);
    a.set(0, 2, f64::INFINITY);
    assert!(eigenvalues(&a).is_none());
    assert_eq!(eigenvalues(&Matrix::zeros(0, 0)).unwrap(), Vec::new());
}

#[test]
fn least_squares_solves_exact_and_overdetermined_systems() {
    // An exactly consistent overdetermined system: y = 2 − 3t + 0.5t².
    let ts: Vec<f64> = (0..9).map(|i| i as f64 * 0.25).collect();
    let rows: Vec<Vec<f64>> = ts.iter().map(|t| vec![1.0, *t, t * t]).collect();
    let a = Matrix::from_rows(&rows).unwrap();
    let b: Vec<f64> = ts.iter().map(|t| 2.0 - 3.0 * t + 0.5 * t * t).collect();
    let x = least_squares(&a, &b).unwrap();
    for (got, want) in x.iter().zip([2.0, -3.0, 0.5]) {
        assert!((got - want).abs() < 1e-12);
    }
    // With a known residual: the mean of 1, 2, 3, 6 is the least-squares constant.
    let a = Matrix::from_rows(&[vec![1.0], vec![1.0], vec![1.0], vec![1.0]]).unwrap();
    let x = least_squares(&a, &[1.0, 2.0, 3.0, 6.0]).unwrap();
    assert!((x[0] - 3.0).abs() < 1e-12);
}

#[test]
fn least_squares_refuses_a_rank_deficient_system() {
    let a = Matrix::from_rows(&[vec![1.0, 2.0], vec![2.0, 4.0], vec![3.0, 6.0]]).unwrap();
    assert!(least_squares(&a, &[1.0, 2.0, 3.0]).is_none());
    let a = Matrix::identity(2);
    assert!(least_squares(&a, &[1.0, f64::NAN]).is_none());
}
