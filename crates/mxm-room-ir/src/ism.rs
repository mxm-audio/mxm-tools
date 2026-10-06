//! Image sources in arbitrary polyhedra.
//!
//! After Borish, *Extension of the image model to arbitrary polyhedra*, JASA 75(6), 1827–1836
//! (1984). Read for R0 as an abstract only, so the tests below are what establishes this
//! implementation, not the paper. Every sequence of distinct consecutive faces defines a candidate
//! image. A candidate becomes an arrival only if it passes both tests:
//!
//! - **Validity:** each mirroring starts from a point on the interior side of the face's plane.
//! - **Visibility:** tracing back from the receiver, each segment crosses its face inside the
//!   polygon, and no segment is blocked by any other polygon.
//!
//! For a rigid box this reproduces Allen & Berkley's image set exactly (JASA 65(4), 1979). The
//! `rigid_box_matches_allen_berkley` test checks that image for image.
//!
//! **Pruning is safe in any geometry.** An ancestor image is never farther from the receiver than
//! the complete path. The straight line to it is shorter than the folded path, which has the full
//! length, so a subtree whose root already exceeds the length limit can be dropped without losing
//! a valid path.
//!
//! The walls' specular pressure reflection is applied here; air, spreading and time are the
//! renderer's.

use crate::bands::{self, Bands};
use crate::error::Error;
use crate::geometry::{Room, Vec3};
use crate::scene::Scene;

/// Parameter tolerance for segment–polygon crossings.
const SEGMENT_EPS: f64 = 1e-9;
/// Distance below which a point counts as lying on a plane.
const PLANE_EPS: f64 = 1e-9;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageSourceOptions {
    /// Highest reflection order.
    pub max_order: usize,
    /// Longest path kept, m.
    pub max_path_length_m: f64,
    /// Upper bound on candidate images visited, so a request that would run for days is refused.
    pub max_candidates: usize,
}

impl Default for ImageSourceOptions {
    fn default() -> Self {
        Self {
            max_order: 3,
            max_path_length_m: f64::INFINITY,
            max_candidates: 50_000_000,
        }
    }
}

/// One specular arrival at the receiver.
#[derive(Debug, Clone, PartialEq)]
pub struct Arrival {
    /// Unfolded path length, m.
    pub path_length_m: f64,
    /// Unit vector from the receiver toward where the sound arrives from (its last reflection point,
    /// or the source for the direct path).
    pub direction: Vec3,
    /// Unit vector from the source toward where the sound leaves it (its first reflection point,
    /// or the receiver for the direct path). Source directivity is evaluated along it.
    pub departure: Vec3,
    /// Faces reflected from, in the order the sound meets them.
    pub faces: Vec<usize>,
    /// Cosine of the angle of incidence at each face, in the same order. A wave-solved scene takes
    /// each wall's reflection coefficient at this angle.
    pub incidence_cos: Vec<f64>,
    /// Product of the walls' specular pressure reflections, per band. Air is not included.
    pub reflection: Bands,
}

impl Arrival {
    pub fn order(&self) -> usize {
        self.faces.len()
    }
}

struct Candidate {
    image: Vec3,
    /// Faces in the order they were applied to the source (the order the sound meets them).
    faces: Vec<usize>,
}

/// Every valid, visible specular path up to the options' order and length.
///
/// Arrivals are returned in a deterministic order: by path length, then by face sequence.
pub fn image_sources(scene: &Scene, options: &ImageSourceOptions) -> Result<Vec<Arrival>, Error> {
    if options.max_path_length_m.is_nan() || options.max_path_length_m <= 0.0 {
        return Err(Error::InvalidOption(
            "max_path_length_m must be positive".into(),
        ));
    }
    let room = &scene.room;
    let (source, receiver) = (scene.source, scene.receiver);
    let reflections: Vec<Bands> = room
        .materials
        .iter()
        .map(|m| m.specular_pressure_reflection())
        .collect();

    let mut arrivals = Vec::new();
    let mut stack = vec![Candidate {
        image: source,
        faces: Vec::new(),
    }];
    let mut visited = 0usize;
    while let Some(candidate) = stack.pop() {
        visited += 1;
        if visited > options.max_candidates {
            return Err(Error::TooManyImages(options.max_candidates));
        }
        let path_length = (candidate.image - receiver).length();
        if path_length > options.max_path_length_m {
            continue;
        }
        if let Some(arrival) = trace(room, source, receiver, &candidate, &reflections) {
            arrivals.push(arrival);
        }
        if candidate.faces.len() == options.max_order {
            continue;
        }
        for (f, poly) in room.polygons.iter().enumerate() {
            if candidate.faces.last() == Some(&f) {
                continue;
            }
            if poly.signed_distance(candidate.image) <= PLANE_EPS {
                continue;
            }
            let image = poly.mirror(candidate.image);
            if (image - receiver).length() > options.max_path_length_m {
                continue;
            }
            let mut faces = candidate.faces.clone();
            faces.push(f);
            stack.push(Candidate { image, faces });
        }
    }
    arrivals.sort_by(|a, b| {
        a.path_length_m
            .partial_cmp(&b.path_length_m)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.faces.cmp(&b.faces))
    });
    Ok(arrivals)
}

/// The visibility test for one candidate, returning its arrival if the path exists.
fn trace(
    room: &Room,
    source: Vec3,
    receiver: Vec3,
    candidate: &Candidate,
    reflections: &[Bands],
) -> Option<Arrival> {
    let order = candidate.faces.len();
    // Rebuild the image chain I_0 = source … I_order = candidate.image.
    let mut images = Vec::with_capacity(order + 1);
    images.push(source);
    for &f in &candidate.faces {
        let previous = *images.last().unwrap();
        images.push(room.polygons[f].mirror(previous));
    }
    let mut point = receiver;
    let mut point_face: Option<usize> = None;
    let mut first_hit: Option<Vec3> = None;
    let mut last_hit: Option<Vec3> = None;
    let mut incidence_cos = vec![0.0; order];
    for k in (1..=order).rev() {
        let f = candidate.faces[k - 1];
        let poly = &room.polygons[f];
        if poly.signed_distance(point) <= PLANE_EPS {
            return None;
        }
        // The leg toward an image makes the same angle with the face as the incoming sound.
        let leg = images[k] - point;
        incidence_cos[k - 1] = (poly.normal.dot(leg) / leg.length()).abs();
        let (_, hit) = poly.segment_intersection(point, images[k], SEGMENT_EPS)?;
        if occluded(room, point, hit, point_face, Some(f)) {
            return None;
        }
        if first_hit.is_none() {
            first_hit = Some(hit);
        }
        last_hit = Some(hit);
        point = hit;
        point_face = Some(f);
    }
    if occluded(room, point, source, point_face, None) {
        return None;
    }
    let mut reflection = bands::uniform(1.0);
    for &f in &candidate.faces {
        reflection = bands::mul(&reflection, &reflections[room.polygons[f].material]);
    }
    let toward = first_hit.unwrap_or(source) - receiver;
    let departure = last_hit.unwrap_or(receiver) - source;
    Some(Arrival {
        path_length_m: (candidate.image - receiver).length(),
        direction: toward.normalized(),
        departure: departure.normalized(),
        faces: candidate.faces.clone(),
        incidence_cos,
        reflection,
    })
}

fn occluded(room: &Room, a: Vec3, b: Vec3, skip_a: Option<usize>, skip_b: Option<usize>) -> bool {
    room.segment_blocked(a, b, skip_a, skip_b)
}

/// The arrival for one named face sequence, if that specular path is valid and visible. This is
/// how a ray's specular history becomes an exact image source (Vorländer 1989, read as an abstract
/// only; the ray-assisted tests in `rays` are the evidence).
pub fn arrival_for_sequence(scene: &Scene, faces: &[usize]) -> Option<Arrival> {
    let room = &scene.room;
    let mut image = scene.source;
    for (i, &f) in faces.iter().enumerate() {
        if f >= room.polygons.len() || (i > 0 && faces[i - 1] == f) {
            return None;
        }
        let poly = &room.polygons[f];
        if poly.signed_distance(image) <= PLANE_EPS {
            return None;
        }
        image = poly.mirror(image);
    }
    let reflections: Vec<Bands> = room
        .materials
        .iter()
        .map(|m| m.specular_pressure_reflection())
        .collect();
    let candidate = Candidate {
        image,
        faces: faces.to_vec(),
    };
    trace(room, scene.source, scene.receiver, &candidate, &reflections)
}

/// Whether an arrival carries any energy at all.
pub fn is_silent(arrival: &Arrival) -> bool {
    arrival.reflection.iter().all(|&g| g == 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::air::Air;
    use crate::bands::NUM_BANDS;
    use crate::material::Material;

    fn rigid_box_scene() -> Scene {
        let room = Room::shoebox(
            Vec3::new(7.13, 5.31, 3.17),
            std::array::from_fn(|_| Material::rigid()),
        )
        .unwrap();
        Scene::new(
            "box",
            room,
            Air::standard(),
            Vec3::new(2.21, 1.73, 1.41),
            Vec3::new(4.93, 3.12, 1.63),
        )
        .unwrap()
    }

    /// Allen & Berkley's image set for a rectangular room: along each axis, images at `2nL + s`
    /// (|2n| reflections) and `2nL − s` (|2n − 1| reflections).
    fn allen_berkley_distances(size: Vec3, s: Vec3, r: Vec3, max_order: usize) -> Vec<f64> {
        let axis = |l: f64, s: f64| {
            let n_max = max_order as i64 + 1;
            let mut out = Vec::new();
            for n in -n_max..=n_max {
                out.push((2.0 * n as f64 * l + s, (2 * n).unsigned_abs() as usize));
                out.push((2.0 * n as f64 * l - s, (2 * n - 1).unsigned_abs() as usize));
            }
            out
        };
        let (ax, ay, az) = (axis(size.x, s.x), axis(size.y, s.y), axis(size.z, s.z));
        let mut d = Vec::new();
        for &(x, ox) in &ax {
            for &(y, oy) in &ay {
                for &(z, oz) in &az {
                    if ox + oy + oz <= max_order {
                        d.push((Vec3::new(x, y, z) - r).length());
                    }
                }
            }
        }
        d.sort_by(|a, b| a.partial_cmp(b).unwrap());
        d
    }

    #[test]
    fn rigid_box_matches_allen_berkley() {
        let scene = rigid_box_scene();
        for order in 0..=6 {
            let options = ImageSourceOptions {
                max_order: order,
                ..Default::default()
            };
            let arrivals = image_sources(&scene, &options).unwrap();
            let expected = allen_berkley_distances(
                Vec3::new(7.13, 5.31, 3.17),
                scene.source,
                scene.receiver,
                order,
            );
            assert_eq!(arrivals.len(), expected.len(), "order {order}");
            for (a, e) in arrivals.iter().zip(&expected) {
                assert!(
                    (a.path_length_m - e).abs() < 1e-9,
                    "order {order}: {} vs {e}",
                    a.path_length_m
                );
                assert_eq!(a.reflection, [1.0; NUM_BANDS]);
            }
        }
    }

    #[test]
    fn l_shaped_room_occludes_and_keeps_the_right_first_order_paths() {
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

        // Different legs: no line of sight.
        let apart = Scene::new(
            "apart",
            room.clone(),
            Air::standard(),
            Vec3::new(8.0, 2.0, 1.5),
            Vec3::new(2.0, 8.0, 1.5),
        )
        .unwrap();
        let direct = image_sources(
            &apart,
            &ImageSourceOptions {
                max_order: 0,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(direct.is_empty());

        // Same leg. Faces: floor 0, ceiling 1, walls y=0 (2), x=10 (3), y=4 notch (4), x=4 notch (5),
        // y=10 (6), x=0 (7). Worked by hand: the x=4 wall faces away from the source, and the y=10
        // path meets its plane outside the wall.
        let together = Scene::new(
            "together",
            room,
            Air::standard(),
            Vec3::new(8.0, 2.0, 1.5),
            Vec3::new(6.0, 3.0, 1.2),
        )
        .unwrap();
        let arrivals = image_sources(
            &together,
            &ImageSourceOptions {
                max_order: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let mut first: Vec<usize> = arrivals
            .iter()
            .filter(|a| a.order() == 1)
            .map(|a| a.faces[0])
            .collect();
        first.sort_unstable();
        assert_eq!(first, vec![0, 1, 2, 3, 4, 7]);
        assert_eq!(arrivals.iter().filter(|a| a.order() == 0).count(), 1);
    }

    #[test]
    fn path_lengths_equal_the_traced_geometry() {
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
        let scene = Scene::new(
            "l",
            room,
            Air::standard(),
            Vec3::new(8.3, 1.7, 1.3),
            Vec3::new(1.9, 7.6, 1.7),
        )
        .unwrap();
        let arrivals = image_sources(
            &scene,
            &ImageSourceOptions {
                max_order: 4,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!arrivals.is_empty());
        for a in &arrivals {
            // Walk the path forward: mirror the receiver back through the faces in reverse.
            let mut points = vec![scene.receiver];
            let mut images = vec![scene.source];
            for &f in &a.faces {
                images.push(scene.room.polygons[f].mirror(*images.last().unwrap()));
            }
            let mut p = scene.receiver;
            for k in (1..=a.order()).rev() {
                let (_, hit) = scene.room.polygons[a.faces[k - 1]]
                    .segment_intersection(p, images[k], 1e-9)
                    .unwrap();
                points.push(hit);
                p = hit;
            }
            points.push(scene.source);
            let length: f64 = points.windows(2).map(|w| (w[1] - w[0]).length()).sum();
            assert!((length - a.path_length_m).abs() < 1e-9);
        }
    }

    #[test]
    fn length_limit_and_budget() {
        let scene = rigid_box_scene();
        let all = image_sources(
            &scene,
            &ImageSourceOptions {
                max_order: 5,
                ..Default::default()
            },
        )
        .unwrap();
        let limited = image_sources(
            &scene,
            &ImageSourceOptions {
                max_order: 5,
                max_path_length_m: 15.0,
                ..Default::default()
            },
        )
        .unwrap();
        let expected = all.iter().filter(|a| a.path_length_m <= 15.0).count();
        assert_eq!(limited.len(), expected);
        assert!(matches!(
            image_sources(
                &scene,
                &ImageSourceOptions {
                    max_order: 8,
                    max_candidates: 1000,
                    ..Default::default()
                }
            ),
            Err(Error::TooManyImages(1000))
        ));
    }
}
