//! First-order edge diffraction: the Biot–Tolstoy–Medwin line integral (plan §4.2, revision 10).
//!
//! Read in Calamia's thesis (2009), which restates Svensson, Fred & Vanderkooy (1999), for a
//! source whose free-field response is `δ(t − d/c)/d`, the same level convention as the rest of
//! this crate (`research:effects/room-acoustics-simulation.md` §7.1):
//!
//! `h(t) = −(ν/4π) Σᵢ ∫ δ(t − (m + l)/c) · βᵢ/(m·l) dz`,
//! `βᵢ = sin(νφᵢ) / (cosh(νη) − cos(νφᵢ))`,
//! `cosh η = (m·l + (z − z_S)(z − z_R)) / (r_S·r_R)`,
//!
//! with `ν = π/θ_W`, `φᵢ = π ± θ_S ± θ_R`, and `m`, `l` the distances from source and receiver to the
//! edge point at `z`. Faces are rigid, as the model requires. The model has no exact solution for
//! impedance faces, so a wall's absorption only scales the specular legs of a path, never the edge
//! itself.
//!
//! # What is diffracted
//!
//! Every edge whose open angle `θ_W` is neither `π` (coplanar faces) nor `π/m` for an integer `m`
//! (§7.2 of the thesis: such wedges do not diffract, which covers every corner of a box). For each
//! such edge, the paths **source → edge → receiver**, paths with one specular reflection before or
//! after the edge, and paths with one on each side: a partition on a floor needs all four, which
//! arrive together at low frequencies.
//!
//! # How it is integrated, chosen
//!
//! The integral runs along the edge by adaptive Simpson quadrature. The edge is split at the apex,
//! the least-time point (thesis eq. 4.11), where the integrand is sharpest, and each piece is
//! refined until its estimate converges and its span of arrival times is under a quarter of a
//! sample at the options' rate (48 kHz by default). Each converged piece becomes one **node**: an arrival time, a weight (the
//! integral over the piece), and its arrival direction. The renderer area-samples the nodes at
//! the output rate, which is the discrete-time form of eqs. 3.12–3.13.
//!
//! A receiver exactly on a zone boundary makes a term singular. The thesis gives analytic
//! first-sample approximations for it (§4.3), whose coefficients did not survive the text we read,
//! so a term within `1e-12` of `cos(νφ) = 1` is dropped. That happens only for a receiver exactly
//! on the boundary. Next to a boundary the quadrature resolves the peak; a test checks the thesis's
//! limit, half the geometrical component with opposite sign, from both sides.
//!
//! Visibility of the legs is tested at points 5 cm apart along the edge, not per node.

use std::collections::HashMap;
use std::f64::consts::PI;

use crate::bands::{self, Bands};
use crate::geometry::{Room, Vec3};
use crate::scene::Scene;

/// A diffracting edge in its own cylindrical frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    /// Start of the edge; `z` is measured from here along `axis`.
    pub start: Vec3,
    pub length_m: f64,
    /// Unit vector along the edge.
    pub axis: Vec3,
    /// Unit vector in face A, perpendicular to the edge, pointing into face A: `θ = 0`.
    pub along_a: Vec3,
    /// Face A's inward normal: the direction `θ` grows toward.
    pub normal_a: Vec3,
    /// Open angle through the air, `θ_W`.
    pub open_angle: f64,
    pub faces: [usize; 2],
}

impl Edge {
    /// The wedge index `ν = π/θ_W`.
    pub fn index(&self) -> f64 {
        PI / self.open_angle
    }

    /// Cylindrical coordinates `(r, θ, z)` of `p`.
    pub fn coordinates(&self, p: Vec3) -> (f64, f64, f64) {
        let d = p - self.start;
        let z = d.dot(self.axis);
        let perp = d - self.axis * z;
        let theta = perp
            .dot(self.normal_a)
            .atan2(perp.dot(self.along_a))
            .rem_euclid(2.0 * PI);
        (perp.length(), theta, z)
    }

    fn point(&self, z: f64) -> Vec3 {
        self.start + self.axis * z
    }
}

/// Every diffracting edge of a room.
pub fn diffracting_edges(room: &Room) -> Vec<Edge> {
    let key = |p: Vec3| {
        let q = |v: f64| (v * 1e6).round() as i64;
        (q(p.x), q(p.y), q(p.z))
    };
    type Key = (i64, i64, i64);
    let mut directed: HashMap<(Key, Key), (usize, Vec3, Vec3)> = HashMap::new();
    for (f, poly) in room.polygons.iter().enumerate() {
        let n = poly.vertices.len();
        for i in 0..n {
            let (a, b) = (poly.vertices[i], poly.vertices[(i + 1) % n]);
            directed.insert((key(a), key(b)), (f, a, b));
        }
    }
    let mut edges = Vec::new();
    for (&(ka, kb), &(face_a, a, b)) in &directed {
        if ka > kb {
            continue;
        }
        let Some(&(face_b, _, _)) = directed.get(&(kb, ka)) else {
            continue;
        };
        let (pa, pb) = (&room.polygons[face_a], &room.polygons[face_b]);
        let axis_raw = b - a;
        let length = axis_raw.length();
        if length < 1e-9 {
            continue;
        }
        let e = axis_raw * (1.0 / length);
        // Into a face: test a point just off the edge's midpoint. A vertex average would do for a
        // convex face and fails for an L-shaped floor, whose average lies in the notch.
        let mid = (a + b) * 0.5;
        let into = |poly: &crate::geometry::Polygon| {
            let u = poly.normal.cross(e).normalized();
            let probe = mid + u * (1e-6 * length.max(1.0));
            if poly.contains_coplanar(probe) { u } else { -u }
        };
        let (ua, ub) = (into(pa), into(pb));
        let open = ub.dot(pa.normal).atan2(ub.dot(ua)).rem_euclid(2.0 * PI);
        let nu = PI / open;
        let integral = (nu - nu.round()).abs() < 1e-6 && nu.round() >= 1.0;
        if (open - PI).abs() < 1e-6 || integral {
            continue;
        }
        // θ runs from face A toward its normal, so the edge axis is A's in-face direction crossed
        // with that normal; the edge starts at whichever end that axis points away from.
        let axis = ua.cross(pa.normal).normalized();
        let start = if (b - a).dot(axis) >= 0.0 { a } else { b };
        edges.push(Edge {
            start,
            length_m: length,
            axis,
            along_a: ua,
            normal_a: pa.normal,
            open_angle: open,
            faces: [face_a, face_b],
        });
    }
    edges.sort_by(|x, y| {
        (x.start.x, x.start.y, x.start.z, x.length_m)
            .partial_cmp(&(y.start.x, y.start.y, y.start.z, y.length_m))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    edges
}

/// One piece of a diffracted arrival.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Node {
    /// Unfolded path length through the piece, m.
    pub path_length_m: f64,
    /// Pressure impulse weight (the line integral over the piece), for a 1 m reference.
    pub weight: f64,
    /// Unit vector from the receiver toward where this piece arrives from.
    pub direction: Vec3,
    /// Unit vector from the source toward where this piece leaves it.
    pub departure: Vec3,
}

/// A diffracted path: one edge, at most one specular reflection on each side of it.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffractedPath {
    pub edge: usize,
    /// Face reflected from between source and edge, if any.
    pub before: Option<usize>,
    /// Face reflected from between edge and receiver, if any.
    pub after: Option<usize>,
    /// The specular legs' wall reflections, per band.
    pub reflection: Bands,
    pub nodes: Vec<Node>,
}

impl DiffractedPath {
    /// The shortest path length among the nodes, m.
    pub fn earliest_m(&self) -> f64 {
        self.nodes
            .iter()
            .map(|n| n.path_length_m)
            .fold(f64::INFINITY, f64::min)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffractionOptions {
    /// Specular reflections allowed around the edge: 0; 1, before or after it; or 2, also one on
    /// each side.
    pub specular_order: usize,
    /// Rate whose quarter-sample bounds each node's span of arrival times, Hz.
    pub sample_rate: f64,
}

impl Default for DiffractionOptions {
    fn default() -> Self {
        Self {
            specular_order: 2,
            sample_rate: 48_000.0,
        }
    }
}

/// First-order diffracted paths of a scene.
pub fn diffracted_paths(scene: &Scene, options: &DiffractionOptions) -> Vec<DiffractedPath> {
    let room = &scene.room;
    let edges = diffracting_edges(room);
    let speed = scene.air.speed_of_sound();
    let reflections: Vec<Bands> = room
        .materials
        .iter()
        .map(|m| m.specular_pressure_reflection())
        .collect();
    let mut out = Vec::new();
    for (e, edge) in edges.iter().enumerate() {
        // Direct legs.
        let mut combos: Vec<(Option<usize>, Option<usize>)> = vec![(None, None)];
        let faces: Vec<usize> = (0..room.polygons.len())
            .filter(|f| !edge.faces.contains(f))
            .collect();
        if options.specular_order >= 1 {
            for &f in &faces {
                combos.push((Some(f), None));
                combos.push((None, Some(f)));
            }
        }
        if options.specular_order >= 2 {
            // A partition on a floor: the floor before and after the crest. Without this path a
            // low-frequency shadow loses one of four coherent arrivals (BRAS RS5, plan R4).
            for &f in &faces {
                for &g in &faces {
                    combos.push((Some(f), Some(g)));
                }
            }
        }
        for (before, after) in combos {
            let mut reflection = bands::uniform(1.0);
            for f in [before, after].into_iter().flatten() {
                reflection = bands::mul(&reflection, &reflections[room.polygons[f].material]);
            }
            if reflection.iter().all(|&g| g == 0.0) {
                // An anechoic leg: nothing to integrate.
                continue;
            }
            let source = match before {
                None => scene.source,
                Some(f) => {
                    let poly = &room.polygons[f];
                    if poly.signed_distance(scene.source) <= 1e-9 {
                        continue;
                    }
                    poly.mirror(scene.source)
                }
            };
            let receiver = match after {
                None => scene.receiver,
                Some(f) => {
                    let poly = &room.polygons[f];
                    if poly.signed_distance(scene.receiver) <= 1e-9 {
                        continue;
                    }
                    poly.mirror(scene.receiver)
                }
            };
            let (rs, ts, _) = edge.coordinates(source);
            let (rr, tr, _) = edge.coordinates(receiver);
            let inside = |t: f64| t >= 0.0 && t <= edge.open_angle;
            if rs < 1e-6 || rr < 1e-6 || !inside(ts) || !inside(tr) {
                continue;
            }
            // Visibility per 5 cm along the edge.
            let pieces = ((edge.length_m / 0.05).ceil() as usize).max(4);
            let visible: Vec<bool> = (0..pieces)
                .map(|i| {
                    let p = edge.point((i as f64 + 0.5) / pieces as f64 * edge.length_m);
                    leg_valid(room, scene.source, source, before, p, edge)
                        && leg_valid(room, scene.receiver, receiver, after, p, edge)
                })
                .collect();
            if !visible.iter().any(|&v| v) {
                continue;
            }
            let raw = edge_response(edge, source, receiver, speed, options.sample_rate);
            let mut nodes = Vec::with_capacity(raw.len());
            for (z, path_length_m, weight) in raw {
                let piece = ((z / edge.length_m * pieces as f64) as usize).min(pieces - 1);
                if !visible[piece] {
                    continue;
                }
                let p = edge.point(z);
                let direction = match after {
                    None => (p - scene.receiver).normalized(),
                    Some(f) => (reflection_point(&room.polygons[f], p, receiver) - scene.receiver)
                        .normalized(),
                };
                let departure = match before {
                    None => (p - scene.source).normalized(),
                    Some(f) => {
                        (reflection_point(&room.polygons[f], source, p) - scene.source).normalized()
                    }
                };
                nodes.push(Node {
                    path_length_m,
                    weight,
                    direction,
                    departure,
                });
            }
            if nodes.is_empty() {
                continue;
            }
            out.push(DiffractedPath {
                edge: e,
                before,
                after,
                reflection,
                nodes,
            });
        }
    }
    out
}

/// Where the segment from `a` to `b` meets the polygon's plane.
fn reflection_point(poly: &crate::geometry::Polygon, a: Vec3, b: Vec3) -> Vec3 {
    let d = b - a;
    let denom = poly.normal.dot(d);
    if denom.abs() < 1e-12 {
        return a;
    }
    let t = (poly.offset - poly.normal.dot(a)) / denom;
    a + d * t
}

/// Whether the leg from a real point (via an optional mirror face) to the edge point `p` exists.
fn leg_valid(
    room: &Room,
    real: Vec3,
    image: Vec3,
    face: Option<usize>,
    p: Vec3,
    edge: &Edge,
) -> bool {
    let skip = |g: usize| edge.faces.contains(&g);
    let clear = |a: Vec3, b: Vec3, also: Option<usize>| {
        !room.polygons.iter().enumerate().any(|(g, poly)| {
            !skip(g) && Some(g) != also && poly.segment_intersection(a, b, 1e-6).is_some()
        })
    };
    match face {
        None => clear(real, p, None),
        Some(f) => {
            let poly = &room.polygons[f];
            match poly.segment_intersection(image, p, 1e-9) {
                Some((_, hit)) => clear(real, hit, Some(f)) && clear(hit, p, Some(f)),
                None => false,
            }
        }
    }
}

/// The BTM line integral for one edge, as `(z, path length, weight)` nodes. `source` and
/// `receiver` may be images; the weight includes `−ν/4π`.
pub fn edge_response(
    edge: &Edge,
    source: Vec3,
    receiver: Vec3,
    speed: f64,
    sample_rate: f64,
) -> Vec<(f64, f64, f64)> {
    let (rs, ts, zs) = edge.coordinates(source);
    let (rr, tr, zr) = edge.coordinates(receiver);
    let nu = edge.index();
    let phis = [PI + ts + tr, PI + ts - tr, PI - ts + tr, PI - ts - tr];
    let terms: Vec<(f64, f64)> = phis
        .iter()
        .map(|&phi| ((nu * phi).sin(), (nu * phi).cos()))
        .filter(|&(_, c)| (1.0 - c).abs() > 1e-12)
        .collect();
    let lengths = |z: f64| {
        let m = (rs * rs + (z - zs).powi(2)).sqrt();
        let l = (rr * rr + (z - zr).powi(2)).sqrt();
        (m, l)
    };
    let integrand = |z: f64| {
        let (m, l) = lengths(z);
        let cosh_eta = ((m * l + (z - zs) * (z - zr)) / (rs * rr)).max(1.0);
        let eta = cosh_eta.acosh();
        let cosh_nu_eta = (nu * eta).cosh();
        let beta: f64 = terms.iter().map(|&(s, c)| s / (cosh_nu_eta - c)).sum();
        -nu / (4.0 * PI) * beta / (m * l)
    };
    let tau = |z: f64| {
        let (m, l) = lengths(z);
        m + l
    };
    // A piece a ten-millionth of the edge long still resolves the peak next to a zone boundary,
    // whose width grows as the square root of the distance from it; below that, rounding in
    // `cosh η` is all a split would chase.
    let limits = Limits {
        max_span: speed / sample_rate * 0.25,
        min_piece: 1e-7 * edge.length_m.max(1.0),
    };
    let apex = (zr * rs + zs * rr) / (rs + rr);
    let mut splits = vec![0.0];
    if apex > 0.0 && apex < edge.length_m {
        splits.push(apex);
    }
    splits.push(edge.length_m);
    let scale = 1.0 / ((rs + rr) * (rs + rr));
    let mut nodes = Vec::new();
    for w in splits.windows(2) {
        let (a, b) = (w[0], w[1]);
        if b - a <= 0.0 {
            continue;
        }
        let (fa, fb) = (integrand(a), integrand(b));
        let m = 0.5 * (a + b);
        let fm = integrand(m);
        let whole = (b - a) / 6.0 * (fa + 4.0 * fm + fb);
        simpson(
            &integrand,
            &tau,
            Interval {
                a,
                b,
                fa,
                fm,
                fb,
                whole,
            },
            1e-7 * scale * (b - a).max(1e-3),
            limits,
            0,
            &mut nodes,
        );
    }
    nodes
}

#[derive(Clone, Copy)]
struct Limits {
    /// A node's longest span of arrival times, m.
    max_span: f64,
    /// A piece this short is accepted whether or not its estimate converged, m.
    min_piece: f64,
}

#[derive(Clone, Copy)]
struct Interval {
    a: f64,
    b: f64,
    fa: f64,
    fm: f64,
    fb: f64,
    whole: f64,
}

fn simpson(
    f: &impl Fn(f64) -> f64,
    tau: &impl Fn(f64) -> f64,
    iv: Interval,
    tolerance: f64,
    limits: Limits,
    depth: usize,
    out: &mut Vec<(f64, f64, f64)>,
) {
    let Interval {
        a,
        b,
        fa,
        fm,
        fb,
        whole,
    } = iv;
    let m = 0.5 * (a + b);
    let (lm, rm) = (0.5 * (a + m), 0.5 * (m + b));
    let (flm, frm) = (f(lm), f(rm));
    let left = (m - a) / 6.0 * (fa + 4.0 * flm + fm);
    let right = (b - m) / 6.0 * (fm + 4.0 * frm + fb);
    let span = (tau(a) - tau(b)).abs();
    // A relative floor: next to a zone boundary the integrand peaks so high that rounding alone
    // would keep every sub-piece from converging, and the recursion would grow exponentially.
    let converged =
        (left + right - whole).abs() <= 15.0 * tolerance + 1e-9 * (left.abs() + right.abs());
    let negligible = b - a <= limits.min_piece;
    if depth >= 48 || negligible || (converged && span <= limits.max_span && depth >= 2) {
        out.push((lm, tau(lm), left));
        out.push((rm, tau(rm), right));
        return;
    }
    simpson(
        f,
        tau,
        Interval {
            a,
            b: m,
            fa,
            fm: flm,
            fb: fm,
            whole: left,
        },
        tolerance * 0.5,
        limits,
        depth + 1,
        out,
    );
    simpson(
        f,
        tau,
        Interval {
            a: m,
            b,
            fa: fm,
            fm: frm,
            fb,
            whole: right,
        },
        tolerance * 0.5,
        limits,
        depth + 1,
        out,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::Material;

    /// An edge along +z from z = −L/2 with face A along +x and open angle `open`.
    fn wedge(open: f64, length: f64) -> Edge {
        Edge {
            start: Vec3::new(0.0, 0.0, -length / 2.0),
            length_m: length,
            axis: Vec3::new(0.0, 0.0, 1.0),
            along_a: Vec3::new(1.0, 0.0, 0.0),
            normal_a: Vec3::new(0.0, 1.0, 0.0),
            open_angle: open,
            faces: [0, 1],
        }
    }

    fn at(r: f64, theta: f64, z: f64) -> Vec3 {
        Vec3::new(r * theta.cos(), r * theta.sin(), z)
    }

    #[test]
    fn a_box_has_no_diffracting_edge_and_an_l_room_has_one() {
        let rigid = || Material::rigid();
        let boxed =
            Room::shoebox(Vec3::new(5.0, 4.0, 3.0), std::array::from_fn(|_| rigid())).unwrap();
        assert!(diffracting_edges(&boxed).is_empty());
        let plan = [
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 4.0),
            (4.0, 4.0),
            (4.0, 10.0),
            (0.0, 10.0),
        ];
        let l_room = Room::extruded(&plan, 3.0, [rigid(), rigid(), rigid()]).unwrap();
        let edges = diffracting_edges(&l_room);
        assert_eq!(edges.len(), 1, "{edges:#?}");
        assert!((edges[0].open_angle - 1.5 * PI).abs() < 1e-9);
        assert!((edges[0].length_m - 3.0).abs() < 1e-9);
        assert!((edges[0].start.x - 4.0).abs() < 1e-9 && (edges[0].start.y - 4.0).abs() < 1e-9);
    }

    /// Thesis eq. 4.39: next to the direct sound's shadow boundary, the diffraction onset tends to
    /// half the direct sound with opposite sign on the lit side, and to the same half on the shadow
    /// side, so direct plus diffraction is continuous across the boundary.
    #[test]
    fn total_field_is_continuous_across_the_shadow_boundary() {
        let edge = wedge(1.5 * PI, 400.0);
        let speed = 343.0;
        let fs = 48_000.0;
        let (rs, ts) = (2.0, 0.25 * PI);
        let rr = 3.0;
        let source = at(rs, ts, 0.0);
        let r0 = rs + rr;
        let onset = |theta_r: f64| {
            let receiver = at(rr, theta_r, 0.0);
            edge_response(&edge, source, receiver, speed, fs)
                .into_iter()
                .filter(|&(_, tau, _)| tau - r0 < 2.0 * speed / fs)
                .map(|(_, _, w)| w)
                .sum::<f64>()
        };
        let boundary = PI + ts;
        let delta = 0.02f64.to_radians();
        let lit = onset(boundary - delta);
        let shadow = onset(boundary + delta);
        let direct = 1.0 / r0;
        assert!(
            (lit + 0.5 * direct).abs() < 0.1 * direct,
            "lit onset {lit}, expected {}",
            -0.5 * direct
        );
        assert!(
            (shadow - 0.5 * direct).abs() < 0.1 * direct,
            "shadow onset {shadow}, expected {}",
            0.5 * direct
        );
        assert!(((direct + lit) - shadow).abs() < 0.1 * direct);
    }

    #[test]
    fn response_is_reciprocal() {
        let edge = wedge(1.5 * PI, 3.0);
        let a = at(2.0, 0.4, -0.3);
        let b = at(1.3, 3.9, 0.8);
        let total = |x: Vec3, y: Vec3| {
            edge_response(&edge, x, y, 343.0, 48_000.0)
                .iter()
                .map(|n| n.2)
                .sum::<f64>()
        };
        let (ab, ba) = (total(a, b), total(b, a));
        assert!((ab - ba).abs() < 1e-6 * ab.abs().max(1e-9), "{ab} vs {ba}");
        assert!(ab != 0.0);
    }

    #[test]
    fn a_crest_on_a_floor_has_the_path_reflected_on_both_sides() {
        let rigid = || Material::rigid();
        let knife = crate::geometry::Prism {
            section: vec![(5.0, 0.0), (5.05, 2.0), (5.1, 0.0)],
            y: (0.5, 3.5),
            material: rigid(),
        };
        let room = Room::box_with_prisms(
            Vec3::default(),
            Vec3::new(10.0, 4.0, 4.0),
            std::array::from_fn(|_| rigid()),
            &[knife],
        )
        .unwrap();
        let (source, receiver) = (Vec3::new(2.0, 2.0, 1.0), Vec3::new(8.0, 2.0, 1.5));
        let scene =
            Scene::new("crest", room, crate::air::Air::standard(), source, receiver).unwrap();
        let edges = diffracting_edges(&scene.room);
        let on_floor =
            |f: Option<usize>| f.is_some_and(|f| scene.room.polygons[f].normal.z > 0.999);
        let order = |specular_order| {
            diffracted_paths(
                &scene,
                &DiffractionOptions {
                    specular_order,
                    ..Default::default()
                },
            )
        };
        let paths = order(2);
        let both = paths
            .iter()
            .filter(|p| (edges[p.edge].start.z - 2.0).abs() < 1e-9)
            .find(|p| on_floor(p.before) && on_floor(p.after))
            .expect("a crest path reflected by the floor on both sides");
        // The images below the floor, unfolded through the crest's least-time point at y = 2.
        let expected = 3.05f64.hypot(3.0) + 2.95f64.hypot(3.5);
        assert!(
            (both.earliest_m() - expected).abs() < 5e-3,
            "{} vs {expected}",
            both.earliest_m()
        );
        assert!(
            !order(1)
                .iter()
                .any(|p| p.before.is_some() && p.after.is_some())
        );
    }
    /// A receiver a few micro-radians off a shadow boundary: the term survives the singular-term
    /// cut, its peak is sharp and tall, and rounding in `cosh η` once kept the quadrature from
    /// converging until it filled memory (BRAS RS5, a path mirrored in both end walls passing 1 mm
    /// from the crest's boundary). The onset must still tend to half the direct sound, from a
    /// bounded number of nodes.
    #[test]
    fn quadrature_stays_bounded_next_to_a_shadow_boundary() {
        let edge = wedge(1.5 * PI, 4.0);
        let (rs, ts, rr) = (2.0, 0.25 * PI, 3.0);
        let source = at(rs, ts, 0.0);
        let r0 = rs + rr;
        // The singular-term cut drops a term within 1e-12 of cos(νφ) = 1, below δ ≈ 2.2e-6 here.
        for delta in [5e-6, 3e-5, 2e-4] {
            let receiver = at(rr, PI + ts + delta, 0.0);
            let nodes = edge_response(&edge, source, receiver, 343.0, 48_000.0);
            assert!(
                nodes.len() < 200_000,
                "{} nodes at δ = {delta}",
                nodes.len()
            );
            let onset: f64 = nodes
                .iter()
                .filter(|&&(_, tau, _)| tau - r0 < 2.0 * 343.0 / 48_000.0)
                .map(|n| n.2)
                .sum();
            assert!(
                (onset - 0.5 / r0).abs() < 0.1 / r0,
                "onset {onset} at δ = {delta}, expected {}",
                0.5 / r0
            );
        }
    }
}
