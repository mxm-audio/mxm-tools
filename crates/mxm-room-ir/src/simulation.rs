//! The hybrid: image sources, rays, ray-assisted image sources, edge diffraction and the wave
//! solver for one scene, and rendering a set of simulations through one array.
//!
//! **The time seam is by order, not by a time cut.** Image sources deliver every specular path up
//! to their order, exhaustively; the ray tracer delivers everything else (research page §7, *No
//! double counting*). In a non-diffuse scene, specular paths past that order stay discrete too,
//! found by rays. Image sources are never cut by a length shorter than the rays' reach, or the two
//! would disagree about what "up to the order" means.
//!
//! **The frequency seam** (plan §4.2, revision 10). When a scene is wave-solved, the wave solver
//! carries everything below a transition band and the geometric solvers everything above it, and
//! the renderer crosses them over causally. The seam's default is the Schroeder frequency
//! `2000·√(T/V)` (research page §3), with `T` from Eyring at 500 Hz–1 kHz; **chosen:** it is clamped
//! to 60–1,000 Hz, and the transition band is half an octave either side of it. The solver runs at
//! the band's top frequency divided by [`WaveOptions::accuracy`].
//!
//! The wave solver records at each position the scene declares ([`WaveOptions::probe_offsets`]),
//! because its probes are fixed when it runs (plan §4.3).

use crate::bands::Bands;
use crate::boundary::Boundary;
use crate::diffraction::{self, DiffractedPath, DiffractionOptions};
use crate::directivity::{Directivity, PlacedCapsule};
use crate::error::Error;
use crate::fdtd::{self, ProbeSignal, WaveGrid};
use crate::geometry::Vec3;
use crate::ism::{self, Arrival, ImageSourceOptions};
use crate::par::parallel_map;
use crate::rays::{self, RayOptions, RayTrace};
use crate::render::{RenderOptions, Rendered, SourceField, render_fields};
use crate::scene::Scene;

#[derive(Debug, Clone, PartialEq)]
pub struct WaveOptions {
    /// Centre of the transition band, Hz. `None` takes the Schroeder frequency (module note).
    pub seam_hz: Option<f64>,
    /// The transition band's top frequency as a fraction of the solver's rate. 0.03 keeps the
    /// scheme's phase-velocity error near 0.3 % there.
    pub accuracy: f64,
    /// Where to record, relative to the receiver, in world coordinates: every capsule offset of
    /// every array the scene will be rendered through.
    pub probe_offsets: Vec<Vec3>,
}

impl Default for WaveOptions {
    fn default() -> Self {
        Self {
            seam_hz: None,
            accuracy: 0.03,
            probe_offsets: vec![Vec3::default()],
        }
    }
}

impl WaveOptions {
    /// Options recording at every capsule of `arrays` (placed capsules, world offsets).
    pub fn for_arrays(arrays: &[&[PlacedCapsule]]) -> Self {
        let mut offsets: Vec<Vec3> = Vec::new();
        for c in arrays.iter().flat_map(|a| a.iter()) {
            if !offsets.iter().any(|o| (*o - c.offset).length() < 1e-9) {
                offsets.push(c.offset);
            }
        }
        Self {
            probe_offsets: offsets,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SimulationOptions {
    pub image_sources: ImageSourceOptions,
    pub rays: RayOptions,
    /// First-order edge diffraction, when set.
    pub diffraction: Option<DiffractionOptions>,
    /// The wave solver below the frequency seam, when set.
    pub wave: Option<WaveOptions>,
}

/// What the wave solver produced for one scene.
#[derive(Debug, Clone, PartialEq)]
pub struct WaveField {
    pub seam_hz: f64,
    /// Transition band, Hz: the wave solver alone below the first, geometry alone above the second.
    pub transition_hz: (f64, f64),
    pub sample_rate: f64,
    pub spacing_m: f64,
    pub cells: usize,
    pub source: Vec3,
    /// The excitation is a unit-sum windowed sinc of this half-width (samples) and normalised
    /// cutoff `2·f_c/fs`, scaled by `4π`, so the radiated pressure is the unit-sum pulse over `r`.
    pub excitation_half_width: usize,
    pub excitation_cutoff: f64,
    pub excitation_sum: f64,
    pub probes: Vec<ProbeSignal>,
    /// Boundary models, one per material, shared with the image sources' wall reflections.
    pub boundaries: Vec<Boundary>,
    /// Boundary index for each polygon.
    pub face_boundary: Vec<usize>,
    /// Each polygon's band scattering, which the image sources' specular legs still carry.
    pub face_scattering: Vec<Bands>,
}

impl WaveField {
    /// The unit-sum excitation pulse at `u` samples from its centre (fractional).
    pub fn excitation_at(&self, u: f64) -> f64 {
        use std::f64::consts::PI;
        let half = self.excitation_half_width as f64;
        if u.abs() >= half {
            return 0.0;
        }
        let wc = self.excitation_cutoff;
        let sinc = if u == 0.0 {
            1.0
        } else {
            (PI * wc * u).sin() / (PI * wc * u)
        };
        let x = u / half;
        let window = 0.42 + 0.5 * (PI * x).cos() + 0.08 * (2.0 * PI * x).cos();
        sinc * window / self.excitation_sum
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Simulation {
    pub scene: Scene,
    pub options: SimulationOptions,
    /// Image sources and ray-assisted image sources, by path length then face sequence.
    pub arrivals: Vec<Arrival>,
    pub trace: RayTrace,
    pub image_source_arrivals: usize,
    pub ray_assisted_arrivals: usize,
    pub diffracted: Vec<DiffractedPath>,
    pub wave: Option<WaveField>,
}

impl Simulation {
    pub fn field(&self) -> SourceField<'_> {
        SourceField {
            arrivals: &self.arrivals,
            directivity: &self.scene.source_directivity,
            histogram: Some(&self.trace.histogram),
            volume_m3: self.scene.room.volume(),
            receiver: self.scene.receiver,
            diffracted: &self.diffracted,
            wave: self.wave.as_ref(),
            seed: self.trace.seed,
        }
    }

    /// Energy of the ray-assisted arrivals at band `k`, walls only: the quantity whose convergence
    /// with ray count shows the specular search is complete (V10).
    pub fn ray_assisted_energy(&self, band: usize) -> f64 {
        self.arrivals
            .iter()
            .filter(|a| a.order() > self.options.image_sources.max_order)
            .map(|a| (a.reflection[band] / a.path_length_m).powi(2))
            .sum()
    }
}

pub fn simulate(scene: &Scene, options: &SimulationOptions) -> Result<Simulation, Error> {
    let reach = options.rays.max_time_s * scene.air.speed_of_sound();
    let image_options = ImageSourceOptions {
        max_path_length_m: options.image_sources.max_path_length_m.max(reach),
        ..options.image_sources
    };
    let mut arrivals = ism::image_sources(scene, &image_options)?;
    let image_source_arrivals = arrivals.len();
    let trace = rays::trace(scene, image_options.max_order, &options.rays)?;
    let found = parallel_map(&trace.specular_sequences, options.rays.threads, |faces| {
        ism::arrival_for_sequence(scene, faces)
    });
    let before = arrivals.len();
    arrivals.extend(found.into_iter().flatten());
    let ray_assisted_arrivals = arrivals.len() - before;
    arrivals.sort_by(|a, b| {
        a.path_length_m
            .partial_cmp(&b.path_length_m)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.faces.cmp(&b.faces))
    });
    let diffracted = options
        .diffraction
        .as_ref()
        .map_or_else(Vec::new, |d| diffraction::diffracted_paths(scene, d));
    let wave = options
        .wave
        .as_ref()
        .map(|w| wave_field(scene, &trace, w, options.rays.threads))
        .transpose()?;
    Ok(Simulation {
        scene: scene.clone(),
        options: SimulationOptions {
            image_sources: image_options,
            ..options.clone()
        },
        arrivals,
        trace,
        image_source_arrivals,
        ray_assisted_arrivals,
        diffracted,
        wave,
    })
}

/// The Schroeder frequency from an Eyring estimate at 500 Hz–1 kHz, clamped to 60–1,000 Hz.
pub fn schroeder_seam(scene: &Scene) -> f64 {
    let room = &scene.room;
    let (volume, surface) = (room.volume(), room.surface_area());
    let mean_absorption = room
        .polygons
        .iter()
        .map(|p| {
            let a = &room.materials[p.material].absorption;
            p.area() * 0.5 * (a[3] + a[4])
        })
        .sum::<f64>()
        / surface;
    let c = scene.air.speed_of_sound();
    let t = 24.0 * std::f64::consts::LN_10 * volume
        / (c * surface * -(1.0 - mean_absorption.clamp(1e-4, 0.99)).ln());
    (2000.0 * (t / volume).sqrt()).clamp(60.0, 1_000.0)
}

fn wave_field(
    scene: &Scene,
    trace: &RayTrace,
    options: &WaveOptions,
    threads: usize,
) -> Result<WaveField, Error> {
    if !matches!(scene.source_directivity, Directivity::Omni) {
        return Err(Error::InvalidOption(
            "the wave solver's source is omnidirectional; a directional source cannot be wave-solved"
                .into(),
        ));
    }
    if !(options.accuracy > 0.0 && options.accuracy <= 0.075) {
        return Err(Error::InvalidOption(
            "wave accuracy must lie in (0, 0.075], the scheme's 2 % band".into(),
        ));
    }
    let seam = options.seam_hz.unwrap_or_else(|| schroeder_seam(scene));
    if seam.is_nan() || seam <= 0.0 {
        return Err(Error::InvalidOption("seam must be positive".into()));
    }
    let transition = (
        seam / std::f64::consts::SQRT_2,
        seam * std::f64::consts::SQRT_2,
    );
    let fs = transition.1 / options.accuracy;
    let grid = WaveGrid::from_room(&scene.room, &scene.air, fs)?;
    let half = 40;
    let cutoff = 0.1 * fs;
    let raw = fdtd::pulse(fs, cutoff, half);
    let sum: f64 = raw.iter().sum();
    let excitation: Vec<f64> = raw
        .iter()
        .map(|v| 4.0 * std::f64::consts::PI * v / sum)
        .collect();
    let duration = (trace.histogram.bins() as f64 * trace.histogram.bin_width_s).max(0.5);
    let steps = (duration * fs).ceil() as usize + 2 * half;
    let positions: Vec<Vec3> = options
        .probe_offsets
        .iter()
        .map(|o| scene.receiver + *o)
        .collect();
    let probes = fdtd::run(&grid, scene.source, &excitation, &positions, steps, threads)?;
    Ok(WaveField {
        seam_hz: seam,
        transition_hz: transition,
        sample_rate: fs,
        spacing_m: grid.spacing_m,
        cells: grid.cells(),
        source: scene.source,
        excitation_half_width: half,
        excitation_cutoff: 2.0 * cutoff / fs,
        excitation_sum: sum,
        probes,
        boundaries: grid.boundaries.clone(),
        face_boundary: scene.room.polygons.iter().map(|p| p.material).collect(),
        face_scattering: scene
            .room
            .polygons
            .iter()
            .map(|p| scene.room.materials[p.material].scattering)
            .collect(),
    })
}

/// Renders simulations of one room and receiver position, one per source, through one array.
/// They share time zero, leading silence and length (plan §4.5).
pub fn render_set(
    simulations: &[&Simulation],
    capsules: &[PlacedCapsule],
    options: &RenderOptions,
) -> Result<Vec<Rendered>, Error> {
    check_set(simulations)?;
    let fields: Vec<SourceField> = simulations.iter().map(|s| s.field()).collect();
    render_fields(&fields, &simulations[0].scene.air, capsules, options)
}

pub(crate) fn check_set(simulations: &[&Simulation]) -> Result<(), Error> {
    let Some(first) = simulations.first() else {
        return Err(Error::InvalidOption("nothing to render".into()));
    };
    for s in simulations {
        if s.scene.room != first.scene.room
            || s.scene.receiver != first.scene.receiver
            || s.scene.air != first.scene.air
        {
            return Err(Error::InvalidOption(
                "a set shares its room, air and receiver position".into(),
            ));
        }
    }
    Ok(())
}
