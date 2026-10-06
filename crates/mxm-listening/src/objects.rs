//! The ideal objects a pitched note is read against, each as its partials' ratios to the lowest,
//! `f_k / f₁` — an object's "tonal signature" in AAS's phrase.
//!
//! Source: `research:physical-modelling/physical-modelling-synthesis.md` §6.2, whose string, bar and
//! membrane rows were computed from the boundary-value equations and checked against every published
//! value beside them. The same rows are computed here the same way — the Euler–Bernoulli beam's roots
//! of `cos x·cosh x = ±1` and the zeros of the Bessel functions `J_m` — and tested against that table.
//! The tuned-bar rows are the makers' targets (Essl & Cook; marimba makers aim nearer 1 : 4 : 9.8) and
//! the circular-plate rows Leissa's tabulated values (*Vibration of Plates*, NASA SP-160, 1969), both
//! copied from the same table.

/// One ideal object: its name and its partial ratios, ascending from 1.
#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    pub name: &'static str,
    pub ratios: Vec<f64>,
}

/// A partial further than this from every ratio of an object is unexplained by it, and counts as this
/// far, cents.
pub const UNEXPLAINED_CENTS: f64 = 100.0;
/// What an object's ratio left without a partial costs, cents, per share of its ratios in the
/// measured range: a dense row (the harmonic series) explains any partial it happens to meet, and
/// pays here for the ratios it holds that the note does not. 30 let the harmonic row take a marimba
/// bar whose 4× and 9.9× partials it also holds.
pub const EMPTY_CENTS: f64 = 50.0;
/// An object whose score is over this, cents, does not fit: the note is nearest it, not like it. 50
/// called a xylophone bar (1 : 2.99 : 5.68 : 6.36) a clamped bar at 40 c.
pub const FIT_CENTS: f64 = 30.0;

/// Every ideal object, in a fixed order.
#[must_use]
pub fn all() -> Vec<Object> {
    let harmonic: Vec<f64> = (1..=16).map(f64::from).collect();
    let odd: Vec<f64> = (0..8).map(|k| f64::from(2 * k + 1)).collect();
    vec![
        Object {
            name: "string or open tube (harmonic)",
            ratios: harmonic,
        },
        Object {
            name: "closed tube (odd harmonics)",
            ratios: odd,
        },
        Object {
            name: "uniform bar, free ends (glockenspiel)",
            ratios: beam(-1.0, 6),
        },
        Object {
            name: "bar clamped at one end (tine, reed)",
            ratios: beam(1.0, 5),
        },
        Object {
            name: "tuned bar 1 : 4 : 10 (marimba, vibraphone)",
            ratios: vec![1.0, 4.0, 10.0],
        },
        Object {
            name: "tuned bar 1 : 3 : 6 (xylophone)",
            ratios: vec![1.0, 3.0, 6.0],
        },
        Object {
            name: "circular membrane, fixed rim",
            ratios: membrane(10),
        },
        Object {
            name: "circular plate, clamped rim",
            ratios: vec![1.0, 2.081, 3.414, 3.893, 4.996, 5.954, 6.819, 8.279, 8.722],
        },
        Object {
            name: "circular plate, simply supported",
            ratios: vec![1.0, 2.801, 5.154, 5.980, 9.747, 14.09, 14.91],
        },
        Object {
            name: "circular plate, free (gong, cymbal)",
            ratios: vec![1.0, 1.729, 2.328, 3.906, 4.112, 6.301, 6.710, 7.339],
        },
    ]
}

/// A root of `g` in `[a, b]` by bisection, where `g(a)` and `g(b)` differ in sign.
fn bisect(g: impl Fn(f64) -> f64, mut a: f64, mut b: f64) -> Option<f64> {
    let (mut ga, gb) = (g(a), g(b));
    if ga * gb > 0.0 {
        return None;
    }
    for _ in 0..200 {
        let m = 0.5 * (a + b);
        let gm = g(m);
        if gm == 0.0 {
            return Some(m);
        }
        if ga * gm < 0.0 {
            b = m;
        } else {
            (a, ga) = (m, gm);
        }
    }
    Some(0.5 * (a + b))
}

/// An Euler–Bernoulli beam's first `count` partial ratios: the roots of `cos x·cosh x = −sign`,
/// squared and divided by the first. `sign` −1 is free at both ends (`cos x·cosh x = 1`, whose root
/// at zero is the rigid motion and is skipped), +1 clamped at one end and free at the other. Divided
/// by `cosh x`, which leaves the roots where they are and keeps the function small: root `k` of the
/// free beam lies in `[kπ, (k + 1)π]`, of the clamped one in `[(k − 1)π, kπ]`.
fn beam(sign: f64, count: usize) -> Vec<f64> {
    let pi = std::f64::consts::PI;
    let g = |x: f64| x.cos() + sign / x.cosh();
    let roots: Vec<f64> = (1..=count)
        .filter_map(|k| {
            let k = k as f64;
            if sign < 0.0 {
                bisect(g, k * pi, (k + 1.0) * pi)
            } else {
                bisect(g, (k - 1.0) * pi, k * pi)
            }
        })
        .collect();
    let first = roots.first().copied().unwrap_or(1.0);
    roots.iter().map(|x| (x / first).powi(2)).collect()
}

/// The Bessel function of the first kind `J_m(x)` from its power series, which is exact enough for
/// the first zeros used here (x under 12).
fn bessel_j(m: u32, x: f64) -> f64 {
    let half = 0.5 * x;
    let mut term = half.powi(m as i32) / (1..=m).map(f64::from).product::<f64>();
    let mut sum = term;
    for k in 1..60 {
        let k = f64::from(k);
        term *= -half * half / (k * (k + f64::from(m)));
        sum += term;
        if term.abs() < 1e-17 * sum.abs().max(1e-300) {
            break;
        }
    }
    sum
}

/// A fixed-rim circular membrane's first `count` partial ratios: the zeros `j_mn` of `J_m`, for
/// `m` from 0 to 6, sorted and divided by `j_01`.
fn membrane(count: usize) -> Vec<f64> {
    let mut zeros = Vec::new();
    for m in 0..=6u32 {
        let g = |x: f64| bessel_j(m, x);
        let mut x = 0.5;
        while x < 12.0 {
            if let Some(z) = bisect(g, x, x + 0.05) {
                zeros.push(z);
            }
            x += 0.05;
        }
    }
    zeros.sort_by(f64::total_cmp);
    zeros.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    let first = zeros.first().copied().unwrap_or(1.0);
    zeros.iter().take(count).map(|z| z / first).collect()
}

/// How a note's partials sit against one object.
#[derive(Clone, Debug, PartialEq)]
pub struct Fit {
    /// Index into [`all`].
    pub object: usize,
    /// The partials' mean distance from the object's nearest ratio, weighted as the caller weighs
    /// them, cents (each at most [`UNEXPLAINED_CENTS`]).
    pub cents: f64,
    /// The share of the object's ratios in the measured range that no partial sits near.
    pub empty: f64,
    /// `cents + EMPTY_CENTS·empty`: lower is nearer.
    pub score: f64,
}

/// The ratio of `object` nearest `ratio`, and how far `ratio` is from it, cents.
#[must_use]
pub fn nearest_ratio(object: &Object, ratio: f64) -> Option<(f64, f64)> {
    object
        .ratios
        .iter()
        .map(|&r| (r, 1200.0 * (ratio / r).log2()))
        .min_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
}

/// Every object's fit to `partials` — each `(ratio to the fundamental, weight)`, the fundamental
/// itself left out; the note weighs each by its energy at the strike, so the loud principal modes
/// decide and a resonator's or a twisting mode's weaker lines do not — nearest first. Only partials within an object's range count for it (up to its
/// highest ratio and [`UNEXPLAINED_CENTS`] beyond); an object none of the partials reaches is left
/// out. Empty when no partial is given.
#[must_use]
pub fn fits(partials: &[(f64, f64)]) -> Vec<Fit> {
    let objects = all();
    let top_ratio = partials.iter().map(|p| p.0).fold(0.0, f64::max);
    let reach = 2f64.powf(UNEXPLAINED_CENTS / 1200.0);
    let mut out: Vec<Fit> = objects
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            let highest = o.ratios.last().copied()?;
            let inside: Vec<&(f64, f64)> = partials
                .iter()
                .filter(|p| p.0 <= highest * reach && p.1 > 0.0)
                .collect();
            let weight: f64 = inside.iter().map(|p| p.1).sum();
            if inside.is_empty() || weight <= 0.0 {
                return None;
            }
            let cents = inside
                .iter()
                .map(|p| {
                    let c = nearest_ratio(o, p.0).map_or(UNEXPLAINED_CENTS, |(_, c)| c.abs());
                    p.1 * c.min(UNEXPLAINED_CENTS)
                })
                .sum::<f64>()
                / weight;
            // The object's ratios above the fundamental, up to the highest partial measured.
            let slots: Vec<f64> = o
                .ratios
                .iter()
                .copied()
                .filter(|&r| r > 1.0 + 1e-9 && r <= top_ratio * reach)
                .collect();
            let empty = if slots.is_empty() {
                0.0
            } else {
                let unmet = slots
                    .iter()
                    .filter(|&&r| {
                        !inside
                            .iter()
                            .any(|p| (1200.0 * (p.0 / r).log2()).abs() < UNEXPLAINED_CENTS)
                    })
                    .count();
                unmet as f64 / slots.len() as f64
            };
            Some(Fit {
                object: i,
                cents,
                empty,
                score: cents + EMPTY_CENTS * empty,
            })
        })
        .collect();
    out.sort_by(|a, b| a.score.total_cmp(&b.score));
    out
}
