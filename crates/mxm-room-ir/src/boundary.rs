//! Boundary models: the locally reacting, frequency-dependent wall shared by the wave solver and the
//! image sources of a wave-solved scene (plan §4.1, §4.2).
//!
//! # The model
//!
//! A boundary is a normalised specific admittance `β(ω) = ρc·Y(ω)`: the wall's normal velocity,
//! times `ρc`, per unit pressure. It is written as parallel **series mass–resistance–spring
//! branches**, each with impedance `jωℓ + r + κ/(jω)` (`ℓ` in s, `r` dimensionless, `κ` in 1/s). With
//! `ℓ, r, κ ≥ 0` every branch, and so the sum, is positive real. That is the condition that makes a
//! boundary passive and a time-domain scheme stable (Hamilton 2016 §2.1.3, via the research page §6;
//! its example is the parallel form `Y₀(As + B + C/s)`). The plane-wave reflection coefficient at
//! incidence `θ` is `R = (ζ cos θ − 1)/(ζ cos θ + 1)` with `ζ = 1/β` (Kowalczyk & van Walstijn 2011
//! eq. 28; Hamilton eq. 2.44), and random-incidence absorption is Paris' integral
//! `α = ∫₀^{π/2} (1 − |R(θ)|²) sin 2θ dθ` (Jeong 2009 eq. 3a).
//!
//! # Where the branches come from
//!
//! - **Panel absorber** (research page §8.1): `Z = jωM + D + ρc²/(jωd)` is exactly one branch.
//! - **Porous layer on a rigid backing** (Miki 1990, eqs. 30–35, via the research page): its
//!   admittance is not a branch sum, so branches are fitted to it and the fit error is recorded.
//! - **Fitted to band absorption**, when the construction is not known: branches are fitted so that
//!   Paris' integral reproduces the material's band absorption, and the boundary is labelled
//!   *fitted*. The reactance such a fit has is not data. The research page (§8.1 gap 4) records
//!   that no data read determines masonry reactance.
//!
//! # Chosen, not read
//!
//! - The fitting dictionary: a pure resistance, a pure spring, a pure mass, and resonant branches at
//!   half-octave steps from 16 Hz to 24 kHz with damping ratios 0.3, 1 and 3. Coefficients come
//!   from non-negative least squares (Lawson & Hanson's active-set method, written from its
//!   textbook description, not read here), with each passive subproblem solved by Householder QR.
//!   Non-negative coefficients keep every branch passive by construction.
//! - The fitting grid: 120 log-spaced frequencies from 16 Hz to 24 kHz; Paris' integral by
//!   48-point Gauss–Legendre.
//! - Band absorption is fitted by iterating the band targets until the fitted model's absorption at
//!   each band centre matches. Coefficients above 0.95 are held at 0.95, just below the 0.951
//!   ceiling a locally reacting surface can reach.

use std::f64::consts::PI;

use crate::air::Air;
use crate::bands::{self, Bands, NUM_BANDS};
use crate::complex::C64;
use crate::error::Error;

/// The highest random-incidence absorption a fit aims for.
const MAX_FITTED_ABSORPTION: f64 = 0.95;

/// One series mass–resistance–spring branch of a normalised admittance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Branch {
    /// `ℓ`, s.
    pub mass_s: f64,
    /// `r`, dimensionless.
    pub resistance: f64,
    /// `κ`, 1/s.
    pub stiffness_per_s: f64,
}

impl Branch {
    /// The branch's normalised admittance at angular frequency `omega > 0`.
    pub fn admittance(&self, omega: f64) -> C64 {
        let z = C64::new(
            self.resistance,
            omega * self.mass_s - self.stiffness_per_s / omega,
        );
        if z.norm_sqr() == 0.0 {
            return C64::ZERO;
        }
        z.inv()
    }

    fn is_passive(&self) -> bool {
        [self.mass_s, self.resistance, self.stiffness_per_s]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0)
            && (self.mass_s > 0.0 || self.resistance > 0.0 || self.stiffness_per_s > 0.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Provenance {
    Rigid,
    /// Fitted to the material's band absorption; the largest absorption error at a band centre.
    FittedToBandAbsorption {
        max_error: f64,
    },
    /// Miki's porous layer on a rigid backing, fitted with branches; the largest relative error of
    /// the fitted admittance inside the model's validity range.
    PorousLayerMiki {
        flow_resistivity_pa_s_m2: f64,
        thickness_m: f64,
        fit_error: f64,
    },
    PanelAbsorber {
        mass_kg_m2: f64,
        depth_m: f64,
        resistance_pa_s_m: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Boundary {
    pub branches: Vec<Branch>,
    pub provenance: Provenance,
}

impl Boundary {
    pub fn rigid() -> Self {
        Self {
            branches: Vec::new(),
            provenance: Provenance::Rigid,
        }
    }

    pub fn is_rigid(&self) -> bool {
        self.branches.is_empty()
    }

    /// Normalised admittance at `frequency_hz`.
    pub fn admittance(&self, frequency_hz: f64) -> C64 {
        let omega = 2.0 * PI * frequency_hz.max(1e-6);
        self.branches
            .iter()
            .fold(C64::ZERO, |sum, b| sum + b.admittance(omega))
    }

    /// Plane-wave pressure reflection coefficient at incidence `cos θ`.
    pub fn reflection(&self, cos_theta: f64, frequency_hz: f64) -> C64 {
        let beta = self.admittance(frequency_hz);
        let c = C64::real(cos_theta.clamp(0.0, 1.0));
        (c - beta) / (c + beta)
    }

    /// Random-incidence absorption by Paris' integral.
    pub fn random_incidence_absorption(&self, frequency_hz: f64) -> f64 {
        let beta = self.admittance(frequency_hz);
        paris(|cos| {
            let c = C64::real(cos);
            1.0 - ((c - beta) / (c + beta)).norm_sqr()
        })
    }

    /// Whether every branch is positive real.
    pub fn is_passive(&self) -> bool {
        self.branches.iter().all(Branch::is_passive)
    }

    /// A panel absorber: surface mass `M` (kg/m²) over an air gap of depth `d` (m) with flow
    /// resistance `D` (Pa·s/m). Exactly one branch.
    pub fn panel(
        air: &Air,
        mass_kg_m2: f64,
        depth_m: f64,
        resistance_pa_s_m: f64,
    ) -> Result<Self, Error> {
        if !(mass_kg_m2 > 0.0 && depth_m > 0.0 && resistance_pa_s_m >= 0.0) {
            return Err(Error::InvalidMaterial(
                "a panel needs positive mass and depth and non-negative resistance".into(),
            ));
        }
        let rho_c = air.characteristic_impedance();
        Ok(Self {
            branches: vec![Branch {
                mass_s: mass_kg_m2 / rho_c,
                resistance: resistance_pa_s_m / rho_c,
                stiffness_per_s: air.speed_of_sound() / depth_m,
            }],
            provenance: Provenance::PanelAbsorber {
                mass_kg_m2,
                depth_m,
                resistance_pa_s_m,
            },
        })
    }

    /// Miki's porous layer of flow resistivity `σ` (Pa·s/m²) and thickness `l` (m) on a rigid
    /// backing, fitted with branches.
    pub fn miki_layer(
        air: &Air,
        flow_resistivity_pa_s_m2: f64,
        thickness_m: f64,
    ) -> Result<Self, Error> {
        let sigma = flow_resistivity_pa_s_m2;
        if !(sigma > 0.0 && thickness_m > 0.0) {
            return Err(Error::InvalidMaterial(
                "a porous layer needs positive flow resistivity and thickness".into(),
            ));
        }
        let c = air.speed_of_sound();
        let target = |f: f64| {
            let x = f / sigma;
            let omega = 2.0 * PI * f;
            let zc = C64::new(1.0 + 0.070 * x.powf(-0.632), -0.107 * x.powf(-0.632));
            let gamma = C64::new(
                omega / c * 0.160 * x.powf(-0.618),
                omega / c * (1.0 + 0.109 * x.powf(-0.618)),
            );
            (zc * (gamma * thickness_m).coth()).inv()
        };
        // Miki's fit is valid for f/σ above about 0.01 (research page §8.1).
        let valid_from = (0.01 * sigma).max(FIT_GRID_MIN_HZ);
        let grid: Vec<f64> = fit_grid()
            .into_iter()
            .filter(|&f| f >= valid_from)
            .collect();
        let targets: Vec<C64> = grid.iter().map(|&f| target(f)).collect();
        let branches = fit_branches(&grid, &targets, 1.0)?;
        let fitted = Self {
            branches,
            provenance: Provenance::Rigid,
        };
        let fit_error = grid
            .iter()
            .zip(&targets)
            .map(|(&f, t)| (fitted.admittance(f) - *t).abs() / t.abs())
            .fold(0.0, f64::max);
        Ok(Self {
            branches: fitted.branches,
            provenance: Provenance::PorousLayerMiki {
                flow_resistivity_pa_s_m2: sigma,
                thickness_m,
                fit_error,
            },
        })
    }

    /// Branches whose random-incidence absorption reproduces `absorption` at the band centres.
    pub fn fitted_to_absorption(absorption: &Bands) -> Result<Self, Error> {
        if absorption.iter().any(|a| !(0.0..=1.0).contains(a)) {
            return Err(Error::InvalidMaterial(
                "band absorption must lie in 0..=1".into(),
            ));
        }
        let wanted = absorption.map(|a| a.min(MAX_FITTED_ABSORPTION));
        if wanted.iter().all(|&a| a == 0.0) {
            return Ok(Self::rigid());
        }
        let grid = fit_grid();
        let mut aim = wanted;
        let mut best: Option<(f64, Vec<Branch>)> = None;
        for _ in 0..30 {
            let beta = aim.map(|a| real_admittance_for(a.clamp(0.0, MAX_FITTED_ABSORPTION)));
            let targets: Vec<C64> = grid
                .iter()
                .map(|&f| C64::real(interpolate_bands(&beta, f)))
                .collect();
            let branches = fit_branches(&grid, &targets, 0.05)?;
            let model = Self {
                branches: branches.clone(),
                provenance: Provenance::Rigid,
            };
            let achieved: Bands = std::array::from_fn(|k| {
                model.random_incidence_absorption(bands::exact_centre_hz(k))
            });
            let error = (0..NUM_BANDS)
                .map(|k| (achieved[k] - wanted[k]).abs())
                .fold(0.0, f64::max);
            if best.as_ref().is_none_or(|(e, _)| error < *e) {
                best = Some((error, branches));
            }
            if error < 0.002 {
                break;
            }
            for k in 0..NUM_BANDS {
                aim[k] = (aim[k] + wanted[k] - achieved[k]).clamp(0.0, MAX_FITTED_ABSORPTION);
            }
        }
        let (max_error, branches) = best.expect("at least one fit ran");
        Ok(Self {
            branches,
            provenance: Provenance::FittedToBandAbsorption { max_error },
        })
    }

    /// Stable text for scene hashing.
    pub fn canonical_text(&self) -> String {
        let branches: Vec<String> = self
            .branches
            .iter()
            .map(|b| format!("{:e}/{:e}/{:e}", b.mass_s, b.resistance, b.stiffness_per_s))
            .collect();
        format!("{:?} [{}]", self.provenance, branches.join(" "))
    }
}

const FIT_GRID_MIN_HZ: f64 = 16.0;
const FIT_GRID_MAX_HZ: f64 = 24_000.0;

fn fit_grid() -> Vec<f64> {
    let n = 120;
    (0..n)
        .map(|i| {
            FIT_GRID_MIN_HZ * (FIT_GRID_MAX_HZ / FIT_GRID_MIN_HZ).powf(i as f64 / (n - 1) as f64)
        })
        .collect()
}

/// Band values interpolated linearly over log frequency, held beyond the outer centres.
fn interpolate_bands(values: &Bands, frequency_hz: f64) -> f64 {
    let first = bands::exact_centre_hz(0);
    let last = bands::exact_centre_hz(NUM_BANDS - 1);
    if frequency_hz <= first {
        return values[0];
    }
    if frequency_hz >= last {
        return values[NUM_BANDS - 1];
    }
    let x = (frequency_hz / first).log10() / 0.3;
    let k = (x.floor() as usize).min(NUM_BANDS - 2);
    let t = x - k as f64;
    (1.0 - t) * values[k] + t * values[k + 1]
}

/// Paris' integral of `absorption(cos θ)` by 48-point Gauss–Legendre over `θ ∈ [0, π/2]`.
fn paris(absorption: impl Fn(f64) -> f64) -> f64 {
    let (nodes, weights) = gauss_legendre_48();
    let half = PI / 4.0;
    nodes
        .iter()
        .zip(weights.iter())
        .map(|(&x, &w)| {
            let theta = half * (x + 1.0);
            w * absorption(theta.cos()) * (2.0 * theta).sin()
        })
        .sum::<f64>()
        * half
}

/// Nodes and weights of Gauss–Legendre quadrature on [−1, 1], by Newton iteration on `P₄₈`.
fn gauss_legendre_48() -> (Vec<f64>, Vec<f64>) {
    let n = 48;
    let mut nodes = Vec::with_capacity(n);
    let mut weights = Vec::with_capacity(n);
    for i in 0..n {
        let mut x = (PI * (i as f64 + 0.75) / (n as f64 + 0.5)).cos();
        let mut dp = 0.0;
        for _ in 0..100 {
            let (mut p0, mut p1) = (1.0, x);
            for k in 2..=n {
                let p2 = ((2 * k - 1) as f64 * x * p1 - (k - 1) as f64 * p0) / k as f64;
                p0 = p1;
                p1 = p2;
            }
            dp = n as f64 * (x * p1 - p0) / (x * x - 1.0);
            let dx = p1 / dp;
            x -= dx;
            if dx.abs() < 1e-15 {
                break;
            }
        }
        nodes.push(x);
        weights.push(2.0 / ((1.0 - x * x) * dp * dp));
    }
    (nodes, weights)
}

/// The real normalised admittance whose random-incidence absorption is `alpha` (at most the
/// ceiling), by bisection on the rising branch below the maximum.
fn real_admittance_for(alpha: f64) -> f64 {
    let absorption = |beta: f64| {
        paris(|cos| {
            let r = (cos - beta) / (cos + beta);
            1.0 - r * r
        })
    };
    // The maximum lies at ζ ≈ 1.567, β ≈ 0.638 (Jeong 2009).
    let (mut lo, mut hi) = (0.0, 0.638);
    if alpha >= absorption(hi) {
        return hi;
    }
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if absorption(mid) < alpha {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Fits branches to complex admittance targets on `grid` by non-negative least squares.
/// `reactance_weight` scales the imaginary rows. A band-absorption fit has no reactance target and
/// passes a small weight: a passive admittance whose resistance rises with frequency must carry the
/// reactance Kramers–Kronig ties to it, and demanding zero reactance flattened the rise (measured:
/// carpet reached 0.04 of 0.37 at 1 kHz).
fn fit_branches(
    grid: &[f64],
    targets: &[C64],
    reactance_weight: f64,
) -> Result<Vec<Branch>, Error> {
    // Dictionary, each shape linear in its gain g ≥ 0: a pure resistance (g), a pure spring
    // (g·s), a pure mass (g/s), and resonant branches g·s/(s² + 2ζω₀s + ω₀²).
    #[derive(Clone, Copy)]
    enum Shape {
        Resistance,
        Spring,
        Mass,
        Resonant(f64, f64),
    }
    let mut shapes = vec![Shape::Resistance, Shape::Spring, Shape::Mass];
    let mut f0 = FIT_GRID_MIN_HZ;
    while f0 <= FIT_GRID_MAX_HZ * 1.001 {
        for zeta in [0.3, 1.0, 3.0] {
            shapes.push(Shape::Resonant(f0, zeta));
        }
        f0 *= std::f64::consts::SQRT_2;
    }
    let column = |shape: Shape, f: f64| -> C64 {
        let s = C64::new(0.0, 2.0 * PI * f);
        match shape {
            Shape::Resistance => C64::ONE,
            Shape::Spring => s / (2.0 * PI * 1000.0),
            Shape::Mass => (s / (2.0 * PI * 1000.0)).inv(),
            Shape::Resonant(f0, zeta) => {
                let w0 = 2.0 * PI * f0;
                s / (s * s + s * (2.0 * zeta * w0) + C64::real(w0 * w0))
            }
        }
    };
    // Relative weighting, so small admittances are fitted as carefully as large ones.
    let weights: Vec<f64> = targets.iter().map(|t| 1.0 / t.abs().max(1e-4)).collect();
    let rows = 2 * grid.len();
    let cols = shapes.len();
    let mut a = vec![vec![0.0; cols]; rows];
    let mut b = vec![0.0; rows];
    for (i, (&f, t)) in grid.iter().zip(targets).enumerate() {
        for (j, &shape) in shapes.iter().enumerate() {
            let v = column(shape, f) * weights[i];
            a[2 * i][j] = v.re;
            a[2 * i + 1][j] = v.im * reactance_weight;
        }
        b[2 * i] = t.re * weights[i];
        b[2 * i + 1] = t.im * weights[i] * reactance_weight;
    }
    let gains = nnls(&a, &b);
    let mut branches = Vec::new();
    let unit = 2.0 * PI * 1000.0;
    for (&shape, &g) in shapes.iter().zip(&gains) {
        if g <= 0.0 {
            continue;
        }
        branches.push(match shape {
            Shape::Resistance => Branch {
                mass_s: 0.0,
                resistance: 1.0 / g,
                stiffness_per_s: 0.0,
            },
            // g·s/unit = s/κ.
            Shape::Spring => Branch {
                mass_s: 0.0,
                resistance: 0.0,
                stiffness_per_s: unit / g,
            },
            // g·unit/s = 1/(ℓ·s).
            Shape::Mass => Branch {
                mass_s: 1.0 / (g * unit),
                resistance: 0.0,
                stiffness_per_s: 0.0,
            },
            Shape::Resonant(f0, zeta) => {
                let w0 = 2.0 * PI * f0;
                Branch {
                    mass_s: 1.0 / g,
                    resistance: 2.0 * zeta * w0 / g,
                    stiffness_per_s: w0 * w0 / g,
                }
            }
        });
    }
    if branches.iter().any(|b| !b.is_passive()) {
        return Err(Error::InvalidMaterial(
            "boundary fit produced a non-passive branch".into(),
        ));
    }
    Ok(branches)
}

/// Non-negative least squares, `min ‖Ax − b‖` subject to `x ≥ 0`, by the Lawson–Hanson active-set
/// method with columns normalised to unit length.
fn nnls(a: &[Vec<f64>], b: &[f64]) -> Vec<f64> {
    let rows = a.len();
    let cols = a[0].len();
    let norms: Vec<f64> = (0..cols)
        .map(|j| {
            (0..rows)
                .map(|i| a[i][j] * a[i][j])
                .sum::<f64>()
                .sqrt()
                .max(1e-300)
        })
        .collect();
    let col = |j: usize, i: usize| a[i][j] / norms[j];
    let mut x = vec![0.0; cols];
    let mut passive = vec![false; cols];
    let residual = |x: &[f64]| -> Vec<f64> {
        (0..rows)
            .map(|i| b[i] - (0..cols).map(|j| col(j, i) * x[j]).sum::<f64>())
            .collect()
    };
    // Least squares on the passive columns by Householder QR: the dictionary's columns overlap
    // strongly, and normal equations square that ill-conditioning (measured: the carpet fit
    // failed with them).
    let solve_passive = |passive: &[bool]| -> Vec<f64> {
        let idx: Vec<usize> = (0..cols).filter(|&j| passive[j]).collect();
        let k = idx.len();
        let mut q: Vec<Vec<f64>> = idx
            .iter()
            .map(|&j| (0..rows).map(|i| col(j, i)).collect())
            .collect();
        let mut rhs = b.to_vec();
        for j in 0..k.min(rows) {
            let norm = q[j][j..].iter().map(|v| v * v).sum::<f64>().sqrt();
            if norm < 1e-14 {
                continue;
            }
            let alpha = if q[j][j] > 0.0 { -norm } else { norm };
            let mut v: Vec<f64> = q[j][j..].to_vec();
            v[0] -= alpha;
            let vv: f64 = v.iter().map(|x| x * x).sum();
            if vv == 0.0 {
                continue;
            }
            for column in q.iter_mut().skip(j) {
                let dot: f64 = v.iter().zip(&column[j..]).map(|(a, c)| a * c).sum();
                let f = 2.0 * dot / vv;
                for (c, a) in column[j..].iter_mut().zip(&v) {
                    *c -= f * a;
                }
            }
            let dot: f64 = v.iter().zip(&rhs[j..]).map(|(a, c)| a * c).sum();
            let f = 2.0 * dot / vv;
            for (c, a) in rhs[j..].iter_mut().zip(&v) {
                *c -= f * a;
            }
        }
        let mut solution = vec![0.0; k];
        for j in (0..k.min(rows)).rev() {
            if q[j][j].abs() < 1e-12 {
                continue;
            }
            let mut acc = rhs[j];
            for c in j + 1..k {
                acc -= q[c][j] * solution[c];
            }
            solution[j] = acc / q[j][j];
        }
        let mut z = vec![0.0; cols];
        for (p, &j) in idx.iter().enumerate() {
            z[j] = solution[p];
        }
        z
    };
    for _ in 0..(20 * cols) {
        let r = residual(&x);
        let w: Vec<f64> = (0..cols)
            .map(|j| (0..rows).map(|i| col(j, i) * r[i]).sum())
            .collect();
        let candidate = (0..cols)
            .filter(|&j| !passive[j] && w[j] > 1e-12)
            .max_by(|&p, &q| w[p].partial_cmp(&w[q]).unwrap_or(std::cmp::Ordering::Equal));
        let Some(j) = candidate else {
            break;
        };
        passive[j] = true;
        loop {
            let z = solve_passive(&passive);
            if (0..cols).filter(|&k| passive[k]).all(|k| z[k] > 0.0) {
                x = z;
                break;
            }
            let alpha = (0..cols)
                .filter(|&k| passive[k] && z[k] <= 0.0)
                .map(|k| x[k] / (x[k] - z[k]))
                .fold(f64::INFINITY, f64::min);
            for k in 0..cols {
                x[k] += alpha * (z[k] - x[k]);
                if passive[k] && x[k] <= 1e-15 {
                    passive[k] = false;
                    x[k] = 0.0;
                }
            }
            if !passive.iter().any(|&p| p) {
                break;
            }
        }
    }
    x.iter()
        .zip(&norms)
        .map(|(v, n)| (v / n).max(0.0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Jeong 2009: a locally reacting surface's random-incidence absorption cannot exceed 0.951, at
    /// specific impedance 1.567 (research page §8.1).
    #[test]
    fn paris_integral_reaches_its_published_ceiling() {
        let beta = 1.0 / 1.567;
        let alpha = paris(|cos| {
            let r = (cos - beta) / (cos + beta);
            1.0 - r * r
        });
        assert!((alpha - 0.9512).abs() < 5e-4, "{alpha}");
        assert!((real_admittance_for(0.5) - 0.12).abs() < 0.1);
    }

    #[test]
    fn panel_resonates_where_the_formula_says() {
        let air = Air::standard();
        let (m, d) = (11.0, 0.05);
        let panel = Boundary::panel(&air, m, d, 150.0).unwrap();
        assert!(panel.is_passive());
        // Research page §8.1: f0 ≈ 60/√(M·d).
        let expected = 60.0 / (m * d).sqrt();
        let best = (0..2000)
            .map(|i| 20.0 * 1.003f64.powi(i))
            .max_by(|&a, &b| {
                panel
                    .random_incidence_absorption(a)
                    .partial_cmp(&panel.random_incidence_absorption(b))
                    .unwrap()
            })
            .unwrap();
        assert!((best / expected - 1.0).abs() < 0.08, "{best} vs {expected}");
    }

    #[test]
    fn fits_reproduce_band_absorption_and_stay_passive() {
        for (name, alpha) in [
            ("plaster", [0.02, 0.02, 0.03, 0.04, 0.05, 0.05]),
            ("wood lining", [0.28, 0.22, 0.17, 0.09, 0.10, 0.11]),
            ("carpet", [0.02, 0.06, 0.14, 0.37, 0.60, 0.65]),
            ("glass wool", [0.17, 0.55, 0.80, 0.90, 0.85, 0.80]),
        ] {
            let bands = bands::from_125_to_4k(alpha);
            let fit = Boundary::fitted_to_absorption(&bands).unwrap();
            assert!(fit.is_passive(), "{name}");
            for f in fit_grid() {
                assert!(fit.admittance(f).re >= 0.0, "{name} at {f} Hz");
            }
            for (k, &target) in bands.iter().enumerate() {
                let got = fit.random_incidence_absorption(bands::exact_centre_hz(k));
                assert!(
                    (got - target).abs() < 0.02,
                    "{name} band {k}: {got:.3} vs {target:.3}"
                );
            }
        }
        assert!(
            Boundary::fitted_to_absorption(&[0.0; NUM_BANDS])
                .unwrap()
                .is_rigid()
        );
    }

    #[test]
    fn miki_layer_fits_its_model() {
        let air = Air::standard();
        // Glass wool: σ within Komatsu's 6,000–72,900 Pa·s/m² range (research page §8.1).
        let layer = Boundary::miki_layer(&air, 15_000.0, 0.05).unwrap();
        assert!(layer.is_passive());
        let Provenance::PorousLayerMiki { fit_error, .. } = layer.provenance else {
            panic!("provenance");
        };
        assert!(fit_error < 0.15, "fit error {fit_error}");
        // A 50 mm layer absorbs little at 125 Hz and most of the energy by 1 kHz.
        assert!(layer.random_incidence_absorption(125.0) < 0.4);
        assert!(layer.random_incidence_absorption(1000.0) > 0.7);
    }

    #[test]
    fn rigid_reflects_everything() {
        let r = Boundary::rigid().reflection(0.3, 500.0);
        assert!((r - C64::ONE).abs() < 1e-15);
    }
}
