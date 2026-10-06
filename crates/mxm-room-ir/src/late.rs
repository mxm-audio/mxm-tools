//! Late synthesis: the ray histogram becomes sound.
//!
//! The recipe (research page §12, *Auralize* 2): per band and direction, a Poisson impulse sequence
//! whose rate follows the reflection density `4πc³t²/V`, weighted by the histogram's envelope,
//! through a band split that sums flat. Kuttruff and Vorländer, who describe it, were not read; the
//! V4, V6 and V7 tests are the evidence.
//!
//! # How
//!
//! 1. **Envelope.** The histogram is smoothed in time with a window that widens with time (ray
//!    noise grows as energy falls, research page §9 wart 4), air absorption is applied at each
//!    bin's propagation distance, and the level is scaled to the reference distance.
//! 2. **Events.** Poisson times with rate `μ(t)`. Each event draws a direction cell with probability
//!    `P_j` equal to the cell's band-averaged share of the energy near that time, a uniform direction
//!    inside the cell, and a random sign. Its amplitude in band `k` is
//!    `a_k = sqrt(H_jk / (μ·Δ·P_j))`, so the expected energy per band and bin is the histogram's.
//! 3. **Capsules.** Every capsule hears every event: delayed as a plane wave from its direction
//!    (`−offset·u/c`) and weighted by its directivity. Inter-capsule coherence therefore follows
//!    from the directions, not from a decorrelation rule.
//! 4. **Bands.** Each band's impulse train is filtered by a minimum-phase filter whose magnitude is
//!    the analyser's power-complementary octave weight, so bands sum to a flat power spectrum and
//!    nothing rings before an event.
//!
//! # Chosen, not read
//!
//! - Rate `μ(t) = clamp(4πc³t²/V, min_rate_hz, max_rate_hz)`, with a floor because scattered energy
//!   arrives denser than specular reflections.
//! - Smoothing window: half-width `max(1 bin, smoothing·t)`.
//! - Events are placed by a Blackman-windowed sinc of half-width [`EVENT_KERNEL_HALF_WIDTH`].
//! - A band ends where its envelope stays below `floor_db` of its own peak.
//! - Events start at the earliest deposited arrival. Found by listening to the first church render:
//!   the smoothing window had lifted energy up to 3 ms before the direct sound.

use std::f64::consts::PI;

use crate::air::Air;
use crate::analysis::band_weight;
use crate::bands::{Bands, NUM_BANDS};
use crate::directivity::PlacedCapsule;
use crate::fft::{self, Complex};
use crate::geometry::Vec3;
use crate::rays::{DIRECTION_CELLS, Histogram, sample_in_cell};
use crate::rng::SplitMix64;

/// Half-width of the sinc that places a late event, samples.
pub const EVENT_KERNEL_HALF_WIDTH: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LateOptions {
    pub seed: u64,
    pub min_rate_hz: f64,
    pub max_rate_hz: f64,
    /// Smoothing half-width as a fraction of propagation time.
    pub smoothing: f64,
    pub floor_db: f64,
}

impl Default for LateOptions {
    fn default() -> Self {
        Self {
            seed: 1,
            min_rate_hz: 2_000.0,
            max_rate_hz: 20_000.0,
            smoothing: 0.02,
            floor_db: -90.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// Absolute propagation time, s.
    pub time_s: f64,
    /// Unit vector toward where the event arrives from.
    pub direction: Vec3,
    /// Signed pressure amplitude per band.
    pub amplitude: Bands,
}

/// The smoothed, air-attenuated, level-scaled envelope: energy per bin, cell and band.
struct Envelope {
    bin_width_s: f64,
    bins: usize,
    energy: Vec<f64>,
    /// Per bin: cumulative cell probabilities.
    cumulative: Vec<f64>,
    end_bin: usize,
}

fn envelope(
    histogram: &Histogram,
    air: Option<&Air>,
    speed: f64,
    reference_m: f64,
    options: &LateOptions,
) -> Envelope {
    let bins = histogram.bins();
    let width = DIRECTION_CELLS * NUM_BANDS;
    let dt = histogram.bin_width_s;
    let mut prefix = vec![0.0; (bins + 1) * width];
    for b in 0..bins {
        let row = histogram.bin(b);
        for i in 0..width {
            prefix[(b + 1) * width + i] = prefix[b * width + i] + row[i];
        }
    }
    let mut energy = vec![0.0; bins * width];
    for b in 0..bins {
        let t = (b as f64 + 0.5) * dt;
        let half = ((options.smoothing * t / dt).round() as usize).max(1);
        let lo = b.saturating_sub(half);
        let hi = (b + half).min(bins - 1);
        let span = (hi - lo + 1) as f64;
        let gain = air.map(|a| a.band_pressure_gain(t * speed));
        for i in 0..width {
            let k = i % NUM_BANDS;
            let mean = (prefix[(hi + 1) * width + i] - prefix[lo * width + i]) / span;
            let g = gain.map_or(1.0, |g| g[k]);
            energy[b * width + i] = mean * g * g * reference_m * reference_m;
        }
    }
    // Where every band has fallen below its floor for good.
    let mut end_bin = 0;
    for k in 0..NUM_BANDS {
        let band = |b: usize| {
            (0..DIRECTION_CELLS)
                .map(|c| energy[(b * DIRECTION_CELLS + c) * NUM_BANDS + k])
                .sum::<f64>()
        };
        let per_bin: Vec<f64> = (0..bins).map(band).collect();
        let peak = per_bin.iter().copied().fold(0.0, f64::max);
        if peak <= 0.0 {
            continue;
        }
        let floor = peak * 10f64.powf(options.floor_db / 10.0);
        if let Some(last) = per_bin.iter().rposition(|&e| e > floor) {
            end_bin = end_bin.max(last + 1);
        }
    }
    let mut cumulative = vec![0.0; bins * DIRECTION_CELLS];
    for b in 0..bins {
        let pair = |c: usize, k: usize| {
            let at = |bb: usize| energy[(bb * DIRECTION_CELLS + c) * NUM_BANDS + k];
            at(b) + if b + 1 < bins { at(b + 1) } else { 0.0 }
        };
        let totals: Bands = std::array::from_fn(|k| (0..DIRECTION_CELLS).map(|c| pair(c, k)).sum());
        let active = totals.iter().filter(|&&t| t > 0.0).count();
        let mut running = 0.0;
        for c in 0..DIRECTION_CELLS {
            if active > 0 {
                let share: f64 = (0..NUM_BANDS)
                    .filter(|&k| totals[k] > 0.0)
                    .map(|k| pair(c, k) / totals[k])
                    .sum();
                running += share / active as f64;
            }
            cumulative[b * DIRECTION_CELLS + c] = running;
        }
    }
    Envelope {
        bin_width_s: dt,
        bins,
        energy,
        cumulative,
        end_bin,
    }
}

/// Draws the late events for one source.
pub fn events(
    histogram: &Histogram,
    volume_m3: f64,
    speed: f64,
    air: Option<&Air>,
    reference_m: f64,
    options: &LateOptions,
) -> Vec<Event> {
    let env = envelope(histogram, air, speed, reference_m, options);
    let dt = env.bin_width_s;
    let Some(first) =
        (0..env.end_bin).find(|&b| env.cumulative[(b + 1) * DIRECTION_CELLS - 1] > 0.0)
    else {
        return Vec::new();
    };
    let end_time = env.end_bin as f64 * dt;
    let density = 4.0 * PI * speed.powi(3) / volume_m3;
    let rate = |t: f64| (density * t * t).clamp(options.min_rate_hz, options.max_rate_hz);
    let mut rng = SplitMix64::stream(options.seed, 0x1a7e);
    let mut out = Vec::new();
    // Never before the earliest physical arrival: the smoothing window reaches a bin or two back.
    let mut t = (first as f64 * dt).max(histogram.earliest_s);
    loop {
        let mu = rate(t);
        t += -(1.0 - rng.next_f64()).ln() / mu;
        if t >= end_time {
            break;
        }
        let x = t / dt - 0.5;
        let b0 = (x.floor().max(0.0) as usize).min(env.bins - 1);
        let b1 = (b0 + 1).min(env.bins - 1);
        let w = (x - b0 as f64).clamp(0.0, 1.0);
        let cum = &env.cumulative[b0 * DIRECTION_CELLS..(b0 + 1) * DIRECTION_CELLS];
        let total = cum[DIRECTION_CELLS - 1];
        if total <= 0.0 {
            continue;
        }
        let pick = rng.next_f64() * total;
        let cell = cum.partition_point(|&c| c <= pick).min(DIRECTION_CELLS - 1);
        let p = (cum[cell] - if cell == 0 { 0.0 } else { cum[cell - 1] }) / total;
        if p <= 0.0 {
            continue;
        }
        let direction = sample_in_cell(cell, rng.next_f64(), rng.next_f64());
        let sign = if rng.next_u64() & 1 == 0 { 1.0 } else { -1.0 };
        let amplitude = std::array::from_fn(|k| {
            let at = |b: usize| env.energy[(b * DIRECTION_CELLS + cell) * NUM_BANDS + k];
            let h = (1.0 - w) * at(b0) + w * at(b1);
            sign * (h / (mu * dt * p)).sqrt()
        });
        out.push(Event {
            time_s: t,
            direction,
            amplitude,
        });
    }
    out
}

/// Minimum-phase octave filters whose squared magnitudes are the analyser's band weights.
pub(crate) fn band_filters(sample_rate: f64) -> [Vec<f64>; NUM_BANDS] {
    let grid = ((sample_rate / 0.75).ceil() as usize).next_power_of_two();
    std::array::from_fn(|k| {
        fft::minimum_phase_fir(|f| band_weight(k, f).max(1e-4), sample_rate, grid, grid / 4)
    })
}

/// One capsule's late signal: the first sample's index relative to time zero (before any leading
/// silence), and the samples.
pub(crate) fn render_capsule(
    events: &[Event],
    capsule: &PlacedCapsule,
    origin_m: f64,
    speed: f64,
    sample_rate: f64,
    filters: &[Vec<f64>; NUM_BANDS],
) -> Option<(i64, Vec<f64>)> {
    if events.is_empty() {
        return None;
    }
    let hw = EVENT_KERNEL_HALF_WIDTH as i64;
    let position = |e: &Event| {
        ((e.time_s * speed - origin_m - capsule.offset.dot(e.direction)) / speed) * sample_rate
    };
    let first = events
        .iter()
        .map(|e| position(e).floor() as i64)
        .min()
        .unwrap()
        - hw;
    let last = events
        .iter()
        .map(|e| position(e).floor() as i64)
        .max()
        .unwrap()
        + hw
        + 1;
    let span = (last - first + 1) as usize;
    let taps = filters.iter().map(Vec::len).max().unwrap();
    let length = span + taps;
    let mut out = vec![0.0; length];
    let n = length.next_power_of_two();
    let mut trains: Vec<Vec<f64>> = vec![vec![0.0; span]; NUM_BANDS];
    for e in events {
        let gain = capsule.directivity.band_gains(e.direction);
        let pos = position(e);
        let whole = pos.floor();
        let frac = pos - whole;
        let base = whole as i64 - first;
        for j in -hw + 1..=hw {
            let u = j as f64 - frac;
            let sinc = if u == 0.0 {
                1.0
            } else {
                (PI * u).sin() / (PI * u)
            };
            let x = u / hw as f64;
            let window = 0.42 + 0.5 * (PI * x).cos() + 0.08 * (2.0 * PI * x).cos();
            let v = sinc * window;
            let idx = (base + j) as usize;
            for k in 0..NUM_BANDS {
                trains[k][idx] += v * e.amplitude[k] * gain[k];
            }
        }
    }
    for (k, train) in trains.iter().enumerate() {
        let mut a = vec![Complex::default(); n];
        for (c, &v) in a.iter_mut().zip(train) {
            c.re = v;
        }
        let mut b = vec![Complex::default(); n];
        for (c, &v) in b.iter_mut().zip(&filters[k]) {
            c.re = v;
        }
        fft::fft_in_place(&mut a, false);
        fft::fft_in_place(&mut b, false);
        for (x, y) in a.iter_mut().zip(&b) {
            *x = Complex {
                re: x.re * y.re - x.im * y.im,
                im: x.re * y.im + x.im * y.re,
            };
        }
        fft::fft_in_place(&mut a, true);
        for (o, c) in out.iter_mut().zip(&a) {
            *o += c.re;
        }
    }
    Some((first, out))
}
