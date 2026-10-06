//! Stochastic ray tracing: the late energy, the scattered energy, and the specular paths past the
//! image-source order.
//!
//! After Krokstad, Strøm & Sørsdal 1968 and Vorländer 1989, both read for R0 as abstracts only, so
//! the tests below are what establishes this implementation. What the method is, as read in the
//! research page §5: particles leave the source carrying band energy; at each wall, energy is
//! reduced by absorption and the rest splits into a specular fraction `1 − s` and a scattered
//! fraction `s` (ISO 17497-1); energy reaching the receiver is binned by band, direction and time.
//!
//! # Energy
//!
//! Units follow the renderer: a discrete arrival of pressure amplitude `g` has energy `g²`, and the
//! direct sound at 1 m has amplitude 1. A source of unit amplitude at 1 m radiates `4π` in total,
//! so each of `N` rays starts with `4π/N`, weighted by the source directivity squared. [D] Two
//! estimators deposit it:
//!
//! - **Receiver sphere** of radius `r`: a ray segment crossing it with chord `ℓ` deposits
//!   `E·ℓ / (4/3·π·r³)`. The expected chord of a crossing is `4r/3`, so a crossing deposits
//!   `E/(π·r²)` on average, and `N` rays at distance `d` deposit `1/d²`: the direct sound.
//! - **Diffuse rain** (next-event estimation, derived from Lambert's law, not read): at every hit,
//!   the scattered energy `E·s` radiates with intensity `cos θ/π` per steradian, so a visible
//!   receiver at distance `d` receives `E·s·cos θ/(π·d²)`.
//!
//! At each hit a ray continues **either** specularly or scattered. Each ray draws one band as its
//! strategy and scatters with that band's probability `q_j = s_j`. Each band's estimate divides
//! its physical energy (a factor `s` or `1 − s` per hit) by the mean of all bands' strategy
//! probabilities for the path taken: the balance heuristic of multiple importance sampling. Every
//! band is unbiased. While each band's probability is its coefficient (0.01 ≤ s ≤ 0.99, or exactly
//! 0 or 1), its weight is bounded by the number of bands; outside that the clamp lets it grow slowly.
//!
//! **One shared probability is not enough.** With the band-mean `q` the weights `s_k/q` and
//! `(1 − s_k)/(1 − q)` multiply without bound when scattering varies with frequency (0.05 at 125 Hz
//! and 0.99 at 8 kHz on one wall): a few rays carried a low band's whole late energy, and decays
//! fell in steps (plan revision 13). A box with uniform scattering never shows it.
//!
//! # No double counting
//!
//! Each receiver-reaching leg is counted by exactly one estimator.
//! - A **purely specular** ray (no scattering yet) of order at most the image-source order carries
//!   what the image sources deliver exactly. It deposits nothing.
//! - A purely specular ray of higher order deposits into the histogram, or, in a non-diffuse
//!   scene, names its face sequence for an exact ray-assisted image source instead.
//! - A leg that **leaves a scattering event** toward the receiver is the rain's, so the sphere
//!   ignores it.
//! - Every other leg (specular after an earlier scattering) is the sphere's.
//!
//! # Chosen, not read
//!
//! - Directions are binned in [`DIRECTION_CELLS`] equal-area cells: rows uniform in `z`, columns
//!   uniform in azimuth.
//! - Time bins of [`RayOptions::bin_width_s`]; a ray ends below [`RayOptions::energy_floor_db`] of
//!   its start (air included) or at [`RayOptions::max_time_s`].
//! - The receiver radius defaults to `0.1·V^(1/3)`, clamped to 0.2–1.0 m and to 80 % of the
//!   receiver's clearance from every surface.
//! - **Discovery of ray-assisted image sources uses its own purely specular rays**, one per energy
//!   ray, in non-diffuse scenes. An energy ray stays purely specular only with probability
//!   `(1 − s)ⁿ`, so it rarely names a high-order sequence; measured in the V10 test, that alone
//!   found 88 % of a corridor's specular energy with 200,000 rays. A discovery ray reflects
//!   specularly until its wall-and-air product falls below [`RayOptions::discovery_floor_db`].
//! - Discovery happens inside a larger sphere, `0.3·V^(1/3)` clamped between the receiver radius
//!   and 3 m. Discovery only names a face sequence; the exact trace from the receiver point decides
//!   whether the path exists, so a large sphere adds no false arrival and finds far paths with
//!   fewer rays.
//! - A strategy's scattering probability is its band's coefficient, clamped to 0.01–0.99 when it
//!   is strictly between 0 and 1.

use std::collections::BTreeSet;
use std::f64::consts::PI;

use crate::bands::{Bands, NUM_BANDS};
use crate::error::Error;
use crate::geometry::Vec3;
use crate::par::parallel_map;
use crate::rng::SplitMix64;
use crate::scene::Scene;

pub const DIRECTION_ROWS: usize = 8;
pub const DIRECTION_COLUMNS: usize = 16;
pub const DIRECTION_CELLS: usize = DIRECTION_ROWS * DIRECTION_COLUMNS;

/// Rays traced per batch. Batches are the unit of parallel work and of ordered accumulation, so
/// this is fixed, never derived from the thread count.
const BATCH: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayOptions {
    pub rays: usize,
    pub seed: u64,
    pub bin_width_s: f64,
    pub max_time_s: f64,
    /// A ray ends when every band, with air absorption, is this far below its starting energy.
    pub energy_floor_db: f64,
    pub receiver_radius_m: Option<f64>,
    /// Radius inside which a discovery ray names its face sequence (non-diffuse scenes).
    pub discovery_radius_m: Option<f64>,
    /// A discovery ray ends when its walls and air have taken every band this far down.
    pub discovery_floor_db: f64,
    pub threads: usize,
    pub estimator: Estimator,
}

impl Default for RayOptions {
    fn default() -> Self {
        Self {
            rays: 20_000,
            seed: 1,
            bin_width_s: 0.002,
            max_time_s: 30.0,
            energy_floor_db: -100.0,
            receiver_radius_m: None,
            discovery_radius_m: None,
            discovery_floor_db: -60.0,
            threads: std::thread::available_parallelism().map_or(1, |n| n.get()),
            estimator: Estimator::Hybrid,
        }
    }
}

/// What the tracer deposits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Estimator {
    /// Everything the image sources do not already deliver (see the module note).
    Hybrid,
    /// Every leg through the receiver sphere, no rain, nothing excluded. A calibration reference.
    SphereOnly,
}

/// Energy per time bin, direction cell and band. Time is absolute propagation time from emission.
#[derive(Debug, Clone, PartialEq)]
pub struct Histogram {
    pub bin_width_s: f64,
    /// Earliest propagation time of any deposit, s; infinite when nothing was deposited. Late
    /// synthesis starts no earlier, because smoothing the bins lifts energy before it.
    pub earliest_s: f64,
    energy: Vec<f64>,
}

impl Histogram {
    fn new(bin_width_s: f64) -> Self {
        Self {
            bin_width_s,
            earliest_s: f64::INFINITY,
            energy: Vec::new(),
        }
    }

    pub fn bins(&self) -> usize {
        self.energy.len() / (DIRECTION_CELLS * NUM_BANDS)
    }

    fn index(bin: usize, cell: usize, band: usize) -> usize {
        (bin * DIRECTION_CELLS + cell) * NUM_BANDS + band
    }

    pub fn get(&self, bin: usize, cell: usize, band: usize) -> f64 {
        self.energy
            .get(Self::index(bin, cell, band))
            .copied()
            .unwrap_or(0.0)
    }

    /// The cell and band energies of one bin, `[cell][band]` flattened.
    pub fn bin(&self, bin: usize) -> &[f64] {
        let start = bin * DIRECTION_CELLS * NUM_BANDS;
        &self.energy[start..start + DIRECTION_CELLS * NUM_BANDS]
    }

    /// Energy of one bin in one band, all directions.
    pub fn band_energy(&self, bin: usize, band: usize) -> f64 {
        (0..DIRECTION_CELLS).map(|c| self.get(bin, c, band)).sum()
    }

    /// Total energy per band.
    pub fn total_per_band(&self) -> Bands {
        let mut out = [0.0; NUM_BANDS];
        for (i, e) in self.energy.iter().enumerate() {
            out[i % NUM_BANDS] += e;
        }
        out
    }

    fn add(&mut self, bin: usize, cell: usize, energy: &Bands) {
        let needed = (bin + 1) * DIRECTION_CELLS * NUM_BANDS;
        if self.energy.len() < needed {
            self.energy.resize(needed, 0.0);
        }
        let start = Self::index(bin, cell, 0);
        for (slot, e) in self.energy[start..start + NUM_BANDS].iter_mut().zip(energy) {
            *slot += e;
        }
    }
}

/// The direction cell of a unit vector.
pub fn cell_of(direction: Vec3) -> usize {
    let row =
        (((direction.z + 1.0) * 0.5 * DIRECTION_ROWS as f64) as usize).min(DIRECTION_ROWS - 1);
    let azimuth = direction.y.atan2(direction.x).rem_euclid(2.0 * PI);
    let column =
        ((azimuth / (2.0 * PI) * DIRECTION_COLUMNS as f64) as usize).min(DIRECTION_COLUMNS - 1);
    row * DIRECTION_COLUMNS + column
}

/// A uniformly distributed direction inside `cell`, from two uniform numbers.
pub fn sample_in_cell(cell: usize, u1: f64, u2: f64) -> Vec3 {
    let row = cell / DIRECTION_COLUMNS;
    let column = cell % DIRECTION_COLUMNS;
    let z = -1.0 + 2.0 * (row as f64 + u1) / DIRECTION_ROWS as f64;
    let azimuth = 2.0 * PI * (column as f64 + u2) / DIRECTION_COLUMNS as f64;
    let s = (1.0 - z * z).max(0.0).sqrt();
    Vec3::new(s * azimuth.cos(), s * azimuth.sin(), z)
}

#[derive(Debug, Clone, PartialEq)]
pub struct RayTrace {
    pub histogram: Histogram,
    /// Distinct specular face sequences past the image-source order that reached the receiver, in
    /// a non-diffuse scene. Sorted.
    pub specular_sequences: Vec<Vec<usize>>,
    pub rays: usize,
    pub receiver_radius_m: f64,
    pub discovery_radius_m: f64,
    /// Rays that found no face ahead (escaped through an edge). They end there.
    pub leaked: usize,
    pub hits: u64,
    pub seed: u64,
}

struct Deposit {
    bin: u32,
    cell: u8,
    energy: Bands,
}

#[derive(Default)]
struct BatchOut {
    deposits: Vec<Deposit>,
    sequences: BTreeSet<Vec<usize>>,
    leaked: usize,
    hits: u64,
    earliest: Option<f64>,
}

struct Context<'a> {
    scene: &'a Scene,
    options: &'a RayOptions,
    image_source_order: usize,
    radius: f64,
    discovery_radius: f64,
    seed: u64,
    start_energy: f64,
    speed: f64,
    air_db_per_m: Bands,
    max_length: f64,
    floor: f64,
}

/// Traces rays for `scene`, excluding what image sources up to `image_source_order` deliver.
pub fn trace(
    scene: &Scene,
    image_source_order: usize,
    options: &RayOptions,
) -> Result<RayTrace, Error> {
    if options.rays == 0
        || options.threads == 0
        || options.bin_width_s.is_nan()
        || options.bin_width_s <= 0.0
        || options.max_time_s.is_nan()
        || options.max_time_s <= 0.0
    {
        return Err(Error::InvalidOption(
            "rays, threads, bin width and maximum time must be positive".into(),
        ));
    }
    let room = &scene.room;
    let radius = match options.receiver_radius_m {
        Some(r) if r > 0.0 => r,
        Some(_) => {
            return Err(Error::InvalidOption(
                "receiver radius must be positive".into(),
            ));
        }
        None => (0.1 * room.volume().cbrt())
            .clamp(0.2, 1.0)
            .min(0.8 * room.clearance(scene.receiver)),
    };
    let discovery_radius = match options.discovery_radius_m {
        Some(r) if r > 0.0 => r.max(radius),
        Some(_) => {
            return Err(Error::InvalidOption(
                "discovery radius must be positive".into(),
            ));
        }
        None => (0.3 * room.volume().cbrt()).clamp(radius, 3.0),
    };
    let air_db_per_m = std::array::from_fn(|k| {
        scene
            .air
            .attenuation_db_per_m(crate::bands::exact_centre_hz(k))
    });
    let seed = crate::scene::fnv1a64(scene.canonical_text().as_bytes()) ^ options.seed;
    let context = Context {
        scene,
        options,
        image_source_order,
        radius,
        discovery_radius,
        seed,
        start_energy: 4.0 * PI / options.rays as f64,
        speed: scene.air.speed_of_sound(),
        air_db_per_m,
        max_length: options.max_time_s * scene.air.speed_of_sound(),
        floor: 10f64.powf(options.energy_floor_db / 10.0),
    };

    let batches: Vec<usize> = (0..options.rays.div_ceil(BATCH)).collect();
    let mut histogram = Histogram::new(options.bin_width_s);
    let mut sequences = BTreeSet::new();
    let (mut leaked, mut hits) = (0usize, 0u64);
    let mut earliest = f64::INFINITY;
    // Work in windows of batches so memory stays bounded; accumulate each window in batch order.
    for window in batches.chunks(options.threads.max(1) * 4) {
        let outs = parallel_map(window, options.threads, |&b| {
            let mut out = BatchOut::default();
            let end = ((b + 1) * BATCH).min(options.rays);
            for index in b * BATCH..end {
                trace_ray(&context, index, &mut out);
                if scene.non_diffuse && options.estimator == Estimator::Hybrid {
                    discover_ray(&context, index, &mut out);
                }
            }
            out
        });
        for out in outs {
            for d in &out.deposits {
                histogram.add(d.bin as usize, d.cell as usize, &d.energy);
            }
            sequences.extend(out.sequences);
            leaked += out.leaked;
            if let Some(e) = out.earliest {
                earliest = earliest.min(e);
            }
            hits += out.hits;
        }
    }
    histogram.earliest_s = earliest;
    Ok(RayTrace {
        histogram,
        specular_sequences: sequences.into_iter().collect(),
        rays: options.rays,
        receiver_radius_m: radius,
        discovery_radius_m: discovery_radius,
        leaked,
        hits,
        seed,
    })
}

fn uniform_sphere(rng: &mut SplitMix64) -> Vec3 {
    let z = 2.0 * rng.next_f64() - 1.0;
    let azimuth = 2.0 * PI * rng.next_f64();
    let s = (1.0 - z * z).max(0.0).sqrt();
    Vec3::new(s * azimuth.cos(), s * azimuth.sin(), z)
}

/// A cosine-weighted direction about unit `normal`.
fn lambert(normal: Vec3, rng: &mut SplitMix64) -> Vec3 {
    let u1 = rng.next_f64();
    let u2 = rng.next_f64();
    let r = u1.sqrt();
    let phi = 2.0 * PI * u2;
    let helper = if normal.x.abs() < 0.9 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let t = normal.cross(helper).normalized();
    let b = normal.cross(t);
    (t * (r * phi.cos()) + b * (r * phi.sin()) + normal * (1.0 - u1).max(0.0).sqrt()).normalized()
}

/// Chord length and distance along the segment to the closest approach, if the segment from
/// `origin` along unit `direction` for `length` crosses the sphere.
fn sphere_crossing(
    origin: Vec3,
    direction: Vec3,
    length: f64,
    centre: Vec3,
    radius: f64,
) -> Option<(f64, f64)> {
    let w = centre - origin;
    let along = w.dot(direction);
    let miss2 = w.dot(w) - along * along;
    let r2 = radius * radius;
    if miss2 >= r2 {
        return None;
    }
    let half = (r2 - miss2).sqrt();
    let t0 = (along - half).max(0.0);
    let t1 = (along + half).min(length);
    (t1 > t0).then(|| (t1 - t0, along.clamp(0.0, length)))
}

/// The mean of a band vector.
fn mean(b: &Bands) -> f64 {
    b.iter().sum::<f64>() / NUM_BANDS as f64
}

/// A strategy's scattering probability for coefficient `s`.
fn scatter_probability(s: f64) -> f64 {
    if s <= 0.0 || s >= 1.0 {
        s.clamp(0.0, 1.0)
    } else {
        s.clamp(0.01, 0.99)
    }
}

impl Context<'_> {
    fn deposit(&self, out: &mut BatchOut, length: f64, from: Vec3, energy: Bands) {
        let time = length / self.speed;
        let bin = (time / self.options.bin_width_s) as usize;
        if bin > u32::MAX as usize || energy.iter().all(|&e| e == 0.0) {
            return;
        }
        out.earliest = Some(out.earliest.map_or(time, |e| e.min(time)));
        out.deposits.push(Deposit {
            bin: bin as u32,
            cell: cell_of(from) as u8,
            energy,
        });
    }
}

fn trace_ray(cx: &Context, index: usize, out: &mut BatchOut) {
    let scene = cx.scene;
    let room = &scene.room;
    let hybrid = cx.options.estimator == Estimator::Hybrid;
    let mut rng = SplitMix64::stream(cx.seed, index as u64);
    let mut direction = uniform_sphere(&mut rng);
    let gain = scene.source_directivity.band_gains(direction);
    let mut energy: Bands = std::array::from_fn(|k| cx.start_energy * gain[k] * gain[k]);
    if energy.iter().all(|&e| e == 0.0) {
        return;
    }
    let sphere_volume = 4.0 / 3.0 * PI * cx.radius.powi(3);
    let mut position = scene.source;
    let mut length = 0.0;
    let mut skip = None;
    let mut purely_specular = true;
    let mut specular_order = 0usize;
    let mut after_scatter = false;
    // The band whose scattering decides this ray's continuations, and every band's probability of
    // the choices made so far.
    let strategy = (rng.next_u64() % NUM_BANDS as u64) as usize;
    let mut strategies: Bands = [1.0; NUM_BANDS];
    loop {
        let Some((distance, face, point)) = room.first_hit(position, direction, skip) else {
            out.leaked += 1;
            return;
        };
        if let Some((chord, along)) =
            sphere_crossing(position, direction, distance, scene.receiver, cx.radius)
        {
            let counted = if !hybrid {
                true
            } else if purely_specular {
                // Image sources own low orders; discovery rays own the rest in non-diffuse scenes.
                specular_order > cx.image_source_order && !scene.non_diffuse
            } else {
                !after_scatter
            };
            if counted {
                let scale = chord / (sphere_volume * mean(&strategies));
                cx.deposit(out, length + along, -direction, energy.map(|e| e * scale));
            }
        }
        length += distance;
        out.hits += 1;
        if length > cx.max_length {
            return;
        }
        let poly = &room.polygons[face];
        let material = &room.materials[poly.material];
        for (e, a) in energy.iter_mut().zip(&material.absorption) {
            *e *= 1.0 - a;
        }
        if hybrid && material.scattering.iter().any(|&s| s > 0.0) {
            let to_receiver = scene.receiver - point;
            let d = to_receiver.length();
            let cos = poly.normal.dot(to_receiver) / d;
            if cos > 0.0 && !room.segment_blocked(point, scene.receiver, Some(face), None) {
                let w = cos / (PI * d * d * mean(&strategies));
                let rain = std::array::from_fn(|k| energy[k] * material.scattering[k] * w);
                cx.deposit(out, length + d, (point - scene.receiver) * (1.0 / d), rain);
            }
        }
        let q = scatter_probability(material.scattering[strategy]);
        let scatter = q >= 1.0 || (q > 0.0 && rng.next_f64() < q);
        for (k, &sk) in material.scattering.iter().enumerate() {
            let qk = scatter_probability(sk);
            if scatter {
                energy[k] *= sk;
                strategies[k] *= qk;
            } else {
                energy[k] *= 1.0 - sk;
                strategies[k] *= 1.0 - qk;
            }
        }
        // Only the ratio of energy to strategy probability matters; keep both from underflowing.
        // The drawn strategy's own probability is positive, so the largest is.
        let largest = strategies.iter().copied().fold(0.0, f64::max);
        for (e, p) in energy.iter_mut().zip(strategies.iter_mut()) {
            *e /= largest;
            *p /= largest;
        }
        if scatter {
            direction = lambert(poly.normal, &mut rng);
            purely_specular = false;
            after_scatter = true;
        } else {
            direction = direction - poly.normal * (2.0 * poly.normal.dot(direction));
            after_scatter = false;
            if purely_specular {
                specular_order += 1;
            }
        }
        position = point;
        skip = Some(face);
        let mix = mean(&strategies);
        let alive = energy
            .iter()
            .zip(&cx.air_db_per_m)
            .any(|(e, a)| e / mix * 10f64.powf(-a * length / 10.0) > cx.floor * cx.start_energy);
        if !alive {
            return;
        }
    }
}

/// A purely specular ray that only names face sequences reaching the discovery sphere.
fn discover_ray(cx: &Context, index: usize, out: &mut BatchOut) {
    let scene = cx.scene;
    let room = &scene.room;
    let mut rng = SplitMix64::stream(cx.seed ^ 0x6469_7363_6f76_6572, index as u64);
    let mut direction = uniform_sphere(&mut rng);
    let gain = scene.source_directivity.band_gains(direction);
    let mut energy: Bands = gain.map(|g| g * g);
    let floor = 10f64.powf(cx.options.discovery_floor_db / 10.0);
    if energy.iter().all(|&e| e <= floor) {
        return;
    }
    let mut position = scene.source;
    let mut length = 0.0;
    let mut skip = None;
    let mut faces: Vec<usize> = Vec::new();
    loop {
        let Some((distance, face, point)) = room.first_hit(position, direction, skip) else {
            return;
        };
        if faces.len() > cx.image_source_order
            && sphere_crossing(
                position,
                direction,
                distance,
                scene.receiver,
                cx.discovery_radius,
            )
            .is_some()
            && !out.sequences.contains(&faces)
        {
            out.sequences.insert(faces.clone());
        }
        length += distance;
        if length > cx.max_length {
            return;
        }
        let poly = &room.polygons[face];
        let material = &room.materials[poly.material];
        for ((e, a), s) in energy
            .iter_mut()
            .zip(&material.absorption)
            .zip(&material.scattering)
        {
            *e *= (1.0 - a) * (1.0 - s);
        }
        direction = direction - poly.normal * (2.0 * poly.normal.dot(direction));
        faces.push(face);
        position = point;
        skip = Some(face);
        let alive = energy
            .iter()
            .zip(&cx.air_db_per_m)
            .any(|(e, a)| e * 10f64.powf(-a * length / 10.0) > floor);
        if !alive {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::air::Air;
    use crate::geometry::Room;
    use crate::ism::{ImageSourceOptions, image_sources};
    use crate::material::Material;

    fn box_scene(size: Vec3, material: Material) -> Scene {
        let room = Room::shoebox(size, std::array::from_fn(|_| material.clone())).unwrap();
        Scene::new(
            "rays",
            room,
            Air::standard(),
            Vec3::new(size.x * 0.31, size.y * 0.37, size.z * 0.43),
            Vec3::new(size.x * 0.71, size.y * 0.62, size.z * 0.52),
        )
        .unwrap()
    }

    #[test]
    fn direction_cells_round_trip() {
        let mut rng = SplitMix64::new(3);
        for cell in 0..DIRECTION_CELLS {
            let d = sample_in_cell(cell, rng.next_f64(), rng.next_f64());
            assert!((d.length() - 1.0).abs() < 1e-12);
            assert_eq!(cell_of(d), cell);
        }
    }

    /// Rays' V1: in a room that absorbs everything, the sphere estimator returns the direct
    /// sound's energy `1/d²`, in the bin of its arrival time. About 5,400 rays cross the sphere;
    /// with the chord's own spread (coefficient of variation 0.35) the estimate's standard error is
    /// about 1.5 %, so 5 % is a three-sigma bound.
    #[test]
    fn sphere_estimator_is_calibrated_to_the_direct_sound() {
        let scene = box_scene(Vec3::new(20.0, 20.0, 20.0), Material::anechoic());
        let d = (scene.receiver - scene.source).length();
        let options = RayOptions {
            rays: 2_000_000,
            receiver_radius_m: Some(1.0),
            estimator: Estimator::SphereOnly,
            ..Default::default()
        };
        let traced = trace(&scene, 0, &options).unwrap();
        let total = traced.histogram.total_per_band()[4];
        assert!((total * d * d - 1.0).abs() < 0.05, "{} vs 1", total * d * d);
        let bin = (d / scene.air.speed_of_sound() / options.bin_width_s) as usize;
        let near: f64 = (bin.saturating_sub(2)..=bin + 2)
            .map(|b| traced.histogram.band_energy(b, 4))
            .sum();
        assert!((near / total - 1.0).abs() < 1e-9);
        assert_eq!(traced.leaked, 0);
    }

    /// No gap and no overlap: image-source energy plus the hybrid deposit equals everything the
    /// sphere sees when nothing is excluded.
    #[test]
    fn hybrid_plus_images_equals_the_sphere_reference() {
        let material = Material::new("m", [0.12; NUM_BANDS], [0.35; NUM_BANDS]).unwrap();
        let scene = box_scene(Vec3::new(6.3, 4.9, 3.1), material);
        let order = 3;
        let base = RayOptions {
            rays: 60_000,
            max_time_s: 0.6,
            receiver_radius_m: Some(0.6),
            ..Default::default()
        };
        let hybrid = trace(&scene, order, &base).unwrap();
        let reference = trace(
            &scene,
            order,
            &RayOptions {
                estimator: Estimator::SphereOnly,
                ..base
            },
        )
        .unwrap();
        let limit = base.max_time_s * scene.air.speed_of_sound();
        let images: f64 = image_sources(
            &scene,
            &ImageSourceOptions {
                max_order: order,
                max_path_length_m: limit,
                ..Default::default()
            },
        )
        .unwrap()
        .iter()
        .map(|a| (a.reflection[4] / a.path_length_m).powi(2))
        .sum();
        let combined = images + hybrid.histogram.total_per_band()[4];
        let sphere = reference.histogram.total_per_band()[4];
        assert!(
            (combined / sphere - 1.0).abs() < 0.04,
            "images {images} + hybrid {} = {combined}, sphere {sphere}",
            hybrid.histogram.total_per_band()[4]
        );
    }

    #[test]
    fn identical_at_any_thread_count() {
        let material = Material::new("m", [0.2; NUM_BANDS], [0.5; NUM_BANDS]).unwrap();
        let scene = box_scene(Vec3::new(5.0, 4.0, 3.0), material);
        let base = RayOptions {
            rays: 3_000,
            max_time_s: 0.3,
            ..Default::default()
        };
        let one = trace(&scene, 2, &RayOptions { threads: 1, ..base }).unwrap();
        let many = trace(&scene, 2, &RayOptions { threads: 9, ..base }).unwrap();
        assert_eq!(one.histogram.energy.len(), many.histogram.energy.len());
        assert!(
            one.histogram
                .energy
                .iter()
                .zip(&many.histogram.energy)
                .all(|(a, b)| a.to_bits() == b.to_bits())
        );
    }
}
