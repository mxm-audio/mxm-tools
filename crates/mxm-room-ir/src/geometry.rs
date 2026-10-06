//! Closed polygonal rooms.
//!
//! **The winding convention is the contract.** Every polygon's vertices run counter-clockwise when
//! seen from inside the room, so its Newell normal points into the room. A [`Room`] is accepted
//! only if it is planar, watertight (each edge shared by exactly two polygons in opposite
//! directions) and encloses a positive volume under that convention. An image-source validity test
//! that asks "is this point on the interior side of the wall" is then one dot product.

use std::collections::HashMap;
use std::ops::{Add, Mul, Neg, Sub};

use crate::error::Error;
use crate::material::Material;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
    pub fn dot(self, o: Self) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cross(self, o: Self) -> Self {
        Self::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }
    pub fn normalized(self) -> Self {
        self * (1.0 / self.length())
    }
    fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

impl Add for Vec3 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f64> for Vec3 {
    type Output = Self;
    fn mul(self, s: f64) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s)
    }
}
impl Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

/// A planar polygon with an inward unit normal.
#[derive(Debug, Clone, PartialEq)]
pub struct Polygon {
    pub vertices: Vec<Vec3>,
    /// Unit normal pointing into the room.
    pub normal: Vec3,
    /// Index into [`Room::materials`].
    pub material: usize,
    /// Plane offset: `normal · x = offset` on the plane.
    pub offset: f64,
    /// Dropped axis for 2-D point-in-polygon tests (0 = x, 1 = y, 2 = z).
    drop_axis: usize,
}

impl Polygon {
    /// Signed distance of `p` from the plane, positive on the interior side.
    pub fn signed_distance(&self, p: Vec3) -> f64 {
        self.normal.dot(p) - self.offset
    }

    /// Reflection of `p` in the polygon's plane.
    pub fn mirror(&self, p: Vec3) -> Vec3 {
        p - self.normal * (2.0 * self.signed_distance(p))
    }

    fn project(&self, p: Vec3) -> (f64, f64) {
        match self.drop_axis {
            0 => (p.y, p.z),
            1 => (p.z, p.x),
            _ => (p.x, p.y),
        }
    }

    /// Crossing-number point-in-polygon test for a point on the plane. Works for non-convex
    /// polygons; points exactly on an edge are ambiguous, which is why scenes avoid them.
    pub fn contains_coplanar(&self, p: Vec3) -> bool {
        let (px, py) = self.project(p);
        let mut inside = false;
        let n = self.vertices.len();
        let mut j = n - 1;
        for i in 0..n {
            let (xi, yi) = self.project(self.vertices[i]);
            let (xj, yj) = self.project(self.vertices[j]);
            if (yi > py) != (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi {
                inside = !inside;
            }
            j = i;
        }
        inside
    }

    /// The parameter `t` and point where segment `a → b` crosses the polygon, strictly between the
    /// endpoints (by `eps` of the segment's parameter).
    pub fn segment_intersection(&self, a: Vec3, b: Vec3, eps: f64) -> Option<(f64, Vec3)> {
        let d = b - a;
        let denom = self.normal.dot(d);
        if denom.abs() < 1e-12 {
            return None;
        }
        let t = (self.offset - self.normal.dot(a)) / denom;
        if t <= eps || t >= 1.0 - eps {
            return None;
        }
        let q = a + d * t;
        self.contains_coplanar(q).then_some((t, q))
    }

    /// Area, m².
    pub fn area(&self) -> f64 {
        newell(&self.vertices).length() * 0.5
    }
}

fn newell(vertices: &[Vec3]) -> Vec3 {
    let mut n = Vec3::default();
    for i in 0..vertices.len() {
        let a = vertices[i];
        let b = vertices[(i + 1) % vertices.len()];
        n.x += (a.y - b.y) * (a.z + b.z);
        n.y += (a.z - b.z) * (a.x + b.x);
        n.z += (a.x - b.x) * (a.y + b.y);
    }
    n
}

/// A closed polygonal room and its materials.
#[derive(Debug, Clone, PartialEq)]
pub struct Room {
    pub polygons: Vec<Polygon>,
    pub materials: Vec<Material>,
}

impl Room {
    /// Validates and builds a room from `(vertices, material index)` faces.
    pub fn new(faces: Vec<(Vec<Vec3>, usize)>, materials: Vec<Material>) -> Result<Self, Error> {
        if faces.len() < 4 {
            return Err(Error::InvalidGeometry(
                "a closed room needs at least four faces".into(),
            ));
        }
        let scale = faces
            .iter()
            .flat_map(|(v, _)| v.iter())
            .map(|p| p.x.abs().max(p.y.abs()).max(p.z.abs()))
            .fold(1.0, f64::max);
        let tol = 1e-9 * scale;
        let mut polygons = Vec::with_capacity(faces.len());
        for (index, (vertices, material)) in faces.into_iter().enumerate() {
            if vertices.len() < 3 {
                return Err(Error::InvalidGeometry(format!(
                    "face {index} has fewer than 3 vertices"
                )));
            }
            if vertices.iter().any(|v| !v.is_finite()) {
                return Err(Error::InvalidGeometry(format!(
                    "face {index} has a non-finite vertex"
                )));
            }
            if material >= materials.len() {
                return Err(Error::InvalidGeometry(format!(
                    "face {index} names material {material}"
                )));
            }
            let raw = newell(&vertices);
            if raw.length() <= tol * scale {
                return Err(Error::InvalidGeometry(format!("face {index} has no area")));
            }
            let normal = raw.normalized();
            let offset = normal.dot(vertices[0]);
            if let Some(v) = vertices
                .iter()
                .find(|v| (normal.dot(**v) - offset).abs() > tol * 10.0)
            {
                return Err(Error::InvalidGeometry(format!(
                    "face {index} is not planar at {v:?}"
                )));
            }
            let drop_axis = if normal.x.abs() >= normal.y.abs() && normal.x.abs() >= normal.z.abs()
            {
                0
            } else if normal.y.abs() >= normal.z.abs() {
                1
            } else {
                2
            };
            polygons.push(Polygon {
                vertices,
                normal,
                material,
                offset,
                drop_axis,
            });
        }
        let room = Self {
            polygons,
            materials,
        };
        room.check_watertight(tol)?;
        if room.volume() <= 0.0 {
            return Err(Error::InvalidGeometry(
                "faces enclose no positive volume: vertices must run counter-clockwise seen from inside"
                    .into(),
            ));
        }
        Ok(room)
    }

    fn check_watertight(&self, tol: f64) -> Result<(), Error> {
        let key = |p: Vec3| {
            let q = |v: f64| (v / (tol * 100.0)).round() as i64;
            (q(p.x), q(p.y), q(p.z))
        };
        type Point = (i64, i64, i64);
        let mut edges: HashMap<(Point, Point), i32> = HashMap::new();
        for poly in &self.polygons {
            let n = poly.vertices.len();
            for i in 0..n {
                let a = key(poly.vertices[i]);
                let b = key(poly.vertices[(i + 1) % n]);
                *edges.entry((a, b)).or_insert(0) += 1;
            }
        }
        for (&(a, b), &count) in &edges {
            let reverse = edges.get(&(b, a)).copied().unwrap_or(0);
            if count != 1 || reverse != 1 {
                return Err(Error::InvalidGeometry(format!(
                    "edge {a:?}→{b:?} is used {count} time(s) and reversed {reverse} time(s); \
                     a closed room uses each edge once in each direction"
                )));
            }
        }
        Ok(())
    }

    /// Enclosed volume, m³, by the divergence theorem under the inward-winding convention.
    pub fn volume(&self) -> f64 {
        let mut six_v = 0.0;
        for poly in &self.polygons {
            let p0 = poly.vertices[0];
            for i in 1..poly.vertices.len() - 1 {
                six_v += p0.dot(poly.vertices[i].cross(poly.vertices[i + 1]));
            }
        }
        -six_v / 6.0
    }

    /// Total surface area, m².
    pub fn surface_area(&self) -> f64 {
        self.polygons.iter().map(Polygon::area).sum()
    }

    /// The nearest polygon a ray from `origin` along unit `direction` meets, skipping `skip`
    /// (the face the ray leaves). Returns the distance, the face and the point.
    ///
    /// Only faces the ray approaches from their interior side count, so a ray inside the room
    /// never stops on the back of a face.
    pub fn first_hit(
        &self,
        origin: Vec3,
        direction: Vec3,
        skip: Option<usize>,
    ) -> Option<(f64, usize, Vec3)> {
        let mut best: Option<(f64, usize, Vec3)> = None;
        for (f, poly) in self.polygons.iter().enumerate() {
            if Some(f) == skip {
                continue;
            }
            let denom = poly.normal.dot(direction);
            if denom >= -1e-12 {
                continue;
            }
            let t = (poly.offset - poly.normal.dot(origin)) / denom;
            if t <= 1e-9 || best.is_some_and(|(b, _, _)| t >= b) {
                continue;
            }
            let q = origin + direction * t;
            if poly.contains_coplanar(q) {
                best = Some((t, f, q));
            }
        }
        best
    }

    /// Whether the open segment `a → b` crosses any polygon other than `skip_a` and `skip_b`.
    pub fn segment_blocked(
        &self,
        a: Vec3,
        b: Vec3,
        skip_a: Option<usize>,
        skip_b: Option<usize>,
    ) -> bool {
        self.polygons.iter().enumerate().any(|(g, poly)| {
            Some(g) != skip_a
                && Some(g) != skip_b
                && poly.segment_intersection(a, b, 1e-9).is_some()
        })
    }

    /// Shortest distance from `p` to any polygon's plane where the foot of the perpendicular lies
    /// inside that polygon, or to any vertex otherwise. A lower bound on clearance is all callers
    /// need.
    pub fn clearance(&self, p: Vec3) -> f64 {
        let mut best = f64::INFINITY;
        for poly in &self.polygons {
            let d = poly.signed_distance(p);
            let foot = p - poly.normal * d;
            if poly.contains_coplanar(foot) {
                best = best.min(d.abs());
            }
            for v in &poly.vertices {
                best = best.min((*v - p).length());
            }
            let n = poly.vertices.len();
            for i in 0..n {
                let (a, b) = (poly.vertices[i], poly.vertices[(i + 1) % n]);
                let ab = b - a;
                let t = ((p - a).dot(ab) / ab.dot(ab)).clamp(0.0, 1.0);
                best = best.min((a + ab * t - p).length());
            }
        }
        best
    }

    /// Whether `p` is strictly inside, by counting crossings along a fixed skew ray.
    pub fn contains(&self, p: Vec3) -> bool {
        if self
            .polygons
            .iter()
            .any(|poly| poly.signed_distance(p).abs() < 1e-9 && poly.contains_coplanar(p))
        {
            return false;
        }
        let far = p + Vec3::new(0.573_218_4, 0.715_906_3, 0.398_775_1) * 1.0e6;
        let crossings = self
            .polygons
            .iter()
            .filter(|poly| poly.segment_intersection(p, far, 0.0).is_some())
            .count();
        crossings % 2 == 1
    }

    /// A box `[0, Lx] × [0, Ly] × [0, Lz]`. Materials in face order: x = 0, x = Lx, y = 0, y = Ly,
    /// floor (z = 0), ceiling (z = Lz).
    pub fn shoebox(size: Vec3, materials: [Material; 6]) -> Result<Self, Error> {
        let (lx, ly, lz) = (size.x, size.y, size.z);
        let v = Vec3::new;
        let faces = vec![
            (
                vec![
                    v(0.0, 0.0, 0.0),
                    v(0.0, 0.0, lz),
                    v(0.0, ly, lz),
                    v(0.0, ly, 0.0),
                ],
                0,
            ),
            (
                vec![
                    v(lx, 0.0, 0.0),
                    v(lx, ly, 0.0),
                    v(lx, ly, lz),
                    v(lx, 0.0, lz),
                ],
                1,
            ),
            (
                vec![
                    v(0.0, 0.0, 0.0),
                    v(lx, 0.0, 0.0),
                    v(lx, 0.0, lz),
                    v(0.0, 0.0, lz),
                ],
                2,
            ),
            (
                vec![
                    v(0.0, ly, 0.0),
                    v(0.0, ly, lz),
                    v(lx, ly, lz),
                    v(lx, ly, 0.0),
                ],
                3,
            ),
            (
                vec![
                    v(0.0, 0.0, 0.0),
                    v(0.0, ly, 0.0),
                    v(lx, ly, 0.0),
                    v(lx, 0.0, 0.0),
                ],
                4,
            ),
            (
                vec![
                    v(0.0, 0.0, lz),
                    v(lx, 0.0, lz),
                    v(lx, ly, lz),
                    v(0.0, ly, lz),
                ],
                5,
            ),
        ];
        // Written counter-clockwise seen from outside, which reads more naturally; reversed here
        // into the inward convention the room validates.
        let faces = faces
            .into_iter()
            .map(|(mut vertices, m)| {
                vertices.reverse();
                (vertices, m)
            })
            .collect();
        Self::new(faces, materials.into())
    }

    /// A floor plan extruded to `height`. `plan` is a simple polygon in the xy-plane, counter-
    /// clockwise seen from above. Faces: floor (0), ceiling (1), then one wall per plan edge in plan
    /// order. Materials: `[floor, ceiling, walls]`.
    pub fn extruded(
        plan: &[(f64, f64)],
        height: f64,
        materials: [Material; 3],
    ) -> Result<Self, Error> {
        if plan.len() < 3 || height.is_nan() || height <= 0.0 {
            return Err(Error::InvalidGeometry(
                "an extruded room needs a plan and a positive height".into(),
            ));
        }
        let floor: Vec<Vec3> = plan.iter().map(|&(x, y)| Vec3::new(x, y, 0.0)).collect();
        let ceiling: Vec<Vec3> = plan
            .iter()
            .rev()
            .map(|&(x, y)| Vec3::new(x, y, height))
            .collect();
        let mut faces = vec![(floor, 0), (ceiling, 1)];
        for i in 0..plan.len() {
            let (ax, ay) = plan[i];
            let (bx, by) = plan[(i + 1) % plan.len()];
            faces.push((
                vec![
                    Vec3::new(ax, ay, 0.0),
                    Vec3::new(ax, ay, height),
                    Vec3::new(bx, by, height),
                    Vec3::new(bx, by, 0.0),
                ],
                2,
            ));
        }
        Self::new(faces, materials.into())
    }
}

/// A floor-plan region: a simple polygon in the xy-plane and its `[floor, ceiling, walls]`.
pub type PlanRegion = (Vec<(f64, f64)>, [Material; 3]);

/// A floor-plan region with its own ceiling height, for [`Room::stepped_regions`].
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    /// A simple polygon in the xy-plane, counter-clockwise seen from above.
    pub plan: Vec<(f64, f64)>,
    /// `[floor, ceiling, walls]`.
    pub materials: [Material; 3],
    pub height: f64,
}

impl Room {
    /// Several floor-plan regions extruded to one `height`. Each region is a simple polygon,
    /// counter-clockwise seen from above, with its own `[floor, ceiling, walls]` materials. An edge
    /// two regions share in opposite directions becomes an opening, not a wall; so regions must
    /// meet vertex to vertex, with no T-junction. Materials are stored three per region in order.
    pub fn extruded_regions(regions: &[PlanRegion], height: f64) -> Result<Self, Error> {
        let regions: Vec<Region> = regions
            .iter()
            .map(|(plan, materials)| Region {
                plan: plan.clone(),
                materials: materials.clone(),
                height,
            })
            .collect();
        Self::stepped_regions(&regions)
    }

    /// Floor-plan regions on one floor, each extruded to its own height: a nave and its lower
    /// aisles, an auditorium and its taller stage house. Regions meet vertex to vertex as in
    /// [`Room::extruded_regions`]. A shared edge is open up to the lower region's ceiling; above it
    /// the taller region keeps its wall, in its wall material. Every wall carries a vertex at each
    /// ceiling height meeting its vertical edges, so no edge ends in a T-junction. Faces per region:
    /// floor, ceiling, then walls in plan order; materials three per region in order.
    pub fn stepped_regions(regions: &[Region]) -> Result<Self, Error> {
        if regions.is_empty() {
            return Err(Error::InvalidGeometry("a room needs a region".into()));
        }
        let key = |p: (f64, f64)| ((p.0 * 1e9).round() as i64, (p.1 * 1e9).round() as i64);
        let mut edges = HashMap::new();
        let mut heights_at: HashMap<(i64, i64), Vec<f64>> = HashMap::new();
        for (r, region) in regions.iter().enumerate() {
            if region.plan.len() < 3 || !(region.height.is_finite() && region.height > 0.0) {
                return Err(Error::InvalidGeometry(format!(
                    "region {r} needs at least 3 points and a positive height"
                )));
            }
            let plan = &region.plan;
            for i in 0..plan.len() {
                edges.insert(
                    (key(plan[i]), key(plan[(i + 1) % plan.len()])),
                    region.height,
                );
                heights_at
                    .entry(key(plan[i]))
                    .or_default()
                    .push(region.height);
            }
        }
        // A wall over the plan edge a → b from `lo` to `hi`, inward-facing, with a vertex at every
        // other ceiling height on each vertical edge.
        let wall = |a: (f64, f64), b: (f64, f64), lo: f64, hi: f64| {
            let cuts = |p: (f64, f64)| {
                let mut c: Vec<f64> = heights_at[&key(p)]
                    .iter()
                    .copied()
                    .filter(|&h| h > lo && h < hi)
                    .collect();
                c.sort_by(f64::total_cmp);
                c.dedup();
                c
            };
            let mut v = vec![Vec3::new(a.0, a.1, lo)];
            v.extend(cuts(a).into_iter().map(|h| Vec3::new(a.0, a.1, h)));
            v.push(Vec3::new(a.0, a.1, hi));
            v.push(Vec3::new(b.0, b.1, hi));
            v.extend(cuts(b).into_iter().rev().map(|h| Vec3::new(b.0, b.1, h)));
            v.push(Vec3::new(b.0, b.1, lo));
            v
        };
        let mut faces = Vec::new();
        let mut materials = Vec::new();
        for (r, region) in regions.iter().enumerate() {
            let (plan, height) = (&region.plan, region.height);
            let base = 3 * r;
            materials.extend(region.materials.iter().cloned());
            faces.push((
                plan.iter().map(|&(x, y)| Vec3::new(x, y, 0.0)).collect(),
                base,
            ));
            faces.push((
                plan.iter()
                    .rev()
                    .map(|&(x, y)| Vec3::new(x, y, height))
                    .collect(),
                base + 1,
            ));
            for i in 0..plan.len() {
                let (a, b) = (plan[i], plan[(i + 1) % plan.len()]);
                let bottom = match edges.get(&(key(b), key(a))) {
                    Some(&other) if other >= height => continue,
                    Some(&other) => other,
                    None => 0.0,
                };
                faces.push((wall(a, b, bottom, height), base + 2));
            }
        }
        Self::new(faces, materials)
    }
}

/// A solid inside a box room: a cross-section in the xz-plane extruded along y. One standing on
/// the floor has its footprint cut out of the floor; one clear of it floats as a closed shell.
#[derive(Debug, Clone, PartialEq)]
pub struct Prism {
    /// The cross-section as `(x, z)` points, in either winding. A standing prism has exactly one
    /// section edge on the floor and no other point there.
    pub section: Vec<(f64, f64)>,
    /// The extent along y, low to high.
    pub y: (f64, f64),
    pub material: Material,
}

impl Room {
    /// A box from `min` to `max` holding `prisms`: plates, partitions and blocks in a free field,
    /// the shape of a measurement-chamber benchmark. Box materials are in [`Room::shoebox`]'s
    /// order, then one per prism.
    ///
    /// Prisms lie strictly inside the walls and below the ceiling, and their bounding boxes may not
    /// touch. So that faces meet vertex to vertex, the floor is split into a grid of rectangles at
    /// every standing footprint's edges, and the walls' feet, the prisms' feet and the footprints'
    /// ends carry the grid's vertices.
    pub fn box_with_prisms(
        min: Vec3,
        max: Vec3,
        materials: [Material; 6],
        prisms: &[Prism],
    ) -> Result<Self, Error> {
        if !(min.is_finite() && max.is_finite() && min.x < max.x && min.y < max.y && min.z < max.z)
        {
            return Err(Error::InvalidGeometry(
                "a box needs min below max on every axis".into(),
            ));
        }
        let scale = [min.x, min.y, min.z, max.x, max.y, max.z]
            .iter()
            .fold(1.0_f64, |m, v| m.max(v.abs()));
        let tol = 1e-9 * scale;
        struct Shaped {
            section: Vec<(f64, f64)>,
            y: (f64, f64),
            standing: bool,
        }
        let mut shaped = Vec::with_capacity(prisms.len());
        for (index, prism) in prisms.iter().enumerate() {
            let bad = |why: &str| Error::InvalidGeometry(format!("prism {index} {why}"));
            let mut section = prism.section.clone();
            let n = section.len();
            if n < 3 || section.iter().any(|p| !p.0.is_finite() || !p.1.is_finite()) {
                return Err(bad("needs three or more finite section points"));
            }
            let (y0, y1) = prism.y;
            if !(y0 > min.y + tol && y0 < y1 && y1 < max.y - tol) {
                return Err(bad("must span a y range strictly inside the box"));
            }
            if section.iter().any(|&(x, z)| {
                x <= min.x + tol || x >= max.x - tol || z < min.z - tol || z >= max.z - tol
            }) {
                return Err(bad("must lie inside the walls and below the ceiling"));
            }
            let twice_area: f64 = (0..n)
                .map(|i| {
                    let (a, b) = (section[i], section[(i + 1) % n]);
                    a.0 * b.1 - b.0 * a.1
                })
                .sum();
            if twice_area.abs() <= tol {
                return Err(bad("has a section with no area"));
            }
            if twice_area < 0.0 {
                section.reverse();
            }
            let on_floor: Vec<usize> = (0..n)
                .filter(|&i| (section[i].1 - min.z).abs() <= tol)
                .collect();
            let standing = match *on_floor.as_slice() {
                [] => false,
                [a, b] if b == a + 1 || (a == 0 && b == n - 1) => {
                    // Counter-clockwise with z up, the floor edge runs toward +x: start on it.
                    section.rotate_left(if b == a + 1 { a } else { b });
                    section[0].1 = min.z;
                    section[1].1 = min.z;
                    true
                }
                _ => {
                    return Err(bad(
                        "must meet the floor along one section edge or not at all",
                    ));
                }
            };
            shaped.push(Shaped {
                section,
                y: (y0, y1),
                standing,
            });
        }
        let bounds = |s: &Shaped| {
            s.section.iter().fold(
                (
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ),
                |(x0, z0, x1, z1), &(x, z)| (x0.min(x), z0.min(z), x1.max(x), z1.max(z)),
            )
        };
        for i in 0..shaped.len() {
            for j in i + 1..shaped.len() {
                let (a, b) = (bounds(&shaped[i]), bounds(&shaped[j]));
                let apart = a.0 > b.2 + tol
                    || b.0 > a.2 + tol
                    || a.1 > b.3 + tol
                    || b.1 > a.3 + tol
                    || shaped[i].y.0 > shaped[j].y.1 + tol
                    || shaped[j].y.0 > shaped[i].y.1 + tol;
                if !apart {
                    return Err(Error::InvalidGeometry(format!(
                        "prisms {i} and {j} touch or overlap"
                    )));
                }
            }
        }
        let mut xs = vec![min.x, max.x];
        let mut ys = vec![min.y, max.y];
        for s in shaped.iter().filter(|s| s.standing) {
            xs.extend([s.section[0].0, s.section[1].0]);
            ys.extend([s.y.0, s.y.1]);
        }
        let grid = |mut cuts: Vec<f64>| {
            cuts.sort_by(f64::total_cmp);
            cuts.dedup_by(|a, b| (*a - *b).abs() <= tol);
            cuts
        };
        let (xs, ys) = (grid(xs), grid(ys));
        let inside = |cuts: &[f64], lo: f64, hi: f64| -> Vec<f64> {
            cuts.iter()
                .copied()
                .filter(|&c| c > lo + tol && c < hi - tol)
                .collect()
        };
        let v = Vec3::new;
        let mut faces: Vec<(Vec<Vec3>, usize)> = Vec::new();
        // Floor cells, counter-clockwise from above, less the standing footprints.
        for wx in xs.windows(2) {
            for wy in ys.windows(2) {
                let covered = shaped.iter().any(|s| {
                    s.standing
                        && s.section[0].0 <= wx[0] + tol
                        && wx[1] <= s.section[1].0 + tol
                        && s.y.0 <= wy[0] + tol
                        && wy[1] <= s.y.1 + tol
                });
                if !covered {
                    faces.push((
                        vec![
                            v(wx[0], wy[0], min.z),
                            v(wx[1], wy[0], min.z),
                            v(wx[1], wy[1], min.z),
                            v(wx[0], wy[1], min.z),
                        ],
                        4,
                    ));
                }
            }
        }
        faces.push((
            vec![
                v(min.x, min.y, max.z),
                v(min.x, max.y, max.z),
                v(max.x, max.y, max.z),
                v(max.x, min.y, max.z),
            ],
            5,
        ));
        // Each wall's foot runs opposite the floor cells' edges beside it, through the grid.
        let (lo, hi) = (min.z, max.z);
        let mut wall = |foot: Vec<Vec3>, top: [Vec3; 2], material: usize| {
            let mut vertices = foot;
            vertices.extend(top);
            faces.push((vertices, material));
        };
        wall(
            ys.iter().map(|&y| v(min.x, y, lo)).collect(),
            [v(min.x, max.y, hi), v(min.x, min.y, hi)],
            0,
        );
        wall(
            ys.iter().rev().map(|&y| v(max.x, y, lo)).collect(),
            [v(max.x, min.y, hi), v(max.x, max.y, hi)],
            1,
        );
        wall(
            xs.iter().rev().map(|&x| v(x, min.y, lo)).collect(),
            [v(min.x, min.y, hi), v(max.x, min.y, hi)],
            2,
        );
        wall(
            xs.iter().map(|&x| v(x, max.y, lo)).collect(),
            [v(max.x, max.y, hi), v(min.x, max.y, hi)],
            3,
        );
        for (index, s) in shaped.iter().enumerate() {
            let material = 6 + index;
            let n = s.section.len();
            let (y0, y1) = s.y;
            let at = |p: (f64, f64), y: f64| v(p.0, y, p.1);
            // Side faces [a0, a1, b1, b0] face away from a counter-clockwise section. A standing
            // prism has no face on its floor edge, and its two feet pass through the grid's y cuts.
            for i in usize::from(s.standing)..n {
                let (a, b) = (s.section[i], s.section[(i + 1) % n]);
                let mut quad = vec![at(a, y0)];
                if s.standing && i == 1 {
                    quad.extend(inside(&ys, y0, y1).into_iter().map(|y| at(a, y)));
                }
                quad.push(at(a, y1));
                quad.push(at(b, y1));
                if s.standing && i == n - 1 {
                    quad.extend(inside(&ys, y0, y1).into_iter().rev().map(|y| at(b, y)));
                }
                quad.push(at(b, y0));
                faces.push((quad, material));
            }
            // End faces: the section itself faces −y at y0; reversed, +y at y1.
            let mut near = vec![at(s.section[0], y0)];
            if s.standing {
                near.extend(
                    inside(&xs, s.section[0].0, s.section[1].0)
                        .into_iter()
                        .map(|x| v(x, y0, min.z)),
                );
            }
            near.extend(s.section[1..].iter().map(|&p| at(p, y0)));
            let far: Vec<Vec3> = near.iter().rev().map(|p| v(p.x, y1, p.z)).collect();
            faces.push((near, material));
            faces.push((far, material));
        }
        let mut all = Vec::from(materials);
        all.extend(prisms.iter().map(|p| p.material.clone()));
        Self::new(faces, all)
    }

    /// `self` holding `prisms` as solids that float clear of its faces: furniture, pews, boulders,
    /// racking. A prism's section is `(x, z)` swept along y, as in [`Room::box_with_prisms`], and
    /// its faces are added to the room's own, one material per solid.
    ///
    /// **A solid may not stand on the floor or touch any face.** A footprint on the floor has to cut
    /// that face into a grid at its edges, which only [`Room::box_with_prisms`] does, and only
    /// because a box's floor is a rectangle. A solid that floats needs none of it, so any room can
    /// hold one: what a chair or a boulder does to a room is scatter and absorb, not carve its
    /// floor. Solids may not touch each other either, as in a box.
    pub fn with_solids(self, prisms: &[Prism]) -> Result<Self, Error> {
        if prisms.is_empty() {
            return Ok(self);
        }
        let scale = self
            .polygons
            .iter()
            .flat_map(|p| p.vertices.iter())
            .fold(1.0_f64, |m, v| {
                m.max(v.x.abs()).max(v.y.abs()).max(v.z.abs())
            });
        let tol = 1e-9 * scale;
        struct Floating {
            section: Vec<(f64, f64)>,
            y: (f64, f64),
        }
        let mut shaped: Vec<Floating> = Vec::with_capacity(prisms.len());
        for (index, prism) in prisms.iter().enumerate() {
            let bad = |why: &str| Error::InvalidGeometry(format!("solid {index} {why}"));
            let mut section = prism.section.clone();
            let n = section.len();
            if n < 3 || section.iter().any(|p| !p.0.is_finite() || !p.1.is_finite()) {
                return Err(bad("needs three or more finite section points"));
            }
            let (y0, y1) = prism.y;
            if !(y0.is_finite() && y1.is_finite() && y0 < y1) {
                return Err(bad("must span a finite y range, low to high"));
            }
            let twice_area: f64 = (0..n)
                .map(|i| {
                    let (a, b) = (section[i], section[(i + 1) % n]);
                    a.0 * b.1 - b.0 * a.1
                })
                .sum();
            if twice_area.abs() <= tol {
                return Err(bad("has a section with no area"));
            }
            if twice_area < 0.0 {
                section.reverse();
            }
            for &(x, z) in &section {
                for y in [y0, y1] {
                    let corner = Vec3::new(x, y, z);
                    if !self.contains(corner) || self.clearance(corner) <= tol {
                        return Err(bad("must float clear inside the room, touching no face"));
                    }
                }
            }
            shaped.push(Floating {
                section,
                y: (y0, y1),
            });
        }
        let bounds = |s: &Floating| {
            let (mut x0, mut z0, mut x1, mut z1) = (
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            );
            for &(x, z) in &s.section {
                x0 = x0.min(x);
                z0 = z0.min(z);
                x1 = x1.max(x);
                z1 = z1.max(z);
            }
            (x0, z0, x1, z1, s.y.0, s.y.1)
        };
        for i in 0..shaped.len() {
            for j in i + 1..shaped.len() {
                let (a, b) = (bounds(&shaped[i]), bounds(&shaped[j]));
                let apart = a.0 > b.2 + tol
                    || b.0 > a.2 + tol
                    || a.1 > b.3 + tol
                    || b.1 > a.3 + tol
                    || a.4 > b.5 + tol
                    || b.4 > a.5 + tol;
                if !apart {
                    return Err(Error::InvalidGeometry(format!(
                        "solids {i} and {j} touch or overlap"
                    )));
                }
            }
        }
        let base = self.materials.len();
        let mut materials = self.materials;
        materials.extend(prisms.iter().map(|p| p.material.clone()));
        let mut faces: Vec<(Vec<Vec3>, usize)> = self
            .polygons
            .into_iter()
            .map(|p| (p.vertices, p.material))
            .collect();
        let at = |p: (f64, f64), y: f64| Vec3::new(p.0, y, p.1);
        for (index, solid) in shaped.iter().enumerate() {
            let (section, (y0, y1)) = (&solid.section, (&solid.y.0, &solid.y.1));
            let material = base + index;
            let n = section.len();
            // Side faces [a0, a1, b1, b0] face away from a counter-clockwise section, so into the
            // room, as every face of a room must.
            for i in 0..n {
                let (a, b) = (section[i], section[(i + 1) % n]);
                faces.push((
                    vec![at(a, *y0), at(a, *y1), at(b, *y1), at(b, *y0)],
                    material,
                ));
            }
            let near: Vec<Vec3> = section.iter().map(|&p| at(p, *y0)).collect();
            let far: Vec<Vec3> = near
                .iter()
                .rev()
                .map(|p| Vec3::new(p.x, *y1, p.z))
                .collect();
            faces.push((near, material));
            faces.push((far, material));
        }
        Self::new(faces, materials)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rigid6() -> [Material; 6] {
        std::array::from_fn(|_| Material::rigid())
    }

    #[test]
    fn shoebox_is_valid_with_inward_normals() {
        let room = Room::shoebox(Vec3::new(7.0, 5.0, 3.0), rigid6()).unwrap();
        assert!((room.volume() - 105.0).abs() < 1e-9);
        assert!((room.surface_area() - 2.0 * (35.0 + 21.0 + 15.0)).abs() < 1e-9);
        let centre = Vec3::new(3.5, 2.5, 1.5);
        for poly in &room.polygons {
            assert!(poly.signed_distance(centre) > 0.0);
        }
        assert!(room.contains(centre));
        assert!(!room.contains(Vec3::new(8.0, 2.5, 1.5)));
    }

    #[test]
    fn l_shaped_extrusion_is_valid() {
        let plan = [
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 4.0),
            (4.0, 4.0),
            (4.0, 10.0),
            (0.0, 10.0),
        ];
        let m = || Material::rigid();
        let room = Room::extruded(&plan, 3.0, [m(), m(), m()]).unwrap();
        assert!((room.volume() - (100.0 - 36.0) * 3.0).abs() < 1e-9);
        assert!(room.contains(Vec3::new(2.0, 8.0, 1.0)));
        assert!(!room.contains(Vec3::new(7.0, 7.0, 1.0)));
    }

    #[test]
    fn regions_share_openings() {
        let m = || Material::rigid();
        let left = vec![
            (0.0, 0.0),
            (4.0, 0.0),
            (4.0, 1.0),
            (4.0, 2.0),
            (4.0, 3.0),
            (0.0, 3.0),
        ];
        let neck = vec![(4.0, 1.0), (5.0, 1.0), (5.0, 2.0), (4.0, 2.0)];
        let room = Room::extruded_regions(&[(left, [m(), m(), m()]), (neck, [m(), m(), m()])], 2.0)
            .unwrap();
        assert!((room.volume() - (12.0 + 1.0) * 2.0).abs() < 1e-9);
        assert_eq!(room.materials.len(), 6);
    }

    #[test]
    fn solids_float_inside_any_room_and_take_their_volume() {
        let m = || Material::rigid();
        let plan = [(0.0, 0.0), (8.0, 0.0), (8.0, 6.0), (0.0, 6.0)];
        let room = Room::extruded(&plan, 4.0, [m(), m(), m()]).unwrap();
        let block = |x: (f64, f64), z: (f64, f64), y: (f64, f64)| Prism {
            section: vec![(x.0, z.0), (x.1, z.0), (x.1, z.1), (x.0, z.1)],
            y,
            material: Material::rigid(),
        };
        let solids = [
            block((1.0, 3.0), (0.4, 1.2), (1.0, 2.0)),
            block((5.0, 6.0), (0.4, 1.0), (3.0, 5.0)),
        ];
        let held = room.clone().with_solids(&solids).unwrap();
        let taken = 2.0 * 0.8 * 1.0 + 1.0 * 0.6 * 2.0;
        assert!((held.volume() - (8.0 * 6.0 * 4.0 - taken)).abs() < 1e-9);
        assert_eq!(held.materials.len(), room.materials.len() + 2);
        assert!(held.contains(Vec3::new(7.0, 5.0, 3.0)));
        // A solid standing on the floor, one overlapping another, and one outside are all refused.
        assert!(
            room.clone()
                .with_solids(&[block((1.0, 2.0), (0.0, 1.0), (1.0, 2.0))])
                .is_err()
        );
        assert!(
            room.clone()
                .with_solids(&[solids[0].clone(), solids[0].clone()])
                .is_err()
        );
        assert!(
            room.with_solids(&[block((9.0, 10.0), (0.4, 1.0), (1.0, 2.0))])
                .is_err()
        );
    }

    #[test]
    fn rays_hit_the_nearest_face_from_inside() {
        let room = Room::shoebox(Vec3::new(4.0, 3.0, 2.0), rigid6()).unwrap();
        let (t, face, q) = room
            .first_hit(Vec3::new(1.0, 1.0, 1.0), Vec3::new(1.0, 0.0, 0.0), None)
            .unwrap();
        assert!((t - 3.0).abs() < 1e-12 && face == 1 && (q.x - 4.0).abs() < 1e-12);
        assert!((room.clearance(Vec3::new(1.0, 1.0, 0.5)) - 0.5).abs() < 1e-12);
        assert!(!room.segment_blocked(
            Vec3::new(1.0, 1.0, 1.0),
            Vec3::new(3.0, 2.0, 1.5),
            None,
            None
        ));
    }

    #[test]
    fn rejects_open_and_inverted_rooms() {
        let mut room_faces = Room::shoebox(Vec3::new(2.0, 2.0, 2.0), rigid6())
            .unwrap()
            .polygons;
        let faces: Vec<_> = room_faces.drain(1..).map(|p| (p.vertices, 0)).collect();
        assert!(Room::new(faces, vec![Material::rigid()]).is_err());
        let inverted: Vec<_> = Room::shoebox(Vec3::new(2.0, 2.0, 2.0), rigid6())
            .unwrap()
            .polygons
            .into_iter()
            .map(|p| (p.vertices.into_iter().rev().collect(), 0))
            .collect();
        assert!(Room::new(inverted, vec![Material::rigid()]).is_err());
    }

    #[test]
    fn stepped_regions_open_below_the_lower_ceiling_and_stay_closed() {
        // A nave 10 × 6 × 12 between two aisles 10 × 3 × 5, and a low apse 4 × 4 × 3 off the nave's
        // end: every shared edge is open to the lower ceiling and walled above it.
        let m = || [Material::rigid(), Material::rigid(), Material::rigid()];
        let nave = Region {
            plan: vec![
                (0.0, 3.0),
                (10.0, 3.0),
                (10.0, 4.0),
                (10.0, 8.0),
                (10.0, 9.0),
                (0.0, 9.0),
            ],
            materials: m(),
            height: 12.0,
        };
        let aisle_a = Region {
            plan: vec![(0.0, 0.0), (10.0, 0.0), (10.0, 3.0), (0.0, 3.0)],
            materials: m(),
            height: 5.0,
        };
        let aisle_b = Region {
            plan: vec![(0.0, 9.0), (10.0, 9.0), (10.0, 12.0), (0.0, 12.0)],
            materials: m(),
            height: 5.0,
        };
        let apse = Region {
            plan: vec![(10.0, 4.0), (14.0, 4.0), (14.0, 8.0), (10.0, 8.0)],
            materials: m(),
            height: 3.0,
        };
        let room = Room::stepped_regions(&[nave, aisle_a, aisle_b, apse]).unwrap();
        let expected = 10.0 * 6.0 * 12.0 + 2.0 * 10.0 * 3.0 * 5.0 + 4.0 * 4.0 * 3.0;
        assert!((room.volume() - expected).abs() < 1e-9, "{}", room.volume());
        assert!(room.contains(Vec3::new(5.0, 1.5, 4.9)));
        assert!(room.contains(Vec3::new(5.0, 6.0, 11.9)));
        assert!(!room.contains(Vec3::new(5.0, 1.5, 5.1)));
        assert!(room.contains(Vec3::new(13.0, 6.0, 2.9)));
        assert!(!room.contains(Vec3::new(13.0, 6.0, 3.1)));
        // Sound passes under the arcade and is stopped by the clerestory wall above it.
        assert!(!room.segment_blocked(
            Vec3::new(5.0, 6.0, 2.0),
            Vec3::new(5.0, 1.5, 2.0),
            None,
            None
        ));
        assert!(room.segment_blocked(
            Vec3::new(5.0, 6.0, 8.0),
            Vec3::new(5.0, 1.5, 4.0),
            None,
            None
        ));
        // Equal heights reduce to the extruded regions, face for face.
        let plan_a = vec![(0.0, 0.0), (4.0, 0.0), (4.0, 3.0), (0.0, 3.0)];
        let plan_b = vec![(4.0, 0.0), (7.0, 0.0), (7.0, 3.0), (4.0, 3.0)];
        let flat =
            Room::extruded_regions(&[(plan_a.clone(), m()), (plan_b.clone(), m())], 2.5).unwrap();
        let stepped = Room::stepped_regions(&[
            Region {
                plan: plan_a,
                materials: m(),
                height: 2.5,
            },
            Region {
                plan: plan_b,
                materials: m(),
                height: 2.5,
            },
        ])
        .unwrap();
        assert_eq!(flat, stepped);
    }

    #[test]
    fn prisms_cut_the_floor_and_the_room_stays_closed() {
        let m = || Material::rigid();
        let block = |x: f64| Prism {
            section: vec![(x, 0.0), (x + 0.2, 0.0), (x + 0.2, 0.5), (x, 0.5)],
            y: (1.0, 3.0),
            material: m(),
        };
        // Written clockwise, to exercise the reversal.
        let knife = Prism {
            section: vec![(5.0, 0.0), (5.05, 2.0), (5.1, 0.0)],
            y: (0.5, 3.5),
            material: m(),
        };
        let plate = Prism {
            section: vec![(7.0, 1.0), (7.025, 1.0), (7.025, 2.0), (7.0, 2.0)],
            y: (1.5, 2.5),
            material: m(),
        };
        let prisms = [block(1.0), block(2.0), knife, plate];
        let room = Room::box_with_prisms(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(9.0, 4.0, 3.0),
            rigid6(),
            &prisms,
        )
        .unwrap();
        let solid = 2.0 * 0.2 * 0.5 * 2.0 + 0.5 * 0.1 * 2.0 * 3.0 + 0.025 * 1.0 * 1.0;
        assert!((room.volume() - (108.0 - solid)).abs() < 1e-9);
        assert_eq!(room.materials.len(), 10);
        assert!(!room.contains(Vec3::new(1.1, 2.0, 0.25)));
        assert!(room.contains(Vec3::new(1.5, 2.0, 0.25)));
        assert!(!room.contains(Vec3::new(7.01, 2.0, 1.5)));
        assert!(room.contains(Vec3::new(6.0, 2.0, 1.5)));
        // The knife's apex is one diffracting edge; on the floor only its two feet diffract, and
        // the grid's seams and the blocks' square feet do not.
        let edges = crate::diffraction::diffracting_edges(&room);
        assert!(
            edges
                .iter()
                .any(|e| { (e.start.z - 2.0).abs() < 1e-9 && (e.length_m - 3.0).abs() < 1e-9 })
        );
        for e in edges
            .iter()
            .filter(|e| e.start.z.abs() < 1e-9 && e.axis.z.abs() < 1e-9)
        {
            assert!(
                (e.start.x - 5.0).abs() < 1e-9 || (e.start.x - 5.1).abs() < 1e-9,
                "unexpected floor edge at {:?}",
                e.start
            );
        }
        let touching = Prism {
            section: vec![(3.0, 0.0), (3.5, 0.5), (4.0, 0.0), (3.5, 1.0)],
            y: (1.0, 2.0),
            material: m(),
        };
        assert!(
            Room::box_with_prisms(
                Vec3::default(),
                Vec3::new(9.0, 4.0, 3.0),
                rigid6(),
                &[touching]
            )
            .is_err()
        );
        assert!(
            Room::box_with_prisms(
                Vec3::default(),
                Vec3::new(9.0, 4.0, 3.0),
                rigid6(),
                &[block(1.0), block(1.1)]
            )
            .is_err()
        );
    }
}
