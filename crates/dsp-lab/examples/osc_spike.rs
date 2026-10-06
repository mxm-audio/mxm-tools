//! Oscillator spike: measure what the oscillator section actually does, and what
//! the alternatives would have done instead.
//!
//! The published comparisons of alias-reduced oscillator algorithms (Välimäki,
//! Pekonen and Nam, *JASA* 131(1), 2012) are perceptual — noise-to-mask ratios from
//! a masking model. This measures something narrower and reproducible: total aliased
//! energy against total wanted-harmonic energy, for our own code and for the
//! algorithms we could have used instead.
//!
//! Method: pick `f0 = P * fs / N` with `P` odd and `N = 65536`, so the waveform is
//! exactly periodic in the analysis window. No window function is then needed, every
//! harmonic lands exactly on bin `k*P`, and every aliased component lands exactly on
//! bin `k*P mod N` — which, because `gcd(P, N) = 1`, is never a wanted-harmonic bin.
//! Alias energy is separable *exactly* rather than estimated, and the measurement
//! floor is the FFT's own, around -230 dB.
//!
//! Run with: `cargo run -p dsp-lab --release --example osc_spike`

use mxm_measure::spectrum::{N, a_weight, fft, ifft, periods_for, princarg, spectral_flatness_db};
use mxm_mono_01_dsp::oscillator::{DcBlocker, Phasor, clamp_pulse_width, pulse, saw};
use std::f64::consts::PI;
use std::hint::black_box;
use std::time::Instant;

/// Samples discarded before the analysis window.
///
/// Every algorithm here has state — a one-sample memory in DPW, a two-sample
/// correction buffer in the four-point BLEP, a few hundred taps of decimation filter
/// in the oversampled candidates — and its startup transient is broadband. Left in
/// the window it lands in exactly the bins the alias measurement reads, and it is
/// not small: an unsettled DPW emits one sample of `1/(4T)`, which at 220 Hz is
/// +26 dB relative to the waveform and buries every result below it. Discarding a
/// prefix costs nothing, because the signal repeats exactly every `N` samples
/// whatever the starting phase.
const SETTLE: usize = 4096;

const RATES: [f64; 4] = [44_100.0, 48_000.0, 96_000.0, 192_000.0];

// ---------------------------------------------------------------------------
// Spectrum analysis
// ---------------------------------------------------------------------------

/// What one measurement of one waveform tells us.
#[derive(Debug, Clone, Copy)]
struct Spectrum {
    /// Total aliased energy over total wanted-harmonic energy, in dB.
    alias_db: f64,
    /// The same ratio with both sides A-weighted — a crude audibility proxy, and
    /// not the noise-to-mask ratio the literature reports.
    alias_db_a: f64,
    /// Loudest single aliased component, relative to the fundamental, in dB.
    worst_alias_db: f64,
    /// Largest harmonic amplitude error against an ideal `1/k` sawtooth, in dB,
    /// over harmonics below 0.9 * Nyquist. This is the dullness number: alias
    /// reduction bought by quietly removing the top octave shows up here.
    harmonic_err_db: f64,
    /// Wanted harmonics below Nyquist.
    harmonics: usize,
    /// Harmonics below 0.45*fs that come out more than 3 dB below the ideal `1/k`
    /// level — ones the algorithm has effectively thrown away rather than merely
    /// attenuated. One number for "how much of the top did this cost".
    missing: usize,
}

/// Analyse one exactly-periodic buffer containing `p` waveform periods.
fn analyse(x: &[f64], p: usize, fs: f64) -> Spectrum {
    assert_eq!(x.len(), N);
    let mut re = x.to_vec();
    let mut im = vec![0.0f64; N];
    fft(&mut re, &mut im).expect("the harness allocates power-of-two buffers");

    let half = N / 2;
    let mag: Vec<f64> = (0..half)
        .map(|i| (re[i] * re[i] + im[i] * im[i]).sqrt() / (N as f64 / 2.0))
        .collect();

    let n_harm = (half - 1) / p;
    let mut is_harmonic = vec![false; half];
    let (mut harm, mut harm_a) = (0.0f64, 0.0f64);
    let mut fundamental = 1e-30f64;
    let mut harmonic_err_db = 0.0f64;
    let mut missing = 0usize;

    for k in 1..=n_harm {
        let bin = k * p;
        is_harmonic[bin] = true;
        let m = mag[bin];
        if k == 1 {
            fundamental = m.max(1e-30);
        }
        let f = bin as f64 * fs / N as f64;
        harm += m * m;
        harm_a += (m * a_weight(f)).powi(2);

        if f < 0.45 * fs {
            let ideal = fundamental / k as f64;
            let err = 20.0 * (m.max(1e-30) / ideal).log10();
            if err.abs() > harmonic_err_db.abs() {
                harmonic_err_db = err;
            }
            if err < -3.0 {
                missing += 1;
            }
        }
    }

    let (mut alias, mut alias_a, mut worst) = (0.0f64, 0.0f64, 0.0f64);
    for (bin, &m) in mag.iter().enumerate().take(half).skip(1) {
        if is_harmonic[bin] {
            continue;
        }
        let f = bin as f64 * fs / N as f64;
        alias += m * m;
        alias_a += (m * a_weight(f)).powi(2);
        worst = worst.max(m);
    }

    Spectrum {
        alias_db: 10.0 * (alias / harm.max(1e-30)).max(1e-30).log10(),
        alias_db_a: 10.0 * (alias_a / harm_a.max(1e-30)).max(1e-30).log10(),
        worst_alias_db: 20.0 * (worst / fundamental).max(1e-30).log10(),
        harmonic_err_db,
        harmonics: n_harm,
        missing,
    }
}

// ---------------------------------------------------------------------------
// Candidate oscillator algorithms
// ---------------------------------------------------------------------------

/// One sample at a time, so the spectra and the cost figures below measure the
/// same code rather than two versions of it.
trait Osc {
    fn next(&mut self) -> f64;
}

fn render(osc: &mut dyn Osc, n: usize) -> Vec<f64> {
    (0..n).map(|_| osc.next()).collect()
}

/// Trivial (aliasing) sawtooth: a bare modulo counter, and the baseline.
struct Trivial {
    phase: f64,
    inc: f64,
}

impl Trivial {
    fn new(inc: f64) -> Self {
        Self { phase: 0.0, inc }
    }
}

impl Osc for Trivial {
    fn next(&mut self) -> f64 {
        let y = 2.0 * self.phase - 1.0;
        self.phase += self.inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        y
    }
}

/// The shipped mxm-mono-01 sawtooth: two-point PolyBLEP, driven through the crate's
/// own `Phasor` and `saw` so this measures the real code and not a copy of it.
struct Shipped {
    ph: Phasor,
}

impl Shipped {
    fn new(inc: f64) -> Self {
        let mut ph = Phasor::new();
        ph.set_inc(inc as f32);
        Self { ph }
    }
}

impl Osc for Shipped {
    fn next(&mut self) -> f64 {
        let y = saw(&self.ph);
        self.ph.advance();
        y as f64
    }
}

/// Four-point PolyBLEP residual, built on the third-order B-spline basis.
///
/// Polynomials from Välimäki, Pekonen and Nam (*JASA* 131(1), 2012), Table VII,
/// cross-checked against Esqueda, Välimäki and Bilbao (DAFx-16), Table 1, whose
/// four-point polyBLAMP residual differentiates to exactly these four.
///
/// All four spans take the *same* `d`: the fraction of a sample by which the first
/// sample after the discontinuity trails it. That is the same parameterisation the
/// two-point residual in `oscillator.rs` uses.
#[inline]
fn blep4(d: f64) -> [f64; 4] {
    let d2 = d * d;
    let d3 = d2 * d;
    let d4 = d2 * d2;
    [
        d4 / 24.0,
        -d4 / 8.0 + d3 / 6.0 + d2 / 4.0 + d / 6.0 + 1.0 / 24.0,
        d4 / 8.0 - d3 / 3.0 + 2.0 * d / 3.0 - 1.0 / 2.0,
        -d4 / 24.0 + d3 / 6.0 - d2 / 4.0 + d / 6.0 - 1.0 / 24.0,
    ]
}

/// Four-point PolyBLEP sawtooth.
///
/// Corrects two samples either side of the discontinuity, so it needs a two-sample
/// output delay: the correction for the sample *before* a transition can only be
/// computed once the transition is known, one sample later.
struct Blep4 {
    phase: f64,
    inc: f64,
    triv: [f64; Self::RING],
    corr: [f64; Self::RING],
    i: usize,
}

impl Blep4 {
    const RING: usize = 8;

    fn new(inc: f64) -> Self {
        let mut osc = Self {
            phase: 0.0,
            inc,
            triv: [0.0; Self::RING],
            corr: [0.0; Self::RING],
            i: 0,
        };
        // Prime the two-sample delay so `next` can emit from the first call.
        osc.step();
        osc.step();
        osc
    }

    /// Advance one sample without emitting.
    fn step(&mut self) {
        let v = 2.0 * self.phase - 1.0;
        self.phase += self.inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            let r = blep4(self.phase / self.inc);
            // The trivial sawtooth jumps by -2 at the wrap.
            let h = -2.0;
            // Spans [-2T,-T], [-T,0], [0,T], [T,2T] are samples i-1, i, i+1, i+2.
            let i = self.i;
            self.corr[(i + Self::RING - 1) % Self::RING] += h * r[0];
            self.corr[i % Self::RING] += h * r[1];
            self.corr[(i + 1) % Self::RING] += h * r[2];
            self.corr[(i + 2) % Self::RING] += h * r[3];
        }
        self.triv[self.i % Self::RING] = v;
        self.i += 1;
    }
}

impl Osc for Blep4 {
    fn next(&mut self) -> f64 {
        let k = (self.i + Self::RING - 2) % Self::RING;
        let y = self.triv[k] + self.corr[k];
        self.corr[k] = 0.0;
        self.step();
        y
    }
}

/// Second-order differentiated parabolic waveform (Välimäki, *IEEE SPL* 12(3), 2005).
struct Dpw2 {
    phase: f64,
    inc: f64,
    /// `1 / (4T)`, precomputed. A division per sample would make the cost figure
    /// a measurement of the divider rather than of the algorithm.
    scale: f64,
    prev: f64,
}

impl Dpw2 {
    fn new(inc: f64) -> Self {
        // Start from the steady-state memory rather than zero, so there is no
        // startup spike even before `SETTLE` gets a chance to hide one.
        let prev = (2.0 * (1.0 - inc) - 1.0).powi(2);
        Self {
            phase: 0.0,
            inc,
            scale: 1.0 / (4.0 * inc),
            prev,
        }
    }
}

impl Osc for Dpw2 {
    fn next(&mut self) -> f64 {
        let tri = 2.0 * self.phase - 1.0;
        let par = tri * tri;
        let y = (par - self.prev) * self.scale;
        self.prev = par;
        self.phase += self.inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        y
    }
}

/// Efficient polynomial transition regions (Ambrits and Bank, SMC 2013): the
/// closed-form equivalent of DPW2 with its half-sample delay removed.
struct Eptr {
    counter: f64,
    t: f64,
    /// `1 / T`, precomputed for the same reason as `Dpw2::scale`.
    inv_t: f64,
}

impl Eptr {
    fn new(inc: f64) -> Self {
        Self {
            counter: -1.0,
            t: inc,
            inv_t: 1.0 / inc,
        }
    }
}

impl Osc for Eptr {
    fn next(&mut self) -> f64 {
        let (p, t, r) = (self.counter, self.t, self.inv_t);
        let y = if p > 1.0 - t {
            p - p * r + r - 1.0
        } else if p < -1.0 + t {
            p - p * r - r + 1.0
        } else {
            p
        };
        self.counter += 2.0 * t;
        if self.counter >= 1.0 {
            self.counter -= 2.0;
        }
        y
    }
}

/// Windowed-sinc decimation filter for the oversampled candidates.
///
/// 32 taps per output sample: long enough that the filter is not what limits the
/// result. A shipping implementation would use a polyphase halfband cascade, so
/// treat the cost figure for these as an upper bound and the quality figure as
/// what oversampling can buy at its best.
fn decimation_taps(factor: usize) -> Vec<f64> {
    let len = 64 * factor + 1;
    let fc = 0.45 / factor as f64;
    let mid = (len - 1) as f64 / 2.0;
    let mut taps: Vec<f64> = (0..len)
        .map(|i| {
            let x = i as f64 - mid;
            let sinc = if x == 0.0 {
                2.0 * fc
            } else {
                (2.0 * PI * fc * x).sin() / (PI * x)
            };
            let w = 0.42 - 0.5 * (2.0 * PI * i as f64 / (len - 1) as f64).cos()
                + 0.08 * (4.0 * PI * i as f64 / (len - 1) as f64).cos();
            sinc * w
        })
        .collect();
    let sum: f64 = taps.iter().sum();
    for t in taps.iter_mut() {
        *t /= sum;
    }
    taps
}

/// Trivial sawtooth generated at `factor` times the rate, then decimated.
struct Oversampled {
    inner: Trivial,
    taps: Vec<f64>,
    hist: Vec<f64>,
    pos: usize,
    factor: usize,
}

impl Oversampled {
    fn new(inc: f64, factor: usize) -> Self {
        let taps = decimation_taps(factor);
        let hist = vec![0.0; taps.len()];
        Self {
            inner: Trivial::new(inc / factor as f64),
            taps,
            hist,
            pos: 0,
            factor,
        }
    }
}

impl Osc for Oversampled {
    fn next(&mut self) -> f64 {
        for _ in 0..self.factor {
            self.hist[self.pos] = self.inner.next();
            self.pos = (self.pos + 1) % self.hist.len();
        }
        let len = self.hist.len();
        let mut acc = 0.0;
        for (k, &t) in self.taps.iter().enumerate() {
            acc += t * self.hist[(self.pos + k) % len];
        }
        acc
    }
}

/// How a wavetable reads between its stored points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Interp {
    /// Truncate to the sample below. The cheapest possible read, and the one that
    /// turns a wavetable into a sample-rate converter with no filter.
    Drop,
    /// Two-point linear. The usual default.
    Linear,
    /// Four-point cubic (Catmull-Rom): the cheapest read that is not obviously a
    /// lowpass.
    Cubic,
}

/// A bank of band-limited sawtooth tables, `steps_per_octave` of them per octave,
/// each holding every harmonic that fits below Nyquist for the *top* of the band it
/// covers.
///
/// Built once and shared. Constructing a bank is additive synthesis and costs tens
/// of milliseconds: a plugin-load cost, never a `process()` cost.
struct WtBank {
    tables: Vec<Vec<f64>>,
    steps_per_octave: usize,
    len: usize,
    fs: f64,
}

impl WtBank {
    const BASE_HZ: f64 = 20.0;

    fn new(fs: f64, len: usize, steps_per_octave: usize) -> Self {
        let mut tables = Vec::new();
        let mut step = 0usize;
        loop {
            let hz = Self::BASE_HZ * 2f64.powf(step as f64 / steps_per_octave as f64);
            if hz >= fs / 2.0 {
                break;
            }
            // A table cannot hold a harmonic above its own Nyquist: an L-point
            // table carries L/2 - 1 of them. Without this clamp a short table
            // aliases while it is being *built*, and the resulting measurement
            // blames the interpolator for the table's own folding.
            let harmonics = (((fs / 2.0) / hz) as usize).max(1).min(len / 2 - 1);
            let mut t = vec![0.0f64; len];
            for (i, v) in t.iter_mut().enumerate() {
                let ph = 2.0 * PI * i as f64 / len as f64;
                let mut acc = 0.0;
                for k in 1..=harmonics {
                    acc -= (k as f64 * ph).sin() / k as f64;
                }
                *v = acc * 2.0 / PI;
            }
            tables.push(t);
            step += 1;
        }
        Self {
            tables,
            steps_per_octave,
            len,
            fs,
        }
    }

    /// Fractional position of a pitch in the bank. The table at `ceil` of this is
    /// always safe; the one at `floor` holds up to a band's worth more harmonics
    /// than this pitch can carry.
    fn position(&self, hz: f64) -> f64 {
        (hz / Self::BASE_HZ).max(1.0).log2() * self.steps_per_octave as f64
    }

    fn bytes(&self) -> usize {
        self.tables.len() * self.len * 8
    }
}

/// A wavetable oscillator reading one bank.
struct WtOsc<'a> {
    bank: &'a WtBank,
    phase: f64,
    inc: f64,
    interp: Interp,
    /// The table that is safe at this pitch, the brighter one below it, and the
    /// blend between them. `mix = 0` is the safe table alone.
    safe: usize,
    bright: usize,
    mix: f64,
}

impl<'a> WtOsc<'a> {
    fn new(bank: &'a WtBank, inc: f64, interp: Interp, crossfade: bool) -> Self {
        let hz = inc * bank.fs;
        let pos = bank.position(hz);
        let last = bank.tables.len() - 1;
        let safe = (pos.ceil() as usize).min(last);
        let bright = (pos.floor() as usize).min(last);
        // Crossfading toward the brighter table restores harmonics the safe table
        // does not hold, and imports the aliasing that comes with them. `mix` is how
        // far into the band the pitch sits: 0 at the top of it, 1 at the bottom.
        let mix = if crossfade { safe as f64 - pos } else { 0.0 };
        Self {
            bank,
            phase: 0.0,
            inc,
            interp,
            safe,
            bright,
            mix,
        }
    }

    #[inline]
    fn read(&self, table: &[f64], x: f64) -> f64 {
        let len = self.bank.len;
        let i = x as usize % len;
        match self.interp {
            Interp::Drop => table[i],
            Interp::Linear => {
                let f = x - x.floor();
                table[i] * (1.0 - f) + table[(i + 1) % len] * f
            }
            Interp::Cubic => {
                let f = x - x.floor();
                let m1 = table[(i + len - 1) % len];
                let p0 = table[i];
                let p1 = table[(i + 1) % len];
                let p2 = table[(i + 2) % len];
                // Catmull-Rom.
                let a = -0.5 * m1 + 1.5 * p0 - 1.5 * p1 + 0.5 * p2;
                let b = m1 - 2.5 * p0 + 2.0 * p1 - 0.5 * p2;
                let c = -0.5 * m1 + 0.5 * p1;
                ((a * f + b) * f + c) * f + p0
            }
        }
    }
}

impl Osc for WtOsc<'_> {
    fn next(&mut self) -> f64 {
        let x = self.phase * self.bank.len as f64;
        let y = if self.mix > 0.0 {
            let s = self.read(&self.bank.tables[self.safe], x);
            let b = self.read(&self.bank.tables[self.bright], x);
            s * (1.0 - self.mix) + b * self.mix
        } else {
            self.read(&self.bank.tables[self.safe], x)
        };
        self.phase += self.inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        y
    }
}

// ---------------------------------------------------------------------------
// Granular
// ---------------------------------------------------------------------------

/// Grain amplitude envelopes, over `u` in `0..1`.
#[derive(Debug, Clone, Copy, PartialEq)]
enum GrainEnv {
    /// No envelope at all: the grain starts and stops at full amplitude.
    Rect,
    /// Raised cosine. The default in most granular instruments, and the one that
    /// sums to a constant at 50% overlap.
    Hann,
    /// Raised-cosine ends with a flat middle. `r` is the fraction of the grain
    /// spent on the two tapers together.
    Tukey(f64),
    /// Truncated Gaussian, `sigma` in units of half the grain length.
    Gauss(f64),
    /// A FOF-shaped burst: half a raised cosine rising over `rise` of the grain,
    /// then exponential decay. The envelope Rodet's formant-wave-function synthesis
    /// uses, and the one whose *end* is a discontinuity.
    Fof { rise: f64, decay: f64 },
}

impl GrainEnv {
    #[inline]
    fn at(self, u: f64) -> f64 {
        if !(0.0..1.0).contains(&u) {
            return 0.0;
        }
        match self {
            GrainEnv::Rect => 1.0,
            GrainEnv::Hann => 0.5 - 0.5 * (2.0 * PI * u).cos(),
            GrainEnv::Tukey(r) => {
                let r = r.clamp(1e-6, 1.0);
                if u < r / 2.0 {
                    0.5 - 0.5 * (2.0 * PI * u / r).cos()
                } else if u > 1.0 - r / 2.0 {
                    0.5 - 0.5 * (2.0 * PI * (1.0 - u) / r).cos()
                } else {
                    1.0
                }
            }
            GrainEnv::Gauss(sigma) => {
                let x = (u - 0.5) / (sigma * 0.5);
                (-0.5 * x * x).exp()
            }
            GrainEnv::Fof { rise, decay } => {
                if u < rise {
                    0.5 - 0.5 * (PI * u / rise).cos()
                } else {
                    (-(u - rise) / decay).exp()
                }
            }
        }
    }

    fn name(self) -> String {
        match self {
            GrainEnv::Rect => "rect".to_string(),
            GrainEnv::Hann => "hann".to_string(),
            GrainEnv::Tukey(r) => format!("tukey {r:.2}"),
            GrainEnv::Gauss(s) => format!("gauss {s:.2}"),
            GrainEnv::Fof { rise, .. } => format!("fof {rise:.3}"),
        }
    }
}

/// Where a grain's carrier waveform comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Carrier {
    /// `f64::sin` per grain per sample.
    Sin,
    /// A 2048-point sine table with linear interpolation — the same read as a
    /// wavetable oscillator, and the usual way a grain engine avoids paying for a
    /// transcendental function once per grain per sample.
    Table,
}

/// One shared sine table for every grain in the run.
fn sine_table() -> &'static [f64] {
    static TABLE: std::sync::OnceLock<Vec<f64>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        (0..2048)
            .map(|i| (2.0 * PI * i as f64 / 2048.0).sin())
            .collect()
    })
}

#[derive(Debug, Clone, Copy, Default)]
struct Grain {
    active: bool,
    /// Onset in samples, fractional unless the scheduler quantises it.
    start: f64,
    len_samples: f64,
    carrier_inc: f64,
}

/// A grain train: a fixed pool of overlapping windowed sinusoids.
///
/// A fixed pool rather than a growing list, because that is what a realtime
/// implementation has to do. Grains that would exceed the pool are dropped, and the
/// drops are counted rather than hidden.
struct GrainTrain {
    fs: f64,
    /// Grains per second.
    rate: f64,
    /// Grain length in seconds.
    dur: f64,
    /// Carrier frequency inside the grain — the formant.
    formant: f64,
    env: GrainEnv,
    /// Round each onset to the nearest sample instead of placing it fractionally.
    quantise: bool,
    grains: Vec<Grain>,
    next_onset: f64,
    n: f64,
    dropped: usize,
    /// Random onset displacement, in units of the grain period. Zero is synchronous.
    jitter: f64,
    carrier: Carrier,
    /// Keep the live grains packed at the front of the pool, so the per-sample loop
    /// visits only grains that are sounding. With this off, every sample scans all
    /// `POOL` slots — the obvious implementation, and the one §9e measures against.
    compact: bool,
    live: usize,
    rng: mxm_mono_01_dsp::Rng,
}

impl GrainTrain {
    const POOL: usize = 64;

    fn new(fs: f64, rate: f64, dur: f64, formant: f64, env: GrainEnv) -> Self {
        Self {
            fs,
            rate,
            dur,
            formant,
            env,
            quantise: false,
            grains: vec![Grain::default(); Self::POOL],
            next_onset: 0.0,
            n: 0.0,
            dropped: 0,
            jitter: 0.0,
            carrier: Carrier::Sin,
            compact: false,
            live: 0,
            rng: mxm_mono_01_dsp::Rng::new(0x6772_6169),
        }
    }

    fn compacted(mut self) -> Self {
        self.compact = true;
        self
    }

    fn with_carrier(mut self, carrier: Carrier) -> Self {
        self.carrier = carrier;
        self
    }

    /// One sample of the grain's carrier, `t` samples after its onset.
    #[inline]
    fn carrier_at(&self, g: &Grain, t: f64) -> f64 {
        match self.carrier {
            Carrier::Sin => (2.0 * PI * g.carrier_inc * t).sin(),
            Carrier::Table => {
                let table = sine_table();
                // `x - x.floor()` rather than `rem_euclid`, which is a division.
                let raw = g.carrier_inc * t;
                let phase = raw - raw.floor();
                let x = phase * table.len() as f64;
                let i = x as usize % table.len();
                let f = x - x.floor();
                table[i] * (1.0 - f) + table[(i + 1) % table.len()] * f
            }
        }
    }

    fn quantised(mut self) -> Self {
        self.quantise = true;
        self
    }

    fn with_jitter(mut self, j: f64) -> Self {
        self.jitter = j;
        self
    }

    fn spawn(&mut self, at: f64) {
        let g = Grain {
            active: true,
            start: if self.quantise { at.round() } else { at },
            len_samples: self.dur * self.fs,
            carrier_inc: self.formant / self.fs,
        };
        if self.compact {
            if self.live < Self::POOL {
                self.grains[self.live] = g;
                self.live += 1;
            } else {
                self.dropped += 1;
            }
        } else {
            match self.grains.iter().position(|x| !x.active) {
                Some(i) => self.grains[i] = g,
                None => self.dropped += 1,
            }
        }
    }

    /// Grains actually *sounding* right now — the number that sets the cost.
    ///
    /// Not the same as the number of occupied slots: with onset jitter a grain can
    /// be scheduled ahead of time and sit in the pool waiting, and counting those
    /// makes a sparse cloud look twice as busy as it is.
    fn active(&self) -> usize {
        let sounding = |g: &Grain| {
            let t = self.n - g.start;
            (0.0..g.len_samples).contains(&t)
        };
        if self.compact {
            self.grains[..self.live]
                .iter()
                .filter(|g| sounding(g))
                .count()
        } else {
            self.grains
                .iter()
                .filter(|g| g.active && sounding(g))
                .count()
        }
    }
}

impl Osc for GrainTrain {
    fn next(&mut self) -> f64 {
        let period = self.fs / self.rate;
        while self.n >= self.next_onset {
            let at = if self.jitter > 0.0 {
                self.next_onset + self.jitter * period * self.rng.next_bipolar() as f64
            } else {
                self.next_onset
            };
            self.spawn(at.max(0.0));
            self.next_onset += period;
        }

        let mut y = 0.0;
        if self.compact {
            let mut i = 0usize;
            while i < self.live {
                let g = self.grains[i];
                let t = self.n - g.start;
                if t >= g.len_samples {
                    // Swap the dead grain out and re-test this slot.
                    self.live -= 1;
                    self.grains[i] = self.grains[self.live];
                    continue;
                }
                if t >= 0.0 {
                    y += self.env.at(t / g.len_samples) * self.carrier_at(&g, t);
                }
                i += 1;
            }
        } else {
            for g in self.grains.iter_mut() {
                if !g.active {
                    continue;
                }
                let t = self.n - g.start;
                if t < 0.0 {
                    continue;
                }
                if t >= g.len_samples {
                    g.active = false;
                    continue;
                }
                // The carrier's phase is measured from the grain's own fractional
                // onset, so a grain that starts between two samples is rendered as
                // starting between two samples rather than nudged onto the grid.
                let u = t / g.len_samples;
                y += self.env.at(u) * (2.0 * PI * g.carrier_inc * t).sin();
            }
        }
        self.n += 1.0;
        y
    }
}
// ---------------------------------------------------------------------------
// Granular: what a sample-reading grain costs
// ---------------------------------------------------------------------------

/// How a grain interpolates the source it reads.
///
/// Both shipped grain engines in this collection are on this list.
/// `Sinc16Recurrence` is `mxm-creative-sampler-dsp`'s kernel transcribed — it walks
/// its sinc and its Blackman window with a `sin_cos` recurrence and pays one
/// division per tap. `Linear` is `mxm-grain-fx-dsp`'s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kernel {
    /// Two taps.
    Linear,
    /// Four-point cubic Hermite (Catmull-Rom).
    Cubic,
    /// Eight-tap windowed sinc read from a polyphase table, `SINC_PHASES` phases
    /// deep. The coefficients are precomputed and normalised once, so the inner
    /// loop is loads and multiply-adds and nothing else.
    Sinc8Table,
    /// Sixteen-tap windowed sinc with the coefficients computed per read.
    Sinc16Recurrence,
}

impl Kernel {
    /// Taps needed before the read position, and taps from it onward.
    fn span(self) -> (i64, i64) {
        match self {
            Kernel::Linear => (0, 1),
            Kernel::Cubic => (1, 2),
            Kernel::Sinc8Table => (3, 4),
            Kernel::Sinc16Recurrence => (7, 8),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Kernel::Linear => "linear",
            Kernel::Cubic => "cubic",
            Kernel::Sinc8Table => "sinc8 table",
            Kernel::Sinc16Recurrence => "sinc16 recur",
        }
    }
}

/// How a grain gets its envelope value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GrainWin {
    /// `0.5 - 0.5 * (TAU * phase).cos()` per grain per sample — what both shipped
    /// engines do.
    Cosine,
    /// One shared table, linearly interpolated.
    Table,
}

/// How a grain resolves the ends of the region it reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bounds {
    /// `rem_euclid` per tap, as `mxm-creative-sampler-dsp`'s `frame()` does.
    Wrapped,
    /// The source carries `PAD` frames of margin at each end and the spawn places
    /// the grain so that its whole life, kernel span included, stays inside. The
    /// inner loop then tests nothing.
    Padded,
}

/// How the engine walks its pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Walk {
    /// Visit every pool slot, every sample.
    Scan,
    /// Keep the live grains packed and visit only those, every sample.
    Compact,
    /// Compact, and render each grain across a whole block before moving to the
    /// next, so its state is loaded once per block instead of once per sample.
    Block,
}

/// Frames of margin at each end of the source.
const PAD: usize = 8;
/// Fractional positions in the polyphase sinc table.
const SINC_PHASES: usize = 512;
/// Taps in the polyphase sinc table.
const SINC8: usize = 8;
/// Entries in the shared grain-window table.
const WIN_POINTS: usize = 1024;
/// Frames rendered per pass in `Walk::Block`.
const GRAIN_BLOCK: usize = 64;

/// The source every grain in §9f reads: planar, `f32`, padded at both ends.
///
/// `f32` and planar because that is what ships, and one second of it because a
/// grain engine's cost depends on where its reads land in cache — a source small
/// enough to sit in L1 would measure a machine nobody has.
fn grain_source() -> &'static (Vec<f32>, Vec<f32>) {
    static SRC: std::sync::OnceLock<(Vec<f32>, Vec<f32>)> = std::sync::OnceLock::new();
    SRC.get_or_init(|| {
        let frames = 48_000usize;
        let mut left = vec![0.0f32; frames + 2 * PAD];
        let mut right = vec![0.0f32; frames + 2 * PAD];
        let mut phase = 0.0f64;
        for i in 0..frames {
            let t = i as f64 / frames as f64;
            phase += 2.0 * PI * (80.0 + 6_000.0 * t) / 48_000.0;
            let tone = phase.sin() * 0.6;
            let transient = if i % 3_000 < 24 { 0.4 } else { 0.0 };
            left[PAD + i] = (tone + transient) as f32;
            right[PAD + i] = (tone - transient) as f32;
        }
        (left, right)
    })
}

/// The polyphase sinc table: `SINC_PHASES` fractional positions, `SINC8` taps each.
///
/// Normalised per phase here, once, rather than per read. The sampler's kernel sums
/// its weights and divides by the sum on every grain on every sample; that division
/// is a property of computing the coefficients late, not of the sinc.
fn sinc8_table() -> &'static [f32] {
    static TABLE: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = vec![0.0f32; SINC_PHASES * SINC8];
        for p in 0..SINC_PHASES {
            let frac = p as f64 / SINC_PHASES as f64;
            let mut sum = 0.0f64;
            for tap in 0..SINC8 {
                let x = tap as f64 - (SINC8 / 2 - 1) as f64 - frac;
                let sinc = if x.abs() < 1.0e-9 {
                    1.0
                } else {
                    (PI * x).sin() / (PI * x)
                };
                let u = (x + SINC8 as f64 / 2.0) / SINC8 as f64;
                let w = 0.42 - 0.5 * (2.0 * PI * u).cos() + 0.08 * (4.0 * PI * u).cos();
                let weight = sinc * w;
                table[p * SINC8 + tap] = weight as f32;
                sum += weight;
            }
            if sum.abs() > 1.0e-9 {
                for tap in 0..SINC8 {
                    table[p * SINC8 + tap] = (f64::from(table[p * SINC8 + tap]) / sum) as f32;
                }
            }
        }
        table
    })
}

/// One shared Hann table for every grain in the run.
fn grain_window_table() -> &'static [f32] {
    static TABLE: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        (0..=WIN_POINTS)
            .map(|i| {
                let u = i as f64 / WIN_POINTS as f64;
                (0.5 - 0.5 * (2.0 * PI * u).cos()) as f32
            })
            .collect()
    })
}

#[derive(Debug, Clone, Copy, Default)]
struct SampleGrain {
    active: bool,
    /// Read position in source frames, before the pad.
    pos: f32,
    /// Source frames advanced per output frame — the grain's pitch, latched.
    step: f32,
    age: u32,
    len: u32,
    win_phase: f32,
    win_inc: f32,
    /// Pan resolved at spawn, because it is constant for a grain's life.
    gain_l: f32,
    gain_r: f32,
    /// Offset into the current block at which this grain starts sounding. Only
    /// `Walk::Block` reads it; it is what keeps a block render sample-exact.
    born_at: u32,
}

/// A cloud of windowed reads from a stored sample — what every granular instrument
/// in chapter 15 actually renders, as against §9a-e's windowed sines.
///
/// Every variant below is a *cost* decision. All of them render the same cloud to
/// within their interpolator's error, and the point of the section is that the
/// spread between them is two orders of magnitude.
struct GrainCloud {
    fs: f64,
    rate: f64,
    dur: f64,
    kernel: Kernel,
    win: GrainWin,
    bounds: Bounds,
    walk: Walk,
    stereo: bool,
    /// Playback rate for every grain. 1.0 reads the source at its own pitch; 4.0 is
    /// two octaves up and strides four times the memory per output frame.
    step: f32,
    pool: usize,
    grains: Vec<SampleGrain>,
    /// Grains packed at the front, under `Walk::Compact` and `Walk::Block`.
    live: usize,
    next_onset: f64,
    n: f64,
    dropped: usize,
    rng: mxm_mono_01_dsp::Rng,
    /// `Walk::Block` renders ahead into these and hands out one frame per `next()`.
    block_l: Vec<f32>,
    block_r: Vec<f32>,
    block_pos: usize,
    block_len: usize,
}

impl GrainCloud {
    fn new(fs: f64, rate: f64, dur: f64, kernel: Kernel) -> Self {
        // Size the pool from `density x duration`, as 10-granular.md §10.5 says to.
        // Scan against compact is then measured at a correctly sized pool, rather
        // than against one that is mostly empty by construction.
        let live = (rate * dur).ceil().max(1.0);
        let pool = ((live * 1.5).ceil() as usize).max(4);
        Self {
            fs,
            rate,
            dur,
            kernel,
            win: GrainWin::Cosine,
            bounds: Bounds::Wrapped,
            walk: Walk::Scan,
            stereo: true,
            step: 1.0,
            pool,
            grains: vec![SampleGrain::default(); pool],
            live: 0,
            next_onset: 0.0,
            n: 0.0,
            dropped: 0,
            rng: mxm_mono_01_dsp::Rng::new(0x51ED_2701),
            block_l: vec![0.0; GRAIN_BLOCK],
            block_r: vec![0.0; GRAIN_BLOCK],
            block_pos: 0,
            block_len: 0,
        }
    }

    fn with_window(mut self, win: GrainWin) -> Self {
        self.win = win;
        self
    }

    fn with_bounds(mut self, bounds: Bounds) -> Self {
        self.bounds = bounds;
        self
    }

    fn with_walk(mut self, walk: Walk) -> Self {
        self.walk = walk;
        self
    }

    fn mono(mut self) -> Self {
        self.stereo = false;
        self
    }

    fn with_step(mut self, step: f32) -> Self {
        self.step = step;
        self
    }

    /// Take a slot and fill it. Returns the slot, or `None` if the pool is full.
    fn spawn(&mut self) -> Option<usize> {
        let slot = match self.walk {
            Walk::Scan => self.grains.iter().position(|g| !g.active),
            _ => (self.live < self.pool).then_some(self.live),
        };
        let Some(slot) = slot else {
            self.dropped += 1;
            return None;
        };
        let (src, _) = grain_source();
        let frames = src.len() - 2 * PAD;
        let len = (self.dur * self.fs).round().max(4.0) as u32;
        let (before, after) = self.kernel.span();
        // Place so that the whole grain, kernel span included, stays inside the
        // source. This is what makes `Bounds::Padded` legal with no per-tap test,
        // and it is `mxm-grain-fx-dsp`'s `place()` in miniature.
        let travel = len as f32 * self.step.abs();
        let room = (frames as f32 - travel - (before + after) as f32 - 2.0).max(1.0);
        let unit = (self.rng.next_bipolar() + 1.0) * 0.5;
        let start = before as f32 + 1.0 + unit * room;
        let angle = (self.rng.next_bipolar() + 1.0) * std::f32::consts::FRAC_PI_4;
        self.grains[slot] = SampleGrain {
            active: true,
            pos: start,
            step: self.step,
            age: 0,
            len,
            win_phase: 0.0,
            win_inc: 1.0 / len as f32,
            gain_l: angle.cos() * std::f32::consts::SQRT_2,
            gain_r: angle.sin() * std::f32::consts::SQRT_2,
            born_at: 0,
        };
        if self.walk != Walk::Scan {
            self.live += 1;
        }
        Some(slot)
    }

    /// One interpolated frame. `pos` is in source frames, before the pad.
    #[inline]
    fn read(&self, pos: f32) -> (f32, f32) {
        let (left, right) = grain_source();
        let frames = (left.len() - 2 * PAD) as i64;
        let base = pos.floor() as i64;
        let frac = pos - base as f32;

        // `Padded` walks contiguous memory. `Wrapped` pays an integer division per
        // tap, which is the shape the sampler ships.
        let idx = |tap: i64| -> usize {
            match self.bounds {
                Bounds::Padded => (base + tap + PAD as i64) as usize,
                Bounds::Wrapped => ((base + tap).rem_euclid(frames) + PAD as i64) as usize,
            }
        };

        match self.kernel {
            Kernel::Linear => {
                let (a, b) = (idx(0), idx(1));
                let l = left[a] + frac * (left[b] - left[a]);
                let r = if self.stereo {
                    right[a] + frac * (right[b] - right[a])
                } else {
                    0.0
                };
                (l, r)
            }
            Kernel::Cubic => {
                let i = [idx(-1), idx(0), idx(1), idx(2)];
                let herm = |x: [f32; 4]| -> f32 {
                    let c0 = x[1];
                    let c1 = 0.5 * (x[2] - x[0]);
                    let c2 = x[0] - 2.5 * x[1] + 2.0 * x[2] - 0.5 * x[3];
                    let c3 = 0.5 * (x[3] - x[0]) + 1.5 * (x[1] - x[2]);
                    ((c3 * frac + c2) * frac + c1) * frac + c0
                };
                let l = herm([left[i[0]], left[i[1]], left[i[2]], left[i[3]]]);
                let r = if self.stereo {
                    herm([right[i[0]], right[i[1]], right[i[2]], right[i[3]]])
                } else {
                    0.0
                };
                (l, r)
            }
            Kernel::Sinc8Table => {
                let table = sinc8_table();
                let phase = ((frac * SINC_PHASES as f32) as usize).min(SINC_PHASES - 1);
                let coef = &table[phase * SINC8..phase * SINC8 + SINC8];
                let mut l = 0.0f32;
                let mut r = 0.0f32;
                for (tap, &c) in coef.iter().enumerate() {
                    let j = idx(tap as i64 - (SINC8 / 2 - 1) as i64);
                    l += left[j] * c;
                    if self.stereo {
                        r += right[j] * c;
                    }
                }
                (l, r)
            }
            Kernel::Sinc16Recurrence => {
                // Transcribed from `mxm-creative-sampler-dsp/src/lib.rs`'s
                // `sinc_read`: adjacent taps differ by constant angles, so the sinc
                // and its Blackman window are walked with a recurrence rather than
                // three transcendentals per tap. The division per tap remains, and
                // so does the normalisation at the end.
                const TAPS: i64 = 16;
                let pi = std::f32::consts::PI;
                let cutoff = 1.0f32;
                let first = -(TAPS / 2 - 1);
                let mut distance = pos - (base + first) as f32;
                let (mut sinc_sin, mut sinc_cos) = (pi * cutoff * distance).sin_cos();
                let (step_sin, step_cos) = (-pi * cutoff).sin_cos();
                let half = TAPS as f32 / 2.0;
                let (mut win_sin, mut win_cos) = (pi * distance / half).sin_cos();
                let (win_step_sin, win_step_cos) = (-pi / half).sin_cos();
                let mut l = 0.0f32;
                let mut r = 0.0f32;
                let mut sum = 0.0f32;
                for tap in first..=TAPS / 2 {
                    let sinc = if distance.abs() < 1.0e-6 {
                        cutoff
                    } else {
                        sinc_sin / (pi * distance)
                    };
                    let window = 0.42 + 0.5 * win_cos + 0.08 * (2.0 * win_cos * win_cos - 1.0);
                    let weight = sinc * window;
                    let j = idx(tap);
                    l += left[j] * weight;
                    if self.stereo {
                        r += right[j] * weight;
                    }
                    sum += weight;
                    (sinc_sin, sinc_cos) = (
                        sinc_sin * step_cos + sinc_cos * step_sin,
                        sinc_cos * step_cos - sinc_sin * step_sin,
                    );
                    (win_sin, win_cos) = (
                        win_sin * win_step_cos + win_cos * win_step_sin,
                        win_cos * win_step_cos - win_sin * win_step_sin,
                    );
                    distance -= 1.0;
                }
                let _ = (sinc_cos, win_sin);
                if sum.abs() > 1.0e-6 {
                    (l / sum, r / sum)
                } else {
                    (l, r)
                }
            }
        }
    }

    #[inline]
    fn envelope(&self, phase: f32) -> f32 {
        match self.win {
            GrainWin::Cosine => 0.5 - 0.5 * (2.0 * std::f32::consts::PI * phase).cos(),
            GrainWin::Table => {
                let table = grain_window_table();
                let x = phase.clamp(0.0, 1.0) * WIN_POINTS as f32;
                let i = (x as usize).min(WIN_POINTS);
                let f = x - i as f32;
                let a = table[i];
                let b = table[(i + 1).min(WIN_POINTS)];
                a + f * (b - a)
            }
        }
    }

    /// One output frame, visiting the pool once per sample.
    #[inline]
    fn render_frame(&mut self) -> (f32, f32) {
        let (mut out_l, mut out_r) = (0.0f32, 0.0f32);
        if self.walk == Walk::Scan {
            for i in 0..self.pool {
                let g = self.grains[i];
                if !g.active {
                    continue;
                }
                let w = self.envelope(g.win_phase);
                let (l, r) = self.read(g.pos);
                out_l += l * w * g.gain_l;
                out_r += r * w * g.gain_r;
                let g = &mut self.grains[i];
                g.pos += g.step;
                g.win_phase += g.win_inc;
                g.age += 1;
                if g.age >= g.len {
                    g.active = false;
                }
            }
        } else {
            let mut i = 0usize;
            while i < self.live {
                let g = self.grains[i];
                if !g.active || g.age >= g.len {
                    // Swap the dead grain out and re-test this slot.
                    self.live -= 1;
                    self.grains[i] = self.grains[self.live];
                    self.grains[self.live].active = false;
                    continue;
                }
                let w = self.envelope(g.win_phase);
                let (l, r) = self.read(g.pos);
                out_l += l * w * g.gain_l;
                out_r += r * w * g.gain_r;
                let g = &mut self.grains[i];
                g.pos += g.step;
                g.win_phase += g.win_inc;
                g.age += 1;
                i += 1;
            }
        }
        (out_l, out_r)
    }

    /// Fire whatever onsets belong to the frame at `self.n`, then advance it.
    #[inline]
    fn schedule(&mut self) {
        let period = self.fs / self.rate;
        while self.n >= self.next_onset {
            self.spawn();
            self.next_onset += period;
        }
        self.n += 1.0;
    }

    /// Render `GRAIN_BLOCK` frames with one grain carried across the whole block.
    ///
    /// The grain's state — position, step, window phase, pan — is loaded once per
    /// block instead of once per sample, and the pool is walked once per block.
    /// Onsets still land on the sample the scheduler chose: every onset in the block
    /// is spawned first and records the frame it belongs to, and its render starts
    /// there. Nothing here rounds an onset to a block boundary, which
    /// [§10.3](../../docs/oscillators/10-granular.md) measured at 71 dB.
    fn render_block(&mut self) {
        let n = GRAIN_BLOCK;
        self.block_l[..n].fill(0.0);
        self.block_r[..n].fill(0.0);
        let period = self.fs / self.rate;
        for i in 0..n {
            while self.n >= self.next_onset {
                if let Some(slot) = self.spawn() {
                    self.grains[slot].born_at = i as u32;
                }
                self.next_onset += period;
            }
            self.n += 1.0;
        }
        for slot in 0..self.live {
            let mut g = self.grains[slot];
            if !g.active {
                continue;
            }
            for i in g.born_at as usize..n {
                if g.age >= g.len {
                    g.active = false;
                    break;
                }
                let w = self.envelope(g.win_phase);
                let (l, r) = self.read(g.pos);
                self.block_l[i] += l * w * g.gain_l;
                self.block_r[i] += r * w * g.gain_r;
                g.pos += g.step;
                g.win_phase += g.win_inc;
                g.age += 1;
            }
            g.born_at = 0;
            self.grains[slot] = g;
        }
        // Compact what died during the block.
        let mut i = 0usize;
        while i < self.live {
            if self.grains[i].active {
                i += 1;
                continue;
            }
            self.live -= 1;
            self.grains[i] = self.grains[self.live];
            self.grains[self.live].active = false;
        }
        self.block_len = n;
        self.block_pos = 0;
    }
}

impl Osc for GrainCloud {
    fn next(&mut self) -> f64 {
        if self.walk == Walk::Block {
            if self.block_pos >= self.block_len {
                self.render_block();
            }
            let y = self.block_l[self.block_pos] + self.block_r[self.block_pos];
            self.block_pos += 1;
            return f64::from(y);
        }
        self.schedule();
        let (l, r) = self.render_frame();
        f64::from(l + r)
    }
}

/// Report one §9f variant: total ns per output sample, and what one live grain in
/// it costs once the engine's fixed cost is taken off.
///
/// The per-grain figure is `(ns - baseline) / live`, and the grain budget is the
/// 48 kHz sample period divided by it. Both are derived from the measurement on the
/// same line, not measured separately.
fn grain_row<T: Osc>(name: &str, make: impl Fn(f64) -> T, live: f64, baseline: f64) -> f64 {
    const REPS: usize = 8;
    const LANES: usize = 4;
    let mut oscs: Vec<T> = (0..LANES).map(|i| make(1.0 + i as f64 * 0.013)).collect();
    for o in oscs.iter_mut() {
        for _ in 0..4096 {
            black_box(o.next());
        }
    }
    let t0 = Instant::now();
    let mut acc = [0.0f64; LANES];
    for _ in 0..REPS {
        for _ in 0..N / LANES {
            for (a, o) in acc.iter_mut().zip(oscs.iter_mut()) {
                *a += o.next();
            }
        }
    }
    black_box(acc);
    let ns = t0.elapsed().as_nanos() as f64 / (REPS * N) as f64;
    let per_grain = ((ns - baseline) / live).max(1.0e-6);
    // One sample at 48 kHz lasts 20833 ns. Grains affordable at 100% of one core is
    // that divided by what one costs.
    let budget = 1.0e9 / 48_000.0 / per_grain;
    println!(
        "      {name:>26}  {ns:>9.1}  {per_grain:>9.2}  {budget:>9.0}  {:>8.0}",
        budget * 0.25
    );
    ns
}

// ---------------------------------------------------------------------------
// Additive
// ---------------------------------------------------------------------------

/// How one partial of an additive bank produces its sine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PartialKind {
    /// `f64::sin` of a phase accumulator.
    Sin,
    /// Shared sine table, linear interpolation.
    Table,
    /// Magic-circle resonator: two state variables, two multiplies and two adds,
    /// no transcendental and no table. Frequency is fixed once `eps` is set, so
    /// retuning means recomputing a sine per partial.
    Resonator,
}

#[derive(Debug, Clone, Copy, Default)]
struct Partial {
    phase: f64,
    inc: f64,
    amp: f64,
    /// Resonator state.
    x: f64,
    y: f64,
    eps: f64,
}

/// A bank of independent sine partials — the engine underneath additive synthesis
/// and additive resynthesis.
struct PartialBank {
    partials: Vec<Partial>,
    kind: PartialKind,
}

impl PartialBank {
    /// `count` harmonics of `f0` with `1/k` amplitudes, normalised so the sum
    /// cannot clip.
    fn new(fs: f64, f0: f64, count: usize, kind: PartialKind) -> Self {
        let norm: f64 = (1..=count).map(|k| 1.0 / k as f64).sum();
        let partials = (1..=count)
            .map(|k| {
                let f = f0 * k as f64;
                let amp = (1.0 / k as f64) / norm;
                let eps = 2.0 * (PI * f / fs).sin();
                Partial {
                    phase: 0.0,
                    inc: f / fs,
                    amp,
                    // Magic-circle initial condition for a sine of unit amplitude.
                    x: 0.0,
                    y: 1.0,
                    eps,
                }
            })
            .collect();
        Self { partials, kind }
    }
}

impl Osc for PartialBank {
    fn next(&mut self) -> f64 {
        let mut acc = 0.0;
        match self.kind {
            PartialKind::Sin => {
                for p in self.partials.iter_mut() {
                    acc += p.amp * (2.0 * PI * p.phase).sin();
                    p.phase += p.inc;
                    if p.phase >= 1.0 {
                        p.phase -= 1.0;
                    }
                }
            }
            PartialKind::Table => {
                let table = sine_table();
                let len = table.len();
                for p in self.partials.iter_mut() {
                    let x = p.phase * len as f64;
                    let i = x as usize % len;
                    let f = x - x.floor();
                    acc += p.amp * (table[i] * (1.0 - f) + table[(i + 1) % len] * f);
                    p.phase += p.inc;
                    if p.phase >= 1.0 {
                        p.phase -= 1.0;
                    }
                }
            }
            PartialKind::Resonator => {
                for p in self.partials.iter_mut() {
                    p.x += p.eps * p.y;
                    p.y -= p.eps * p.x;
                    acc += p.amp * p.x;
                }
            }
        }
        acc
    }
}

/// A harmonic analysis of a source: the amplitude of each partial, frame by frame.
///
/// The pitch is known in advance here, which is the easy case and the one additive
/// resynthesis is best at. Estimating it — and tracking partials that appear,
/// vanish and drift — is the hard part of the real problem, and is not modelled.
struct Analysis {
    /// `frames[i][k]` is the amplitude of partial `k+1` at frame `i`.
    frames: Vec<Vec<f64>>,
    hop: usize,
    f0: f64,
    fs: f64,
}

impl Analysis {
    /// Goertzel-style amplitude estimate for each harmonic, per frame. A window of
    /// a whole number of periods means no leakage between harmonics.
    fn of(src: &[f64], fs: f64, f0: f64, partials: usize, hop: usize) -> Self {
        let period = fs / f0;
        let win = (period * 8.0).round() as usize;
        let mut frames = Vec::new();
        let mut start = 0usize;
        while start + win < src.len() {
            let mut amps = Vec::with_capacity(partials);
            for k in 1..=partials {
                let w = 2.0 * PI * f0 * k as f64 / fs;
                let (mut re, mut im) = (0.0f64, 0.0f64);
                for (i, &v) in src[start..start + win].iter().enumerate() {
                    re += v * (w * i as f64).cos();
                    im -= v * (w * i as f64).sin();
                }
                amps.push(2.0 * (re * re + im * im).sqrt() / win as f64);
            }
            frames.push(amps);
            start += hop;
        }
        Self {
            frames,
            hop,
            f0,
            fs,
        }
    }

    /// Amplitudes at an arbitrary time in samples, linearly interpolated between
    /// frames. Reading this at `t / stretch` is the whole of time-stretching.
    fn at(&self, t: f64, out: &mut [f64]) {
        let x = t / self.hop as f64;
        let i = (x.floor() as usize).min(self.frames.len().saturating_sub(1));
        let j = (i + 1).min(self.frames.len() - 1);
        let f = (x - x.floor()).clamp(0.0, 1.0);
        for (k, v) in out.iter_mut().enumerate() {
            *v = self.frames[i][k] * (1.0 - f) + self.frames[j][k] * f;
        }
    }
}

/// Additive resynthesis of an `Analysis`, at any rate.
///
/// The partial frequencies never move, whatever the stretch factor is. That is the
/// property the whole technique exists for: time and pitch are separate parameters
/// rather than two consequences of one playback rate.
struct Resynth<'a> {
    analysis: &'a Analysis,
    stretch: f64,
    phases: Vec<f64>,
    amps: Vec<f64>,
    n: f64,
}

impl<'a> Resynth<'a> {
    fn new(analysis: &'a Analysis, stretch: f64) -> Self {
        let count = analysis.frames[0].len();
        Self {
            analysis,
            stretch,
            phases: vec![0.0; count],
            amps: vec![0.0; count],
            n: 0.0,
        }
    }
}

impl Osc for Resynth<'_> {
    fn next(&mut self) -> f64 {
        self.analysis.at(self.n / self.stretch, &mut self.amps);
        let table = sine_table();
        let len = table.len();
        let mut acc = 0.0;
        for (k, phase) in self.phases.iter_mut().enumerate() {
            let inc = self.analysis.f0 * (k + 1) as f64 / self.analysis.fs;
            let x = *phase * len as f64;
            let i = x as usize % len;
            let f = x - x.floor();
            acc += self.amps[k] * (table[i] * (1.0 - f) + table[(i + 1) % len] * f);
            *phase += inc;
            if *phase >= 1.0 {
                *phase -= 1.0;
            }
        }
        self.n += 1.0;
        acc
    }
}

/// Naive overlap-add time stretch: Hann-windowed chunks read at one rate and
/// written at another, with no attempt to align their phases.
///
/// The baseline every frequency-domain method is measured against, and the source
/// of the comb-filtered, phasey sound that gives time-stretching its reputation.
fn ola_stretch(src: &[f64], stretch: f64, grain: usize, out_len: usize) -> Vec<f64> {
    let hop_out = grain / 2;
    let hop_in = hop_out as f64 / stretch;
    let mut out = vec![0.0f64; out_len + grain];
    let mut frame = 0usize;
    loop {
        let start_out = frame * hop_out;
        if start_out >= out_len {
            break;
        }
        let start_in = (frame as f64 * hop_in) as usize;
        if start_in + grain >= src.len() {
            break;
        }
        for i in 0..grain {
            let w = 0.5 - 0.5 * (2.0 * PI * i as f64 / grain as f64).cos();
            out[start_out + i] += w * src[start_in + i];
        }
        frame += 1;
    }
    out.truncate(out_len);
    out
}

/// Peak-to-trough variation of a signal's short-term RMS, in dB: how much a
/// steady tone wobbles once something has processed it.
fn envelope_ripple_db(x: &[f64], fs: f64) -> f64 {
    let win = (fs * 0.010) as usize;
    let mut lo = f64::MAX;
    let mut hi: f64 = 0.0;
    let mut i = win; // skip the first window, which is still filling
    while i + win < x.len() {
        let rms = (x[i..i + win].iter().map(|v| v * v).sum::<f64>() / win as f64).sqrt();
        if rms > 1e-6 {
            lo = lo.min(rms);
            hi = hi.max(rms);
        }
        i += win;
    }
    if lo == f64::MAX {
        0.0
    } else {
        20.0 * (hi / lo).log10()
    }
}

/// Fundamental frequency from the largest spectral peak, by parabolic
/// interpolation over the three bins around it.
fn peak_frequency(x: &[f64], fs: f64) -> f64 {
    let n = x.len().next_power_of_two().min(N);
    let mut re = vec![0.0f64; n];
    let mut im = vec![0.0f64; n];
    for i in 0..n.min(x.len()) {
        // Hann, because this buffer is not exactly periodic.
        let w = 0.5 - 0.5 * (2.0 * PI * i as f64 / n as f64).cos();
        re[i] = x[i] * w;
    }
    fft(&mut re, &mut im).expect("the harness allocates power-of-two buffers");
    let mag: Vec<f64> = (0..n / 2)
        .map(|i| (re[i] * re[i] + im[i] * im[i]).sqrt())
        .collect();
    let mut best = 1usize;
    for i in 2..n / 2 - 1 {
        if mag[i] > mag[best] {
            best = i;
        }
    }
    let (a, b, c) = (mag[best - 1], mag[best], mag[best + 1]);
    // Quadratic peak interpolation. The denominator is negative at a peak, so it
    // must not be clamped to a small positive number — that turns a flat top into
    // an enormous offset rather than into no offset at all.
    let denom = a - 2.0 * b + c;
    let delta = if denom.abs() > 1e-30 {
        (0.5 * (a - c) / denom).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    (best as f64 + delta) * fs / n as f64
}

// ---------------------------------------------------------------------------
// Frequency and phase modulation
// ---------------------------------------------------------------------------

/// Whether the modulator is added to the carrier's phase or to its frequency.
///
/// For a sinusoidal modulator the two are spectrally identical — a phase deviation
/// of `I` radians is a frequency deviation of `I·fm` — which is why the DX7 could
/// implement "FM" as phase modulation and nobody minded. They stop being identical
/// the moment the modulator has a non-zero mean: see §11d.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FmKind {
    Phase,
    Frequency,
}

/// One carrier, one modulator, one index: the two-operator FM pair.
struct FmPair {
    fs: f64,
    fc: f64,
    fm: f64,
    index: f64,
    kind: FmKind,
    /// Constant added to the modulator's output. Zero for a plain sine; non-zero is
    /// what a modulator stack, an offset envelope or a feedback path produces.
    mod_offset: f64,
    car_phase: f64,
    mod_phase: f64,
    /// Total carrier phase advance in cycles, never wrapped. Divided by elapsed
    /// time this is the mean carrier frequency, which is the quantity phase and
    /// frequency modulation actually disagree about.
    advance: f64,
    samples: f64,
}

impl FmPair {
    fn new(fs: f64, fc: f64, fm: f64, index: f64, kind: FmKind) -> Self {
        Self {
            fs,
            fc,
            fm,
            index,
            kind,
            mod_offset: 0.0,
            car_phase: 0.0,
            mod_phase: 0.0,
            advance: 0.0,
            samples: 0.0,
        }
    }

    /// Mean carrier frequency over everything rendered so far.
    fn mean_carrier_hz(&self) -> f64 {
        if self.samples > 0.0 {
            self.advance * self.fs / self.samples
        } else {
            0.0
        }
    }

    fn with_offset(mut self, offset: f64) -> Self {
        self.mod_offset = offset;
        self
    }
}

impl Osc for FmPair {
    fn next(&mut self) -> f64 {
        let m = (2.0 * PI * self.mod_phase).sin() + self.mod_offset;
        self.mod_phase += self.fm / self.fs;
        if self.mod_phase >= 1.0 {
            self.mod_phase -= 1.0;
        }

        let y = match self.kind {
            FmKind::Phase => {
                let y = (2.0 * PI * self.car_phase + self.index * m).sin();
                let step = self.fc / self.fs;
                self.car_phase += step;
                self.advance += step;
                y
            }
            FmKind::Frequency => {
                let y = (2.0 * PI * self.car_phase).sin();
                // Instantaneous frequency, which at a high index goes negative for
                // part of every cycle. The phase then runs backwards, which is
                // legal and is why the wrap below has to handle both directions.
                let step = (self.fc + self.index * self.fm * m) / self.fs;
                self.car_phase += step;
                self.advance += step;
                y
            }
        };
        while self.car_phase >= 1.0 {
            self.car_phase -= 1.0;
        }
        while self.car_phase < 0.0 {
            self.car_phase += 1.0;
        }
        self.samples += 1.0;
        y
    }
}

/// An operator modulating itself, as in the DX7's feedback path.
///
/// The two-sample average is the hardware's trick for damping the loop: without it
/// the feedback path is a delay-free loop with a unit delay standing in for it, and
/// it breaks into oscillation at a much lower feedback amount.
struct FmFeedback {
    fs: f64,
    f0: f64,
    feedback: f64,
    phase: f64,
    y1: f64,
    y2: f64,
}

impl FmFeedback {
    fn new(fs: f64, f0: f64, feedback: f64) -> Self {
        Self {
            fs,
            f0,
            feedback,
            phase: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }
}

impl Osc for FmFeedback {
    fn next(&mut self) -> f64 {
        let avg = 0.5 * (self.y1 + self.y2);
        let y = (2.0 * PI * self.phase + self.feedback * avg).sin();
        self.phase += self.f0 / self.fs;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// A chain of `n` operators, each modulating the next: the cost of a real algorithm
/// rather than of a single pair.
struct FmStack {
    ops: Vec<FmPair>,
}

impl FmStack {
    fn new(fs: f64, f0: f64, n: usize) -> Self {
        Self {
            ops: (0..n)
                .map(|i| FmPair::new(fs, f0 * (i + 1) as f64, f0, 1.0, FmKind::Phase))
                .collect(),
        }
    }
}

impl Osc for FmStack {
    fn next(&mut self) -> f64 {
        let mut acc = 0.0;
        for op in self.ops.iter_mut() {
            acc += op.next();
        }
        acc / self.ops.len() as f64
    }
}

// ---------------------------------------------------------------------------
// Phase distortion
// ---------------------------------------------------------------------------

/// Casio-style phase distortion: read a cosine, but bend the phase on the way in.
///
/// The phase-shaping function is two straight lines meeting at `(d, 0.5)`, so the
/// first `d` of the cycle is traversed at `0.5/d` times the nominal rate and the
/// rest at `0.5/(1-d)`. At `d = 0.5` the function is linear and the output is a
/// pure cosine; as `d` falls the first half-cycle is crammed into a shrinking
/// window and the spectrum brightens, which is what Casio sold as a filter.
///
/// Formulation from Kleimola, Lazzarini, Välimäki and Timoney (DAFx-11), which
/// treats classic PD as the one-breakpoint case of phaseshaping.
struct PhaseDistort {
    fs: f64,
    f0: f64,
    d: f64,
    phase: f64,
}

impl PhaseDistort {
    fn new(fs: f64, f0: f64, d: f64) -> Self {
        Self {
            fs,
            f0,
            d: d.clamp(1e-6, 1.0 - 1e-6),
            phase: 0.0,
        }
    }

    /// Instantaneous frequency of the fast segment. Aliasing starts when this
    /// passes Nyquist, which is the whole of the method's alias story.
    fn fast_segment_hz(&self) -> f64 {
        self.f0 * 0.5 / self.d
    }
}

impl Osc for PhaseDistort {
    fn next(&mut self) -> f64 {
        let g = if self.phase < self.d {
            0.5 * self.phase / self.d
        } else {
            0.5 + 0.5 * (self.phase - self.d) / (1.0 - self.d)
        };
        let y = (2.0 * PI * g).cos();
        self.phase += self.f0 / self.fs;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        y
    }
}

/// The window shapes the CZ used for its resonant waveforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PdWindow {
    /// Falls linearly from 1 to 0 across the fundamental period: the saw-resonance
    /// shape, and the one whose reset is a full-height step.
    Saw,
    /// Rises then falls.
    Triangle,
    /// Flat, then falls over the last quarter.
    Trapezoid,
}

impl PdWindow {
    #[inline]
    fn at(self, u: f64) -> f64 {
        match self {
            PdWindow::Saw => 1.0 - u,
            PdWindow::Triangle => 1.0 - (2.0 * u - 1.0).abs(),
            PdWindow::Trapezoid => {
                if u < 0.75 {
                    1.0
                } else {
                    (1.0 - u) * 4.0
                }
            }
        }
    }

    /// Window value just after the reset, which is the height of the step there.
    #[inline]
    fn start(self) -> f64 {
        self.at(0.0)
    }
}

/// A CZ resonant waveform: a sine at `k` times the fundamental, restarted every
/// fundamental period and windowed so it dies away before the restart.
///
/// This is hard sync and formant synthesis at the same time — the same object as
/// the synchronous grain train of §9, with the grain rate locked to the pitch and
/// the grain's carrier setting the resonant peak.
struct PdResonant {
    fs: f64,
    f0: f64,
    k: f64,
    window: PdWindow,
    /// Correct the reset step with the shipped two-point PolyBLEP.
    corrected: bool,
    phase: f64,
}

impl PdResonant {
    fn new(fs: f64, f0: f64, k: f64, window: PdWindow, corrected: bool) -> Self {
        Self {
            fs,
            f0,
            k,
            window,
            corrected,
            phase: 0.0,
        }
    }
}

impl Osc for PdResonant {
    fn next(&mut self) -> f64 {
        let dt = self.f0 / self.fs;
        let mut y = self.window.at(self.phase) * (2.0 * PI * self.k * self.phase).cos();
        if self.corrected {
            // The waveform jumps from zero back to `window.start()` at every reset.
            // `poly_blep` returns twice the residual, so the scale is h/2 — the same
            // convention `saw()` uses with its jump of -2.
            let h = self.window.start();
            y += 0.5 * h * poly_blep_at(self.phase, dt);
        }
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        y
    }
}

/// The shipped two-point PolyBLEP residual, in `f64` and callable directly.
///
/// `oscillator.rs` keeps its own copy private and reaches it through `saw` and
/// `pulse`; this is the same polynomial, transcribed so the phase-distortion
/// experiment can apply it to a discontinuity those two functions do not know
/// about. Any change there has to be mirrored here.
#[inline]
fn poly_blep_at(t: f64, dt: f64) -> f64 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

// ---------------------------------------------------------------------------
// Sampler playback
// ---------------------------------------------------------------------------

/// How a sampler's converter quantises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Converter {
    /// Uniform steps across the whole range: the modern default, and what a
    /// bit-crusher does.
    Linear(u32),
    /// Logarithmic companding to 8 bits, the mu-255 law the AM6070 in an
    /// Emulator II implements. Coarse steps at the top, fine steps near zero.
    ///
    /// This is the analytic law. Real companding converters use the piecewise
    /// linear segmented approximation of G.711, which tracks it closely but is not
    /// identical.
    MuLaw,
}

impl Converter {
    #[inline]
    fn quantise(self, x: f64) -> f64 {
        match self {
            Converter::Linear(bits) => {
                let levels = (1u32 << (bits - 1)) as f64;
                (x * levels).round().clamp(-levels, levels - 1.0) / levels
            }
            Converter::MuLaw => {
                const MU: f64 = 255.0;
                let x = x.clamp(-1.0, 1.0);
                // Compress, quantise uniformly to 8 bits, expand.
                let c = x.signum() * (1.0 + MU * x.abs()).ln() / (1.0 + MU).ln();
                let q = (c * 128.0).round().clamp(-128.0, 127.0) / 128.0;
                q.signum() * ((1.0 + MU).powf(q.abs()) - 1.0) / MU
            }
        }
    }

    fn name(self) -> String {
        match self {
            Converter::Linear(b) => format!("{b}-bit linear"),
            Converter::MuLaw => "8-bit mu-law".to_string(),
        }
    }
}

/// An ideal sawtooth of bandwidth `source_bw_hz`, evaluated at a sample rate of
/// `fs_v`.
///
/// Evaluating a band-limited analog signal at the sample instants *is* naive
/// sampling: any harmonic above `fs_v/2` folds on its own, with no modelling
/// required. Limiting `source_bw_hz` to `0.45*fs_v` is what the anti-alias filter
/// in front of a sampler's input does, and omitting it is what a machine without
/// one sounds like.
fn sampled_at(f0: f64, fs_v: f64, source_bw_hz: f64, n: usize) -> Vec<f64> {
    let harmonics = (source_bw_hz / f0) as usize;
    let mut out = vec![0.0f64; n];
    for k in 1..=harmonics.max(1) {
        let a = 2.0 / (PI * k as f64);
        let w = 2.0 * PI * k as f64 * f0 / fs_v;
        for (i, v) in out.iter_mut().enumerate() {
            *v -= a * (w * i as f64).sin();
        }
    }
    out
}

/// Zero-order hold: repeat each sample `factor` times, which is what a converter
/// does between updates. Introduces the `sinc(pi f / fs)` droop and leaves the
/// spectral images a reconstruction filter is supposed to remove.
fn zero_order_hold(x: &[f64], factor: usize) -> Vec<f64> {
    let mut out = Vec::with_capacity(x.len() * factor);
    for &v in x {
        for _ in 0..factor {
            out.push(v);
        }
    }
    out
}

/// A one-shot sample player: a fixed table read at an arbitrary rate.
///
/// The difference from the wavetable oscillator of §8 is that the table's harmonic
/// content is fixed by the recording, not chosen for the pitch — so transposing up
/// pushes it past Nyquist and no amount of interpolation quality helps.
struct SampleOsc {
    table: Vec<f64>,
    phase: f64,
    inc: f64,
    interp: Interp,
}

impl SampleOsc {
    /// One period of a sawtooth carrying `harmonics` partials.
    fn new(len: usize, harmonics: usize, inc: f64, interp: Interp) -> Self {
        let mut table = vec![0.0f64; len];
        for (i, v) in table.iter_mut().enumerate() {
            let ph = 2.0 * PI * i as f64 / len as f64;
            let mut acc = 0.0;
            for k in 1..=harmonics.min(len / 2 - 1) {
                acc -= (k as f64 * ph).sin() / k as f64;
            }
            *v = acc * 2.0 / PI;
        }
        Self {
            table,
            phase: 0.0,
            inc,
            interp,
        }
    }
}

impl Osc for SampleOsc {
    fn next(&mut self) -> f64 {
        let len = self.table.len();
        let x = self.phase * len as f64;
        let i = x as usize % len;
        let f = x - x.floor();
        let y = match self.interp {
            Interp::Drop => self.table[i],
            Interp::Linear => self.table[i] * (1.0 - f) + self.table[(i + 1) % len] * f,
            Interp::Cubic => {
                let m1 = self.table[(i + len - 1) % len];
                let p0 = self.table[i];
                let p1 = self.table[(i + 1) % len];
                let p2 = self.table[(i + 2) % len];
                let a = -0.5 * m1 + 1.5 * p0 - 1.5 * p1 + 0.5 * p2;
                let b = m1 - 2.5 * p0 + 2.0 * p1 - 0.5 * p2;
                let c = -0.5 * m1 + 0.5 * p1;
                ((a * f + b) * f + c) * f + p0
            }
        };
        self.phase += self.inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        y
    }
}

// ---------------------------------------------------------------------------
// Phase vocoder
// ---------------------------------------------------------------------------

/// What the resynthesis does with each bin's phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PhaseMode {
    /// Carry the analysis phase through unchanged. Correct only at a stretch of
    /// 1.0; this is naive overlap-add in the frequency domain.
    Keep,
    /// Estimate each bin's instantaneous frequency from the phase advance between
    /// analysis frames and integrate it at the synthesis hop. The standard phase
    /// vocoder.
    Propagate,
    /// Throw the analysis phase away and use a new random phase every frame. What
    /// PaulStretch does, and the reason extreme stretches of textural material come
    /// out smooth rather than tonal.
    Randomise,
}

/// A short-time Fourier transform time-stretcher.
///
/// `n_fft` window, Hann on both analysis and synthesis, with a window-sum
/// normalisation so the overlap-add is flat. The only thing that varies between the
/// modes is what happens to phase.
///
/// **The synthesis hop is the fixed one** and the analysis hop is derived from it,
/// not the other way round. Getting this backwards is the classic mistake: with a
/// fixed analysis hop, a stretch of 8 puts the synthesis frames `8 * hop` apart,
/// which for any hop above `n_fft / 8` means the output frames do not overlap — or
/// even touch — and the window-sum normalisation then divides the gaps between them
/// by nearly zero. It measured as a 309-cent pitch error before this was fixed.
fn pvoc_stretch(
    x: &[f64],
    stretch: f64,
    n_fft: usize,
    hop_out: usize,
    mode: PhaseMode,
    out_len: usize,
) -> Vec<f64> {
    let hop_in = ((hop_out as f64) / stretch).round().max(1.0) as usize;
    let ratio = hop_out as f64 / hop_in as f64;
    let window: Vec<f64> = (0..n_fft)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / n_fft as f64).cos())
        .collect();

    let mut out = vec![0.0f64; out_len + n_fft];
    let mut norm = vec![0.0f64; out_len + n_fft];
    let mut prev_phase = vec![0.0f64; n_fft / 2 + 1];
    let mut sum_phase = vec![0.0f64; n_fft / 2 + 1];
    let mut rng = mxm_mono_01_dsp::Rng::new(0x5056_4f43);

    let mut frame = 0usize;
    loop {
        let start_in = frame * hop_in;
        let start_out = frame * hop_out;
        if start_in + n_fft >= x.len() || start_out >= out_len {
            break;
        }

        let mut re: Vec<f64> = (0..n_fft).map(|i| x[start_in + i] * window[i]).collect();
        let mut im = vec![0.0f64; n_fft];
        fft(&mut re, &mut im).expect("the harness allocates power-of-two buffers");

        for k in 0..=n_fft / 2 {
            let mag = (re[k] * re[k] + im[k] * im[k]).sqrt();
            let phase = im[k].atan2(re[k]);

            let out_phase = match mode {
                PhaseMode::Keep => phase,
                PhaseMode::Randomise => PI * rng.next_bipolar() as f64,
                PhaseMode::Propagate => {
                    // Expected advance for this bin over the analysis hop, then the
                    // deviation from it, which is the bin's true frequency offset.
                    let expected = 2.0 * PI * k as f64 * hop_in as f64 / n_fft as f64;
                    let delta = princarg(phase - prev_phase[k] - expected);
                    let true_freq = expected + delta;
                    sum_phase[k] += true_freq * ratio;
                    sum_phase[k]
                }
            };
            prev_phase[k] = phase;

            re[k] = mag * out_phase.cos();
            im[k] = mag * out_phase.sin();
            if k > 0 && k < n_fft / 2 {
                re[n_fft - k] = re[k];
                im[n_fft - k] = -im[k];
            }
        }

        ifft(&mut re, &mut im).expect("the harness allocates power-of-two buffers");
        for i in 0..n_fft {
            if start_out + i < out.len() {
                out[start_out + i] += re[i] * window[i];
                norm[start_out + i] += window[i] * window[i];
            }
        }
        frame += 1;
    }

    for i in 0..out.len() {
        if norm[i] > 1e-9 {
            out[i] /= norm[i];
        }
    }
    out.truncate(out_len);
    out
}

/// Two oscillators multiplied — ring modulation.
///
/// Each input is the shipped band-limited sawtooth. Multiplication in time is
/// convolution in frequency, so every pair of partials produces a sum and a difference
/// — and the sums run to twice the bandwidth of the inputs, which is past Nyquist even
/// when both inputs are perfectly band-limited.
struct RingMod {
    a: Phasor,
    b: Phasor,
    /// Use sines instead of sawtooths, which have two partials rather than hundreds.
    sine: bool,
}

impl RingMod {
    fn new(inc_a: f64, inc_b: f64, sine: bool) -> Self {
        let mut a = Phasor::new();
        let mut b = Phasor::new();
        a.set_inc(inc_a as f32);
        b.set_inc(inc_b as f32);
        Self { a, b, sine }
    }
}

impl Osc for RingMod {
    fn next(&mut self) -> f64 {
        let (x, y) = if self.sine {
            (
                (2.0 * PI * self.a.phase() as f64).sin(),
                (2.0 * PI * self.b.phase() as f64).sin(),
            )
        } else {
            (saw(&self.a) as f64, saw(&self.b) as f64)
        };
        self.a.advance();
        self.b.advance();
        x * y
    }
}

/// Noise with a spectral slope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoiseColour {
    /// Flat. The crate's xorshift, straight out.
    White,
    /// −3 dB/octave, by the usual three-pole approximation.
    Pink,
    /// −6 dB/octave, a leaky integrator.
    Brown,
    /// +6 dB/octave, a difference.
    Blue,
}

struct ColouredNoise {
    rng: mxm_mono_01_dsp::Rng,
    colour: NoiseColour,
    b0: f64,
    b1: f64,
    b2: f64,
    last: f64,
}

impl ColouredNoise {
    fn new(colour: NoiseColour) -> Self {
        Self {
            rng: mxm_mono_01_dsp::Rng::new(0x4e43_4c52),
            colour,
            b0: 0.0,
            b1: 0.0,
            b2: 0.0,
            last: 0.0,
        }
    }
}

impl Osc for ColouredNoise {
    fn next(&mut self) -> f64 {
        let w = self.rng.next_bipolar() as f64;
        match self.colour {
            NoiseColour::White => w,
            NoiseColour::Pink => {
                // Three one-poles staggered by decades: the standard cheap -3 dB/oct.
                self.b0 = 0.99765 * self.b0 + w * 0.0990460;
                self.b1 = 0.96300 * self.b1 + w * 0.2965164;
                self.b2 = 0.57000 * self.b2 + w * 1.0526913;
                (self.b0 + self.b1 + self.b2 + w * 0.1848) * 0.2
            }
            NoiseColour::Brown => {
                self.b0 = (self.b0 + w * 0.02).clamp(-1.0, 1.0);
                self.b0 * 3.0
            }
            NoiseColour::Blue => {
                let y = w - self.last;
                self.last = w;
                y * 0.5
            }
        }
    }
}

/// Least-squares spectral slope in dB per octave, over octave-averaged bands.
fn spectral_slope_db_per_oct(x: &[f64], fs: f64, lo_hz: f64, hi_hz: f64) -> f64 {
    let n = x.len().next_power_of_two().min(N);
    let mut re = vec![0.0f64; n];
    let mut im = vec![0.0f64; n];
    for i in 0..n.min(x.len()) {
        let w = 0.5 - 0.5 * (2.0 * PI * i as f64 / n as f64).cos();
        re[i] = x[i] * w;
    }
    fft(&mut re, &mut im).expect("the harness allocates power-of-two buffers");

    let (mut sx, mut sy, mut sxy, mut sxx, mut count) = (0.0f64, 0.0, 0.0, 0.0, 0usize);
    let mut band_lo = lo_hz;
    while band_lo * 2.0 <= hi_hz {
        let band_hi = band_lo * 2.0;
        let (a, b) = (
            (band_lo * n as f64 / fs) as usize,
            ((band_hi * n as f64 / fs) as usize).min(n / 2),
        );
        if b > a {
            let mut e = 0.0f64;
            for k in a..b {
                e += re[k] * re[k] + im[k] * im[k];
            }
            let power = e / (b - a) as f64;
            let lx = (band_lo * 1.5).log2();
            let ly = 10.0 * power.max(1e-30).log10();
            sx += lx;
            sy += ly;
            sxy += lx * ly;
            sxx += lx * lx;
            count += 1;
        }
        band_lo = band_hi;
    }
    let c = count as f64;
    if count < 2 {
        0.0
    } else {
        (c * sxy - sx * sy) / (c * sxx - sx * sx)
    }
}

/// A sawtooth-like waveform from Moorer's discrete summation formula.
///
/// `Σ_{k=1..K} a^k sin(kθ)` has a closed form, so a waveform with exactly `K` harmonics
/// and no others costs O(1) per sample rather than O(K):
///
/// ```text
/// y = [a sinθ − a^(K+1) sin((K+1)θ) + a^(K+2) sin(Kθ)] / (1 − 2a cosθ + a²)
/// ```
///
/// It is **exactly** band-limited — nothing above harmonic `K` is generated — but the
/// harmonic amplitudes fall geometrically as `a^k` rather than as `1/k`, so the spectral
/// tilt is not a sawtooth's. That trade is the whole character of the method and is what
/// §2.12 measures.
struct Dsf {
    phase: f64,
    inc: f64,
    a: f64,
    k: f64,
    /// `a^(K+1)`, `a^(K+2)` and the output scale are constant for a note.
    /// Recomputing them per sample would make the method look several times more
    /// expensive than it is — the trap the EPTR reciprocal fell into in §2.7.
    a_k1: f64,
    a_k2: f64,
    scale: f64,
}

impl Dsf {
    /// `a` is chosen so the highest harmonic sits `top_db` below the first.
    fn new(inc: f64, harmonics: usize, top_db: f64) -> Self {
        let k = harmonics.max(1);
        let a = 10f64.powf(top_db / (20.0 * k as f64));
        Self {
            phase: 0.0,
            inc,
            a,
            k: k as f64,
            a_k1: a.powf(k as f64 + 1.0),
            a_k2: a.powf(k as f64 + 2.0),
            scale: 1.0 - a,
        }
    }
}

impl Osc for Dsf {
    fn next(&mut self) -> f64 {
        let th = 2.0 * PI * self.phase;
        let (a, k) = (self.a, self.k);
        let den = 1.0 - 2.0 * a * th.cos() + a * a;
        let num = a * th.sin() - self.a_k1 * ((k + 1.0) * th).sin() + self.a_k2 * (k * th).sin();
        self.phase += self.inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        // Scaled so the fundamental sits near unity for comparability.
        num / den.max(1e-12) * self.scale
    }
}

// ---------------------------------------------------------------------------
// Waveshaping
// ---------------------------------------------------------------------------

/// The three shapers measured in §19.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Shaper {
    /// Soft saturation. The everyday case.
    Tanh(f64),
    /// A sine folder — the West Coast timbre control. Smooth, and it folds rather than
    /// clips, so it generates far more bandwidth than saturation does.
    SineFold(f64),
    /// Fifth-order Chebyshev. A polynomial, so its antiderivative is exact and it is the
    /// cleanest demonstration of what ADAA can do.
    Cheb5(f64),
}

impl Shaper {
    #[inline]
    fn f(self, x: f64) -> f64 {
        match self {
            Shaper::Tanh(g) => (g * x).tanh(),
            Shaper::SineFold(g) => (PI * g * x / 2.0).sin(),
            Shaper::Cheb5(g) => {
                let u = (g * x).clamp(-1.0, 1.0);
                16.0 * u.powi(5) - 20.0 * u.powi(3) + 5.0 * u
            }
        }
    }

    /// Antiderivative of `f`. This is the whole of antiderivative antialiasing: if you
    /// can write it down, you get most of an oversampler for one divide.
    #[inline]
    fn antiderivative(self, x: f64) -> f64 {
        match self {
            Shaper::Tanh(g) => (g * x).cosh().ln() / g,
            Shaper::SineFold(g) => -2.0 * (PI * g * x / 2.0).cos() / (PI * g),
            Shaper::Cheb5(g) => {
                let u = (g * x).clamp(-1.0, 1.0);
                (16.0 * u.powi(6) / 6.0 - 20.0 * u.powi(4) / 4.0 + 5.0 * u * u / 2.0) / g
            }
        }
    }

    fn name(self) -> String {
        match self {
            Shaper::Tanh(g) => format!("tanh x{g:.0}"),
            Shaper::SineFold(g) => format!("fold x{g:.0}"),
            Shaper::Cheb5(g) => format!("cheb5 x{g:.1}"),
        }
    }
}

/// A sine driven through a shaper, with a choice of antialiasing.
struct Shaped {
    phase: f64,
    inc: f64,
    shaper: Shaper,
    /// First-order antiderivative antialiasing instead of the direct evaluation.
    adaa: bool,
    prev_x: f64,
    prev_f: f64,
}

impl Shaped {
    fn new(inc: f64, shaper: Shaper, adaa: bool) -> Self {
        Self {
            phase: 0.0,
            inc,
            shaper,
            adaa,
            prev_x: 0.0,
            prev_f: shaper.antiderivative(0.0),
        }
    }
}

impl Osc for Shaped {
    fn next(&mut self) -> f64 {
        let x = (2.0 * PI * self.phase).sin();
        self.phase += self.inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        if self.adaa {
            let f = self.shaper.antiderivative(x);
            let dx = x - self.prev_x;
            // The difference quotient is the average of the shaper over the segment the
            // signal covered this sample. Near a stationary point it is 0/0, so fall
            // back to the midpoint — the standard guard, and the reason ADAA needs a
            // tolerance rather than being unconditionally exact.
            let out = if dx.abs() > 1e-6 {
                (f - self.prev_f) / dx
            } else {
                self.shaper.f(0.5 * (x + self.prev_x))
            };
            self.prev_x = x;
            self.prev_f = f;
            out
        } else {
            self.shaper.f(x)
        }
    }
}

/// Alias measurement for a signal built from **several** harmonic series at once.
///
/// `analyse` assumes one fundamental and marks only `k*p` as wanted, which for a detuned
/// stack would score every voice but the first as aliasing — the technique counted as
/// its own defect. This marks `k*Pᵢ` for every voice, so the wanted set is the union of
/// the series and everything else is alias by construction.
///
/// Each `Pᵢ` must be a distinct odd integer, so every voice is exactly periodic in the
/// window and lands on bins with no leakage. Detune is then quantised to the bin grid —
/// about 2.6 cents at `N = 65536` and `P ≈ 655`, far finer than a supersaw uses.
///
/// `collisions` counts alias components that land on a wanted bin and are therefore
/// miscounted as signal: for each voice, the harmonics above Nyquist fold to
/// `k*Pᵢ mod N`, and some of those hit the wanted set. It is reported rather than
/// hidden, because it bounds how much the separation can be trusted.
struct MultiSpectrum {
    /// Energy off the wanted set over energy on it, in dB.
    alias_db: f64,
    /// Spectral flatness of the **alias bins only**, in dB. Near 0 dB means the alias
    /// energy has spread into a noise floor; strongly negative means it is still a few
    /// discrete ghost tones.
    alias_flatness_db: f64,
    /// Aliased components landing on a wanted bin, out of those checked.
    collisions: usize,
}

fn analyse_multi(x: &[f64], periods: &[usize], _fs: f64) -> MultiSpectrum {
    let mut re = x.to_vec();
    let mut im = vec![0.0f64; x.len()];
    fft(&mut re, &mut im).expect("the harness allocates power-of-two buffers");
    let half = N / 2;

    let mut wanted = vec![false; half];
    for &p in periods {
        let mut k = 1usize;
        while k * p < half {
            wanted[k * p] = true;
            k += 1;
        }
    }

    // Fold each voice's above-Nyquist harmonics and count the ones that land on the
    // wanted set. Twenty times the audible harmonic count is far past where a
    // band-limited oscillator has meaningful energy.
    let mut collisions = 0usize;
    for &p in periods {
        let first_over = half / p + 1;
        for k in first_over..first_over * 20 {
            let bin = (k * p) % N;
            let bin = if bin >= half { N - bin } else { bin };
            if bin > 0 && bin < half && wanted[bin] {
                collisions += 1;
            }
        }
    }

    let (mut want_e, mut alias_e) = (0.0f64, 0.0f64);
    let (mut log_sum, mut count) = (0.0f64, 0usize);
    for bin in 1..half {
        let e = re[bin] * re[bin] + im[bin] * im[bin];
        if wanted[bin] {
            want_e += e;
        } else {
            alias_e += e;
            log_sum += e.max(1e-30).ln();
            count += 1;
        }
    }
    let flat = if count > 0 {
        let geo = (log_sum / count as f64).exp();
        let arith = alias_e / count as f64;
        10.0 * (geo / arith.max(1e-30)).max(1e-30).log10()
    } else {
        0.0
    };

    MultiSpectrum {
        alias_db: 10.0 * (alias_e / want_e.max(1e-30)).max(1e-30).log10(),
        alias_flatness_db: flat,
        collisions,
    }
}

/// A stack of detuned sawtooths — the supersaw.
///
/// Each voice is the shipped `saw()` on its own `Phasor`, so this measures the real
/// oscillator stacked, not a model of one.
struct Supersaw {
    voices: Vec<Phasor>,
    gain: f64,
}

impl Supersaw {
    /// `periods` are the per-voice period counts in the analysis window; `power_norm`
    /// selects `1/√N` (power-preserving) over `1/N` (peak-safe).
    fn new(periods: &[usize], power_norm: bool, random_phase: bool) -> Self {
        let mut rng = mxm_mono_01_dsp::Rng::new(0x5355_5045);
        let voices = periods
            .iter()
            .map(|&p| {
                let mut ph = Phasor::new();
                ph.set_inc(p as f32 / N as f32);
                if random_phase {
                    // Advance by a random part of a cycle so the voices do not all
                    // start at zero and sum to an N-times transient.
                    let steps =
                        ((rng.next_bipolar() as f64 * 0.5 + 0.5) * (N as f64 / p as f64)) as usize;
                    for _ in 0..steps {
                        ph.advance();
                    }
                }
                ph
            })
            .collect::<Vec<_>>();
        let n = periods.len() as f64;
        Self {
            voices,
            gain: if power_norm { 1.0 / n.sqrt() } else { 1.0 / n },
        }
    }
}

impl Osc for Supersaw {
    fn next(&mut self) -> f64 {
        let mut acc = 0.0;
        for v in self.voices.iter_mut() {
            acc += saw(v) as f64;
            v.advance();
        }
        acc * self.gain
    }
}

/// Period counts for `count` voices spread over `spread` units either side of `centre`.
///
/// Every count is kept odd so each voice is exactly periodic in the window with
/// `gcd(P, N) = 1`, which is what makes the wanted set exact.
fn detune_periods(centre: usize, count: usize, spread: usize) -> Vec<usize> {
    if count == 1 {
        return vec![centre];
    }
    (0..count)
        .map(|i| {
            let t = i as f64 / (count - 1) as f64 * 2.0 - 1.0;
            let offset = (t * spread as f64).round() as i64;
            // Keep it odd by moving in steps of two.
            let p = centre as i64 + offset * 2;
            p.max(3) as usize
        })
        .collect()
}

/// Hard sync: a slave sawtooth whose phase is reset by a master oscillator.
///
/// `corrected` applies the shipped two-point residual twice — once for the slave's own
/// wrap, which is an ordinary sawtooth discontinuity, and once for the reset, whose
/// height is **computed from the slave's instantaneous value** rather than assumed.
struct HardSync {
    fs: f64,
    master_hz: f64,
    slave_hz: f64,
    master_phase: f64,
    slave_phase: f64,
    corrected: bool,
}

impl HardSync {
    fn new(fs: f64, master_hz: f64, slave_hz: f64, corrected: bool) -> Self {
        Self {
            fs,
            master_hz,
            slave_hz,
            master_phase: 0.0,
            slave_phase: 0.0,
            corrected,
        }
    }
}

impl Osc for HardSync {
    fn next(&mut self) -> f64 {
        let dt_m = self.master_hz / self.fs;
        let dt_s = self.slave_hz / self.fs;

        let mut y = 2.0 * self.slave_phase - 1.0;

        if self.corrected {
            // The slave's own wrap, exactly as `saw()` corrects it.
            y -= poly_blep_at(self.slave_phase, dt_s);
            // The reset. Height is where the slave will land minus where it is, which
            // is only knowable from the slave's current value.
            let h = -2.0 * self.slave_phase;
            y += 0.5 * h * poly_blep_at(self.master_phase, dt_m);
        }

        self.slave_phase += dt_s;
        if self.slave_phase >= 1.0 {
            self.slave_phase -= 1.0;
        }
        self.master_phase += dt_m;
        if self.master_phase >= 1.0 {
            self.master_phase -= 1.0;
            // Reset, with the fractional overshoot carried so the slave restarts at the
            // right point between samples rather than on the grid.
            self.slave_phase = (self.master_phase / dt_m) * dt_s;
        }
        y
    }
}

/// Normalised autocorrelation at one lag: how much the signal repeats itself after
/// `lag` samples.
///
/// The measure that catches what a magnitude-domain measure cannot. A phase vocoder
/// copies the analysis magnitudes into the synthesis frames unchanged, so any
/// long-window spectral measure is biased toward saying the output is fine. What
/// actually goes wrong is temporal: if successive output frames are phase-coherent
/// copies of the same material, the output repeats at the synthesis hop, and that is
/// audible as a buzz at the frame rate.
fn periodicity_at_lag(x: &[f64], lag: usize) -> f64 {
    if x.len() <= lag {
        return 0.0;
    }
    let (mut num, mut den) = (0.0f64, 0.0f64);
    for i in 0..x.len() - lag {
        num += x[i] * x[i + lag];
        den += x[i] * x[i];
    }
    (num / den.max(1e-30)).abs()
}

/// Read a table at a fractional position, with a chosen interpolator.
///
/// The same three reads as `WtOsc` and `SampleOsc`, factored out so the grain
/// engine below can use them without owning a table of its own.
#[inline]
fn read_table(table: &[f64], x: f64, interp: Interp) -> f64 {
    let len = table.len();
    let i = x as usize % len;
    let f = x - x.floor();
    match interp {
        Interp::Drop => table[i],
        Interp::Linear => table[i] * (1.0 - f) + table[(i + 1) % len] * f,
        Interp::Cubic => {
            let m1 = table[(i + len - 1) % len];
            let p0 = table[i];
            let p1 = table[(i + 1) % len];
            let p2 = table[(i + 2) % len];
            let a = -0.5 * m1 + 1.5 * p0 - 1.5 * p1 + 0.5 * p2;
            let b = m1 - 2.5 * p0 + 2.0 * p1 - 0.5 * p2;
            let c = -0.5 * m1 + 0.5 * p1;
            ((a * f + b) * f + c) * f + p0
        }
    }
}

/// A synchronous grain train whose grains read a *recording* at a chosen rate.
///
/// This is grain pitch, the control every granular instrument offers: each grain
/// plays the source faster or slower than it was recorded. Grains fire at the note
/// rate, so the output stays periodic at the note however the grains are
/// transposed — which means §1's analysis can separate the aliasing the
/// transposition causes from the harmonics it is supposed to produce.
struct SampleGrainTrain {
    table: Vec<f64>,
    interp: Interp,
    /// Samples between grain onsets — the note period.
    period: f64,
    /// Grain length in samples.
    dur: f64,
    /// Table phase advance per sample: the source's own frequency times the
    /// transposition ratio.
    tbl_inc: f64,
    n: f64,
    next_onset: f64,
    starts: Vec<f64>,
}

impl SampleGrainTrain {
    fn new(
        fs: f64,
        note_hz: f64,
        overlap: f64,
        source_hz: f64,
        ratio: f64,
        harmonics: usize,
        interp: Interp,
    ) -> Self {
        let len = 4096usize;
        let mut table = vec![0.0f64; len];
        for (i, v) in table.iter_mut().enumerate() {
            let ph = 2.0 * PI * i as f64 / len as f64;
            let mut acc = 0.0;
            for k in 1..=harmonics.min(len / 2 - 1) {
                acc -= (k as f64 * ph).sin() / k as f64;
            }
            *v = acc * 2.0 / PI;
        }
        let period = fs / note_hz;
        Self {
            table,
            interp,
            period,
            dur: period * overlap,
            tbl_inc: source_hz * ratio / fs,
            n: 0.0,
            next_onset: 0.0,
            starts: Vec::with_capacity(64),
        }
    }
}

impl Osc for SampleGrainTrain {
    fn next(&mut self) -> f64 {
        while self.n >= self.next_onset {
            self.starts.push(self.next_onset);
            self.next_onset += self.period;
        }
        let (n, dur) = (self.n, self.dur);
        self.starts.retain(|&s| n - s < dur);

        let mut y = 0.0;
        for &start in &self.starts {
            let t = self.n - start;
            if t < 0.0 {
                continue;
            }
            let u = t / self.dur;
            let w = 0.5 - 0.5 * (2.0 * PI * u).cos();
            // Every grain reads the source from the same place, so this is pure
            // transposition with no position scatter.
            let x = (self.tbl_inc * t).fract() * self.table.len() as f64;
            y += w * read_table(&self.table, x, self.interp);
        }
        self.n += 1.0;
        y
    }
}

/// Highest frequency carrying more than `floor_db` below the loudest component.
///
/// The measured answer to "how wide is this spectrum", for comparison against
/// Carson's rule.
fn occupied_bandwidth_hz(x: &[f64], fs: f64, floor_db: f64) -> f64 {
    let mut re = x.to_vec();
    let mut im = vec![0.0f64; x.len()];
    fft(&mut re, &mut im).expect("the harness allocates power-of-two buffers");
    let mag: Vec<f64> = (0..N / 2)
        .map(|i| (re[i] * re[i] + im[i] * im[i]).sqrt())
        .collect();
    let peak = mag.iter().cloned().fold(0.0f64, f64::max).max(1e-30);
    let threshold = peak * 10f64.powf(floor_db / 20.0);
    let mut highest = 0usize;
    for (bin, &m) in mag.iter().enumerate() {
        if m > threshold {
            highest = bin;
        }
    }
    highest as f64 * fs / N as f64
}

/// Energy anywhere except the one bin a pure sine should occupy, relative to that
/// bin, in dB. Distortion and aliasing together, which for a single partial is
/// exactly what "how clean is this sine" means.
fn sine_purity_db(x: &[f64], p: usize) -> f64 {
    let mut re = x.to_vec();
    let mut im = vec![0.0f64; x.len()];
    fft(&mut re, &mut im).expect("the harness allocates power-of-two buffers");
    let (mut want, mut other) = (0.0f64, 0.0f64);
    for bin in 1..N / 2 {
        let e = re[bin] * re[bin] + im[bin] * im[bin];
        if bin == p {
            want += e;
        } else {
            other += e;
        }
    }
    10.0 * (other / want.max(1e-30)).max(1e-30).log10()
}

/// Render at `factor` times the rate and decimate: the reference that an oscillator
/// with no closed form gets measured against.
fn oversampled_reference(
    make: impl Fn(f64) -> Box<dyn Osc>,
    fs: f64,
    factor: usize,
    n: usize,
) -> Vec<f64> {
    let taps = decimation_taps(factor);
    let mut src = make(fs * factor as f64);
    let total = n * factor + taps.len();
    let hi: Vec<f64> = (0..total).map(|_| src.next()).collect();
    (0..n)
        .map(|i| {
            let start = i * factor;
            taps.iter()
                .enumerate()
                .map(|(k, &t)| t * hi[start + k])
                .sum()
        })
        .collect()
}

/// Harmonic-by-harmonic deviation of `x` from `reference`, both exactly periodic
/// with `p` periods in the window. Returns the worst single harmonic error and the
/// total error energy, both in dB.
///
/// `analyse` can say how much energy sits off the harmonic grid, but not whether the
/// harmonics themselves came out at the right level — and for a grain train, an FM
/// pair or a resynthesis bank there is no closed form to say what "right" is. This
/// compares against a rendering of the same algorithm at a higher rate instead, so
/// folded aliasing, interpolation error and a wrong harmonic level all show up
/// together.
///
/// Pass `p = 1` to compare every bin rather than only the harmonic ones.
///
/// The floor is the decimation filter's, not the FFT's: about -115 dB on this
/// harness. Always measure a known-clean control alongside.
fn harmonic_deviation(x: &[f64], reference: &[f64], p: usize, fs: f64) -> (f64, f64) {
    let spectrum = |v: &[f64]| {
        let mut re = v.to_vec();
        let mut im = vec![0.0f64; v.len()];
        fft(&mut re, &mut im).expect("the harness allocates power-of-two buffers");
        (0..N / 2)
            .map(|i| (re[i] * re[i] + im[i] * im[i]).sqrt() / (N as f64 / 2.0))
            .collect::<Vec<f64>>()
    };
    let a = spectrum(x);
    let b = spectrum(reference);
    let n_harm = (N / 2 - 1) / p;
    let (mut err, mut sig, mut worst) = (0.0f64, 0.0f64, 0.0f64);
    for k in 1..=n_harm {
        let bin = k * p;
        if bin as f64 * fs / N as f64 >= 0.45 * fs {
            break;
        }
        let d = a[bin] - b[bin];
        err += d * d;
        sig += b[bin] * b[bin];
        // Only judge harmonics with something in them: a component 80 dB down that
        // is wrong by 3 dB is not a finding.
        if b[bin] > 1e-4 {
            let e = 20.0 * (a[bin].max(1e-30) / b[bin]).log10();
            if e.abs() > worst.abs() {
                worst = e;
            }
        }
    }
    (worst, 10.0 * (err / sig.max(1e-30)).max(1e-30).log10())
}

/// Ideal bandlimited sawtooth by additive synthesis. Not a candidate algorithm —
/// it validates the measurement, which should find essentially no aliasing in it.
fn render_ideal(p: usize, n: usize) -> Vec<f64> {
    let harmonics = (N / 2 - 1) / p;
    let mut out = vec![0.0f64; n];
    for k in 1..=harmonics {
        let a = 2.0 / (PI * k as f64);
        for (i, v) in out.iter_mut().enumerate() {
            *v -= a * (2.0 * PI * (k * p) as f64 * i as f64 / N as f64).sin();
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

fn build(name: &str, inc: f64, fs: f64) -> Box<dyn Osc> {
    match name {
        "trivial" => Box::new(Trivial::new(inc)),
        "DPW2" => Box::new(Dpw2::new(inc)),
        "EPTR" => Box::new(Eptr::new(inc)),
        "PolyBLEP2 (shipped)" => Box::new(Shipped::new(inc)),
        "PolyBLEP4 (B-spline)" => Box::new(Blep4::new(inc)),
        "trivial + 2x OS" => Box::new(Oversampled::new(inc, 2)),
        "trivial + 4x OS" => Box::new(Oversampled::new(inc, 4)),
        "trivial + 8x OS" => Box::new(Oversampled::new(inc, 8)),
        "wavetable (mipmap)" => {
            // Leaked so the returned oscillator can borrow it for the whole run.
            // A measurement harness's licence, not a pattern for `process()`.
            let bank: &'static WtBank = Box::leak(Box::new(WtBank::new(fs, 2048, 1)));
            Box::new(WtOsc::new(bank, inc, Interp::Linear, false))
        }
        other => panic!("unknown algorithm {other}"),
    }
}

const ALGORITHMS: [&str; 9] = [
    "trivial",
    "DPW2",
    "EPTR",
    "PolyBLEP2 (shipped)",
    "PolyBLEP4 (B-spline)",
    "trivial + 2x OS",
    "trivial + 4x OS",
    "trivial + 8x OS",
    "wavetable (mipmap)",
];

fn header() {
    println!(
        "      {:>8}  {:>9}  {:>9}  {:>9}  {:>8}  {:>5}  {:>5}",
        "f0 (Hz)", "alias dB", "A-wtd dB", "worst dB", "h.err dB", "harm", "gone"
    );
}

fn row(f0: f64, s: Spectrum) {
    println!(
        "      {:>8.1}  {:>9.1}  {:>9.1}  {:>9.1}  {:>8.2}  {:>5}  {:>5}",
        f0, s.alias_db, s.alias_db_a, s.worst_alias_db, s.harmonic_err_db, s.harmonics, s.missing
    );
}

/// Time one algorithm, statically dispatched and inlinable. Returns ns/sample.
///
/// Four independent instances run interleaved. A single instance measures the
/// latency of its own `phase += inc` dependency chain rather than the algorithm's
/// throughput, which at these sizes is the larger number and hides the differences
/// between the candidates.
fn cost<T: Osc>(name: &str, make: impl Fn(f64) -> T, inc: f64, baseline: f64) -> f64 {
    const REPS: usize = 24;
    const LANES: usize = 4;
    let mut oscs: Vec<T> = (0..LANES)
        .map(|i| make(inc * (1.0 + i as f64 * 0.013)))
        .collect();
    for o in oscs.iter_mut() {
        for _ in 0..4096 {
            black_box(o.next());
        }
    }
    let t0 = Instant::now();
    let mut acc = [0.0f64; LANES];
    for _ in 0..REPS {
        for _ in 0..N / LANES {
            for (a, o) in acc.iter_mut().zip(oscs.iter_mut()) {
                *a += o.next();
            }
        }
    }
    black_box(acc);
    let ns = t0.elapsed().as_nanos() as f64 / (REPS * N) as f64;
    let rel = if baseline > 0.0 { ns / baseline } else { 1.0 };
    println!("      {name:>22}  {ns:>10.2}  {rel:>7.1}x");
    ns
}

fn main() {
    let fs = RATES[0];
    println!("mxm-mono-01 oscillator spike\n");
    println!(
        "Analysis: N = {N}, exactly-periodic frequencies, rectangular window.\n\
         alias    total aliased energy / total wanted-harmonic energy (dB)\n\
         A-wtd    the same ratio with both sides A-weighted\n\
         worst    loudest single aliased component, relative to the fundamental (dB)\n\
         h.err    largest harmonic amplitude error vs an ideal 1/k saw below 0.9*Nyquist (dB)\n"
    );

    let freqs = [55.0f64, 110.0, 220.0, 440.0, 880.0, 1760.0, 3520.0, 7040.0];

    println!("1. Sawtooth aliasing vs fundamental, fs = {fs} Hz\n");
    println!("   ideal (additive) — validates the measurement");
    header();
    for &f in &freqs {
        let p = periods_for(f, fs, N);
        let x = render_ideal(p, N);
        row(p as f64 * fs / N as f64, analyse(&x, p, fs));
    }
    println!();

    for name in ALGORITHMS {
        println!("   {name}");
        header();
        for &f in &freqs {
            let p = periods_for(f, fs, N);
            let inc = p as f64 / N as f64;
            let mut osc = build(name, inc, fs);
            let x = render(osc.as_mut(), N + SETTLE);
            row(p as f64 * fs / N as f64, analyse(&x[SETTLE..], p, fs));
        }
        println!();
    }

    println!("2. The shipped sawtooth across sample rates, 440 Hz\n");
    header();
    for fs in RATES {
        let p = periods_for(440.0, fs, N);
        let inc = p as f64 / N as f64;
        let mut osc = Shipped::new(inc);
        let x = render(&mut osc, N + SETTLE);
        let s = analyse(&x[SETTLE..], p, fs);
        println!(
            "      {:>8.1}  {:>9.1}  {:>9.1}  {:>9.1}  {:>8.2}  {:>5}  {:>5}   at {} Hz",
            p as f64 * fs / N as f64,
            s.alias_db,
            s.alias_db_a,
            s.worst_alias_db,
            s.harmonic_err_db,
            s.harmonics,
            s.missing,
            fs
        );
    }

    println!("\n3. Shipped pulse wave: aliasing vs width, 440 Hz, fs = {fs} Hz\n");
    println!(
        "      {:>7}  {:>9}  {:>9}  {:>9}",
        "width", "alias dB", "A-wtd dB", "worst dB"
    );
    let p440 = periods_for(440.0, fs, N);
    for w in [0.05f32, 0.1, 0.25, 0.5, 0.75, 0.9, 0.95] {
        let mut ph = Phasor::new();
        ph.set_inc(p440 as f32 / N as f32);
        let x: Vec<f64> = (0..N + SETTLE)
            .map(|_| {
                let y = pulse(&ph, w);
                ph.advance();
                y as f64
            })
            .collect();
        let s = analyse(&x[SETTLE..], p440, fs);
        println!(
            "      {:>7.2}  {:>9.1}  {:>9.1}  {:>9.1}",
            w, s.alias_db, s.alias_db_a, s.worst_alias_db
        );
    }

    println!(
        "\n4. Sub-oscillator shapes. The sub is `pulse()` on its own phasor, so each\n   \
         shape is that pulse at its own frequency and width. Main note 220 Hz.\n"
    );
    println!(
        "      {:>14}  {:>8}  {:>6}  {:>9}  {:>9}",
        "shape", "f0 (Hz)", "width", "alias dB", "A-wtd dB"
    );
    for (label, div, width) in [
        ("1 oct square", 2usize, 0.5f32),
        ("2 oct square", 4, 0.5),
        ("2 oct pulse", 4, 0.25),
    ] {
        let p = periods_for(220.0 / div as f64, fs, N);
        let mut ph = Phasor::new();
        ph.set_inc(p as f32 / N as f32);
        let x: Vec<f64> = (0..N + SETTLE)
            .map(|_| {
                let y = pulse(&ph, width);
                ph.advance();
                y as f64
            })
            .collect();
        let s = analyse(&x[SETTLE..], p, fs);
        println!(
            "      {:>14}  {:>8.1}  {:>6.2}  {:>9.1}  {:>9.1}",
            label,
            p as f64 * fs / N as f64,
            width,
            s.alias_db,
            s.alias_db_a
        );
    }

    println!(
        "
5. Cost in nanoseconds per sample, release build
"
    );
    println!(
        "      {:>22}  {:>10}  {:>8}",
        "algorithm", "ns/sample", "rel"
    );
    let cost_inc = periods_for(440.0, fs, N) as f64 / N as f64;
    let baseline = cost("trivial", Trivial::new, cost_inc, 0.0);
    cost("DPW2", Dpw2::new, cost_inc, baseline);
    cost("EPTR", Eptr::new, cost_inc, baseline);
    cost("PolyBLEP2 (shipped)", Shipped::new, cost_inc, baseline);
    cost("PolyBLEP4 (B-spline)", Blep4::new, cost_inc, baseline);
    cost(
        "trivial + 2x OS",
        |i| Oversampled::new(i, 2),
        cost_inc,
        baseline,
    );
    cost(
        "trivial + 4x OS",
        |i| Oversampled::new(i, 4),
        cost_inc,
        baseline,
    );
    cost(
        "trivial + 8x OS",
        |i| Oversampled::new(i, 8),
        cost_inc,
        baseline,
    );
    let cost_bank = WtBank::new(fs, 2048, 1);
    cost(
        "wavetable (mipmap)",
        |i| WtOsc::new(&cost_bank, i, Interp::Linear, false),
        cost_inc,
        baseline,
    );
    let build_ms = {
        let t0 = Instant::now();
        black_box(WtBank::new(fs, 2048, 1));
        t0.elapsed().as_secs_f64() * 1e3
    };
    println!(
        "      Construction is excluded from the rows above: the wavetable's {} tables
               take {:.0} ms to build and {} kB to hold.",
        cost_bank.tables.len(),
        build_ms,
        cost_bank.bytes() / 1024
    );

    println!("\n6. Pulse-width clamp: what the shipped `clamp_pulse_width` allows\n");
    println!(
        "      {:>10}  {:>10}  {:>10}  {:>10}",
        "f0 (Hz)", "dt", "w=0.05", "w=0.95"
    );
    for f in [55.0f64, 440.0, 2_000.0, 8_000.0, 16_000.0] {
        let dt = (f / fs) as f32;
        println!(
            "      {:>10.0}  {:>10.5}  {:>10.4}  {:>10.4}",
            f,
            dt,
            clamp_pulse_width(0.05, dt),
            clamp_pulse_width(0.95, dt)
        );
    }

    println!("\n7. DC through the shipped blocker, across pulse width, 220 Hz\n");
    println!(
        "      {:>7}  {:>12}  {:>14}",
        "width", "raw mean", "blocked mean"
    );
    for w in [0.05f32, 0.25, 0.5, 0.75, 0.95] {
        let mut ph = Phasor::new();
        ph.set_inc(periods_for(220.0, fs, N) as f32 / N as f32);
        let mut dc = DcBlocker::new();
        dc.set_sample_rate(fs as f32);
        let (mut raw, mut blocked) = (0.0f64, 0.0f64);
        for i in 0..N * 2 {
            let y = pulse(&ph, w);
            ph.advance();
            let b = dc.process(y);
            if i >= N {
                raw += y as f64;
                blocked += b as f64;
            }
        }
        println!(
            "      {:>7.2}  {:>12.6}  {:>14.8}",
            w,
            raw / N as f64,
            blocked / N as f64
        );
    }

    // -----------------------------------------------------------------------
    // 8. Wavetable variants
    // -----------------------------------------------------------------------

    println!("\n8. Wavetable oscillators, one variable at a time\n");

    println!("   8a. Interpolation. 2048-point tables, one per octave, safe table only.\n");
    let bank_2048 = WtBank::new(fs, 2048, 1);
    for interp in [Interp::Drop, Interp::Linear, Interp::Cubic] {
        println!("   {interp:?}");
        header();
        for &f in &[55.0f64, 220.0, 880.0, 3520.0] {
            let p = periods_for(f, fs, N);
            let inc = p as f64 / N as f64;
            let mut osc = WtOsc::new(&bank_2048, inc, interp, false);
            let x = render(&mut osc, N + SETTLE);
            row(p as f64 * fs / N as f64, analyse(&x[SETTLE..], p, fs));
        }
        println!();
    }

    println!("   8b. Table length, at 220 Hz, one table per octave, safe table only.\n");
    println!(
        "      {:>7}  {:>9}  {:>9}  {:>9}  {:>5}  {:>9}  {:>7}",
        "length", "drop dB", "linear dB", "cubic dB", "gone", "bank kB", "build"
    );
    for len in [64usize, 256, 1024, 4096, 16384] {
        let t0 = Instant::now();
        let bank = WtBank::new(fs, len, 1);
        let build = t0.elapsed().as_secs_f64() * 1e3;
        let p = periods_for(220.0, fs, N);
        let inc = p as f64 / N as f64;
        let mut db = [0.0f64; 3];
        let mut gone = 0usize;
        for (j, interp) in [Interp::Drop, Interp::Linear, Interp::Cubic]
            .into_iter()
            .enumerate()
        {
            let mut osc = WtOsc::new(&bank, inc, interp, false);
            let x = render(&mut osc, N + SETTLE);
            let sp = analyse(&x[SETTLE..], p, fs);
            db[j] = sp.alias_db;
            gone = sp.missing;
        }
        println!(
            "      {:>7}  {:>9.1}  {:>9.1}  {:>9.1}  {:>5}  {:>9}  {:>5.0} ms",
            len,
            db[0],
            db[1],
            db[2],
            gone,
            bank.bytes() / 1024,
            build
        );
    }

    println!(
        "\n   8c. Tables per octave, and crossfading toward the brighter neighbour.\n       \
         Linear interpolation, 2048-point tables. `gone` counts harmonics more than\n       \
         3 dB low, which is what a coarse bank costs.\n"
    );
    println!(
        "      {:>6}  {:>9}  {:>7}  {:>9}  {:>6}  {:>5}  {:>8}",
        "f0", "per oct", "xfade", "alias dB", "h.err", "gone", "bank kB"
    );
    for steps in [1usize, 2, 4, 12] {
        let bank = WtBank::new(fs, 2048, steps);
        for xfade in [false, true] {
            for &f in &[55.0f64, 220.0] {
                let p = periods_for(f, fs, N);
                let inc = p as f64 / N as f64;
                let mut osc = WtOsc::new(&bank, inc, Interp::Linear, xfade);
                let x = render(&mut osc, N + SETTLE);
                let sp = analyse(&x[SETTLE..], p, fs);
                println!(
                    "      {:>6.1}  {:>9}  {:>7}  {:>9.1}  {:>6.2}  {:>5}  {:>8}",
                    p as f64 * fs / N as f64,
                    steps,
                    if xfade { "yes" } else { "no" },
                    sp.alias_db,
                    sp.harmonic_err_db,
                    sp.missing,
                    bank.bytes() / 1024
                );
            }
        }
    }

    println!("\n   8d. Cost of the read, 440 Hz\n");
    println!("      {:>22}  {:>10}  {:>8}", "variant", "ns/sample", "rel");
    cost(
        "table, drop-sample",
        |i| WtOsc::new(&bank_2048, i, Interp::Drop, false),
        cost_inc,
        baseline,
    );
    cost(
        "table, linear",
        |i| WtOsc::new(&bank_2048, i, Interp::Linear, false),
        cost_inc,
        baseline,
    );
    cost(
        "table, cubic",
        |i| WtOsc::new(&bank_2048, i, Interp::Cubic, false),
        cost_inc,
        baseline,
    );
    cost(
        "table, cubic + xfade",
        |i| WtOsc::new(&bank_2048, i, Interp::Cubic, true),
        cost_inc,
        baseline,
    );

    // -----------------------------------------------------------------------
    // 9. Granular
    // -----------------------------------------------------------------------

    println!("\n9. Granular oscillators\n");

    let grain_p = periods_for(200.0, fs, N);
    let grain_rate = grain_p as f64 * fs / N as f64;
    let grain_dur = 4.0 / grain_rate; // four grains overlapping
    let formant = 1500.0;

    println!(
        "   9a. Grain envelope. Synchronous train at {:.1} Hz, {:.1} ms grains,\n       \
         {:.0} Hz carrier, four overlapping. `alias` is what the exactly-periodic\n       \
         analysis of §1 reports; `worst`/`total` are deviation from an 8x-oversampled\n       \
         rendering of the same train.\n",
        grain_rate,
        grain_dur * 1e3,
        formant
    );
    println!(
        "      {:>12}  {:>9}  {:>10}  {:>10}",
        "envelope", "alias dB", "worst dB", "total dB"
    );
    for env in [
        GrainEnv::Rect,
        GrainEnv::Hann,
        GrainEnv::Tukey(0.25),
        GrainEnv::Gauss(0.5),
        GrainEnv::Fof {
            rise: 0.02,
            decay: 0.15,
        },
    ] {
        let make = |rate_fs: f64| -> Box<dyn Osc> {
            Box::new(GrainTrain::new(
                rate_fs, grain_rate, grain_dur, formant, env,
            ))
        };
        let mut osc = make(fs);
        let x = render(osc.as_mut(), N + SETTLE);
        let reference = oversampled_reference(make, fs, 8, N + SETTLE);
        let sp = analyse(&x[SETTLE..], grain_p, fs);
        let (worst, total) = harmonic_deviation(&x[SETTLE..], &reference[SETTLE..], grain_p, fs);
        println!(
            "      {:>12}  {:>9.1}  {:>10.2}  {:>10.1}",
            env.name(),
            sp.alias_db,
            worst,
            total
        );
    }

    println!(
        "\n   9b. Onset placement. The same train, with each grain's onset either placed\n       \
         at its exact fractional time or rounded to the nearest sample. Both are\n       \
         measured against the same fractional-onset 8x reference.\n"
    );
    println!(
        "      {:>12}  {:>12}  {:>9}  {:>10}",
        "envelope", "onsets", "alias dB", "vs ref dB"
    );
    for env in [GrainEnv::Hann, GrainEnv::Tukey(0.25)] {
        let make = |rate_fs: f64| -> Box<dyn Osc> {
            Box::new(GrainTrain::new(
                rate_fs, grain_rate, grain_dur, formant, env,
            ))
        };
        let reference = oversampled_reference(make, fs, 8, N + SETTLE);
        for quantise in [false, true] {
            let mut osc = GrainTrain::new(fs, grain_rate, grain_dur, formant, env);
            if quantise {
                osc = osc.quantised();
            }
            let x = render(&mut osc, N + SETTLE);
            let sp = analyse(&x[SETTLE..], grain_p, fs);
            let (_, total) = harmonic_deviation(&x[SETTLE..], &reference[SETTLE..], grain_p, fs);
            println!(
                "      {:>12}  {:>12}  {:>9.1}  {:>10.1}",
                env.name(),
                if quantise {
                    "sample grid"
                } else {
                    "fractional"
                },
                sp.alias_db,
                total
            );
        }
    }

    println!(
        "\n   9c. Overlap-add ripple. Envelopes summed at a hop of `dur/overlap`, with the\n       \
         peak-to-peak amplitude variation of the sum measured over the steady region.\n"
    );
    println!(
        "      {:>12}  {:>8}  {:>8}  {:>8}  {:>8}  {:>8}",
        "envelope", "ovl 1", "ovl 2", "ovl 3", "ovl 4", "ovl 8"
    );
    for env in [
        GrainEnv::Rect,
        GrainEnv::Hann,
        GrainEnv::Tukey(0.25),
        GrainEnv::Gauss(0.5),
    ] {
        print!("      {:>12}", env.name());
        for overlap in [1usize, 2, 3, 4, 8] {
            const GRAIN: f64 = 1024.0;
            let hop = GRAIN / overlap as f64;
            let n = 8192usize;
            let mut sum = vec![0.0f64; n];
            let mut start = 0.0f64;
            while start < n as f64 {
                for (i, v) in sum.iter_mut().enumerate() {
                    let t = i as f64 - start;
                    if (0.0..GRAIN).contains(&t) {
                        *v += env.at(t / GRAIN);
                    }
                }
                start += hop;
            }
            // Judge only where the overlap has built up and not yet run out.
            let lo = (2.0 * GRAIN) as usize;
            let hi = n - (2.0 * GRAIN) as usize;
            let (mut mn, mut mx) = (f64::MAX, f64::MIN);
            for &v in &sum[lo..hi] {
                mn = mn.min(v);
                mx = mx.max(v);
            }
            if mn < 1e-6 {
                // The sum reaches zero: the train has silent gaps between grains,
                // which is a different failure from ripple and deserves saying so.
                print!("  {:>8}", "gaps");
            } else {
                print!("  {:>8.2}", 20.0 * (mx / mn).log10());
            }
        }
        println!();
    }
    println!("      (dB peak-to-trough; 0.00 is exact constant-overlap-add.)");

    println!(
        "\n   9d. Asynchronous clouds. 20 ms Hann grains, onsets jittered by half a grain\n       \
         period, pool of {} grains.\n",
        GrainTrain::POOL
    );
    println!(
        "      {:>10}  {:>8}  {:>9}  {:>9}  {:>8}  {:>8}",
        "grains/s", "active", "rms", "peak", "crest dB", "dropped"
    );
    for density in [10.0f64, 50.0, 200.0, 1000.0, 5000.0] {
        let mut osc = GrainTrain::new(fs, density, 0.020, formant, GrainEnv::Hann).with_jitter(0.5);
        let n = (fs * 0.5) as usize;
        let (mut sq, mut peak) = (0.0f64, 0.0f64);
        let mut active_sum = 0usize;
        for i in 0..n {
            let y = osc.next();
            if i > n / 4 {
                sq += y * y;
                peak = peak.max(y.abs());
                active_sum += osc.active();
            }
        }
        let count = n - n / 4 - 1;
        let rms = (sq / count as f64).sqrt();
        println!(
            "      {:>10.0}  {:>8.1}  {:>9.3}  {:>9.3}  {:>8.1}  {:>8}",
            density,
            active_sum as f64 / count as f64,
            rms,
            peak,
            20.0 * (peak / rms.max(1e-12)).log10(),
            osc.dropped
        );
    }

    println!(
        "\n   9e. Cost against the number of grains alive, 20 ms Hann grains. `scan`\n       \
         visits all {} pool slots every sample; `compact` keeps the live grains\n       \
         packed and visits only those.\n",
        GrainTrain::POOL
    );
    println!("      {:>22}  {:>10}  {:>8}", "variant", "ns/sample", "rel");
    for density in [50.0f64, 200.0, 800.0, 2400.0] {
        let active = density * 0.020;
        cost(
            &format!("scan, {active:.0} live"),
            |_| GrainTrain::new(fs, density, 0.020, formant, GrainEnv::Hann),
            cost_inc,
            baseline,
        );
        cost(
            &format!("compact, {active:.0} live"),
            |_| GrainTrain::new(fs, density, 0.020, formant, GrainEnv::Hann).compacted(),
            cost_inc,
            baseline,
        );
        cost(
            &format!("+table carrier, {active:.0}"),
            |_| {
                GrainTrain::new(fs, density, 0.020, formant, GrainEnv::Hann)
                    .compacted()
                    .with_carrier(Carrier::Table)
            },
            cost_inc,
            baseline,
        );
    }
    // 9f works in live grains rather than density, because live grains is what
    // costs: 10.5 measured `live = density x duration` exact.
    let grain_len = 0.020f64;
    let grain_dur_ms = grain_len * 1e3;
    let grain_live = 32.0f64;
    let grain_density = grain_live / grain_len;

    println!(
        "\n   9f. What a sample-reading grain costs. Every grain in 9a-e is a windowed\n       \
         sine; every grain in a granular instrument is a windowed read from a stored\n       \
         sample, and the two are not the same animal. Planar f32 source, one second,\n       \
         stereo unless marked, grains scattered over it, {:.0} ms grains.\n       \
         `per grain` is (ns/sample - baseline) / live grains. `@100%` and `@25%` are\n       \
         grains affordable at 48 kHz in that much of one core.\n",
        grain_dur_ms
    );
    println!(
        "      {:>26}  {:>9}  {:>9}  {:>9}  {:>8}",
        "variant", "ns/samp", "per grain", "@100%", "@25%"
    );

    // The ladder starts at the shape `mxm-creative-sampler-dsp` ships and removes
    // one cost at a time. Nothing below the fourth row changes what the cloud is;
    // the kernel rows below that do, and 15-granular-in-the-wild.md 15.9 prices
    // the quality each one buys.
    grain_row(
        "sinc16 recur, wrap, scan",
        |_| GrainCloud::new(fs, grain_density, grain_len, Kernel::Sinc16Recurrence),
        grain_live,
        baseline,
    );
    grain_row(
        "+ padded, no rem_euclid",
        |_| {
            GrainCloud::new(fs, grain_density, grain_len, Kernel::Sinc16Recurrence)
                .with_bounds(Bounds::Padded)
        },
        grain_live,
        baseline,
    );
    grain_row(
        "+ window table",
        |_| {
            GrainCloud::new(fs, grain_density, grain_len, Kernel::Sinc16Recurrence)
                .with_bounds(Bounds::Padded)
                .with_window(GrainWin::Table)
        },
        grain_live,
        baseline,
    );
    grain_row(
        "+ compact live list",
        |_| {
            GrainCloud::new(fs, grain_density, grain_len, Kernel::Sinc16Recurrence)
                .with_bounds(Bounds::Padded)
                .with_window(GrainWin::Table)
                .with_walk(Walk::Compact)
        },
        grain_live,
        baseline,
    );
    grain_row(
        "+ block render",
        |_| {
            GrainCloud::new(fs, grain_density, grain_len, Kernel::Sinc16Recurrence)
                .with_bounds(Bounds::Padded)
                .with_window(GrainWin::Table)
                .with_walk(Walk::Block)
        },
        grain_live,
        baseline,
    );
    for kernel in [Kernel::Sinc8Table, Kernel::Cubic, Kernel::Linear] {
        grain_row(
            &format!("-> {}", kernel.name()),
            |_| {
                GrainCloud::new(fs, grain_density, grain_len, kernel)
                    .with_bounds(Bounds::Padded)
                    .with_window(GrainWin::Table)
                    .with_walk(Walk::Block)
            },
            grain_live,
            baseline,
        );
    }
    for kernel in [Kernel::Cubic, Kernel::Linear] {
        grain_row(
            &format!("-> {}, mono", kernel.name()),
            |_| {
                GrainCloud::new(fs, grain_density, grain_len, kernel)
                    .with_bounds(Bounds::Padded)
                    .with_window(GrainWin::Table)
                    .with_walk(Walk::Block)
                    .mono()
            },
            grain_live,
            baseline,
        );
    }

    println!(
        "\n       The same structural ladder with the kernel already cheap. The four\n       \
         structural rows are worth little against a kernel that dominates them, and\n       \
         worth measuring against one that does not.\n"
    );
    println!(
        "      {:>26}  {:>9}  {:>9}  {:>9}  {:>8}",
        "variant", "ns/samp", "per grain", "@100%", "@25%"
    );
    grain_row(
        "cubic, wrap, cos, scan",
        |_| GrainCloud::new(fs, grain_density, grain_len, Kernel::Cubic),
        grain_live,
        baseline,
    );
    grain_row(
        "+ padded, no rem_euclid",
        |_| {
            GrainCloud::new(fs, grain_density, grain_len, Kernel::Cubic).with_bounds(Bounds::Padded)
        },
        grain_live,
        baseline,
    );
    grain_row(
        "+ window table",
        |_| {
            GrainCloud::new(fs, grain_density, grain_len, Kernel::Cubic)
                .with_bounds(Bounds::Padded)
                .with_window(GrainWin::Table)
        },
        grain_live,
        baseline,
    );
    grain_row(
        "+ compact live list",
        |_| {
            GrainCloud::new(fs, grain_density, grain_len, Kernel::Cubic)
                .with_bounds(Bounds::Padded)
                .with_window(GrainWin::Table)
                .with_walk(Walk::Compact)
        },
        grain_live,
        baseline,
    );
    grain_row(
        "+ block render",
        |_| {
            GrainCloud::new(fs, grain_density, grain_len, Kernel::Cubic)
                .with_bounds(Bounds::Padded)
                .with_window(GrainWin::Table)
                .with_walk(Walk::Block)
        },
        grain_live,
        baseline,
    );

    println!(
        "\n       Marginal cost against the number of grains alive, cubic kernel at the\n       \
         bottom of the ladder. A grain engine's cost is its overlap and nothing else,\n       \
         so these rows should agree.\n"
    );
    println!(
        "      {:>26}  {:>9}  {:>9}  {:>9}  {:>8}",
        "variant", "ns/samp", "per grain", "@100%", "@25%"
    );
    for live in [4.0f64, 16.0, 64.0, 128.0] {
        let density = live / grain_len;
        grain_row(
            &format!("cubic, {live:.0} live"),
            |_| {
                GrainCloud::new(fs, density, grain_len, Kernel::Cubic)
                    .with_bounds(Bounds::Padded)
                    .with_window(GrainWin::Table)
                    .with_walk(Walk::Block)
            },
            live,
            baseline,
        );
    }

    println!(
        "\n       Playback rate, cubic at the bottom of the ladder. A grain pitched up\n       \
         strides that many source frames per output frame and touches that many more\n       \
         cache lines, so transposition might have been expected to cost. It does not:\n       \
         at this overlap the working set stays inside L2 at every rate. The mipmap\n       \
         15.10 asks for is a quality argument, not a cost one.\n"
    );
    println!(
        "      {:>26}  {:>9}  {:>9}  {:>9}  {:>8}",
        "variant", "ns/samp", "per grain", "@100%", "@25%"
    );
    for step in [0.5f32, 1.0, 2.0, 4.0] {
        grain_row(
            &format!("cubic, rate {step:.1}x"),
            |_| {
                GrainCloud::new(fs, grain_density, grain_len, Kernel::Cubic)
                    .with_bounds(Bounds::Padded)
                    .with_window(GrainWin::Table)
                    .with_walk(Walk::Block)
                    .with_step(step)
            },
            grain_live,
            baseline,
        );
    }

    // -----------------------------------------------------------------------
    // 10. Additive
    // -----------------------------------------------------------------------

    println!("\n10. Additive banks\n");

    println!(
        "   10a. Purity of one partial at 440 Hz: everything that is not the wanted\n        \
         bin, relative to it.\n"
    );
    println!("      {:>12}  {:>12}", "partial", "junk dB");
    for kind in [PartialKind::Sin, PartialKind::Table, PartialKind::Resonator] {
        let p1 = periods_for(440.0, fs, N);
        let f1 = p1 as f64 * fs / N as f64;
        let mut bank = PartialBank::new(fs, f1, 1, kind);
        let x = render(&mut bank, N + SETTLE);
        println!(
            "      {:>12}  {:>12.1}",
            format!("{kind:?}"),
            sine_purity_db(&x[SETTLE..], p1)
        );
    }

    println!(
        "\n   10b. Resonator stability. Peak amplitude after 60 s of running, against\n        \
         the peak over the first 100 ms.\n"
    );
    println!(
        "      {:>10}  {:>12}  {:>12}",
        "f (Hz)", "drift dB", "final peak"
    );
    for f in [55.0f64, 440.0, 4_000.0, 15_000.0] {
        let mut bank = PartialBank::new(fs, f, 1, PartialKind::Resonator);
        let early = (fs * 0.1) as usize;
        let mut first = 0.0f64;
        for _ in 0..early {
            first = first.max(bank.next().abs());
        }
        for _ in 0..(fs * 59.0) as usize {
            black_box(bank.next());
        }
        let mut last = 0.0f64;
        for _ in 0..early {
            last = last.max(bank.next().abs());
        }
        println!(
            "      {:>10.0}  {:>12.4}  {:>12.6}",
            f,
            20.0 * (last / first.max(1e-30)).log10(),
            last
        );
    }

    println!(
        "\n   10c. Cost per sample against partial count. The right-hand column is one\n        \
         voice's share of a single core at 48 kHz.\n"
    );
    println!(
        "      {:>10}  {:>12}  {:>12}  {:>12}  {:>10}",
        "partials", "sin ns", "table ns", "resonator ns", "core % (tbl)"
    );
    for count in [16usize, 64, 256, 516] {
        let f0 = (fs / 2.2) / count as f64;
        let mut ns = [0.0f64; 3];
        for (j, kind) in [PartialKind::Sin, PartialKind::Table, PartialKind::Resonator]
            .into_iter()
            .enumerate()
        {
            const REPS: usize = 4;
            let mut bank = PartialBank::new(fs, f0, count, kind);
            for _ in 0..2048 {
                black_box(bank.next());
            }
            let t0 = Instant::now();
            let mut acc = 0.0f64;
            for _ in 0..REPS {
                for _ in 0..N {
                    acc += bank.next();
                }
            }
            black_box(acc);
            ns[j] = t0.elapsed().as_nanos() as f64 / (REPS * N) as f64;
        }
        println!(
            "      {:>10}  {:>12.1}  {:>12.1}  {:>12.1}  {:>9.1}%",
            count,
            ns[0],
            ns[1],
            ns[2],
            ns[1] * 48_000.0 / 1e9 * 100.0
        );
    }

    println!(
        "\n   10d. Time-stretching. The source is a 220 Hz additive tone whose partial\n        \
         levels sweep (a brightness sweep, so the analysis has something to track).\n        \
         Additive resynthesis reads the analysed amplitudes at 1/stretch speed and\n        \
         never touches the partial frequencies. Naive overlap-add is the baseline.\n"
    );

    let src_p = periods_for(220.0, fs, N);
    let src_f0 = src_p as f64 * fs / N as f64;
    let src_len = (fs * 1.5) as usize;
    let partials = 24usize;
    // The "recording": 24 harmonics whose levels sweep as a one-pole lowpass would,
    // from 300 Hz to 5 kHz over the length of the source.
    let source: Vec<f64> = {
        let mut phases = vec![0.0f64; partials];
        (0..src_len)
            .map(|n| {
                let cutoff = 300.0 * (5000.0f64 / 300.0).powf(n as f64 / src_len as f64);
                let mut acc = 0.0;
                for (k, phase) in phases.iter_mut().enumerate() {
                    let f = src_f0 * (k + 1) as f64;
                    let gain = 1.0 / (1.0 + (f / cutoff).powi(2)).sqrt();
                    acc += gain / (k + 1) as f64 * (2.0 * PI * *phase).sin();
                    *phase += f / fs;
                    if *phase >= 1.0 {
                        *phase -= 1.0;
                    }
                }
                acc * 0.5
            })
            .collect()
    };

    let analysis = Analysis::of(&source, fs, src_f0, partials, 256);
    println!(
        "      source {:.2} Hz, {} partials, {} analysis frames at a 256-sample hop\n",
        src_f0,
        partials,
        analysis.frames.len()
    );
    println!(
        "      {:>8}  {:>22}  {:>10}  {:>12}  {:>12}",
        "stretch", "method", "out f0", "cents off", "ripple dB"
    );
    for stretch in [0.5f64, 1.0, 2.0, 8.0] {
        let out_len = ((src_len as f64) * stretch) as usize;
        let mut resynth = Resynth::new(&analysis, stretch);
        let additive: Vec<f64> = (0..out_len).map(|_| resynth.next()).collect();
        let ola = ola_stretch(&source, stretch, 2048, out_len);

        // Measure over the middle of the output, away from the ends.
        let a = out_len / 3;
        let b = (2 * out_len / 3).min(a + N);
        for (name, buf) in [
            ("additive resynthesis", &additive),
            ("naive overlap-add", &ola),
        ] {
            let f = peak_frequency(&buf[a..b], fs);
            println!(
                "      {:>8.2}  {:>22}  {:>10.2}  {:>12.2}  {:>12.2}",
                stretch,
                name,
                f,
                1200.0 * (f / src_f0).log2(),
                envelope_ripple_db(&buf[a..b], fs)
            );
        }
    }
    println!(
        "      (the source's own ripple over the same measure is {:.2} dB — the sweep\n       \
         itself moves the level, so that is the floor, not zero.)",
        envelope_ripple_db(&source[src_len / 3..(2 * src_len / 3)], fs)
    );

    println!(
        "\n   10e. Cost of the resynthesis read, {} partials\n",
        partials
    );
    println!("      {:>22}  {:>10}  {:>8}", "variant", "ns/sample", "rel");
    cost(
        "resynth, 24 partials",
        |_| Resynth::new(&analysis, 4.0),
        cost_inc,
        baseline,
    );

    // -----------------------------------------------------------------------
    // 11. Frequency and phase modulation
    // -----------------------------------------------------------------------

    println!("\n11. FM and PM\n");

    let fm_p = periods_for(440.0, fs, N);
    let fm_f0 = fm_p as f64 * fs / N as f64;

    println!(
        "   11a. Modulation index and ratio. Carrier {:.1} Hz, phase modulation.\n        \
         `alias` is the exactly-periodic analysis of §1; `vs ref` is the deviation\n        \
         from an 8x-oversampled rendering over every bin below 0.45*fs. `bw` is the\n        \
         highest bin within 60 dB of the peak; `carson` is fc + (I+1)*fm, the\n        \
         bandwidth that has to exceed Nyquist ({:.0} Hz) before anything folds.\n",
        fm_f0,
        fs / 2.0
    );
    println!(
        "      {:>6}  {:>6}  {:>10}  {:>10}  {:>9}  {:>9}",
        "ratio", "index", "alias dB", "vs ref dB", "bw (Hz)", "carson"
    );
    for (label, mult) in [("1:1", 1.0f64), ("1:8", 8.0)] {
        for index in [1.0f64, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0] {
            let fmod = fm_f0 * mult;
            let make = |rate_fs: f64| -> Box<dyn Osc> {
                Box::new(FmPair::new(rate_fs, fm_f0, fmod, index, FmKind::Phase))
            };
            let mut osc = make(fs);
            let x = render(osc.as_mut(), N + SETTLE);
            let reference = oversampled_reference(make, fs, 8, N + SETTLE);
            let sp = analyse(&x[SETTLE..], fm_p, fs);
            let (_, total) = harmonic_deviation(&x[SETTLE..], &reference[SETTLE..], 1, fs);
            println!(
                "      {:>6}  {:>6.0}  {:>10.1}  {:>10.1}  {:>9.0}  {:>9.0}",
                label,
                index,
                sp.alias_db,
                total,
                occupied_bandwidth_hz(&x[SETTLE..], fs, -60.0),
                fm_f0 + (index + 1.0) * fmod
            );
        }
    }
    println!(
        "      The `vs ref` column bottoms out at about -115 dB, which is the\n      \
         decimation filter's own floor and not a property of any oscillator: an\n      \
         unmodulated carrier measures the same. Read anything at -115 as \"no\n      \
         measurable aliasing\"."
    );

    println!(
        "\n   11b. Carrier-to-modulator ratio at index 4. The fundamental is\n        \
         {:.1} Hz in every row; `c` and `m` are its multiples.\n",
        fm_f0 / 8.0
    );
    println!(
        "      {:>8}  {:>10}  {:>10}  {:>10}  {:>10}",
        "c:m", "fc (Hz)", "fm (Hz)", "alias dB", "vs ref dB"
    );
    let base_p = periods_for(fm_f0 / 8.0, fs, N);
    let base_f0 = base_p as f64 * fs / N as f64;
    for (c, m) in [
        (1usize, 1usize),
        (1, 2),
        (2, 1),
        (1, 3),
        (3, 2),
        (8, 9),
        (1, 14),
    ] {
        let (fc, fmod) = (base_f0 * 8.0 * c as f64, base_f0 * 8.0 * m as f64);
        let make = |rate_fs: f64| -> Box<dyn Osc> {
            Box::new(FmPair::new(rate_fs, fc, fmod, 4.0, FmKind::Phase))
        };
        let mut osc = make(fs);
        let x = render(osc.as_mut(), N + SETTLE);
        let reference = oversampled_reference(make, fs, 8, N + SETTLE);
        let sp = analyse(&x[SETTLE..], base_p, fs);
        let (_, total) = harmonic_deviation(&x[SETTLE..], &reference[SETTLE..], 1, fs);
        println!(
            "      {:>8}  {:>10.1}  {:>10.1}  {:>10.1}  {:>10.1}",
            format!("{c}:{m}"),
            fc,
            fmod,
            sp.alias_db,
            total
        );
    }

    println!(
        "\n   11c. Oversampling FM, at a setting that genuinely aliases: 1:8, index 8,\n        \
         carrier {:.1} Hz, Carson bandwidth {:.0} Hz against a Nyquist of {:.0} Hz.\n        \
         Every row is compared against the same 16x reference.\n",
        fm_f0,
        fm_f0 + 9.0 * fm_f0 * 8.0,
        fs / 2.0
    );
    println!("      {:>10}  {:>12}", "rate", "vs ref dB");
    let fm_make = |rate_fs: f64| -> Box<dyn Osc> {
        Box::new(FmPair::new(rate_fs, fm_f0, fm_f0 * 8.0, 8.0, FmKind::Phase))
    };
    let fm_ref = oversampled_reference(fm_make, fs, 16, N + SETTLE);
    for factor in [1usize, 2, 4, 8] {
        let x = if factor == 1 {
            let mut osc = fm_make(fs);
            render(osc.as_mut(), N + SETTLE)
        } else {
            oversampled_reference(fm_make, fs, factor, N + SETTLE)
        };
        let (_, total) = harmonic_deviation(&x[SETTLE..], &fm_ref[SETTLE..], 1, fs);
        println!("      {:>10}  {:>12.1}", format!("{factor}x"), total);
    }

    println!(
        "\n   11d. Phase modulation against true frequency modulation, with a modulator\n        \
         that has a non-zero mean. `mean carrier` is the carrier's unwrapped phase\n        \
         advance divided by elapsed time — what the carrier's frequency really was,\n        \
         rather than which spectral peak happens to be loudest.\n"
    );
    println!(
        "      {:>12}  {:>8}  {:>14}  {:>12}  {:>10}",
        "kind", "offset", "mean carrier", "cents off", "alias dB"
    );
    for kind in [FmKind::Phase, FmKind::Frequency] {
        for offset in [0.0f64, 0.3] {
            let mut osc = FmPair::new(fs, fm_f0, fm_f0, 4.0, kind).with_offset(offset);
            let x = render(&mut osc, N + SETTLE);
            let sp = analyse(&x[SETTLE..], fm_p, fs);
            let mean = osc.mean_carrier_hz();
            println!(
                "      {:>12}  {:>8.1}  {:>14.2}  {:>12.1}  {:>10.1}",
                format!("{kind:?}"),
                offset,
                mean,
                1200.0 * (mean / fm_f0).log2(),
                sp.alias_db
            );
        }
    }

    println!(
        "\n   11e. Feedback into one operator. {:.1} Hz, self-modulating, with the\n        \
         two-sample average the hardware uses to keep the loop tame. `tilt` is the\n        \
         least-squares slope over the first eight harmonics; a sawtooth is -6.\n",
        fm_f0
    );
    println!(
        "      {:>10}  {:>10}  {:>12}  {:>12}",
        "feedback", "vs ref dB", "bw (Hz)", "tilt dB/oct"
    );
    for feedback in [0.0f64, 0.25, 0.5, 1.0, 2.0, 3.0, 4.0] {
        let make =
            |rate_fs: f64| -> Box<dyn Osc> { Box::new(FmFeedback::new(rate_fs, fm_f0, feedback)) };
        let mut osc = make(fs);
        let x = render(osc.as_mut(), N + SETTLE);
        let reference = oversampled_reference(make, fs, 8, N + SETTLE);
        let (_, total) = harmonic_deviation(&x[SETTLE..], &reference[SETTLE..], 1, fs);

        let tilt = {
            let mut re = x[SETTLE..].to_vec();
            let mut im = vec![0.0f64; N];
            fft(&mut re, &mut im).expect("the harness allocates power-of-two buffers");
            let mag = |k: usize| {
                let b = k * fm_p;
                (re[b] * re[b] + im[b] * im[b]).sqrt().max(1e-30)
            };
            let (mut sx, mut sy, mut sxy, mut sxx) = (0.0f64, 0.0, 0.0, 0.0);
            for k in 1..=8 {
                let lx = (k as f64).log2();
                let ly = 20.0 * mag(k).log10();
                sx += lx;
                sy += ly;
                sxy += lx * ly;
                sxx += lx * lx;
            }
            (8.0 * sxy - sx * sy) / (8.0 * sxx - sx * sx)
        };
        if feedback == 0.0 {
            println!(
                "      {:>10.2}  {:>10.1}  {:>12.0}  {:>12}",
                feedback,
                total,
                occupied_bandwidth_hz(&x[SETTLE..], fs, -60.0),
                "n/a"
            );
        } else {
            println!(
                "      {:>10.2}  {:>10.1}  {:>12.0}  {:>12.2}",
                feedback,
                total,
                occupied_bandwidth_hz(&x[SETTLE..], fs, -60.0),
                tilt
            );
        }
    }
    println!(
        "      (tilt is meaningless at zero feedback: a pure sine has no harmonics to\n      \
         fit a slope through.)"
    );

    println!("\n   11f. Cost\n");
    println!("      {:>22}  {:>10}  {:>8}", "variant", "ns/sample", "rel");
    cost(
        "FM pair (PM)",
        |_| FmPair::new(fs, fm_f0, fm_f0, 4.0, FmKind::Phase),
        cost_inc,
        baseline,
    );
    cost(
        "FM pair (true FM)",
        |_| FmPair::new(fs, fm_f0, fm_f0, 4.0, FmKind::Frequency),
        cost_inc,
        baseline,
    );
    cost(
        "feedback operator",
        |_| FmFeedback::new(fs, fm_f0, 1.0),
        cost_inc,
        baseline,
    );
    cost(
        "6-operator stack",
        |_| FmStack::new(fs, fm_f0, 6),
        cost_inc,
        baseline,
    );

    // -----------------------------------------------------------------------
    // 12. Phase distortion
    // -----------------------------------------------------------------------

    println!("\n12. Phase distortion\n");

    let pd_p = periods_for(440.0, fs, N);
    let pd_f0 = pd_p as f64 * fs / N as f64;

    println!(
        "   12a. The Casio sawtooth: a cosine read through a two-segment phase warp\n        \
         with its knee at (d, 0.5). {:.1} Hz. `fast seg` is the instantaneous\n        \
         frequency of the compressed segment, f0/(2d); Nyquist is {:.0} Hz.\n",
        pd_f0,
        fs / 2.0
    );
    println!(
        "      {:>8}  {:>10}  {:>10}  {:>9}  {:>12}",
        "d", "fast seg", "alias dB", "bw (Hz)", "tilt dB/oct"
    );
    for d in [0.5f64, 0.25, 0.1, 0.05, 0.02, 0.01, 0.005, 0.002] {
        let mut osc = PhaseDistort::new(fs, pd_f0, d);
        let fast = osc.fast_segment_hz();
        let x = render(&mut osc, N + SETTLE);
        let sp = analyse(&x[SETTLE..], pd_p, fs);
        let tilt = {
            let mut re = x[SETTLE..].to_vec();
            let mut im = vec![0.0f64; N];
            fft(&mut re, &mut im).expect("the harness allocates power-of-two buffers");
            let mag = |k: usize| {
                let b = k * pd_p;
                (re[b] * re[b] + im[b] * im[b]).sqrt().max(1e-30)
            };
            let (mut sx, mut sy, mut sxy, mut sxx) = (0.0f64, 0.0, 0.0, 0.0);
            for k in 1..=8 {
                let lx = (k as f64).log2();
                let ly = 20.0 * mag(k).log10();
                sx += lx;
                sy += ly;
                sxy += lx * ly;
                sxx += lx * lx;
            }
            (8.0 * sxy - sx * sy) / (8.0 * sxx - sx * sx)
        };
        println!(
            "      {:>8.3}  {:>10.0}  {:>10.1}  {:>9.0}  {:>12.2}",
            d,
            fast,
            sp.alias_db,
            occupied_bandwidth_hz(&x[SETTLE..], fs, -60.0),
            tilt
        );
    }

    println!(
        "\n   12b. The same warp across the keyboard, at a fixed d = 0.02. Aliasing\n        \
         should appear where f0/(2d) crosses Nyquist, and nowhere else.\n"
    );
    println!(
        "      {:>10}  {:>10}  {:>10}  {:>9}",
        "f0 (Hz)", "fast seg", "alias dB", "bw (Hz)"
    );
    for f in [55.0f64, 110.0, 220.0, 440.0, 880.0] {
        let p = periods_for(f, fs, N);
        let f0 = p as f64 * fs / N as f64;
        let mut osc = PhaseDistort::new(fs, f0, 0.02);
        let fast = osc.fast_segment_hz();
        let x = render(&mut osc, N + SETTLE);
        let sp = analyse(&x[SETTLE..], p, fs);
        println!(
            "      {:>10.1}  {:>10.0}  {:>10.1}  {:>9.0}",
            f0,
            fast,
            sp.alias_db,
            occupied_bandwidth_hz(&x[SETTLE..], fs, -60.0)
        );
    }

    println!(
        "\n   12c. Resonant waveforms: a sine at k times the fundamental, restarted and\n        \
         windowed every fundamental period. {:.1} Hz. `corrected` applies the shipped\n        \
         two-point PolyBLEP to the step at the restart.\n",
        pd_f0
    );
    println!(
        "      {:>11}  {:>6}  {:>11}  {:>10}  {:>10}",
        "window", "k", "corrected", "alias dB", "A-wtd dB"
    );
    for window in [PdWindow::Saw, PdWindow::Triangle, PdWindow::Trapezoid] {
        for k in [4.0f64, 16.0] {
            for corrected in [false, true] {
                let mut osc = PdResonant::new(fs, pd_f0, k, window, corrected);
                let x = render(&mut osc, N + SETTLE);
                let sp = analyse(&x[SETTLE..], pd_p, fs);
                println!(
                    "      {:>11}  {:>6.0}  {:>11}  {:>10.1}  {:>10.1}",
                    format!("{window:?}"),
                    k,
                    if corrected { "yes" } else { "no" },
                    sp.alias_db,
                    sp.alias_db_a
                );
            }
        }
    }

    println!(
        "\n   12d. Resonance multiple. Saw window, uncorrected and corrected, sweeping\n        \
         the resonant peak up the spectrum the way a CZ filter envelope does.\n"
    );
    println!(
        "      {:>6}  {:>12}  {:>12}  {:>12}",
        "k", "peak (Hz)", "alias dB", "corrected dB"
    );
    for k in [1.0f64, 2.0, 4.0, 8.0, 16.0, 32.0, 48.0] {
        let mut raw = PdResonant::new(fs, pd_f0, k, PdWindow::Saw, false);
        let mut fixed = PdResonant::new(fs, pd_f0, k, PdWindow::Saw, true);
        let a = render(&mut raw, N + SETTLE);
        let b = render(&mut fixed, N + SETTLE);
        println!(
            "      {:>6.0}  {:>12.0}  {:>12.1}  {:>12.1}",
            k,
            k * pd_f0,
            analyse(&a[SETTLE..], pd_p, fs).alias_db,
            analyse(&b[SETTLE..], pd_p, fs).alias_db
        );
    }

    println!("\n   12e. Cost\n");
    println!("      {:>22}  {:>10}  {:>8}", "variant", "ns/sample", "rel");
    cost(
        "PD sawtooth",
        |_| PhaseDistort::new(fs, pd_f0, 0.05),
        cost_inc,
        baseline,
    );
    cost(
        "PD resonant",
        |_| PdResonant::new(fs, pd_f0, 8.0, PdWindow::Saw, false),
        cost_inc,
        baseline,
    );
    cost(
        "PD resonant + BLEP",
        |_| PdResonant::new(fs, pd_f0, 8.0, PdWindow::Saw, true),
        cost_inc,
        baseline,
    );

    // -----------------------------------------------------------------------
    // 13. Sampler playback
    // -----------------------------------------------------------------------

    println!("\n13. Sampler playback\n");

    println!(
        "   13a. Converter and level. A 440 Hz sine quantised, then measured as\n        \
         everything that is not the wanted bin, relative to it. The level column is\n        \
         how far the signal sits below full scale.\n"
    );
    let conv_p = periods_for(440.0, fs, N);
    print!("      {:>14}", "level");
    for c in [
        Converter::Linear(16),
        Converter::Linear(12),
        Converter::Linear(8),
        Converter::MuLaw,
    ] {
        print!("  {:>14}", c.name());
    }
    println!();
    for level_db in [0.0f64, -6.0, -18.0, -36.0, -54.0] {
        let amp = 10f64.powf(level_db / 20.0);
        print!("      {level_db:>11.0} dB");
        for c in [
            Converter::Linear(16),
            Converter::Linear(12),
            Converter::Linear(8),
            Converter::MuLaw,
        ] {
            let x: Vec<f64> = (0..N)
                .map(|i| {
                    let t = 2.0 * PI * conv_p as f64 * i as f64 / N as f64;
                    c.quantise(amp * t.sin())
                })
                .collect();
            print!("  {:>11.1} dB", sine_purity_db(&x, conv_p));
        }
        println!();
    }

    println!(
        "\n   13b. Input sample rate, with and without an anti-alias filter in front of\n        \
         the converter. The source is a 220 Hz sawtooth carrying every harmonic below\n        \
         19.8 kHz, as a full-bandwidth signal arriving at the input would.\n"
    );
    println!(
        "      {:>14}  {:>12}  {:>10}  {:>12}  {:>10}",
        "machine", "rate (Hz)", "harmonics", "filtered dB", "raw dB"
    );
    for (machine, fs_v) in [
        ("CD / S1000", 44100.0f64),
        ("Emulator II", 27700.0),
        ("SP-1200", 26040.0),
        ("Mirage-ish", 22050.0),
        ("low fidelity", 15000.0),
    ] {
        let p = periods_for(220.0, fs_v, N);
        let f0 = p as f64 * fs_v / N as f64;
        let filtered = sampled_at(f0, fs_v, 0.45 * fs_v, N);
        let raw = sampled_at(f0, fs_v, 19845.0, N);
        println!(
            "      {:>14}  {:>12.0}  {:>10}  {:>12.1}  {:>10.1}",
            machine,
            fs_v,
            (0.45 * fs_v / f0) as usize,
            analyse(&filtered, p, fs_v).alias_db,
            analyse(&raw, p, fs_v).alias_db
        );
    }

    println!(
        "\n   13c. Reconstruction. A 27.7 kHz signal held by a zero-order-hold converter\n        \
         and measured at 4x that rate, so the images the hold leaves are visible.\n        \
         `droop` is the level at the top of the band against the level at the bottom.\n"
    );
    {
        let fs_v = 27700.0f64;
        let fs_hi = fs_v * 4.0;
        println!(
            "      {:>12}  {:>12}  {:>14}  {:>12}",
            "tone (Hz)", "images dB", "with 96 dB LP", "droop dB"
        );
        let taps = decimation_taps(4);
        for frac in [0.05f64, 0.2, 0.4] {
            let p_lo = {
                let target = frac * fs_v;
                let p = (target * (N / 4) as f64 / fs_v).round() as usize;
                if p.is_multiple_of(2) { p + 1 } else { p }
            };
            let f0 = p_lo as f64 * fs_v / (N / 4) as f64;
            let low: Vec<f64> = (0..N / 4)
                .map(|i| (2.0 * PI * p_lo as f64 * i as f64 / (N / 4) as f64).sin())
                .collect();
            let held = zero_order_hold(&low, 4);

            // Images: everything above fs_v/2 in the held signal, against the tone.
            let image_db = {
                let mut re = held.clone();
                let mut im = vec![0.0f64; N];
                fft(&mut re, &mut im).expect("the harness allocates power-of-two buffers");
                let (mut want, mut images) = (0.0f64, 0.0f64);
                for bin in 1..N / 2 {
                    let e = re[bin] * re[bin] + im[bin] * im[bin];
                    let hz = bin as f64 * fs_hi / N as f64;
                    if hz < fs_v / 2.0 {
                        want += e;
                    } else {
                        images += e;
                    }
                }
                10.0 * (images / want.max(1e-30)).max(1e-30).log10()
            };

            // The same, through a steep reconstruction filter.
            let filtered_db = {
                let mut y = vec![0.0f64; N];
                for i in 0..N {
                    let mut acc = 0.0;
                    for (k, &t) in taps.iter().enumerate() {
                        if i + k >= taps.len() {
                            acc += t * held[i + k - taps.len()];
                        }
                    }
                    y[i] = acc;
                }
                let mut re = y.clone();
                let mut im = vec![0.0f64; N];
                fft(&mut re, &mut im).expect("the harness allocates power-of-two buffers");
                let (mut want, mut images) = (0.0f64, 0.0f64);
                for bin in 1..N / 2 {
                    let e = re[bin] * re[bin] + im[bin] * im[bin];
                    let hz = bin as f64 * fs_hi / N as f64;
                    if hz < fs_v / 2.0 {
                        want += e;
                    } else {
                        images += e;
                    }
                }
                10.0 * (images / want.max(1e-30)).max(1e-30).log10()
            };

            // Zero-order-hold droop, analytically: sinc(pi f / fs_v).
            let arg = PI * f0 / fs_v;
            let droop = 20.0 * (arg.sin() / arg).log10();
            println!(
                "      {:>12.0}  {:>12.1}  {:>14.1}  {:>12.2}",
                f0, image_db, filtered_db, droop
            );
        }
    }

    println!(
        "\n   13d. Transposition. One period of a sawtooth recorded with every harmonic\n        \
         below 19.8 kHz at a source pitch of 220 Hz, then played back at other\n        \
         pitches — which is how a sampler changes pitch.\n"
    );
    println!(
        "      {:>10}  {:>10}  {:>12}  {:>12}  {:>12}",
        "played (Hz)", "ratio", "drop dB", "linear dB", "cubic dB"
    );
    let source_harmonics = (19845.0f64 / 220.0) as usize;
    for target in [55.0f64, 110.0, 220.0, 440.0, 880.0] {
        let p = periods_for(target, fs, N);
        let f0 = p as f64 * fs / N as f64;
        let inc = p as f64 / N as f64;
        let mut db = [0.0f64; 3];
        for (j, interp) in [Interp::Drop, Interp::Linear, Interp::Cubic]
            .into_iter()
            .enumerate()
        {
            let mut osc = SampleOsc::new(4096, source_harmonics, inc, interp);
            let x = render(&mut osc, N + SETTLE);
            db[j] = analyse(&x[SETTLE..], p, fs).alias_db;
        }
        println!(
            "      {:>10.1}  {:>10.2}  {:>12.1}  {:>12.1}  {:>12.1}",
            f0,
            f0 / 220.0,
            db[0],
            db[1],
            db[2]
        );
    }

    // -----------------------------------------------------------------------
    // 14. Grain pitch
    // -----------------------------------------------------------------------

    println!("\n14. Grain pitch\n");

    println!(
        "   Grains firing at 220 Hz, four overlapping, Hann windowed, each reading a\n   \
         sawtooth recorded at 220 Hz carrying every harmonic below 19.8 kHz. The ratio\n   \
         is what a grain-pitch control does to the source inside each grain.\n"
    );
    println!(
        "      {:>8}  {:>12}  {:>12}  {:>12}",
        "ratio", "linear dB", "cubic dB", "A-wtd cubic"
    );
    {
        let p = periods_for(220.0, fs, N);
        let note = p as f64 * fs / N as f64;
        let harmonics = (19845.0f64 / 220.0) as usize;
        for ratio in [0.25f64, 0.5, 1.0, 2.0, 4.0] {
            let mut lin =
                SampleGrainTrain::new(fs, note, 4.0, note, ratio, harmonics, Interp::Linear);
            let mut cub =
                SampleGrainTrain::new(fs, note, 4.0, note, ratio, harmonics, Interp::Cubic);
            let a = render(&mut lin, N + SETTLE);
            let b = render(&mut cub, N + SETTLE);
            let sa = analyse(&a[SETTLE..], p, fs);
            let sb = analyse(&b[SETTLE..], p, fs);
            println!(
                "      {:>8.2}  {:>12.1}  {:>12.1}  {:>12.1}",
                ratio, sa.alias_db, sb.alias_db, sb.alias_db_a
            );
        }
    }

    // -----------------------------------------------------------------------
    // 15. Phase vocoder, and what it does to noise
    // -----------------------------------------------------------------------

    println!("\n15. Phase vocoder\n");

    const NFFT: usize = 1024;
    const HOP: usize = 256;
    let src_len = (fs * 1.5) as usize;

    // Two sources: a harmonic tone, and band-limited noise.
    let tone: Vec<f64> = {
        let p = periods_for(220.0, fs, N);
        let f0 = p as f64 * fs / N as f64;
        (0..src_len)
            .map(|n| {
                let t = n as f64 / fs;
                (1..=16)
                    .map(|k| (2.0 * PI * f0 * k as f64 * t).sin() / k as f64)
                    .sum::<f64>()
                    * 0.4
            })
            .collect()
    };
    let noise: Vec<f64> = {
        let mut rng = mxm_mono_01_dsp::Rng::new(0x4e4f_4953);
        let raw: Vec<f64> = (0..src_len)
            .map(|_| rng.next_bipolar() as f64 * 0.4)
            .collect();
        // Band-limit it so the flatness measure is not dominated by the very top.
        let taps = decimation_taps(1);
        (0..src_len)
            .map(|i| {
                taps.iter()
                    .enumerate()
                    .map(|(k, &t)| {
                        if i + k >= taps.len() {
                            t * raw[i + k - taps.len()]
                        } else {
                            0.0
                        }
                    })
                    .sum()
            })
            .collect()
    };

    println!(
        "   15a. Does the transform survive a round trip? Stretch 1.0, so analysis and\n        \
         synthesis hops match and nothing has to be invented.\n"
    );
    println!("      {:>12}  {:>16}", "source", "error vs input dB");
    for (name, src) in [("harmonic tone", &tone), ("noise", &noise)] {
        let y = pvoc_stretch(src, 1.0, NFFT, HOP, PhaseMode::Propagate, src_len);
        // Compare over the steady middle, allowing for the analysis latency.
        let a = NFFT * 4;
        let b = src_len - NFFT * 4;
        let (mut err, mut sig) = (0.0f64, 0.0f64);
        for i in a..b {
            let d = y[i] - src[i];
            err += d * d;
            sig += src[i] * src[i];
        }
        println!(
            "      {:>12}  {:>16.1}",
            name,
            10.0 * (err / sig.max(1e-30)).max(1e-30).log10()
        );
    }

    println!(
        "\n   15b. Stretching a harmonic tone 8x. `cents` is the output pitch against the\n        \
         source; additive resynthesis managed 0.00 cents in §10d and naive overlap-add\n        \
         -167.\n"
    );
    println!(
        "      {:>12}  {:>12}  {:>12}",
        "phase mode", "out f0 (Hz)", "cents off"
    );
    {
        let p = periods_for(220.0, fs, N);
        let f0 = p as f64 * fs / N as f64;
        for mode in [PhaseMode::Keep, PhaseMode::Propagate, PhaseMode::Randomise] {
            let out_len = src_len * 8;
            let y = pvoc_stretch(&tone, 8.0, NFFT, HOP, mode, out_len);
            let a = out_len / 3;
            let f = peak_frequency(&y[a..(a + 32768).min(out_len)], fs);
            println!(
                "      {:>12}  {:>12.2}  {:>12.1}",
                format!("{mode:?}"),
                f,
                1200.0 * (f / f0).log2()
            );
        }
    }

    println!(
        "\n   15c. Stretching noise 8x. Spectral flatness measured from 200 Hz to 8 kHz:\n        \
         near 0 dB is noise, strongly negative is energy collected into peaks.\n"
    );
    println!(
        "      {:>12}  {:>16}  {:>14}  {:>18}",
        "phase mode", "flatness dB", "vs source", "repeat at hop"
    );
    {
        let src_flat = spectral_flatness_db(&noise[NFFT * 4..], fs, 200.0, 8000.0)
            .expect("the source band is not empty");
        println!(
            "      {:>12}  {:>16.2}  {:>14}  {:>18.4}",
            "source",
            src_flat,
            "-",
            periodicity_at_lag(&noise[NFFT * 4..], HOP)
        );
        for mode in [PhaseMode::Keep, PhaseMode::Propagate, PhaseMode::Randomise] {
            let out_len = src_len * 8;
            let y = pvoc_stretch(&noise, 8.0, NFFT, HOP, mode, out_len);
            let a = out_len / 3;
            let slice = &y[a..(a + 32768).min(out_len)];
            let flat = spectral_flatness_db(slice, fs, 200.0, 8000.0)
                .expect("a stretched band is not empty");
            println!(
                "      {:>12}  {:>16.2}  {:>+14.2}  {:>18.4}",
                format!("{mode:?}"),
                flat,
                flat - src_flat,
                periodicity_at_lag(slice, HOP)
            );
        }
    }

    println!("\n   15d. The same, across stretch factors, standard phase propagation only.\n");
    println!(
        "      {:>10}  {:>16}  {:>16}",
        "stretch", "noise flatness", "tone cents off"
    );
    {
        let p = periods_for(220.0, fs, N);
        let f0 = p as f64 * fs / N as f64;
        for stretch in [1.0f64, 2.0, 4.0, 8.0, 16.0] {
            let out_len = ((src_len as f64) * stretch) as usize;
            let yn = pvoc_stretch(&noise, stretch, NFFT, HOP, PhaseMode::Propagate, out_len);
            let yt = pvoc_stretch(&tone, stretch, NFFT, HOP, PhaseMode::Propagate, out_len);
            let a = out_len / 3;
            let flat = spectral_flatness_db(&yn[a..(a + 32768).min(out_len)], fs, 200.0, 8000.0)
                .expect("a stretched band is not empty");
            let f = peak_frequency(&yt[a..(a + 32768).min(out_len)], fs);
            println!(
                "      {:>10.1}  {:>16.2}  {:>16.1}",
                stretch,
                flat,
                1200.0 * (f / f0).log2()
            );
        }
    }

    // -----------------------------------------------------------------------
    // 16. Hard sync
    // -----------------------------------------------------------------------

    println!("\n16. Hard sync\n");

    println!(
        "   Master {:.1} Hz. The slave's own wrap gets the ordinary sawtooth correction;\n   \
         the reset gets one scaled by a step height computed from the slave's\n   \
         instantaneous value. `vs ref` is deviation from an 8x rendering, which is the\n   \
         fidelity companion — a correction that works by removing treble fails there.\n",
        440.8
    );
    println!(
        "      {:>8}  {:>12}  {:>12}  {:>12}  {:>12}",
        "ratio", "naive dB", "corrected dB", "naive ref", "corr ref"
    );
    {
        let p = periods_for(440.0, fs, N);
        let master = p as f64 * fs / N as f64;
        for ratio in [1.0f64, 1.5, 2.0, 3.0, 4.5, 6.0, 8.0] {
            let slave = master * ratio;
            let mut naive = HardSync::new(fs, master, slave, false);
            let mut fixed = HardSync::new(fs, master, slave, true);
            let a = render(&mut naive, N + SETTLE);
            let b = render(&mut fixed, N + SETTLE);
            let ref_naive = oversampled_reference(
                |rate| Box::new(HardSync::new(rate, master, slave, false)),
                fs,
                8,
                N + SETTLE,
            );
            let (_, dev_a) = harmonic_deviation(&a[SETTLE..], &ref_naive[SETTLE..], 1, fs);
            let (_, dev_b) = harmonic_deviation(&b[SETTLE..], &ref_naive[SETTLE..], 1, fs);
            println!(
                "      {:>8.1}  {:>12.1}  {:>12.1}  {:>12.1}  {:>12.1}",
                ratio,
                analyse(&a[SETTLE..], p, fs).alias_db,
                analyse(&b[SETTLE..], p, fs).alias_db,
                dev_a,
                dev_b
            );
        }
    }

    // -----------------------------------------------------------------------
    // 17. Supersaw
    // -----------------------------------------------------------------------

    println!("\n17. Supersaw and unison\n");

    let ss_centre = periods_for(220.0, fs, N);
    let ss_f0 = ss_centre as f64 * fs / N as f64;

    println!(
        "   17a. Voice count at a fixed detune spread. {:.1} Hz centre, power-preserving\n        \
         normalisation. `alias` is energy off the union of the voices' harmonic series;\n        \
         `flat` is the flatness of the alias bins only — near 0 dB means the aliasing has\n        \
         spread into a noise floor rather than staying as discrete ghosts.\n",
        ss_f0
    );
    println!(
        "      {:>7}  {:>10}  {:>10}  {:>12}  {:>12}",
        "voices", "alias dB", "flat dB", "collisions", "vs ref dB"
    );
    for count in [1usize, 3, 5, 7, 9] {
        let periods = detune_periods(ss_centre, count, 4);
        let mut osc = Supersaw::new(&periods, true, false);
        let x = render(&mut osc, N + SETTLE);
        let m = analyse_multi(&x[SETTLE..], &periods, fs);
        let reference = oversampled_reference(
            move |_rate| {
                Box::new(Supersaw::new(
                    &detune_periods(ss_centre, count, 4),
                    true,
                    false,
                ))
            },
            fs,
            8,
            N + SETTLE,
        );
        let (_, dev) = harmonic_deviation(&x[SETTLE..], &reference[SETTLE..], 1, fs);
        println!(
            "      {:>7}  {:>10.1}  {:>10.2}  {:>12}  {:>12.1}",
            count, m.alias_db, m.alias_flatness_db, m.collisions, dev
        );
    }

    println!(
        "\n   17b. Detune spread at seven voices. Spread is in period-count units; one\n        \
         unit is about {:.1} cents at this fundamental.\n",
        1200.0 * (1.0 + 2.0 / ss_centre as f64).log2()
    );
    println!(
        "      {:>8}  {:>10}  {:>10}  {:>10}  {:>12}",
        "spread", "cents", "alias dB", "flat dB", "collisions"
    );
    for spread in [0usize, 1, 2, 4, 8, 16] {
        let periods = detune_periods(ss_centre, 7, spread);
        let mut osc = Supersaw::new(&periods, true, false);
        let x = render(&mut osc, N + SETTLE);
        let m = analyse_multi(&x[SETTLE..], &periods, fs);
        let cents = 1200.0 * (1.0 + 2.0 * spread as f64 / ss_centre as f64).log2();
        println!(
            "      {:>8}  {:>10.1}  {:>10.1}  {:>10.2}  {:>12}",
            spread, cents, m.alias_db, m.alias_flatness_db, m.collisions
        );
    }

    println!(
        "\n   17c. Note-on peak: all voices starting at phase zero against randomised\n        \
         start phases. Seven voices, power-preserving normalisation.\n"
    );
    println!(
        "      {:>14}  {:>12}  {:>12}",
        "start phase", "peak", "peak dB"
    );
    for (label, random) in [("all zero", false), ("randomised", true)] {
        let periods = detune_periods(ss_centre, 7, 4);
        let mut osc = Supersaw::new(&periods, true, random);
        let mut peak = 0.0f64;
        for _ in 0..(fs * 0.05) as usize {
            peak = peak.max(osc.next().abs());
        }
        println!(
            "      {:>14}  {:>12.4}  {:>12.2}",
            label,
            peak,
            20.0 * peak.max(1e-30).log10()
        );
    }

    println!("\n   17d. Cost against voice count\n");
    println!("      {:>22}  {:>10}  {:>8}", "voices", "ns/sample", "rel");
    for count in [1usize, 3, 7, 9] {
        let periods = detune_periods(ss_centre, count, 4);
        cost(
            &format!("{count} voices"),
            |_| Supersaw::new(&periods, true, false),
            cost_inc,
            baseline,
        );
    }

    // -----------------------------------------------------------------------
    // 18. Discrete summation formulae
    // -----------------------------------------------------------------------

    println!("\n18. Discrete summation formulae\n");

    println!(
        "   A closed form for a sum of K harmonics: exactly band-limited by construction,\n   \
         O(1) per sample whatever K is, and geometrically tilted rather than 1/k.\n   \
         `tilt` is the least-squares slope over the first eight harmonics; a sawtooth\n   \
         is -6.02 dB/octave.\n"
    );
    println!(
        "      {:>8}  {:>7}  {:>10}  {:>12}  {:>12}",
        "f0 (Hz)", "harm", "alias dB", "tilt dB/oct", "ns/sample"
    );
    for f in [55.0f64, 220.0, 880.0] {
        let p = periods_for(f, fs, N);
        let f0 = p as f64 * fs / N as f64;
        let inc = p as f64 / N as f64;
        let harmonics = ((0.45 * fs / f0) as usize).max(1);
        let mut osc = Dsf::new(inc, harmonics, -60.0);
        let x = render(&mut osc, N + SETTLE);
        let sp = analyse(&x[SETTLE..], p, fs);
        let tilt = {
            let mut re = x[SETTLE..].to_vec();
            let mut im = vec![0.0f64; N];
            fft(&mut re, &mut im).expect("the harness allocates power-of-two buffers");
            let mag = |k: usize| {
                let b = k * p;
                (re[b] * re[b] + im[b] * im[b]).sqrt().max(1e-30)
            };
            let (mut sx, mut sy, mut sxy, mut sxx) = (0.0f64, 0.0, 0.0, 0.0);
            for k in 1..=8 {
                let lx = (k as f64).log2();
                let ly = 20.0 * mag(k).log10();
                sx += lx;
                sy += ly;
                sxy += lx * ly;
                sxx += lx * lx;
            }
            (8.0 * sxy - sx * sy) / (8.0 * sxx - sx * sx)
        };
        let ns = {
            let mut o = Dsf::new(inc, harmonics, -60.0);
            for _ in 0..4096 {
                black_box(o.next());
            }
            let t0 = Instant::now();
            let mut acc = 0.0;
            for _ in 0..N {
                acc += o.next();
            }
            black_box(acc);
            t0.elapsed().as_nanos() as f64 / N as f64
        };
        println!(
            "      {:>8.1}  {:>7}  {:>10.1}  {:>12.2}  {:>12.2}",
            f0, harmonics, sp.alias_db, tilt, ns
        );
    }

    // -----------------------------------------------------------------------
    // 19. Waveshaping, folding and ADAA
    // -----------------------------------------------------------------------

    println!("\n19. Waveshaping, folding and ADAA\n");

    println!(
        "   A {:.0} Hz sine through each shaper, high enough that the harmonics it makes\n   \
         run past Nyquist — the only regime where any of this matters. `alias` is the\n   \
         exactly-periodic analysis; `vs ref` is deviation from a 16x rendering of the\n   \
         same shaper — the fidelity companion, because ADAA attenuates the wanted\n   \
         harmonics too, and that is the trade this table exists to show.\n",
        2000.0
    );
    println!(
        "      {:>12}  {:>12}  {:>10}  {:>10}  {:>10}",
        "shaper", "method", "alias dB", "vs ref dB", "ns/sample"
    );
    {
        let p = periods_for(2000.0, fs, N);
        let inc = p as f64 / N as f64;
        for shaper in [
            Shaper::Tanh(20.0),
            Shaper::SineFold(8.0),
            Shaper::Cheb5(1.0),
        ] {
            let reference = oversampled_reference(
                move |rate| {
                    let hi_inc = inc * fs / rate;
                    Box::new(Shaped::new(hi_inc, shaper, false))
                },
                fs,
                16,
                N + SETTLE,
            );
            for (label, adaa, os) in [
                ("trivial", false, 1usize),
                ("ADAA 1st", true, 1),
                ("2x OS", false, 2),
                ("4x OS", false, 4),
                ("8x OS", false, 8),
            ] {
                let x = if os == 1 {
                    let mut o = Shaped::new(inc, shaper, adaa);
                    render(&mut o, N + SETTLE)
                } else {
                    oversampled_reference(
                        move |rate| {
                            let hi_inc = inc * fs / rate;
                            Box::new(Shaped::new(hi_inc, shaper, false))
                        },
                        fs,
                        os,
                        N + SETTLE,
                    )
                };
                let (_, dev) = harmonic_deviation(&x[SETTLE..], &reference[SETTLE..], 1, fs);
                let ns = {
                    let mut o = Shaped::new(inc, shaper, adaa);
                    for _ in 0..4096 {
                        black_box(o.next());
                    }
                    let t0 = Instant::now();
                    let mut acc = 0.0;
                    for _ in 0..N {
                        acc += o.next();
                    }
                    black_box(acc);
                    let per = t0.elapsed().as_nanos() as f64 / N as f64;
                    if os == 1 { per } else { per * os as f64 }
                };
                println!(
                    "      {:>12}  {:>12}  {:>10.1}  {:>10.1}  {:>10.2}",
                    shaper.name(),
                    label,
                    analyse(&x[SETTLE..], p, fs).alias_db,
                    dev,
                    ns
                );
            }
        }
    }

    println!(
        "\n   19b. Alias against drive, sine folder at 2 kHz, trivial against first-order\n        \
         ADAA. A positive gain means ADAA helped; at low drive there is nothing to fix.\n"
    );
    println!(
        "      {:>8}  {:>12}  {:>12}  {:>10}",
        "drive", "trivial dB", "ADAA dB", "gain dB"
    );
    {
        let p = periods_for(2000.0, fs, N);
        let inc = p as f64 / N as f64;
        for g in [1.0f64, 2.0, 4.0, 8.0, 16.0, 32.0] {
            let shaper = Shaper::SineFold(g);
            let mut a = Shaped::new(inc, shaper, false);
            let mut b = Shaped::new(inc, shaper, true);
            let xa = render(&mut a, N + SETTLE);
            let xb = render(&mut b, N + SETTLE);
            let da = analyse(&xa[SETTLE..], p, fs).alias_db;
            let db_ = analyse(&xb[SETTLE..], p, fs).alias_db;
            println!(
                "      {:>8.0}  {:>12.1}  {:>12.1}  {:>10.1}",
                g,
                da,
                db_,
                da - db_
            );
        }
    }

    // -----------------------------------------------------------------------
    // 20. Ring modulation, and noise
    // -----------------------------------------------------------------------

    println!("\n20. Ring modulation and noise\n");

    println!(
        "   20a. Ring modulation. Both inputs are band-limited; the product is not,\n        \
         because every pair of partials makes a sum as well as a difference. `a:b` are\n        \
         multiples of a {:.1} Hz fundamental.\n",
        periods_for(110.0, fs, N) as f64 * fs / N as f64
    );
    println!(
        "      {:>10}  {:>8}  {:>12}  {:>12}",
        "inputs", "a:b", "alias dB", "A-wtd dB"
    );
    {
        let p = periods_for(110.0, fs, N);
        for (label, sine) in [("sine x sine", true), ("saw x saw", false)] {
            for (a, b) in [(2usize, 3usize), (4, 7), (8, 13)] {
                let mut osc =
                    RingMod::new((a * p) as f64 / N as f64, (b * p) as f64 / N as f64, sine);
                let x = render(&mut osc, N + SETTLE);
                let sp = analyse(&x[SETTLE..], p, fs);
                println!(
                    "      {:>10}  {:>8}  {:>12.1}  {:>12.1}",
                    label,
                    format!("{a}:{b}"),
                    sp.alias_db,
                    sp.alias_db_a
                );
            }
        }
    }

    println!(
        "\n   20b. Noise colours, measured as the least-squares slope over octave bands\n        \
         from 100 Hz to 10 kHz.\n"
    );
    println!(
        "      {:>10}  {:>16}  {:>14}  {:>10}",
        "colour", "slope dB/oct", "flatness dB", "ns/sample"
    );
    for colour in [
        NoiseColour::White,
        NoiseColour::Pink,
        NoiseColour::Brown,
        NoiseColour::Blue,
    ] {
        let mut osc = ColouredNoise::new(colour);
        let x = render(&mut osc, N);
        let slope = spectral_slope_db_per_oct(&x, fs, 100.0, 10_000.0);
        let flat =
            spectral_flatness_db(&x, fs, 100.0, 10_000.0).expect("the noise band is not empty");
        let ns = {
            let mut o = ColouredNoise::new(colour);
            for _ in 0..4096 {
                black_box(o.next());
            }
            let t0 = Instant::now();
            let mut acc = 0.0;
            for _ in 0..N {
                acc += o.next();
            }
            black_box(acc);
            t0.elapsed().as_nanos() as f64 / N as f64
        };
        println!(
            "      {:>10}  {:>16.2}  {:>14.2}  {:>10.2}",
            format!("{colour:?}"),
            slope,
            flat,
            ns
        );
    }
}
