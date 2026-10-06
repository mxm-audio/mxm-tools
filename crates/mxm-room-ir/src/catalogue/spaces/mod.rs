//! The spaces, one module per family, and the generators they share. Dimensions are invented
//! within the research notes' published sizes for the class (`r0-class-ranges.md`) or the
//! survey's estimates; surfaces are blends of [`super::materials`] rows. Coordinates: x along the
//! room, y across it, z up, metres. A listener facing −x has its left toward −y.

pub mod chambers;
pub mod halls;
pub mod industrial;
pub mod rooms;
pub mod studios;
pub mod transit;
pub mod vehicles;
pub mod venues;
pub mod worship;

use std::f64::consts::PI;

use super::materials::{self as m, part, relief, surface};
use super::{Decay, Position, Reference, Sources, Space, Target};
use crate::air::Air;
use crate::error::Error;
use crate::geometry::{Prism, Region, Room, Vec3};
use crate::material::Material;

const fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z)
}

const fn positions(near: Vec3, far: Vec3) -> [Position; 2] {
    [
        Position {
            name: "near",
            receiver: near,
        },
        Position {
            name: "far",
            receiver: far,
        },
    ]
}

const fn sources(centre: Vec3, left: Vec3, right: Vec3) -> Sources {
    Sources {
        centre,
        left,
        right,
    }
}

fn octaves(ranges: [(f64, f64); 6]) -> Target {
    Target::Octaves(ranges.map(Some))
}

const fn gated(decay: Decay, target: Target, basis: &'static str) -> Reference {
    Reference {
        gated: true,
        decay,
        target,
        slack_s: 0.0,
        basis,
    }
}

const fn reported(decay: Decay, target: Target, basis: &'static str) -> Reference {
    Reference {
        gated: false,
        decay,
        target,
        slack_s: 0.0,
        basis,
    }
}

const fn mid(range: (f64, f64)) -> Target {
    Target::Mean {
        first: 2,
        last: 3,
        range,
    }
}

/// Six box faces in [`Room::shoebox`]'s order: x = 0, x = Lx, y = 0, y = Ly, floor, ceiling.
fn box_faces(walls: &Material, floor: Material, ceiling: Material) -> [Material; 6] {
    [
        walls.clone(),
        walls.clone(),
        walls.clone(),
        walls.clone(),
        floor,
        ceiling,
    ]
}

/// A plan extruded up to a sloping ceiling plane `z = h0 + ax·x + ay·y`: walls meet neither each
/// other nor the floor in parallel pairs. Materials `[floor, ceiling, walls]`.
fn under_sloping_ceiling(
    plan: &[(f64, f64)],
    (h0, ax, ay): (f64, f64, f64),
    materials: [Material; 3],
) -> Result<Room, Error> {
    let top = |(x, y): (f64, f64)| h0 + ax * x + ay * y;
    let mut faces = vec![
        (plan.iter().map(|&(x, y)| v(x, y, 0.0)).collect(), 0),
        (
            plan.iter()
                .rev()
                .map(|&(x, y)| v(x, y, top((x, y))))
                .collect(),
            1,
        ),
    ];
    for i in 0..plan.len() {
        let (a, b) = (plan[i], plan[(i + 1) % plan.len()]);
        faces.push((
            vec![
                v(a.0, a.1, 0.0),
                v(a.0, a.1, top(a)),
                v(b.0, b.1, top(b)),
                v(b.0, b.1, 0.0),
            ],
            2,
        ));
    }
    Room::new(faces, materials.into())
}

/// A space in standard air with a diffuse field and the seam at its Schroeder frequency.
fn space(
    room: Room,
    (sources, positions): (Sources, [Position; 2]),
    max_time_s: f64,
    occupancy: &'static str,
    reference: Reference,
    notes: Vec<&'static str>,
) -> Space {
    Space {
        room,
        air: Air::standard(),
        sources,
        positions,
        non_diffuse: false,
        seam_hz: None,
        max_time_s,
        occupancy,
        reference,
        notes,
    }
}

/// A box room with one blend on all four walls.
fn boxed(size: Vec3, walls: &Material, floor: Material, ceiling: Material) -> Result<Room, Error> {
    Room::shoebox(size, box_faces(walls, floor, ceiling))
}

/// A rectangular solid over the ranges `x`, `y` and `z`: furniture, a plinth, a machine, a pew.
fn block(x: (f64, f64), y: (f64, f64), z: (f64, f64), material: &Material) -> Prism {
    Prism {
        section: vec![(x.0, z.0), (x.1, z.0), (x.1, z.1), (x.0, z.1)],
        y,
        material: material.clone(),
    }
}

/// `count` blocks in a row along `long`, evenly spaced from `span.0` to `span.1` and `width` each
/// along that axis; `across` and `z` are the same for every one. Rows of seats, pews, desks,
/// shelving, benches.
fn row_of_blocks(
    count: usize,
    long: Axis,
    span: (f64, f64),
    width: f64,
    across: (f64, f64),
    z: (f64, f64),
    material: &Material,
) -> Vec<Prism> {
    (0..count)
        .map(|i| {
            let step = if count <= 1 {
                0.0
            } else {
                (span.1 - span.0 - width) / (count - 1) as f64
            };
            let a = span.0 + step * i as f64;
            match long {
                Axis::X => block((a, a + width), across, z, material),
                Axis::Y => block(across, (a, a + width), z, material),
            }
        })
        .collect()
}

/// Pilasters proud of the two long walls, and ribs under the ceiling, as solids in a box from the
/// origin to `size`; `long` is the room's long axis. `span` is the run they cover along it, each
/// pilaster `width` along that axis and `depth` proud of its wall, between the heights `z`; the
/// ribs cross the room, `rib_width` wide, between the heights `rib_z`. **Chosen:** they clear the
/// floor, because a solid standing on it cuts the floor into a grid of hundreds of faces, and the
/// wall keeps its published rows.
#[allow(clippy::too_many_arguments)]
fn pilasters_and_ribs(
    size: Vec3,
    long: Axis,
    span: (f64, f64),
    count: usize,
    (width, depth, z): (f64, f64, (f64, f64)),
    (ribs, rib_z, rib_width): (usize, (f64, f64), f64),
    pilaster: &Material,
    rib: &Material,
) -> Vec<Prism> {
    let place = |n: usize, w: f64, i: usize| {
        let step = if n <= 1 {
            0.0
        } else {
            (span.1 - span.0 - w) / (n - 1) as f64
        };
        span.0 + step * i as f64
    };
    let mut out = Vec::new();
    for i in 0..count {
        let a = place(count, width, i);
        match long {
            Axis::Y => {
                for x in [0.08, size.x - 0.08 - depth] {
                    out.push(Prism {
                        section: vec![(x, z.0), (x + depth, z.0), (x + depth, z.1), (x, z.1)],
                        y: (a, a + width),
                        material: pilaster.clone(),
                    });
                }
            }
            Axis::X => {
                for y in [0.08, size.y - 0.08 - depth] {
                    out.push(Prism {
                        section: vec![(a, z.0), (a + width, z.0), (a + width, z.1), (a, z.1)],
                        y: (y, y + depth),
                        material: pilaster.clone(),
                    });
                }
            }
        }
    }
    for i in 0..ribs {
        let a = place(ribs, rib_width, i);
        out.push(match long {
            Axis::Y => Prism {
                section: vec![
                    (0.7, rib_z.0),
                    (size.x - 0.7, rib_z.0),
                    (size.x - 0.7, rib_z.1),
                    (0.7, rib_z.1),
                ],
                y: (a, a + rib_width),
                material: rib.clone(),
            },
            Axis::X => Prism {
                section: vec![
                    (a, rib_z.0),
                    (a + rib_width, rib_z.0),
                    (a + rib_width, rib_z.1),
                    (a, rib_z.1),
                ],
                y: (0.9, size.y - 0.9),
                material: rib.clone(),
            },
        });
    }
    out
}

/// A box holding `prisms`, its faces as [`boxed`]'s.
fn boxed_with(
    size: Vec3,
    walls: &Material,
    floor: Material,
    ceiling: Material,
    prisms: &[Prism],
) -> Result<Room, Error> {
    Room::box_with_prisms(
        v(0.0, 0.0, 0.0),
        size,
        box_faces(walls, floor, ceiling),
        prisms,
    )
}

/// Sources and positions in a box from the origin to `size` whose long axis is x: the sources near
/// x = 0, the receivers down the room facing them and off its centre line, so that neither sits on
/// a plane of symmetry.
fn layout(size: Vec3) -> (Sources, [Position; 2]) {
    let (l, w, h) = (size.x, size.y, size.z);
    let spread = (0.28 * w).min(3.0);
    let (x, y, z) = ((0.18 * l).max(0.35), 0.5 * w, (0.6 * h).min(1.5));
    (
        sources(v(x, y, z), v(x, y - spread, z), v(x, y + spread, z)),
        positions(
            v(0.45 * l, 0.42 * w, (0.55 * h).min(1.4)),
            v(0.78 * l, 0.58 * w, (0.6 * h).min(1.6)),
        ),
    )
}

/// The axis a cross-section is swept along.
#[derive(Clone, Copy)]
enum Axis {
    /// The section is in (y, z).
    X,
    /// The section is in (x, z).
    Y,
}

/// A cross-section swept from 0 to `length` along `axis`: a tunnel, a vault, a gabled nave, a raked
/// auditorium. `section` is a simple polygon, counter-clockwise in its (first, second) coordinates,
/// each point carrying the material index of the edge that leaves it; both end faces take `ends`.
fn swept(
    axis: Axis,
    section: &[((f64, f64), usize)],
    length: f64,
    ends: usize,
    materials: Vec<Material>,
) -> Result<Room, Error> {
    let at = |(u, w): (f64, f64), t: f64| match axis {
        Axis::X => v(t, u, w),
        Axis::Y => v(u, t, w),
    };
    let n = section.len();
    let mut faces: Vec<(Vec<Vec3>, usize)> = (0..n)
        .map(|i| {
            let ((a, material), (b, _)) = (section[i], section[(i + 1) % n]);
            let quad = vec![at(a, 0.0), at(a, length), at(b, length), at(b, 0.0)];
            (quad, material)
        })
        .collect();
    faces.push((section.iter().map(|&(p, _)| at(p, 0.0)).collect(), ends));
    faces.push((
        section.iter().rev().map(|&(p, _)| at(p, length)).collect(),
        ends,
    ));
    if let Axis::Y = axis {
        // (x, z, y) is left-handed, so every face turns the other way.
        for (vertices, _) in &mut faces {
            vertices.reverse();
        }
    }
    Room::new(faces, materials)
}

/// A regular polygon of `sides` and circumradius `radius` centred at (`radius`, `radius`), walled to
/// `drum` and closed by a dome rising `rise` above it in `rings` rings of planar trapezoids under a
/// flat cap. Materials `[floor, walls, dome]`.
fn domed(
    sides: usize,
    radius: f64,
    (drum, rise): (f64, f64),
    rings: usize,
    materials: [Material; 3],
) -> Result<Room, Error> {
    let point = |k: usize, (r, z): (f64, f64)| {
        let angle = 2.0 * PI * (k % sides) as f64 / sides as f64 + PI / sides as f64;
        v(radius + r * angle.cos(), radius + r * angle.sin(), z)
    };
    let ring = |j: usize| {
        let elevation = 0.5 * PI * j as f64 / (rings + 1) as f64;
        (radius * elevation.cos(), drum + rise * elevation.sin())
    };
    let mut faces = vec![((0..sides).map(|k| point(k, (radius, 0.0))).collect(), 0)];
    for k in 0..sides {
        let (foot, top) = ((radius, 0.0), (radius, drum));
        let wall = vec![
            point(k, foot),
            point(k, top),
            point(k + 1, top),
            point(k + 1, foot),
        ];
        faces.push((wall, 1));
    }
    for j in 0..rings {
        let (low, high) = (ring(j), ring(j + 1));
        for k in 0..sides {
            let facet = vec![
                point(k, low),
                point(k, high),
                point(k + 1, high),
                point(k + 1, low),
            ];
            faces.push((facet, 2));
        }
    }
    faces.push(((0..sides).rev().map(|k| point(k, ring(rings))).collect(), 2));
    Room::new(faces, materials.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generators_close_rooms_of_the_expected_volume() {
        let hard = || surface("hard", &[part(m::HARD, 1.0, 0.0)]).unwrap();
        let section = [
            ((0.0, 0.0), 0),
            ((2.0, 0.0), 0),
            ((2.0, 1.5), 1),
            ((0.0, 1.5), 1),
        ];
        for axis in [Axis::X, Axis::Y] {
            let room = swept(axis, &section, 3.0, 0, vec![hard(), hard()]).unwrap();
            assert!((room.volume() - 9.0).abs() < 1e-9, "{}", room.volume());
        }
        // A fine dome approaches a cylinder under half an ellipsoid.
        let (sides, radius, drum, rise) = (48, 5.0, 4.0, 3.0);
        let room = domed(sides, radius, (drum, rise), 24, [hard(), hard(), hard()]).unwrap();
        let exact = PI * radius * radius * (drum + 2.0 * rise / 3.0);
        assert!(
            (room.volume() / exact - 1.0).abs() < 0.01,
            "{}",
            room.volume() / exact
        );
    }
}
