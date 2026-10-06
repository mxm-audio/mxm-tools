//! The wave solver: finite differences in time and space below the frequency seam (plan §4.2).
//!
//! # Scheme
//!
//! The standard rectilinear leapfrog scheme for the pressure wave equation, run at its stability
//! limit `λ = cΔt/h = 1/√3`, where its dispersion is least (Kowalczyk & van Walstijn 2011 §II;
//! Hamilton 2016 eq. 3.55; research page §6). Waves travel slower than `c` at high frequency, most
//! along the axes; the 2 % accuracy band ends at `0.075·fs`. So a scene's solver runs at a rate set
//! from the highest frequency it must carry, not at the audio rate.
//!
//! # Boundaries
//!
//! Cells are cubes centred on grid points; a cell is inside the room when its centre is. A missing
//! neighbour is a boundary face of area `h²` at half a cell's distance. Integrating the wave equation
//! over a cell gives, per boundary face, a flux term `−(λ/2)·(w^{n+1} − w^{n−1})`, where `w = ρc·v`
//! is the face's normal velocity in pressure units [D]. A rigid face contributes nothing, which is
//! the Neumann condition. An impedance face carries its boundary's branches
//! ([`crate::boundary`]), each obeying `ℓ·w'' + r·w' + κ·w = p'`: the branch equation
//! differentiated once, so the boundary flux pairs `p'` with `w'`. Its centred discretisation is
//! `ℓ·δₜₜw + r·δₜ·w + κ·μₜ·w = δₜ·p` [D]. Multiplying by `δₜ·w` gives a non-negative stored energy
//! plus a non-negative dissipation `r(δₜ·w)²` for `ℓ, r, κ ≥ 0`, which is why a positive-real boundary
//! cannot add energy. The research page names the positive-real condition (Hamilton §2.1.3); this
//! particular discretisation is derived here, and the tests are its evidence: reflection against the
//! model at normal and oblique incidence, and monotone decay over long runs.
//!
//! The walls are staircased: a slanted wall becomes steps (research page §9 wart 6). A face takes
//! the material of the nearest polygon.
//!
//! # Source and probes
//!
//! A point source adds `λ²·s/h` to the cells around it (trilinear weights), which approximates
//! `∂ₜₜp − c²∇²p = c²·s·δ(x)` and so radiates `p = s(t − r/c)/(4πr)`. Probes read pressure by
//! trilinear interpolation, and particle velocity (as `w = ρc·v`) by integrating the pressure
//! gradient, `∂ₜw = −c∇p`, from central differences.
//!
//! # Determinism
//!
//! Every cell's update reads only its own state and the previous two time levels, so it is computed in
//! parallel chunks without changing any bit. Each worker owns a contiguous range of cells, and with
//! it that range's boundary cells, their branch states and any source cells, so a whole step is one
//! parallel round. Probes are read serially between steps.

use std::sync::Barrier;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use crate::air::Air;
use crate::boundary::Boundary;
use crate::error::Error;
use crate::geometry::{Polygon, Room, Vec3};

/// `λ²` at the stability limit.
const LAMBDA2: f64 = 1.0 / 3.0;
/// `λ = 1/√3`.
const LAMBDA: f64 = 0.577_350_269_189_625_8;
const OUTSIDE: u32 = u32::MAX;
/// Neighbour directions: +x, −x, +y, −y, +z, −z.
const DIRECTIONS: [[i64; 3]; 6] = [
    [1, 0, 0],
    [-1, 0, 0],
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
];

/// A voxelised room: active cells, their neighbours, and their boundary faces.
#[derive(Debug, Clone)]
pub struct WaveGrid {
    pub sample_rate: f64,
    pub spacing_m: f64,
    pub speed_of_sound: f64,
    /// Centre of cell `(0, 0, 0)`.
    pub origin: Vec3,
    pub dims: [usize; 3],
    index: Vec<u32>,
    /// Grid coordinates of each active cell; read by tests of face assignment.
    #[cfg_attr(not(test), allow(dead_code))]
    positions: Vec<[u32; 3]>,
    neighbours: Vec<[u32; 6]>,
    interior: Vec<u8>,
    /// Boundary faces with a non-rigid boundary: `(cell, boundary index)`, sorted by cell.
    faces: Vec<(u32, u16)>,
    pub boundaries: Vec<Boundary>,
}

impl WaveGrid {
    pub fn cells(&self) -> usize {
        self.neighbours.len()
    }

    /// The spacing the scheme needs at `sample_rate` and speed `c`: `h = c·√3/fs`.
    pub fn spacing_for(sample_rate: f64, speed: f64) -> f64 {
        speed * 3f64.sqrt() / sample_rate
    }

    /// Voxelises `room` at `sample_rate`. Each material's boundary model comes from
    /// [`crate::material::Material::boundary_model`].
    pub fn from_room(room: &Room, air: &Air, sample_rate: f64) -> Result<Self, Error> {
        if sample_rate.is_nan() || sample_rate <= 0.0 {
            return Err(Error::InvalidOption(
                "wave solver rate must be positive".into(),
            ));
        }
        let speed = air.speed_of_sound();
        let h = Self::spacing_for(sample_rate, speed);
        let (mut lo, mut hi) = (
            Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY),
            Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
        );
        for p in room.polygons.iter().flat_map(|p| p.vertices.iter()) {
            lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
            hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
        let dims = [
            ((hi.x - lo.x) / h).ceil() as usize,
            ((hi.y - lo.y) / h).ceil() as usize,
            ((hi.z - lo.z) / h).ceil() as usize,
        ];
        let total = dims[0]
            .checked_mul(dims[1])
            .and_then(|v| v.checked_mul(dims[2]))
            .filter(|&v| v < u32::MAX as usize / 2)
            .ok_or_else(|| Error::InvalidOption("wave grid too large".into()))?;
        let origin = lo + Vec3::new(h / 2.0, h / 2.0, h / 2.0);
        // Inside test by vertical scanlines, jittered off grid lines so no line meets an edge.
        let jitter = (0.000_318_309_886 * h, 0.000_271_828_183 * h);
        let mut inside = vec![false; total];
        let slanted: Vec<&Polygon> = room
            .polygons
            .iter()
            .filter(|p| p.normal.z.abs() > 1e-9)
            .collect();
        for iy in 0..dims[1] {
            for ix in 0..dims[0] {
                let x = origin.x + ix as f64 * h + jitter.0;
                let y = origin.y + iy as f64 * h + jitter.1;
                let mut crossings: Vec<f64> = slanted
                    .iter()
                    .filter_map(|p| {
                        let z = (p.offset - p.normal.x * x - p.normal.y * y) / p.normal.z;
                        p.contains_coplanar(Vec3::new(x, y, z)).then_some(z)
                    })
                    .collect();
                crossings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                for iz in 0..dims[2] {
                    let z = origin.z + iz as f64 * h;
                    let below = crossings.iter().filter(|&&c| c < z).count();
                    if below % 2 == 1 {
                        inside[ix + dims[0] * (iy + dims[1] * iz)] = true;
                    }
                }
            }
        }
        let boundaries = room
            .materials
            .iter()
            .map(|m| m.boundary_model())
            .collect::<Result<Vec<_>, _>>()?;
        let face_material = |centre: Vec3, direction: usize| -> usize {
            let d = DIRECTIONS[direction];
            let face = centre + Vec3::new(d[0] as f64, d[1] as f64, d[2] as f64) * (h / 2.0);
            room.polygons
                .iter()
                .min_by(|a, b| {
                    polygon_distance(a, face)
                        .partial_cmp(&polygon_distance(b, face))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map_or(0, |p| p.material)
        };
        Self::assemble(
            Layout {
                dims,
                h,
                sample_rate,
                speed,
                origin,
            },
            &inside,
            boundaries,
            face_material,
        )
    }

    /// A box of `dims` cells, each side with its own boundary (`None` is rigid), in the order +x,
    /// −x, +y, −y, +z, −z. Cell `(0, 0, 0)` is centred at `(h/2, h/2, h/2)`. A box one cell thick
    /// in some direction is a one- or two-dimensional grid.
    pub fn box_grid(
        dims: [usize; 3],
        sample_rate: f64,
        speed: f64,
        sides: [Option<Boundary>; 6],
    ) -> Result<Self, Error> {
        let h = Self::spacing_for(sample_rate, speed);
        let inside = vec![true; dims[0] * dims[1] * dims[2]];
        let mut boundaries = Vec::new();
        let mut side_index = [usize::MAX; 6];
        for (s, b) in sides.into_iter().enumerate() {
            if let Some(b) = b {
                side_index[s] = boundaries.len();
                boundaries.push(b);
            }
        }
        Self::assemble(
            Layout {
                dims,
                h,
                sample_rate,
                speed,
                origin: Vec3::new(h / 2.0, h / 2.0, h / 2.0),
            },
            &inside,
            boundaries,
            move |_centre: Vec3, direction: usize| side_index[direction],
        )
    }

    fn assemble(
        layout: Layout,
        inside: &[bool],
        boundaries: Vec<Boundary>,
        face_material: impl Fn(Vec3, usize) -> usize,
    ) -> Result<Self, Error> {
        let Layout {
            dims,
            h,
            sample_rate,
            speed,
            origin,
        } = layout;
        let flat = |x: usize, y: usize, z: usize| x + dims[0] * (y + dims[1] * z);
        let mut index = vec![OUTSIDE; inside.len()];
        let mut positions = Vec::new();
        for z in 0..dims[2] {
            for y in 0..dims[1] {
                for x in 0..dims[0] {
                    if inside[flat(x, y, z)] {
                        index[flat(x, y, z)] = positions.len() as u32;
                        positions.push([x as u32, y as u32, z as u32]);
                    }
                }
            }
        }
        if positions.is_empty() {
            return Err(Error::InvalidGeometry(
                "the room contains no wave-solver cell".into(),
            ));
        }
        let mut neighbours = Vec::with_capacity(positions.len());
        let mut interior = Vec::with_capacity(positions.len());
        let mut faces = Vec::new();
        for (cell, p) in positions.iter().enumerate() {
            let mut n = [OUTSIDE; 6];
            let mut count = 0u8;
            for (d, dir) in DIRECTIONS.iter().enumerate() {
                let q = [
                    p[0] as i64 + dir[0],
                    p[1] as i64 + dir[1],
                    p[2] as i64 + dir[2],
                ];
                if (0..3).all(|a| q[a] >= 0 && (q[a] as usize) < dims[a]) {
                    let j = index[flat(q[0] as usize, q[1] as usize, q[2] as usize)];
                    if j != OUTSIDE {
                        n[d] = j;
                        count += 1;
                        continue;
                    }
                }
                let centre = origin + Vec3::new(p[0] as f64, p[1] as f64, p[2] as f64) * h;
                let m = face_material(centre, d);
                if m < boundaries.len() && !boundaries[m].is_rigid() {
                    faces.push((cell as u32, m as u16));
                }
            }
            neighbours.push(n);
            interior.push(count);
        }
        Ok(Self {
            sample_rate,
            spacing_m: h,
            speed_of_sound: speed,
            origin,
            dims,
            index,
            positions,
            neighbours,
            interior,
            faces,
            boundaries,
        })
    }

    fn active(&self, x: i64, y: i64, z: i64) -> u32 {
        if x < 0
            || y < 0
            || z < 0
            || x as usize >= self.dims[0]
            || y as usize >= self.dims[1]
            || z as usize >= self.dims[2]
        {
            return OUTSIDE;
        }
        self.index[x as usize + self.dims[0] * (y as usize + self.dims[1] * z as usize)]
    }

    /// Trilinear weights of the active cells around `p`, renormalised over those inside.
    fn trilinear(&self, p: Vec3) -> Vec<(u32, f64)> {
        let u = (p - self.origin) * (1.0 / self.spacing_m);
        let (x0, y0, z0) = (u.x.floor(), u.y.floor(), u.z.floor());
        let (fx, fy, fz) = (u.x - x0, u.y - y0, u.z - z0);
        let mut out = Vec::with_capacity(8);
        for (dx, wx) in [(0, 1.0 - fx), (1, fx)] {
            for (dy, wy) in [(0, 1.0 - fy), (1, fy)] {
                for (dz, wz) in [(0, 1.0 - fz), (1, fz)] {
                    let w = wx * wy * wz;
                    let cell = self.active(x0 as i64 + dx, y0 as i64 + dy, z0 as i64 + dz);
                    if cell != OUTSIDE && w > 0.0 {
                        out.push((cell, w));
                    }
                }
            }
        }
        let total: f64 = out.iter().map(|(_, w)| w).sum();
        for (_, w) in &mut out {
            *w /= total;
        }
        out
    }

    /// Whether `p` and its gradient stencil lie among active cells.
    fn stencil_inside(&self, p: Vec3) -> bool {
        let h = self.spacing_m;
        [
            Vec3::default(),
            Vec3::new(h, 0.0, 0.0),
            Vec3::new(-h, 0.0, 0.0),
            Vec3::new(0.0, h, 0.0),
            Vec3::new(0.0, -h, 0.0),
            Vec3::new(0.0, 0.0, h),
            Vec3::new(0.0, 0.0, -h),
        ]
        .iter()
        .all(|&o| {
            let u = (p + o - self.origin) * (1.0 / h);
            let (x0, y0, z0) = (u.x.floor() as i64, u.y.floor() as i64, u.z.floor() as i64);
            (0..2).all(|dx| {
                (0..2).all(|dy| (0..2).all(|dz| self.active(x0 + dx, y0 + dy, z0 + dz) != OUTSIDE))
            })
        })
    }
}

struct Layout {
    dims: [usize; 3],
    h: f64,
    sample_rate: f64,
    speed: f64,
    origin: Vec3,
}

/// Distance from `p` to a polygon.
fn polygon_distance(poly: &Polygon, p: Vec3) -> f64 {
    let d = poly.signed_distance(p);
    let foot = p - poly.normal * d;
    if poly.contains_coplanar(foot) {
        return d.abs();
    }
    let n = poly.vertices.len();
    (0..n)
        .map(|i| {
            let (a, b) = (poly.vertices[i], poly.vertices[(i + 1) % n]);
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.dot(ab)).clamp(0.0, 1.0);
            (a + ab * t - p).length()
        })
        .fold(f64::INFINITY, f64::min)
}

/// Pressure and particle velocity (as `ρc·v`, along the world axes) recorded at one probe, one
/// sample per time step.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeSignal {
    pub position: Vec3,
    pub pressure: Vec<f64>,
    pub velocity: [Vec<f64>; 3],
}

/// Runs the scheme for `steps` steps, driving `source` with `excitation` (one value per step; the
/// radiated pressure is `excitation(t − r/c)/(4πr)`), and records `probes`.
pub fn run(
    grid: &WaveGrid,
    source: Vec3,
    excitation: &[f64],
    probes: &[Vec3],
    steps: usize,
    threads: usize,
) -> Result<Vec<ProbeSignal>, Error> {
    for &p in probes {
        if !grid.stencil_inside(p) {
            return Err(Error::PointOutsideRoom(format!(
                "wave-solver probe at {p:?} is within a cell of a wall"
            )));
        }
    }
    let h = grid.spacing_m;
    struct Stencil {
        centre: Vec<(u32, f64)>,
        axes: [[Vec<(u32, f64)>; 2]; 3],
        half_step: [f64; 3],
    }
    let mut stencils: Vec<Stencil> = probes
        .iter()
        .map(|&p| {
            let at = |v: Vec3| grid.trilinear(p + v);
            Stencil {
                centre: grid.trilinear(p),
                axes: [
                    [at(Vec3::new(h, 0.0, 0.0)), at(Vec3::new(-h, 0.0, 0.0))],
                    [at(Vec3::new(0.0, h, 0.0)), at(Vec3::new(0.0, -h, 0.0))],
                    [at(Vec3::new(0.0, 0.0, h)), at(Vec3::new(0.0, 0.0, -h))],
                ],
                half_step: [0.0; 3],
            }
        })
        .collect();
    let mut outputs: Vec<ProbeSignal> = probes
        .iter()
        .map(|&position| ProbeSignal {
            position,
            pressure: Vec::with_capacity(steps),
            velocity: std::array::from_fn(|_| Vec::with_capacity(steps)),
        })
        .collect();
    run_inner(grid, source, excitation, steps, threads, |p| {
        let read = |taps: &[(u32, f64)]| {
            taps.iter()
                .map(|&(c, w)| w * f64::from_bits(p[c as usize].load(Ordering::Relaxed)))
                .sum::<f64>()
        };
        for (stencil, out) in stencils.iter_mut().zip(outputs.iter_mut()) {
            out.pressure.push(read(&stencil.centre));
            for a in 0..3 {
                let gradient = read(&stencil.axes[a][0]) - read(&stencil.axes[a][1]);
                let before = stencil.half_step[a];
                stencil.half_step[a] = before - 0.5 * LAMBDA * gradient;
                out.velocity[a].push(0.5 * (before + stencil.half_step[a]));
            }
        }
    })?;
    if outputs
        .iter()
        .any(|o| o.pressure.iter().any(|v| !v.is_finite()))
    {
        return Err(Error::InvalidOption(
            "wave solver produced a non-finite sample".into(),
        ));
    }
    Ok(outputs)
}

/// One boundary branch at one face, with its update coefficients.
struct FaceBranch {
    inv_a: f64,
    mass: f64,
    resistance: f64,
    stiffness: f64,
}

/// The time loop. `observe` sees the pressure field at step `s` before it advances.
fn run_inner(
    grid: &WaveGrid,
    source: Vec3,
    excitation: &[f64],
    steps: usize,
    threads: usize,
    mut observe: impl FnMut(&[AtomicU64]),
) -> Result<(), Error> {
    if threads == 0 {
        return Err(Error::InvalidOption("threads must be positive".into()));
    }
    let source_cells = grid.trilinear(source);
    if source_cells.is_empty() {
        return Err(Error::PointOutsideRoom(format!(
            "wave-solver source at {source:?}"
        )));
    }
    let k = 1.0 / grid.sample_rate;
    let h = grid.spacing_m;
    let n = grid.cells();

    let mut branches: Vec<FaceBranch> = Vec::new();
    // (cell, first branch, end branch, Σ 1/(2k·a)), sorted by cell.
    let mut boundary_cells: Vec<(u32, usize, usize, f64)> = Vec::new();
    let mut i = 0;
    while i < grid.faces.len() {
        let cell = grid.faces[i].0;
        let start = branches.len();
        let mut d_total = 0.0;
        while i < grid.faces.len() && grid.faces[i].0 == cell {
            for b in &grid.boundaries[grid.faces[i].1 as usize].branches {
                let mass = b.mass_s / (k * k);
                let resistance = b.resistance / (2.0 * k);
                let stiffness = b.stiffness_per_s / 2.0;
                let a = mass + resistance + stiffness;
                d_total += 1.0 / (2.0 * k * a);
                branches.push(FaceBranch {
                    inv_a: 1.0 / a,
                    mass,
                    resistance,
                    stiffness,
                });
            }
            i += 1;
        }
        boundary_cells.push((cell, start, branches.len(), d_total));
    }
    let atomics = |len: usize| -> Vec<AtomicU64> { (0..len).map(|_| AtomicU64::new(0)).collect() };
    let w_now = atomics(branches.len());
    let w_prev = atomics(branches.len());
    let saved_prev = atomics(boundary_cells.len());
    let buffers: [Vec<AtomicU64>; 2] = std::array::from_fn(|_| atomics(n));
    let load = |b: &[AtomicU64], i: usize| f64::from_bits(b[i].load(Ordering::Relaxed));
    let store = |b: &[AtomicU64], i: usize, v: f64| b[i].store(v.to_bits(), Ordering::Relaxed);

    // Every worker owns a contiguous range of cells, and with it the boundary cells and source
    // cells in that range, so a whole step runs in one parallel round.
    let threads = threads
        .min(MAX_THREADS)
        .min(n.div_ceil(CELLS_PER_THREAD))
        .max(1);
    let chunk = n.div_ceil(threads);
    let ranges: Vec<std::ops::Range<usize>> = (0..threads)
        .map(|t| (t * chunk).min(n)..((t + 1) * chunk).min(n))
        .collect();
    let boundary_ranges: Vec<std::ops::Range<usize>> = ranges
        .iter()
        .map(|r| {
            let from = boundary_cells.partition_point(|b| (b.0 as usize) < r.start);
            let to = boundary_cells.partition_point(|b| (b.0 as usize) < r.end);
            from..to
        })
        .collect();
    let step = |t: usize, s: usize| {
        let (p, q) = if s.is_multiple_of(2) {
            (&buffers[0], &buffers[1])
        } else {
            (&buffers[1], &buffers[0])
        };
        let owned = boundary_ranges[t].clone();
        for b in owned.clone() {
            store(&saved_prev, b, load(q, boundary_cells[b].0 as usize));
        }
        for i in ranges[t].clone() {
            let mut sum = 0.0;
            for &j in &grid.neighbours[i] {
                if j != OUTSIDE {
                    sum += load(p, j as usize);
                }
            }
            let next =
                (2.0 - grid.interior[i] as f64 * LAMBDA2) * load(p, i) + LAMBDA2 * sum - load(q, i);
            store(q, i, next);
        }
        if let Some(&value) = excitation.get(s) {
            if value != 0.0 {
                for &(cell, w) in &source_cells {
                    let cell = cell as usize;
                    if ranges[t].contains(&cell) {
                        store(q, cell, load(q, cell) + w * LAMBDA2 * value / h);
                    }
                }
            }
        }
        let half = 0.5 * LAMBDA;
        for b in owned {
            let (cell, from, to, d_total) = boundary_cells[b];
            let cell = cell as usize;
            let prev = load(&saved_prev, b);
            let mut e_total = 0.0;
            for (j, fb) in branches.iter().enumerate().take(to).skip(from) {
                let b_term = fb.mass * (2.0 * load(&w_now, j) - load(&w_prev, j))
                    + (fb.resistance - fb.stiffness) * load(&w_prev, j);
                e_total += b_term * fb.inv_a - load(&w_prev, j);
            }
            let next =
                (load(q, cell) - half * e_total + half * d_total * prev) / (1.0 + half * d_total);
            let dp = (next - prev) / (2.0 * k);
            for (j, fb) in branches.iter().enumerate().take(to).skip(from) {
                let (now, before) = (load(&w_now, j), load(&w_prev, j));
                let b_term =
                    fb.mass * (2.0 * now - before) + (fb.resistance - fb.stiffness) * before;
                store(&w_prev, j, now);
                store(&w_now, j, (b_term + dp) * fb.inv_a);
            }
            store(q, cell, next);
        }
    };

    let step_counter = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let start = Barrier::new(threads);
    let done = Barrier::new(threads);
    std::thread::scope(|scope| {
        for t in 1..threads {
            let (start, done, stop, step_counter, step) =
                (&start, &done, &stop, &step_counter, &step);
            scope.spawn(move || {
                loop {
                    start.wait();
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    step(t, step_counter.load(Ordering::Relaxed));
                    done.wait();
                }
            });
        }
        for s in 0..steps {
            observe(if s % 2 == 0 { &buffers[0] } else { &buffers[1] });
            step_counter.store(s, Ordering::Relaxed);
            if threads > 1 {
                start.wait();
            }
            step(0, s);
            if threads > 1 {
                done.wait();
            }
        }
        if threads > 1 {
            stop.store(true, Ordering::Relaxed);
            start.wait();
        }
    });
    Ok(())
}

/// Most worker threads the scheme uses. **Chosen, measured:** on a 579,150-cell grid the rate peaked
/// at 8–16 threads (1.07 × 10⁹ cell updates per second) and fell to 0.40 × 10⁹ at 64, because every
/// step synchronises all workers.
const MAX_THREADS: usize = 16;
/// Fewest cells per worker thread, so small grids do not pay for synchronisation. **Chosen.**
const CELLS_PER_THREAD: usize = 32_768;

/// A Blackman-windowed sinc pulse: unit peak at sample `half_width`, cut off at `cutoff_hz`.
pub fn pulse(sample_rate: f64, cutoff_hz: f64, half_width: usize) -> Vec<f64> {
    use std::f64::consts::PI;
    let wc = 2.0 * cutoff_hz / sample_rate;
    (0..=2 * half_width)
        .map(|i| {
            let u = i as f64 - half_width as f64;
            let sinc = if u == 0.0 {
                1.0
            } else {
                (PI * wc * u).sin() / (PI * wc * u)
            };
            let x = u / half_width as f64;
            let window = 0.42 + 0.5 * (PI * x).cos() + 0.08 * (2.0 * PI * x).cos();
            sinc * window
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bands;
    use crate::complex::C64;
    use crate::material::Material;
    use std::f64::consts::PI;

    const C: f64 = 343.2;

    fn spectrum(x: &[f64], f: f64, fs: f64) -> C64 {
        x.iter().enumerate().fold(C64::ZERO, |acc, (n, &v)| {
            acc + C64::from_polar(v, -2.0 * PI * f * n as f64 / fs)
        })
    }

    fn cell_centre(grid: &WaveGrid, x: f64, y: f64, z: f64) -> Vec3 {
        grid.origin + Vec3::new(x, y, z) * grid.spacing_m
    }

    /// Pressure at one point, for grids too thin for a velocity stencil.
    fn pressure_at(
        grid: &WaveGrid,
        source: Vec3,
        x: &[f64],
        probe: Vec3,
        steps: usize,
    ) -> Vec<f64> {
        let taps = grid.trilinear(probe);
        let mut out = Vec::with_capacity(steps);
        run_inner(grid, source, x, steps, 1, |p| {
            out.push(
                taps.iter()
                    .map(|&(c, w)| w * f64::from_bits(p[c as usize].load(Ordering::Relaxed)))
                    .sum(),
            );
        })
        .unwrap();
        out
    }

    fn windowed(y: &[f64], centre: usize, half: usize) -> Vec<f64> {
        y.iter()
            .enumerate()
            .map(|(i, &v)| {
                let d = i as f64 - centre as f64;
                if d.abs() >= half as f64 {
                    0.0
                } else {
                    v * 0.5 * (1.0 + (PI * d / half as f64).cos())
                }
            })
            .collect()
    }

    fn fitted(alpha: [f64; 6]) -> Boundary {
        Boundary::fitted_to_absorption(&bands::from_125_to_4k(alpha)).unwrap()
    }

    /// The source radiates `s(t − r/c)/(4πr)`: at low frequency the probe's spectrum is the
    /// excitation's divided by `4πr`, before any reflection arrives.
    #[test]
    fn point_source_radiates_at_the_calibrated_level() {
        let fs = 4000.0;
        let grid = WaveGrid::box_grid([121, 121, 121], fs, C, Default::default()).unwrap();
        let source = cell_centre(&grid, 60.0, 60.0, 60.0);
        let r = 3.0;
        let probe = source + Vec3::new(r, 0.0, 0.0);
        let x = pulse(fs, 0.05 * fs, 60);
        let out = run(&grid, source, &x, &[probe], 170, 8).unwrap();
        let p = &out[0].pressure;
        for f in [50.0, 100.0, 150.0] {
            let ratio = spectrum(p, f, fs).abs() * 4.0 * PI * r / spectrum(&x, f, fs).abs();
            assert!((ratio - 1.0).abs() < 0.03, "{f} Hz: {ratio}");
        }
    }

    /// V2 (modes): a rigid box rings at the analytic mode frequencies, within the scheme's stated
    /// dispersion bound (2 % at 0.075·fs, growing as the square of frequency).
    #[test]
    fn rigid_box_modes_match_analytic_within_dispersion() {
        let fs = 2000.0;
        let dims = [21usize, 15, 10];
        let grid = WaveGrid::box_grid(dims, fs, C, Default::default()).unwrap();
        let h = grid.spacing_m;
        let size = [dims[0] as f64 * h, dims[1] as f64 * h, dims[2] as f64 * h];
        let source = cell_centre(&grid, 2.3, 3.1, 1.7);
        let probe = cell_centre(&grid, 17.6, 11.2, 7.4);
        let x = pulse(fs, 0.2 * fs, 20);
        let out = run(&grid, source, &x, &[probe], 16_000, 4).unwrap();
        let p = &out[0].pressure;
        let magnitude = |f: f64| spectrum(p, f, fs).abs();
        for (l, m, nn) in [
            (1, 0, 0),
            (0, 1, 0),
            (1, 1, 0),
            (2, 0, 0),
            (0, 0, 1),
            (1, 0, 1),
        ] {
            let analytic = C / 2.0
                * ((l as f64 / size[0]).powi(2)
                    + (m as f64 / size[1]).powi(2)
                    + (nn as f64 / size[2]).powi(2))
                .sqrt();
            let peak = (0..=300)
                .map(|i| analytic - 1.5 + i as f64 * 0.01)
                .max_by(|&a, &b| magnitude(a).partial_cmp(&magnitude(b)).unwrap())
                .unwrap();
            let bound = 0.02 * (analytic / (0.075 * fs)).powi(2) * analytic + 0.05;
            assert!(
                (peak - analytic).abs() <= bound,
                "mode ({l},{m},{nn}): {peak:.3} Hz vs {analytic:.3} Hz, bound {bound:.3}"
            );
        }
    }

    /// V2 (boundaries, normal incidence): a plane wave in a rigid one-cell duct meets the boundary
    /// at the far end; the reflection's spectrum relative to a rigid end's is the model's `R(0)`.
    #[test]
    fn normal_incidence_reflection_matches_the_model() {
        let air = Air::standard();
        let fs = 8000.0;
        let duct = |end: Option<Boundary>| {
            let grid = WaveGrid::box_grid([2400, 1, 1], fs, C, [end, None, None, None, None, None])
                .unwrap();
            let x = pulse(fs, 0.1 * fs, 40);
            let source = cell_centre(&grid, 1000.0, 0.0, 0.0);
            let probe = cell_centre(&grid, 1600.0, 0.0, 0.0);
            pressure_at(&grid, source, &x, probe, 4400)
        };
        // Incident 600 cells, back from the end face 1599 cells, each √3 samples, plus the pulse's
        // half-width.
        let arrival = ((600.0 + 1599.0) * 3f64.sqrt()).round() as usize + 40;
        let reference = windowed(&duct(None), arrival, 300);
        for model in [
            Boundary::panel(&air, 5.0, 0.1, 300.0).unwrap(),
            fitted([0.17, 0.55, 0.80, 0.90, 0.85, 0.80]),
        ] {
            let measured = windowed(&duct(Some(model.clone())), arrival, 300);
            for f in [60.0, 120.0, 200.0, 300.0, 400.0] {
                let r = spectrum(&measured, f, fs) / spectrum(&reference, f, fs);
                let expected = model.reflection(1.0, f);
                assert!(
                    (r - expected).abs() < 0.05,
                    "{:?} at {f} Hz: {r:?} vs {expected:?}",
                    model.provenance
                );
            }
        }
    }

    /// V2 (boundaries, oblique incidence): in a two-dimensional slab, the reflection off one wall at
    /// 45° relative to a rigid wall's, with the direct sound removed by a run without that wall.
    #[test]
    fn oblique_reflection_matches_the_model() {
        let fs = 8000.0;
        let model = fitted([0.10, 0.40, 0.70, 0.85, 0.85, 0.85]);
        let slab = |ny: usize, y0: f64, wall: Option<Boundary>| {
            let grid =
                WaveGrid::box_grid([420, ny, 1], fs, C, [None, None, None, wall, None, None])
                    .unwrap();
            let x = pulse(fs, 0.1 * fs, 40);
            let source = cell_centre(&grid, 110.0, y0, 0.0);
            let probe = cell_centre(&grid, 231.0, y0, 0.0);
            pressure_at(&grid, source, &x, probe, 520)
        };
        let free = slab(480, 300.0, None);
        let rigid = slab(240, 60.0, None);
        let absorbing = slab(240, 60.0, Some(model.clone()));
        let path = (121f64.powi(2) + 121f64.powi(2)).sqrt();
        let arrival = (path * 3f64.sqrt()).round() as usize + 40;
        let minus_free =
            |a: &[f64]| -> Vec<f64> { a.iter().zip(&free).map(|(x, y)| x - y).collect() };
        let reference = windowed(&minus_free(&rigid), arrival, 70);
        let measured = windowed(&minus_free(&absorbing), arrival, 70);
        let cos = std::f64::consts::FRAC_1_SQRT_2;
        for f in [150.0, 250.0, 350.0] {
            let r = spectrum(&measured, f, fs) / spectrum(&reference, f, fs);
            let expected = model.reflection(cos, f);
            assert!((r - expected).abs() < 0.08, "{f} Hz: {r:?} vs {expected:?}");
        }
    }

    /// V2 (no boundary adds energy): a small room with a resonant panel and fitted porous walls,
    /// excited once, only decays.
    #[test]
    fn boundaries_never_add_energy() {
        let air = Air::standard();
        let fs = 8000.0;
        let grid = WaveGrid::box_grid(
            [12, 10, 8],
            fs,
            C,
            [
                Some(Boundary::panel(&air, 3.0, 0.05, 50.0).unwrap()),
                None,
                None,
                Some(fitted([0.17, 0.55, 0.80, 0.90, 0.85, 0.80])),
                Some(fitted([0.02, 0.06, 0.14, 0.37, 0.60, 0.65])),
                None,
            ],
        )
        .unwrap();
        let mut rng = crate::rng::SplitMix64::new(5);
        let excitation: Vec<f64> = (0..200).map(|_| rng.next_normal()).collect();
        let source = cell_centre(&grid, 4.2, 3.7, 2.9);
        let probe = cell_centre(&grid, 7.3, 5.6, 4.1);
        let out = run(&grid, source, &excitation, &[probe], 40_000, 3).unwrap();
        let p = &out[0].pressure;
        let rms = |a: usize, b: usize| {
            (p[a..b].iter().map(|v| v * v).sum::<f64>() / (b - a) as f64).sqrt()
        };
        let mut previous = rms(200, 2200);
        for w in 1..9 {
            let now = rms(200 + w * 4000, 2200 + w * 4000);
            assert!(now < previous, "window {w}: rms {now} after {previous}");
            previous = now;
        }
    }

    #[test]
    fn identical_at_any_thread_count() {
        let air = Air::standard();
        let grid = WaveGrid::box_grid(
            [30, 20, 10],
            8000.0,
            C,
            [
                Some(Boundary::panel(&air, 3.0, 0.05, 50.0).unwrap()),
                None,
                Some(fitted([0.17, 0.55, 0.80, 0.90, 0.85, 0.80])),
                None,
                None,
                None,
            ],
        )
        .unwrap();
        let x = pulse(8000.0, 800.0, 20);
        let source = cell_centre(&grid, 5.5, 5.5, 5.5);
        let probe = cell_centre(&grid, 20.2, 12.7, 4.3);
        let one = run(&grid, source, &x, &[probe], 400, 1).unwrap();
        let many = run(&grid, source, &x, &[probe], 400, 5).unwrap();
        assert!(
            one[0]
                .pressure
                .iter()
                .zip(&many[0].pressure)
                .all(|(a, b)| a.to_bits() == b.to_bits())
        );
    }

    #[test]
    fn voxelised_l_room_has_its_volume_and_materials() {
        let plan = [
            (0.0, 0.0),
            (6.0, 0.0),
            (6.0, 3.0),
            (3.0, 3.0),
            (3.0, 6.0),
            (0.0, 6.0),
        ];
        let floor =
            Material::from_125_to_4k("carpet", [0.02, 0.06, 0.14, 0.37, 0.60, 0.65], [0.1; 6])
                .unwrap();
        let room =
            Room::extruded(&plan, 2.5, [floor, Material::rigid(), Material::rigid()]).unwrap();
        let grid = WaveGrid::from_room(&room, &Air::standard(), 4000.0).unwrap();
        let volume = grid.cells() as f64 * grid.spacing_m.powi(3);
        assert!(
            (volume / room.volume() - 1.0).abs() < 0.08,
            "{volume} vs {}",
            room.volume()
        );
        // Only the carpet floor is non-rigid, so every boundary face is a floor face.
        assert!(!grid.faces.is_empty());
        for &(cell, m) in &grid.faces {
            assert_eq!(m, 0);
            assert_eq!(
                grid.positions[cell as usize][2], 0,
                "a non-floor cell got the carpet"
            );
        }
    }
}
