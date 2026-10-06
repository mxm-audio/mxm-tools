//! Rendering: simulated fields into sampled impulse responses, for any array of capsules.
//!
//! **Early part.** Each discrete arrival (image source or ray-assisted image source) is
//! band-limited and fractionally delayed, and passes through a filter carrying its band gains
//! (walls × air × source directivity). The filter is minimum-phase, built from those band gains by
//! real-cepstrum folding ([`crate::fft::minimum_phase_fir`]). Onsets stay sharp: a minimum-phase
//! filter puts nothing before its first tap. **In a wave-solved scene** each wall instead
//! contributes its boundary model's reflection coefficient at the arrival's incidence angle, with
//! phase, so the early field agrees with the wave solver's (plan §4.3). Scattering, air and source
//! directivity stay minimum-phase. Edge-diffracted nodes are area-sampled onto the output grid
//! ([`crate::diffraction`]).
//!
//! **Late part.** The ray histogram becomes Poisson events ([`crate::late`]).
//!
//! **Wave part.** The wave solver's probe signals are combined into each capsule's pattern,
//! stripped of the free-field direct sound when the direct path is excluded, blocked below
//! [`DC_BLOCK_HZ`], faded out over the last [`WAVE_END_FADE_S`] of the solver's run so it never
//! stops mid-swing, and resampled to the output rate with a windowed sinc. A wave-solved render's
//! merged output then passes a sixth-order highpass at [`SUB_AUDIO_HZ`]: in a room of a few
//! cubic metres the fitted boundaries against the air's stiffness make a whole-room mode near
//! 7 Hz, which the source's net volume excites and the drift block barely touches (plan revision
//! 13). It acts on the merged output, so the wave and geometric parts keep their relative phase.
//!
//! **Crossover** (plan revision 10). The wave part passes a minimum-phase lowpass. The early part
//! passes its exact complement, the impulse minus that lowpass, so phase-aligned early fields sum
//! to themselves. The late part passes a minimum-phase highpass whose power complements the
//! lowpass, as uncorrelated tails need. All three are causal.
//!
//! **Capsules.** Every arrival and every late event reaches each capsule as a plane wave from its
//! direction, delayed by `−offset·u/c` from the array centre and weighted by the capsule's
//! directivity (plan §4.3). The wave solver records at each capsule's own position.
//!
//! **Chosen, and decided here:**
//! - The fractional delay is a Blackman-windowed sinc with half-width [`KERNEL_HALF_WIDTH`]
//!   samples, cut off at Nyquist. At an integer position it is exactly a unit impulse.
//! - The minimum-phase design grid is at least 4096 bins and about 6 Hz per bin; a wall's
//!   phase-carrying filter uses 16,384 bins. Filters are a quarter (phase-carrying: half) of the grid
//!   long, faded over their last quarter.
//! - The crossover lowpass is 1 below the transition band, 0 above it, and a raised cosine in
//!   log-frequency between.
//! - **Time zero** is the earliest arrival (image source or diffracted node) over every source and
//!   capsule of a set, placed exactly on a sample. Every other arrival keeps its true delay after
//!   it, so inter-channel and inter-source delays survive. When anything would ring before time
//!   zero, every channel of the set gains the same `lead_samples` of leading silence. The wave part
//!   is gated at time zero: before it the solver holds only the excitation's symmetric precursor.
//! - **Level** is normalized by default: each rendered file is scaled so its loudest sample peaks at
//!   [`DEFAULT_PEAK_DBFS`] (owner, 2026-09-15: the output is material for a creative tool, whose
//!   plugin or DAW sets the level), and [`Rendered::gain`] records the scale. With
//!   [`RenderOptions::normalize_peak_dbfs`] `None` it is physical: pressure
//!   `reference_distance_m / path_length`, so the direct sound at the reference distance has unit
//!   amplitude with air absorption off.
//! - A band-dependent capsule pattern whose gain changes sign between bands is rendered with the
//!   sign of its strongest band. No catalogue capsule has one, and a wave-solved scene refuses one.
//!
//! Accumulation is serial and in a fixed order, whatever the thread count, so the output is
//! bit-identical for a given input on a given platform.

use std::f64::consts::PI;

use crate::air::Air;
use crate::bands::{self, Bands, NUM_BANDS};
use crate::complex::C64;
use crate::diffraction::DiffractedPath;
use crate::directivity::{Directivity, PlacedCapsule};
use crate::error::Error;
use crate::fft::{self, Complex};
use crate::geometry::Vec3;
use crate::ism::{self, Arrival};
use crate::late::{self, LateOptions};
use crate::par::parallel_map;
use crate::rays::Histogram;
use crate::simulation::WaveField;

/// Half-width of the windowed-sinc fractional-delay kernel, samples.
pub const KERNEL_HALF_WIDTH: usize = 32;
/// Half-width of the sinc that resamples the wave solver, in solver samples.
const WAVE_KERNEL_HALF_WIDTH: usize = 16;
/// Corner of the second-order highpass that removes the wave solver's drift, Hz. A source whose
/// free-field response is a pressure impulse injects net volume, and a closed room's pressure
/// then ramps; the geometric solvers have no such term. **Chosen.**
pub const DC_BLOCK_HZ: f64 = 10.0;
/// The peak a rendered file is normalized to by default, dBFS (owner, 2026-09-15). **Chosen:** 1 dB
/// of headroom against inter-sample overs.
pub const DEFAULT_PEAK_DBFS: f64 = -1.0;
/// Corner of the sixth-order Butterworth highpass on a wave-solved render's merged output, Hz.
/// It takes under 0.01 dB off 63 Hz and 0.14 dB off 40 Hz, and about 48 dB off 12 Hz and 76 dB
/// off 7 Hz. A fourth-order filter at 20 Hz left a car cabin's 7 Hz mode 25 dB under its energy.
/// **Chosen.**
pub const SUB_AUDIO_HZ: f64 = 30.0;
/// The wave part fades out over this much of the end of the solver's run, s. **Chosen.**
pub const WAVE_END_FADE_S: f64 = 0.05;
/// Bins of a wall's phase-carrying filter design.
const PHASE_GRID: usize = 16_384;

/// An end imposed on a file: its length and the fade that ends it, both s.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cap {
    pub length_s: f64,
    pub fade_s: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderOptions {
    pub sample_rate: u32,
    /// Distance at which the direct sound has unit pressure (before air absorption), m.
    pub reference_distance_m: f64,
    /// Whether the direct path is rendered. Time zero stays at the earliest arrival either way.
    pub include_direct: bool,
    /// Whether air absorption is applied.
    pub air_absorption: bool,
    /// Worker threads. The result does not depend on it.
    pub threads: usize,
    pub late: LateOptions,
    /// A longer response ends at the cap with a fade the generator applies. `None`: uncapped.
    pub cap: Option<Cap>,
    /// Each rendered file is scaled so its loudest sample peaks here, dBFS. `None` keeps the physical
    /// level, which tests of level and the benchmark need.
    pub normalize_peak_dbfs: Option<f64>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            reference_distance_m: 1.0,
            include_direct: true,
            air_absorption: true,
            threads: std::thread::available_parallelism().map_or(1, |n| n.get()),
            late: LateOptions::default(),
            cap: None,
            normalize_peak_dbfs: Some(DEFAULT_PEAK_DBFS),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    pub channels: Vec<Vec<f32>>,
    pub channel_names: Vec<String>,
    pub sample_rate: u32,
    /// Leading silence added so nothing rings before time zero.
    pub lead_samples: usize,
    /// Path length that defines time zero (earliest arrival, array-centre offset applied), m.
    pub origin_path_length_m: f64,
    /// Propagation delay trimmed from the front, s.
    pub trimmed_delay_s: f64,
    pub direct_included: bool,
    pub arrivals_rendered: usize,
    pub late_events: usize,
    /// The transition band the wave solver was crossed over in, Hz, when there was one.
    pub crossover_hz: Option<(f64, f64)>,
    /// The cap that ended the file, when it did.
    pub capped: Option<Cap>,
    /// The linear gain normalization applied; 1 for a physical render. The physical reference
    /// distance times this gain is the distance at which the direct sound has pressure 1.
    pub gain: f64,
}

impl Rendered {
    /// The first channel: the whole response for a mono render.
    pub fn samples(&self) -> &[f32] {
        &self.channels[0]
    }

    pub fn duration_s(&self) -> f64 {
        self.channels[0].len() as f64 / f64::from(self.sample_rate)
    }
}

/// What one source contributes at one receiver position.
#[derive(Debug, Clone, Copy)]
pub struct SourceField<'a> {
    pub arrivals: &'a [Arrival],
    pub directivity: &'a Directivity,
    pub histogram: Option<&'a Histogram>,
    pub volume_m3: f64,
    /// The receiver position (the array centre).
    pub receiver: Vec3,
    pub diffracted: &'a [DiffractedPath],
    pub wave: Option<&'a WaveField>,
    /// Seeds this field's late synthesis, mixed with the options' seed: a simulation passes its ray
    /// seed, which hashes the scene, so no two scenes share an event sequence (plan §4.6).
    pub seed: u64,
}

/// Renders arrivals for an omni receiver, with no late part.
pub fn render_omni(
    arrivals: &[Arrival],
    air: &Air,
    options: &RenderOptions,
) -> Result<Rendered, Error> {
    let field = SourceField {
        arrivals,
        directivity: &Directivity::Omni,
        histogram: None,
        volume_m3: 1.0,
        receiver: Vec3::default(),
        diffracted: &[],
        wave: None,
        seed: 0,
    };
    let capsule = PlacedCapsule {
        name: "omni".into(),
        offset: Vec3::default(),
        directivity: Directivity::Omni,
    };
    Ok(render_fields(&[field], air, &[capsule], options)?.remove(0))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Early,
    Late,
    Wave,
}

struct Contribution {
    field: usize,
    channel: usize,
    kind: Kind,
    start: i64,
    samples: Vec<f64>,
}

/// The parts of a set before the crossover: per field, per channel, early (image sources and
/// diffraction), late and wave signals, on one grid with one lead.
#[derive(Debug, Clone, PartialEq)]
pub struct Parts {
    pub lead_samples: usize,
    pub origin_path_length_m: f64,
    pub length: usize,
    pub fields: Vec<FieldParts>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldParts {
    pub early: Vec<Vec<f64>>,
    pub late: Vec<Vec<f64>>,
    pub wave: Option<Vec<Vec<f64>>>,
    pub crossover_hz: Option<(f64, f64)>,
    pub arrivals_rendered: usize,
    pub late_events: usize,
}

/// Renders several sources heard by one array, sharing time zero, leading silence and length.
/// Returns one [`Rendered`] per source, each with one channel per capsule.
pub fn render_fields(
    fields: &[SourceField],
    air: &Air,
    capsules: &[PlacedCapsule],
    options: &RenderOptions,
) -> Result<Vec<Rendered>, Error> {
    let parts = render_parts(fields, air, capsules, options)?;
    let fs = f64::from(options.sample_rate);
    let c = air.speed_of_sound();
    let mut crossovers: Vec<((f64, f64), Crossover)> = Vec::new();
    let mut merged: Vec<Vec<Vec<f64>>> = Vec::with_capacity(parts.fields.len());
    for field in &parts.fields {
        let mut channels = Vec::with_capacity(capsules.len());
        for ch in 0..capsules.len() {
            let mut buffer = vec![0.0; parts.length];
            match (&field.wave, field.crossover_hz) {
                (Some(wave), Some(band)) => {
                    if !crossovers.iter().any(|(b, _)| *b == band) {
                        crossovers.push((band, Crossover::design(band, fs)));
                    }
                    let x = &crossovers.iter().find(|(b, _)| *b == band).unwrap().1;
                    let early_low = fft_convolve(&field.early[ch], &x.lowpass, parts.length);
                    let late_high = fft_convolve(&field.late[ch], &x.power_highpass, parts.length);
                    let wave_low = fft_convolve(&wave[ch], &x.lowpass, parts.length);
                    for (i, slot) in buffer.iter_mut().enumerate() {
                        *slot = (field.early[ch][i] - early_low[i]) + late_high[i] + wave_low[i];
                    }
                    for q in BUTTERWORTH_6TH {
                        highpass(&mut buffer, SUB_AUDIO_HZ, q, fs);
                    }
                }
                _ => {
                    for ((slot, e), l) in
                        buffer.iter_mut().zip(&field.early[ch]).zip(&field.late[ch])
                    {
                        *slot = e + l;
                    }
                }
            }
            channels.push(buffer);
        }
        merged.push(channels);
    }

    let mut length = parts.length;
    let mut capped = None;
    if let Some(cap) = options.cap {
        let limit = (cap.length_s * fs).round() as usize;
        if length > limit {
            length = limit;
            capped = Some(cap);
        }
    }
    let fade = capped.map(|cap| ((cap.fade_s * fs).round() as usize).clamp(1, length));
    let mut out = Vec::with_capacity(fields.len());
    for (fi, channels_f64) in merged.into_iter().enumerate() {
        let mut channels = Vec::with_capacity(capsules.len());
        for mut buffer in channels_f64 {
            buffer.truncate(length);
            if buffer.iter().any(|v| !v.is_finite()) {
                return Err(Error::InvalidOption(
                    "render produced a non-finite sample".into(),
                ));
            }
            if let Some(fade) = fade {
                let start = length - fade;
                for (i, v) in buffer[start..].iter_mut().enumerate() {
                    let x = (i + 1) as f64 / fade as f64;
                    *v *= 0.5 * (1.0 + (PI * x).cos());
                }
            }
            let mut samples: Vec<f32> = buffer.iter().map(|&v| v as f32).collect();
            *samples.last_mut().expect("length is at least one") = 0.0;
            channels.push(samples);
        }
        // Normalization scales the finished file, so its exact final zero and causality stay.
        let peak = channels.iter().flatten().fold(0f32, |m, v| m.max(v.abs()));
        let gain = match options.normalize_peak_dbfs {
            Some(peak_dbfs) if peak > 0.0 => {
                let gain = 10f64.powf(peak_dbfs / 20.0) / f64::from(peak);
                for v in channels.iter_mut().flatten() {
                    *v = (f64::from(*v) * gain) as f32;
                }
                gain
            }
            _ => 1.0,
        };
        let field = &parts.fields[fi];
        out.push(Rendered {
            channels,
            channel_names: capsules.iter().map(|c| c.name.clone()).collect(),
            sample_rate: options.sample_rate,
            lead_samples: parts.lead_samples,
            origin_path_length_m: parts.origin_path_length_m,
            trimmed_delay_s: parts.origin_path_length_m / c,
            direct_included: options.include_direct,
            arrivals_rendered: field.arrivals_rendered,
            late_events: field.late_events,
            crossover_hz: field.crossover_hz,
            capped,
            gain,
        });
    }
    Ok(out)
}

/// The parts of a render before the crossover merges them. [`render_fields`] merges these; tests
/// of solver agreement read them directly.
pub fn render_parts(
    fields: &[SourceField],
    air: &Air,
    capsules: &[PlacedCapsule],
    options: &RenderOptions,
) -> Result<Parts, Error> {
    let reference = options.reference_distance_m;
    if options.normalize_peak_dbfs.is_some_and(|p| !p.is_finite()) {
        return Err(Error::InvalidOption(
            "a normalization peak must be finite".into(),
        ));
    }
    if options.sample_rate == 0 || reference.is_nan() || reference <= 0.0 || options.threads == 0 {
        return Err(Error::InvalidOption(
            "sample rate, reference distance and threads must be positive".into(),
        ));
    }
    if fields.is_empty() || capsules.is_empty() {
        return Err(Error::InvalidOption(
            "a render needs a source and a capsule".into(),
        ));
    }
    if let Some(cap) = options.cap {
        if !(cap.length_s > 0.0 && cap.fade_s > 0.0 && cap.fade_s < cap.length_s) {
            return Err(Error::InvalidOption(
                "a cap needs a positive length and a shorter fade".into(),
            ));
        }
    }
    let any_wave = fields.iter().any(|f| f.wave.is_some());
    if any_wave
        && capsules.iter().any(|c| {
            matches!(
                c.directivity,
                Directivity::BandFirstOrder { .. } | Directivity::Tabulated { .. }
            )
        })
    {
        return Err(Error::InvalidOption(
            "a wave-solved render takes omni or first-order capsules only".into(),
        ));
    }
    let fs = f64::from(options.sample_rate);
    let c = air.speed_of_sound();
    let geometric_origin = fields
        .iter()
        .flat_map(|f| f.arrivals.iter())
        .flat_map(|a| {
            capsules
                .iter()
                .map(move |cap| a.path_length_m - cap.offset.dot(a.direction))
        })
        .fold(f64::INFINITY, f64::min);
    let diffracted_origin = fields
        .iter()
        .flat_map(|f| f.diffracted.iter())
        .flat_map(|p| p.nodes.iter())
        .flat_map(|n| {
            capsules
                .iter()
                .map(move |cap| n.path_length_m - cap.offset.dot(n.direction))
        })
        .fold(f64::INFINITY, f64::min);
    let origin = geometric_origin.min(diffracted_origin);
    if !origin.is_finite() {
        return Err(Error::InvalidOption(
            "nothing to render: no arrivals".into(),
        ));
    }

    // Early part: shared minimum-phase filters by band-gain magnitude, and phase-carrying wall
    // filters per arrival in wave-solved fields.
    struct Pair {
        field: usize,
        arrival: usize,
        channel: usize,
        filter: FilterRef,
        scale: f64,
    }
    #[derive(Clone, Copy)]
    enum FilterRef {
        Flat,
        Shared(usize),
        Phase(usize),
    }
    let mut unique: Vec<Bands> = Vec::new();
    let mut keys: Vec<[i64; NUM_BANDS]> = Vec::new();
    let mut intern = |g: &Bands| -> (FilterRef, f64) {
        let strongest = g
            .iter()
            .copied()
            .fold(0.0f64, |m, v| if v.abs() > m.abs() { v } else { m });
        let sign = if strongest < 0.0 { -1.0 } else { 1.0 };
        let magnitude = g.map(f64::abs);
        if magnitude.iter().all(|&v| v == magnitude[0]) {
            return (FilterRef::Flat, sign * magnitude[0]);
        }
        let key = magnitude.map(|v| (v * 1e9).round() as i64);
        let index = match keys.iter().position(|k| *k == key) {
            Some(i) => i,
            None => {
                keys.push(key);
                unique.push(magnitude);
                unique.len() - 1
            }
        };
        (FilterRef::Shared(index), sign)
    };
    let mut pairs = Vec::new();
    let mut phase_jobs: Vec<(usize, usize, Bands)> = Vec::new();
    let mut arrivals_rendered = vec![0usize; fields.len()];
    for (fi, field) in fields.iter().enumerate() {
        for (ai, a) in field.arrivals.iter().enumerate() {
            if !(options.include_direct || a.order() > 0) || ism::is_silent(a) {
                continue;
            }
            let air_gain = if options.air_absorption {
                air.band_pressure_gain(a.path_length_m)
            } else {
                [1.0; NUM_BANDS]
            };
            let directivity = field.directivity.band_gains(a.departure);
            let phase = field.wave.is_some() && a.order() > 0;
            let g = if phase {
                // Walls come from the boundary models; scattering, air and directivity stay here.
                let s = specular_scattering(a, field);
                bands::mul(&bands::mul(&s, &air_gain), &directivity)
            } else {
                bands::mul(&bands::mul(&a.reflection, &air_gain), &directivity)
            };
            if g.iter().all(|&v| v == 0.0) {
                continue;
            }
            arrivals_rendered[fi] += 1;
            let phase_index = if phase {
                phase_jobs.push((fi, ai, g));
                Some(phase_jobs.len() - 1)
            } else {
                None
            };
            for (ci, capsule) in capsules.iter().enumerate() {
                let cg = capsule.directivity.band_gains(a.direction);
                let (filter, scale) = if let Some(p) = phase_index {
                    (FilterRef::Phase(p), cg[0])
                } else if cg.iter().all(|&v| v == cg[0]) {
                    let (f, s) = intern(&g);
                    (f, s * cg[0])
                } else {
                    intern(&bands::mul(&g, &cg))
                };
                if scale == 0.0 {
                    continue;
                }
                pairs.push(Pair {
                    field: fi,
                    arrival: ai,
                    channel: ci,
                    filter,
                    scale,
                });
            }
        }
    }

    // Diffraction: one filter per path, from its walls, air at its earliest node, and directivity.
    struct DiffractionJob {
        field: usize,
        path: usize,
        channel: usize,
        filter: FilterRef,
        scale: f64,
    }
    let mut diffraction_jobs = Vec::new();
    for (fi, field) in fields.iter().enumerate() {
        for (pi, path) in field.diffracted.iter().enumerate() {
            let Some(apex) = path
                .nodes
                .iter()
                .min_by(|a, b| a.path_length_m.partial_cmp(&b.path_length_m).unwrap())
            else {
                continue;
            };
            let mut g = path.reflection;
            if options.air_absorption {
                g = bands::mul(&g, &air.band_pressure_gain(apex.path_length_m));
            }
            g = bands::mul(&g, &field.directivity.band_gains(apex.departure));
            for (ci, capsule) in capsules.iter().enumerate() {
                let cg = capsule.directivity.band_gains(apex.direction);
                let (filter, scale) = if cg.iter().all(|&v| v == cg[0]) {
                    let (f, s) = intern(&g);
                    (f, s)
                } else {
                    intern(&bands::mul(&g, &cg))
                };
                diffraction_jobs.push(DiffractionJob {
                    field: fi,
                    path: pi,
                    channel: ci,
                    filter,
                    scale,
                });
            }
        }
    }

    let grid = ((fs / 6.0).ceil().max(4096.0) as usize).next_power_of_two();
    let taps = grid / 4;
    let filters = parallel_map(&unique, options.threads, |g| {
        fft::minimum_phase_fir(|f| bands::gain_at(g, f), fs, grid, taps)
    });
    let admittances: Vec<Option<Vec<Vec<C64>>>> = fields
        .iter()
        .map(|f| {
            f.wave.map(|w| {
                w.boundaries
                    .iter()
                    .map(|b| {
                        (0..=PHASE_GRID / 2)
                            .map(|k| b.admittance((k as f64 * fs / PHASE_GRID as f64).max(1.0)))
                            .collect()
                    })
                    .collect()
            })
        })
        .collect();
    let phase_filters = parallel_map(&phase_jobs, options.threads, |&(fi, ai, g)| {
        let wave = fields[fi].wave.expect("phase jobs come from wave fields");
        let table = admittances[fi].as_ref().expect("wave fields have tables");
        walls_with_phase(&fields[fi].arrivals[ai], wave, table, &g, fs)
    });
    let filter_of = |r: FilterRef| -> Option<&[f64]> {
        match r {
            FilterRef::Flat => None,
            FilterRef::Shared(i) => Some(filters[i].as_slice()),
            FilterRef::Phase(i) => Some(phase_filters[i].as_slice()),
        }
    };
    let mut contributions: Vec<Contribution> = parallel_map(&pairs, options.threads, |p| {
        let a = &fields[p.field].arrivals[p.arrival];
        let capsule = &capsules[p.channel];
        let delay = (a.path_length_m - origin - capsule.offset.dot(a.direction)) / c * fs;
        let whole = delay.floor();
        let frac = delay - whole;
        let amplitude = reference / a.path_length_m * p.scale;
        let kernel = arrival_kernel(filter_of(p.filter), frac, amplitude);
        let start = if frac == 0.0 {
            whole as i64
        } else {
            whole as i64 + 1 - KERNEL_HALF_WIDTH as i64
        };
        Contribution {
            field: p.field,
            channel: p.channel,
            kind: Kind::Early,
            start,
            samples: kernel,
        }
    });
    contributions.extend(parallel_map(&diffraction_jobs, options.threads, |job| {
        let path = &fields[job.field].diffracted[job.path];
        let capsule = &capsules[job.channel];
        let positions: Vec<f64> = path
            .nodes
            .iter()
            .map(|n| (n.path_length_m - origin - capsule.offset.dot(n.direction)) / c * fs)
            .collect();
        let first = positions
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min)
            .floor() as i64;
        let last = positions
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil() as i64;
        let mut train = vec![0.0; (last - first + 2) as usize];
        for (n, &pos) in path.nodes.iter().zip(&positions) {
            let gain = capsule.directivity.band_gains(n.direction)[0];
            let w = n.weight * reference * job.scale * gain;
            let base = pos.floor();
            let frac = pos - base;
            let i = (base as i64 - first) as usize;
            train[i] += w * (1.0 - frac);
            train[i + 1] += w * frac;
        }
        let samples = match filter_of(job.filter) {
            None => train,
            Some(h) => convolve(&train, h),
        };
        Contribution {
            field: job.field,
            channel: job.channel,
            kind: Kind::Early,
            start: first,
            samples,
        }
    }));

    // Late part.
    let mut late_events = vec![0usize; fields.len()];
    if fields.iter().any(|f| f.histogram.is_some()) {
        let band_filters = late::band_filters(fs);
        for (fi, field) in fields.iter().enumerate() {
            let Some(histogram) = field.histogram else {
                continue;
            };
            let late_options = LateOptions {
                seed: crate::rng::SplitMix64::stream(options.late.seed ^ field.seed, fi as u64)
                    .next_u64(),
                ..options.late
            };
            let events = late::events(
                histogram,
                field.volume_m3,
                c,
                options.air_absorption.then_some(air),
                reference,
                &late_options,
            );
            late_events[fi] = events.len();
            let channels: Vec<usize> = (0..capsules.len()).collect();
            let rendered = parallel_map(&channels, options.threads.min(capsules.len()), |&ci| {
                late::render_capsule(&events, &capsules[ci], origin, c, fs, &band_filters)
            });
            for (ci, r) in rendered.into_iter().enumerate() {
                if let Some((start, samples)) = r {
                    contributions.push(Contribution {
                        field: fi,
                        channel: ci,
                        kind: Kind::Late,
                        start,
                        samples,
                    });
                }
            }
        }
    }

    // Wave part.
    for (fi, field) in fields.iter().enumerate() {
        let Some(wave) = field.wave else {
            continue;
        };
        for (ci, capsule) in capsules.iter().enumerate() {
            let samples = wave_channel(
                wave,
                capsule,
                field.receiver,
                origin,
                c,
                fs,
                options.include_direct,
                reference,
            )?;
            contributions.push(Contribution {
                field: fi,
                channel: ci,
                kind: Kind::Wave,
                start: 0,
                samples,
            });
        }
    }

    // Assembly: one lead and one length for the whole set.
    let min_start = contributions.iter().map(|k| k.start).min().unwrap_or(0);
    let lead = (-min_start).max(0) as usize;
    let length = contributions
        .iter()
        .map(|k| (k.start + lead as i64) as usize + k.samples.len())
        .max()
        .unwrap_or(1)
        + 1;
    let mut out_fields: Vec<FieldParts> = fields
        .iter()
        .enumerate()
        .map(|(fi, f)| FieldParts {
            early: vec![vec![0.0; length]; capsules.len()],
            late: vec![vec![0.0; length]; capsules.len()],
            wave: f.wave.map(|_| vec![vec![0.0; length]; capsules.len()]),
            crossover_hz: f.wave.map(|w| w.transition_hz),
            arrivals_rendered: arrivals_rendered[fi],
            late_events: late_events[fi],
        })
        .collect();
    for k in &contributions {
        let target = &mut out_fields[k.field];
        let buffer = match k.kind {
            Kind::Early => &mut target.early[k.channel],
            Kind::Late => &mut target.late[k.channel],
            Kind::Wave => &mut target
                .wave
                .as_mut()
                .expect("wave contributions come from wave fields")[k.channel],
        };
        let offset = (k.start + lead as i64) as usize;
        for (j, v) in k.samples.iter().enumerate() {
            if let Some(slot) = buffer.get_mut(offset + j) {
                *slot += v;
            }
        }
    }
    Ok(Parts {
        lead_samples: lead,
        origin_path_length_m: origin,
        length,
        fields: out_fields,
    })
}

/// The scattering part of an arrival's specular reflection, per band: `Π sqrt(1 − sᵢ)`. A
/// wave-solved scene takes absorption and phase from the boundary models instead.
fn specular_scattering(a: &Arrival, field: &SourceField) -> Bands {
    let wave = field.wave.expect("called for wave fields");
    let mut out = [1.0; NUM_BANDS];
    for (k, value) in out.iter_mut().enumerate() {
        for &face in &a.faces {
            *value *= (1.0 - wave.face_scattering[face][k]).sqrt();
        }
    }
    out
}

/// An arrival's filter in a wave-solved scene: the minimum-phase part `magnitude` times the
/// product of its walls' complex reflection coefficients at their incidence angles.
fn walls_with_phase(
    a: &Arrival,
    wave: &WaveField,
    admittances: &[Vec<C64>],
    magnitude: &Bands,
    fs: f64,
) -> Vec<f64> {
    let n = PHASE_GRID;
    let taps = n / 2;
    let m = fft::minimum_phase_fir(|f| bands::gain_at(magnitude, f), fs, n, taps);
    let mut spectrum = vec![Complex::default(); n];
    for (c, v) in spectrum.iter_mut().zip(&m) {
        c.re = *v;
    }
    fft::fft_in_place(&mut spectrum, false);
    for (k, bin) in spectrum.iter_mut().enumerate() {
        let mirrored = k > n / 2;
        let index = if mirrored { n - k } else { k };
        let mut r = C64::ONE;
        for (&face, &cos) in a.faces.iter().zip(&a.incidence_cos) {
            let beta = admittances[wave.face_boundary[face]][index];
            let cos = C64::real(cos);
            r = r * ((cos - beta) / (cos + beta));
        }
        if mirrored {
            r = r.conj();
        }
        let z = C64::new(bin.re, bin.im) * r;
        *bin = Complex { re: z.re, im: z.im };
    }
    fft::fft_in_place(&mut spectrum, true);
    let mut out: Vec<f64> = spectrum.iter().take(taps).map(|c| c.re).collect();
    let fade_start = taps - taps / 4;
    let fade_len = (taps - fade_start) as f64;
    for (i, v) in out.iter_mut().enumerate().skip(fade_start) {
        let x = (i - fade_start) as f64 / fade_len;
        *v *= 0.5 * (1.0 + (PI * x).cos());
    }
    out[taps - 1] = 0.0;
    out
}

/// One capsule's wave-solver signal at the output rate, starting at time zero (before lead).
#[allow(clippy::too_many_arguments)]
fn wave_channel(
    wave: &WaveField,
    capsule: &PlacedCapsule,
    receiver: Vec3,
    origin_m: f64,
    speed: f64,
    fs_out: f64,
    include_direct: bool,
    reference: f64,
) -> Result<Vec<f64>, Error> {
    let position = receiver + capsule.offset;
    let probe = wave
        .probes
        .iter()
        .find(|p| (p.position - position).length() < 1e-6)
        .ok_or_else(|| {
            Error::InvalidOption(format!(
                "capsule `{}` is not among the wave solver's probes; declare its array in WaveOptions",
                capsule.name
            ))
        })?;
    let (pattern, axis) = match &capsule.directivity {
        Directivity::Omni => (1.0, Vec3::default()),
        Directivity::FirstOrder { axis, pattern } => (*pattern, *axis),
        Directivity::BandFirstOrder { .. } | Directivity::Tabulated { .. } => {
            return Err(Error::InvalidOption(
                "a wave-solved render takes omni or first-order capsules only".into(),
            ));
        }
    };
    let fs = wave.sample_rate;
    let n = probe.pressure.len();
    // A plane wave from direction u has w = −u·p, and a capsule hears pattern·p + (1 − pattern)·(axis·u)·p.
    let mut signal: Vec<f64> = (0..n)
        .map(|i| {
            let w = Vec3::new(
                probe.velocity[0][i],
                probe.velocity[1][i],
                probe.velocity[2][i],
            );
            pattern * probe.pressure[i] - (1.0 - pattern) * axis.dot(w)
        })
        .collect();
    if !include_direct {
        // The free-field direct sound of the same excitation: pressure `x(t − r/c)/r`, radial
        // velocity `p + (c/r)∫p dt` (a spherical wave's near field included).
        let to_source = wave.source - position;
        let r = to_source.length();
        let u = to_source * (1.0 / r);
        let delay = r / speed * fs + wave.excitation_half_width as f64;
        let mut integral = 0.0;
        for (i, value) in signal.iter_mut().enumerate() {
            let pd = wave.excitation_at(i as f64 - delay) / r;
            integral += pd / fs;
            let radial = pd + speed / r * integral;
            *value -= pattern * pd + (1.0 - pattern) * axis.dot(u) * radial;
        }
    }
    dc_block(&mut signal, DC_BLOCK_HZ, fs);
    let fade = ((WAVE_END_FADE_S * fs).round() as usize).clamp(1, (n / 10).max(1));
    let start = n.saturating_sub(fade);
    for (i, v) in signal[start..].iter_mut().enumerate() {
        let x = (i + 1) as f64 / fade as f64;
        *v *= 0.5 * (1.0 + (PI * x).cos());
    }
    let scale = reference * fs / fs_out;
    let hw = WAVE_KERNEL_HALF_WIDTH as i64;
    let centre = wave.excitation_half_width as f64;
    let t0 = origin_m / speed;
    let end_s = ((n as f64 + hw as f64 - centre) / fs - t0).max(0.0);
    let count = (end_s * fs_out).ceil() as usize;
    Ok((0..count)
        .map(|m| {
            let tau = (m as f64 / fs_out + t0) * fs + centre;
            let base = tau.floor() as i64;
            let mut acc = 0.0;
            for j in base - hw + 1..=base + hw {
                if j < 0 || j as usize >= n {
                    continue;
                }
                let x = tau - j as f64;
                let sinc = if x == 0.0 {
                    1.0
                } else {
                    (PI * x).sin() / (PI * x)
                };
                let win = x / hw as f64;
                let window = 0.42 + 0.5 * (PI * win).cos() + 0.08 * (2.0 * PI * win).cos();
                acc += signal[j as usize] * sinc * window;
            }
            acc * scale
        })
        .collect())
}

/// Quality factors of the three sections of a sixth-order Butterworth filter.
const BUTTERWORTH_6TH: [f64; 3] = [
    0.517_638_090_205_041_5,
    std::f64::consts::FRAC_1_SQRT_2,
    1.931_851_652_578_136_6,
];

/// Second-order Butterworth highpass by the bilinear transform, applied forward.
fn dc_block(x: &mut [f64], corner_hz: f64, fs: f64) {
    highpass(x, corner_hz, std::f64::consts::FRAC_1_SQRT_2, fs);
}

/// A second-order highpass section of quality `q` by the bilinear transform, applied forward.
fn highpass(x: &mut [f64], corner_hz: f64, q: f64, fs: f64) {
    let k = (PI * corner_hz / fs).tan();
    let norm = 1.0 / (1.0 + k / q + k * k);
    let (b0, b1, b2) = (norm, -2.0 * norm, norm);
    let a1 = 2.0 * (k * k - 1.0) * norm;
    let a2 = (1.0 - k / q + k * k) * norm;
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    for v in x.iter_mut() {
        let y = b0 * *v + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
        x2 = x1;
        x1 = *v;
        y2 = y1;
        y1 = y;
        *v = y;
    }
}

/// The crossover filters for one transition band.
struct Crossover {
    lowpass: Vec<f64>,
    power_highpass: Vec<f64>,
}

impl Crossover {
    fn design(band: (f64, f64), fs: f64) -> Self {
        let (lo, hi) = band;
        let low = move |f: f64| {
            if f <= lo {
                1.0
            } else if f >= hi {
                0.0
            } else {
                0.5 * (1.0 + (PI * (f / lo).ln() / (hi / lo).ln()).cos())
            }
        };
        let grid = ((fs / 0.75).ceil() as usize).next_power_of_two();
        Self {
            lowpass: fft::minimum_phase_fir(|f| low(f).max(1e-5), fs, grid, grid / 4),
            power_highpass: fft::minimum_phase_fir(
                |f| (1.0 - low(f).powi(2)).max(0.0).sqrt().max(1e-5),
                fs,
                grid,
                grid / 4,
            ),
        }
    }
}

/// Convolution of `x` with `h`, truncated to `length`, by FFT.
fn fft_convolve(x: &[f64], h: &[f64], length: usize) -> Vec<f64> {
    let n = (x.len() + h.len()).next_power_of_two();
    let mut a = vec![Complex::default(); n];
    for (c, &v) in a.iter_mut().zip(x) {
        c.re = v;
    }
    let mut b = vec![Complex::default(); n];
    for (c, &v) in b.iter_mut().zip(h) {
        c.re = v;
    }
    fft::fft_in_place(&mut a, false);
    fft::fft_in_place(&mut b, false);
    for (p, q) in a.iter_mut().zip(&b) {
        *p = Complex {
            re: p.re * q.re - p.im * q.im,
            im: p.re * q.im + p.im * q.re,
        };
    }
    fft::fft_in_place(&mut a, true);
    let mut out: Vec<f64> = a.iter().take(length).map(|c| c.re).collect();
    out.resize(length, 0.0);
    out
}

/// Direct convolution, for short trains.
fn convolve(x: &[f64], h: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; x.len() + h.len() - 1];
    for (i, &a) in x.iter().enumerate() {
        if a == 0.0 {
            continue;
        }
        for (j, &b) in h.iter().enumerate() {
            out[i + j] += a * b;
        }
    }
    out
}

/// One arrival's samples: the band filter convolved with the fractional-delay kernel, scaled.
/// A zero fraction with no filter is exactly `[amplitude]`.
fn arrival_kernel(filter: Option<&[f64]>, frac: f64, amplitude: f64) -> Vec<f64> {
    let delay: Vec<f64> = if frac == 0.0 {
        vec![1.0]
    } else {
        let w = KERNEL_HALF_WIDTH as f64;
        (1 - KERNEL_HALF_WIDTH as isize..=KERNEL_HALF_WIDTH as isize)
            .map(|k| {
                let u = k as f64 - frac;
                let sinc = if u == 0.0 {
                    1.0
                } else {
                    (PI * u).sin() / (PI * u)
                };
                let x = u / w;
                let window = 0.42 + 0.5 * (PI * x).cos() + 0.08 * (2.0 * PI * x).cos();
                sinc * window
            })
            .collect()
    };
    match filter {
        None => delay.into_iter().map(|v| v * amplitude).collect(),
        Some(h) => {
            let mut out = vec![0.0; h.len() + delay.len() - 1];
            for (i, &a) in h.iter().enumerate() {
                if a == 0.0 {
                    continue;
                }
                for (j, &b) in delay.iter().enumerate() {
                    out[i + j] += a * b * amplitude;
                }
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directivity::{Array, Frame};
    use crate::fft::{Complex, fft_in_place};
    use crate::geometry::Room;
    use crate::ism::{ImageSourceOptions, image_sources};
    use crate::material::Material;
    use crate::scene::Scene;

    fn anechoic_scene() -> Scene {
        let room = Room::shoebox(
            Vec3::new(20.0, 20.0, 20.0),
            std::array::from_fn(|_| Material::anechoic()),
        )
        .unwrap();
        Scene::new(
            "free",
            room,
            Air::standard(),
            Vec3::new(4.0, 5.0, 6.0),
            Vec3::new(10.0, 13.0, 6.0),
        )
        .unwrap()
    }

    fn absorbing_box() -> Scene {
        let wall =
            Material::from_125_to_4k("plaster", [0.02, 0.02, 0.03, 0.04, 0.05, 0.05], [0.05; 6])
                .unwrap();
        let floor =
            Material::from_125_to_4k("wood", [0.15, 0.11, 0.10, 0.07, 0.06, 0.07], [0.1; 6])
                .unwrap();
        let room = Room::shoebox(
            Vec3::new(6.1, 4.7, 2.9),
            [
                wall.clone(),
                wall.clone(),
                wall.clone(),
                wall.clone(),
                floor,
                wall,
            ],
        )
        .unwrap();
        Scene::new(
            "box",
            room,
            Air::standard(),
            Vec3::new(1.3, 1.1, 1.5),
            Vec3::new(4.4, 3.2, 1.2),
        )
        .unwrap()
    }

    #[test]
    fn free_field_direct_has_one_over_r_level_on_sample_zero() {
        let scene = anechoic_scene();
        let arrivals = image_sources(
            &scene,
            &ImageSourceOptions {
                max_order: 2,
                ..Default::default()
            },
        )
        .unwrap();
        let options = RenderOptions {
            air_absorption: false,
            normalize_peak_dbfs: None,
            ..Default::default()
        };
        let out = render_omni(&arrivals, &scene.air, &options).unwrap();
        assert_eq!(
            out.arrivals_rendered, 1,
            "reflections off anechoic walls carry nothing"
        );
        let r = (scene.receiver - scene.source).length();
        assert_eq!(out.samples()[0], (1.0 / r) as f32);
        assert!(out.samples()[1..].iter().all(|&v| v == 0.0));
        assert!((out.trimmed_delay_s - r / scene.air.speed_of_sound()).abs() < 1e-15);
    }

    /// A render is normalized to a −1 dBFS peak by default, records its gain, and equals the
    /// physical render times that gain.
    #[test]
    fn renders_are_normalized_by_default_and_physical_on_request() {
        let scene = absorbing_box();
        let arrivals = image_sources(
            &scene,
            &ImageSourceOptions {
                max_order: 2,
                ..Default::default()
            },
        )
        .unwrap();
        let normalized = render_omni(&arrivals, &scene.air, &RenderOptions::default()).unwrap();
        let physical = render_omni(
            &arrivals,
            &scene.air,
            &RenderOptions {
                normalize_peak_dbfs: None,
                ..Default::default()
            },
        )
        .unwrap();
        let peak = normalized
            .samples()
            .iter()
            .fold(0f32, |m, v| m.max(v.abs()));
        assert!((20.0 * f64::from(peak).log10() - DEFAULT_PEAK_DBFS).abs() < 1e-5);
        assert_eq!(physical.gain, 1.0);
        assert!(normalized.gain > 1.0);
        assert_eq!(normalized.samples().len(), physical.samples().len());
        for (n, p) in normalized.samples().iter().zip(physical.samples()) {
            assert!(
                (f64::from(*n) - f64::from(*p) * normalized.gain).abs() <= 1e-6 * f64::from(peak)
            );
        }
        assert!(
            render_omni(
                &arrivals,
                &scene.air,
                &RenderOptions {
                    normalize_peak_dbfs: Some(f64::NAN),
                    ..Default::default()
                },
            )
            .is_err()
        );
    }

    #[test]
    fn fractional_kernel_delay_is_sub_sample_exact() {
        let frac = 0.3;
        let k = arrival_kernel(None, frac, 1.0);
        let n = 1024;
        let mut buf = vec![Complex::default(); n];
        for (i, v) in k.iter().enumerate() {
            buf[i].re = *v;
        }
        fft_in_place(&mut buf, false);
        let phase = |b: usize| buf[b].im.atan2(buf[b].re);
        let (b1, b2) = (4usize, 8usize);
        let mut dphi = phase(b2) - phase(b1);
        while dphi > PI {
            dphi -= 2.0 * PI;
        }
        while dphi < -PI {
            dphi += 2.0 * PI;
        }
        let delay = -dphi / (2.0 * PI * (b2 - b1) as f64 / n as f64);
        let expected = (KERNEL_HALF_WIDTH - 1) as f64 + frac;
        assert!((delay - expected).abs() < 1e-3, "{delay} vs {expected}");
        assert_eq!(arrival_kernel(None, 0.0, 0.5), vec![0.5]);
    }

    #[test]
    fn result_is_independent_of_thread_count() {
        let scene = absorbing_box();
        let arrivals = image_sources(
            &scene,
            &ImageSourceOptions {
                max_order: 4,
                ..Default::default()
            },
        )
        .unwrap();
        let render = |threads| {
            render_omni(
                &arrivals,
                &scene.air,
                &RenderOptions {
                    threads,
                    ..Default::default()
                },
            )
            .unwrap()
        };
        let (one, many) = (render(1), render(7));
        assert_eq!(one.samples().len(), many.samples().len());
        assert!(
            one.samples()
                .iter()
                .zip(many.samples())
                .all(|(a, b)| a.to_bits() == b.to_bits())
        );
    }

    #[test]
    fn hygiene_causal_finite_and_ends_in_silence() {
        let scene = absorbing_box();
        let arrivals = image_sources(
            &scene,
            &ImageSourceOptions {
                max_order: 4,
                ..Default::default()
            },
        )
        .unwrap();
        for include_direct in [true, false] {
            let options = RenderOptions {
                include_direct,
                ..Default::default()
            };
            let out = render_omni(&arrivals, &scene.air, &options).unwrap();
            let samples = out.samples();
            assert!(samples.iter().all(|v| v.is_finite()));
            assert_eq!(*samples.last().unwrap(), 0.0);
            let c = scene.air.speed_of_sound();
            let fs = f64::from(options.sample_rate);
            let first = arrivals
                .iter()
                .filter(|a| include_direct || a.order() > 0)
                .map(|a| (a.path_length_m - out.origin_path_length_m) / c * fs)
                .fold(f64::INFINITY, f64::min);
            let earliest = (out.lead_samples as f64 + first.floor()) as usize;
            let silent_until = earliest.saturating_sub(KERNEL_HALF_WIDTH);
            assert!(samples[..silent_until].iter().all(|&v| v == 0.0));
            if include_direct {
                assert_eq!(out.lead_samples, 0);
                assert!(samples[0] != 0.0, "the direct sound sits on sample zero");
            } else {
                assert!(samples[0] == 0.0, "time zero is still the direct arrival");
            }
        }
    }

    /// A spaced pair keeps the plane-wave delay between its channels.
    #[test]
    fn spaced_pair_preserves_inter_channel_delay() {
        let scene = anechoic_scene();
        let arrivals = image_sources(
            &scene,
            &ImageSourceOptions {
                max_order: 0,
                ..Default::default()
            },
        )
        .unwrap();
        let capsules = Array::spaced_omni(0.5).placed(&Frame::WORLD).unwrap();
        let field = SourceField {
            arrivals: &arrivals,
            directivity: &Directivity::Omni,
            histogram: None,
            volume_m3: 1.0,
            receiver: scene.receiver,
            diffracted: &[],
            wave: None,
            seed: 0,
        };
        let options = RenderOptions {
            air_absorption: false,
            ..Default::default()
        };
        let out = render_fields(&[field], &scene.air, &capsules, &options)
            .unwrap()
            .remove(0);
        let u = arrivals[0].direction;
        let expected = (capsules[0].offset - capsules[1].offset).dot(u)
            / scene.air.speed_of_sound()
            * f64::from(options.sample_rate);
        let n = 4096;
        let spectrum = |x: &[f32]| {
            let mut buf = vec![Complex::default(); n];
            for (c, &v) in buf.iter_mut().zip(x) {
                c.re = f64::from(v);
            }
            fft_in_place(&mut buf, false);
            buf
        };
        let (a, b) = (spectrum(&out.channels[0]), spectrum(&out.channels[1]));
        let bin = 8;
        let cross_re = b[bin].re * a[bin].re + b[bin].im * a[bin].im;
        let cross_im = b[bin].im * a[bin].re - b[bin].re * a[bin].im;
        let lag = -cross_im.atan2(cross_re) / (2.0 * PI * bin as f64 / n as f64);
        assert!((lag - expected).abs() < 0.005, "{lag} vs {expected}");
        assert!(out.lead_samples > 0);
    }

    /// The crossover's early branch is the exact complement of its lowpass: a signal split and
    /// summed is unchanged.
    #[test]
    fn crossover_lowpass_and_complement_sum_to_the_input() {
        let fs = 48_000.0;
        let x = Crossover::design((141.0, 283.0), fs);
        let mut rng = crate::rng::SplitMix64::new(9);
        let signal: Vec<f64> = (0..20_000).map(|_| rng.next_normal()).collect();
        let low = fft_convolve(&signal, &x.lowpass, signal.len());
        let high: Vec<f64> = signal.iter().zip(&low).map(|(s, l)| s - l).collect();
        let back: Vec<f64> = low.iter().zip(&high).map(|(l, h)| l + h).collect();
        assert!(signal.iter().zip(&back).all(|(a, b)| (a - b).abs() < 1e-9));
        // The power-complementary highpass and the lowpass together pass unit power.
        let spectrum_power = |h: &[f64], f: f64| {
            h.iter()
                .enumerate()
                .fold(C64::ZERO, |acc, (n, &v)| {
                    acc + C64::from_polar(v, -2.0 * PI * f * n as f64 / fs)
                })
                .norm_sqr()
        };
        for f in [100.0, 150.0, 200.0, 250.0, 400.0] {
            let total = spectrum_power(&x.lowpass, f) + spectrum_power(&x.power_highpass, f);
            assert!((total - 1.0).abs() < 0.01, "{f} Hz: {total}");
        }
    }
}
