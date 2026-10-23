//! The screen: whether a candidate is **obviously** unlike its reference, judged against a second
//! take of the reference, before the owner hears it (the owner, 2026-10-22: "Some of the sounds you
//! make me evaluate/compare are very obviously different from the originals ... Some simple way to
//! catch obvious bad ones?"; 2026-10-23, of the drums' own screen: "Should this method not be part
//! of the listener?").
//!
//! Where the auditory map ([`crate::unexplained`]) names coarse regions a finding should explain, the
//! screen reads the whole sound finely and says one thing: off or not. Each sound is read from its
//! own onset ([`crate::prep::onset`]), the candidate matched to the reference in body loudness
//! ([`crate::prep::body_rms`], as the A/B page plays them); every [`HOP_MS`], in third octaves from
//! [`LOWEST_HZ`] to 16 kHz, each band's Hann window long enough for four periods of its lowest
//! frequency (5 ms up high, 160 ms at the bottom), its energy averaged over [`SMOOTH_FRAMES`] and
//! A-weighted at its band's centre (IEC 61672-1), so each cell counts as loud as it is heard. A
//! cell is heard where either sound stands within [`RANGE_DB`] of the reference's loudest cell.
//!
//! **A recording's hiss is no part of the sound** (the owner, 2026-10-23: "Many of the drums have a
//! noise floor that is interfering ... Noise does not change tone or spectrum but the real sound
//! does"). Each band's hiss is read from the recording's end backwards ([`hiss`]) and, where the
//! caller knows how loud the recording's noise can be, from the first stretch after the strike where
//! the band holds still under that level ([`recording_noise`]): a recording that fades out at its
//! end holds there only its export's dither, the noise faded under it (Big Rusty's takes, the drum
//! models' solve, 2026-10-23). The louder of the two is taken out of its cells: what is left is the
//! sound's own part. Where a
//! recording holds nothing but its hiss ([`HISS_CLEAR_DB`]), the candidate is off only for standing
//! over it, never for lacking it. The candidate's own hiss is kept when it is a model: a model that
//! rings a noise of its own still shows. A recording too short to hold a stretch after its strike
//! takes its floor from its quietest tenth while it sounds instead.
//!
//! Three numbers: the share of the heard cells, weighted by loudness, more than [`OFF_DB`] off; the
//! largest connected patch of them, by its share of the loudness; and the tail, the share of the
//! energy over [`TAIL_HZ`] from [`TAIL_FROM_MS`] on, the candidate's against the reference's, dB
//! (the drum gate's sustain, `mxm-model-drums`), each recording's hiss taken out of it. A second
//! take read the same way against the reference is the yardstick: what a second hit of the same
//! instrument differs by is no fault.
//!
//! **Technique:** short-time spectra pooled into bands, each band's window chosen for its frequency
//! (a multi-resolution spectrogram in the spirit of Brown's constant-Q transform, J. Acoust. Soc.
//! Am. 89(1), 1991, here in steps of two); the band power from Parseval's theorem over the Hann
//! window; the hiss taken out by power subtraction (Boll, "Suppression of acoustic noise in speech
//! using spectral subtraction", IEEE Trans. ASSP 27(2), 1979), its level from a least-squares line
//! through the end's frames; connected components of the off cells by flood fill over four
//! neighbours (Rosenfeld & Pfaltz, "Sequential operations in digital picture processing", J. ACM
//! 13(4), 1966). Written here from those descriptions.
//!
//! **The original is the standard, its noise floor aside** (the owner, 2026-10-23: "Remember that
//! the approved ones are not the gold standard. The original one is."): a candidate is judged only
//! against the recording and a second take of it, never against models approved before. The
//! thresholds mark what is obvious: the hits the owner sent back on 2026-10-22 stood 20-80 % off. A
//! model approved earlier that the screen flags differs from its original, and is looked at again.

use mxm_measure::spectrum::a_weight;

use crate::prep;
use crate::repr::{bands, spectrum};
use crate::sound::Sound;

/// The grid's step in time, ms.
pub const HOP_MS: f64 = 5.0;
/// Each cell's energy is averaged over this many steps (20 ms).
pub const SMOOTH_FRAMES: usize = 4;
/// The lowest third octave's centre, Hz: a kick's fundamental sits near 30 Hz.
pub const LOWEST_HZ: f64 = 20.0;
/// Third octaves from [`LOWEST_HZ`]: the last is centred at 16.3 kHz.
pub const BANDS: usize = 30;
/// The windows a band may take, ms: the shortest holding four periods of its lowest frequency.
pub const WINDOWS_MS: [f64; 6] = [5.0, 10.0, 20.0, 40.0, 80.0, 160.0];
/// A cell is heard within this far under the reference's loudest cell, dB.
pub const RANGE_DB: f64 = 60.0;
/// A cell this far from the reference is off, dB.
pub const OFF_DB: f64 = 10.0;
/// Off where more than this share of the loudness is off and well over the takes' own share.
pub const SHARE_OFF: f64 = 0.15;
/// Off where one patch carries more than this share of the loudness.
pub const PATCH_OFF: f64 = 0.15;
/// The grid reads at most this long, ms: where both sounds have fallen 60 dB, or this.
pub const LONGEST_MS: f64 = 3000.0;
/// The tail: the energy from here on, ms from the onset ...
pub const TAIL_FROM_MS: f64 = 150.0;
/// ... over this frequency (under it a recording holds other drums ringing and the room), Hz.
pub const TAIL_HZ: f64 = 250.0;
/// Off where the tail is this far under the reference's, past the takes' own difference, dB.
pub const TAIL_OFF_DB: f64 = 8.0;

/// The hiss is read over a recording's last this long, s ...
pub const HISS_S: f64 = 0.5;
/// ... in frames this long, s ...
pub const HISS_FRAME_S: f64 = 0.1;
/// ... short of its very end, s (a fade or a cut there is the file's, not its floor).
pub const HISS_GUARD_S: f64 = 0.01;
/// A band still falling over the stretch by more than this many times what a still noise's frames
/// could make of it is the sound ringing on, not the floor.
pub const HISS_SURE: f64 = 2.0;
/// The floor under a band still falling lies under its stretch by what it would fall in this much
/// more, s.
pub const HISS_AHEAD_S: f64 = 0.5;
/// A recording's cell within this of its hiss holds nothing else, dB: the hiss read at the end
/// stands within a dB or two of the floor earlier, and a cell that close, less it, leaves only that
/// error.
pub const HISS_CLEAR_DB: f64 = 3.0;
/// The recording's noise is looked for from this long after the onset, s ...
pub const NOISE_FROM_S: f64 = 0.1;
/// ... as a stretch this long ...
pub const NOISE_FLAT_S: f64 = 0.6;
/// ... holding still: its line's slope within [`HISS_SURE`] times what its frames' own scatter could
/// make of a still band, and never steeper than this, dB a second. A ring also holds still (a ride
/// falls a few dB a second), so a stretch counts only under the caller's ceiling.
pub const NOISE_FLAT_DB_S: f64 = 3.0;
/// Where no hiss could be read from the end, a cell within this of its band's quietest tenth while
/// the sound plays is taken for the floor, dB.
pub const ABOVE_FLOOR_DB: f64 = 10.0;

/// A connected patch of off cells.
#[derive(Clone, Debug, PartialEq)]
pub struct Patch {
    /// The candidate is louder there; else it lacks what the reference has.
    pub too_loud: bool,
    /// Candidate minus reference over the patch, dB.
    pub mean_db: f64,
    pub window_ms: (f64, f64),
    pub band_hz: (f64, f64),
    /// Its share of the sound's loudness, 0–1.
    pub loudness: f64,
}

/// One sound screened against its reference.
#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    /// The share of the heard loudness more than [`OFF_DB`] off, 0–1.
    pub off_share: f64,
    /// The largest patch of off cells.
    pub patch: Option<Patch>,
    /// The tail's share of the energy, the candidate's against the reference's, dB; `None` where
    /// either is shorter than twice [`TAIL_FROM_MS`].
    pub tail_db: Option<f64>,
    /// How long the grid read, ms.
    pub length_ms: f64,
    /// The reference's hiss was read from its end (else from its quietest tenth while it sounds).
    pub hiss_from_end: bool,
    /// Where the share comes from, `[band][frame]`: each heard cell's share of the loudness, signed
    /// as the candidate stands over (+) or under (−) the reference, where it is [`OFF_DB`] off; 0
    /// elsewhere.
    pub off_cells: Vec<Vec<f64>>,
}

/// A third octave's edges, Hz.
#[must_use]
pub fn band_hz(band: usize) -> (f64, f64) {
    let centre = LOWEST_HZ * 2f64.powf(band as f64 / 3.0);
    (centre / 2f64.powf(1.0 / 6.0), centre * 2f64.powf(1.0 / 6.0))
}

/// The window a band whose lowest frequency is `lo_hz` takes, ms.
#[must_use]
pub fn window_ms(lo_hz: f64) -> f64 {
    let want = 4000.0 / lo_hz;
    WINDOWS_MS
        .iter()
        .copied()
        .find(|&w| w >= want)
        .unwrap_or(WINDOWS_MS[WINDOWS_MS.len() - 1])
}

/// The samples from the onset; empty for silence.
fn from_onset(sound: &Sound) -> Vec<f32> {
    prep::onset(&sound.samples).map_or_else(Vec::new, |i| sound.samples[i..].to_vec())
}

/// Until the sound falls [`RANGE_DB`] under its loudest step, ms, in steps of [`HOP_MS`].
fn sounding_ms(x: &[f64], rate: f64) -> f64 {
    let hop = ((HOP_MS / 1000.0 * rate).round() as usize).max(1);
    let steps: Vec<f64> = x
        .chunks(hop)
        .map(|c| c.iter().map(|v| v * v).sum::<f64>() / c.len() as f64)
        .collect();
    let loudest = steps.iter().copied().fold(0.0, f64::max);
    let last = steps
        .iter()
        .rposition(|&p| p > loudest * 10f64.powf(-RANGE_DB / 10.0));
    last.map_or(HOP_MS, |i| (i + 1) as f64 * HOP_MS)
}

/// Each band's mean power every [`HOP_MS`], `[band][frame]`: each frame centred on its step, its
/// window as long as its band asks, its power by Parseval over the window, averaged over
/// [`SMOOTH_FRAMES`].
fn cells(x: &[f64], rate: f64, frames: usize) -> Vec<Vec<f64>> {
    let hop = (HOP_MS / 1000.0 * rate).round() as usize;
    let mut out = vec![vec![0.0; frames]; BANDS];
    for &w in &WINDOWS_MS {
        let here: Vec<usize> = (0..BANDS)
            .filter(|&b| (window_ms(band_hz(b).0) - w).abs() < 1e-9)
            .collect();
        if here.is_empty() {
            continue;
        }
        let n = (w / 1000.0 * rate).round() as usize;
        let window_power: f64 = spectrum::hann(n).iter().map(|v| v * v).sum();
        let mut energy = vec![vec![0.0; frames]; here.len()];
        let mut segment = vec![0.0; n];
        for j in 0..frames {
            let start = (j * hop) as isize - (n / 2) as isize;
            for (i, s) in segment.iter_mut().enumerate() {
                let k = start + i as isize;
                *s = if k >= 0 && (k as usize) < x.len() {
                    x[k as usize]
                } else {
                    0.0
                };
            }
            let Some(s) = spectrum::power_spectrum(&segment, rate, n.max(4096)) else {
                continue;
            };
            let nfft = 2 * (s.power.len() - 1);
            for (row, &b) in energy.iter_mut().zip(&here) {
                let (lo, hi) = band_hz(b);
                let e: f64 = s.bins(lo, hi).map(|k| s.power[k]).sum();
                row[j] = 2.0 * e / (nfft as f64 * window_power);
            }
        }
        for (row, &b) in energy.iter().zip(&here) {
            for (j, cell) in out[b].iter_mut().enumerate() {
                // The mean over SMOOTH_FRAMES steps, from two before to one after, as far as it runs.
                let lo = j.saturating_sub(SMOOTH_FRAMES / 2);
                let hi = (j + SMOOTH_FRAMES - SMOOTH_FRAMES / 2).min(frames);
                *cell = (row[lo..hi].iter().sum::<f64>() / (hi - lo) as f64).max(0.0);
            }
        }
    }
    out
}

/// Each band's hiss in the recording `x` (from its onset), a mean power as [`cells`] reads it, read
/// from its end backwards: the band's last [`HISS_S`] short of [`HISS_GUARD_S`], in [`HISS_FRAME_S`]
/// frames, and the least-squares line through their levels. Holding still, the stretch is the
/// recording's floor; still falling beyond [`HISS_SURE`] times what a still noise's frames could
/// make of it (a band `w` Hz wide read over `T` s scatters by about `10/ln 10/√(wT)` dB), it is the
/// sound ringing on, and the floor lies under it by what it would fall in [`HISS_AHEAD_S`] more.
/// `None` where the stretch would reach back into the strike ([`TAIL_FROM_MS`]).
#[must_use]
pub fn hiss(x: &[f64], rate: f64) -> Option<Vec<f64>> {
    let frame = (HISS_FRAME_S * rate) as usize;
    let count = (HISS_S / HISS_FRAME_S).round() as usize;
    let stop = x.len().saturating_sub((HISS_GUARD_S * rate) as usize);
    let from = stop.checked_sub(count * frame)?;
    if frame == 0 || (from as f64) < TAIL_FROM_MS / 1000.0 * rate {
        return None;
    }
    // Each band filtered from a second before the stretch, settled by then.
    let lead = from.saturating_sub(rate as usize);
    let tm = (count as f64 - 1.0) / 2.0;
    let spread = (0..count)
        .map(|i| ((i as f64 - tm) * HISS_FRAME_S).powi(2))
        .sum::<f64>()
        .max(1e-12);
    Some(
        (0..BANDS)
            .map(|k| {
                let (lo, hi) = band_hz(k);
                let Some(y) = bands::band(&x[lead..stop], rate, lo, hi.min(0.49 * rate)) else {
                    return 0.0;
                };
                let tail = &y[from - lead..];
                let db: Vec<f64> = tail
                    .chunks_exact(frame)
                    .map(|c| {
                        10.0 * (c.iter().map(|v| v * v).sum::<f64>() / frame as f64 + 1e-30).log10()
                    })
                    .collect();
                let mean_db = db.iter().sum::<f64>() / db.len() as f64;
                let slope = db
                    .iter()
                    .enumerate()
                    .map(|(i, d)| (i as f64 - tm) * HISS_FRAME_S * (d - mean_db))
                    .sum::<f64>()
                    / spread;
                let scatter = 10.0
                    / std::f64::consts::LN_10
                    / ((hi - lo) * HISS_FRAME_S).max(1.0).sqrt()
                    / spread.sqrt();
                let fall = (-slope - HISS_SURE * scatter).max(0.0);
                let mean = tail.iter().map(|v| v * v).sum::<f64>() / tail.len().max(1) as f64;
                mean * 10f64.powf(-fall * HISS_AHEAD_S / 10.0)
            })
            .collect(),
    )
}

/// Each band's noise in the recording `x` (from its onset), a mean power as [`cells`] reads it: the
/// first stretch of [`NOISE_FLAT_S`] from [`NOISE_FROM_S`] on, in [`HISS_FRAME_S`] frames, that
/// holds still ([`NOISE_FLAT_DB_S`]) with its mean under `ceiling_db` (dB of mean power, in the
/// file's own scale); `None` for a band where none does.
#[must_use]
pub fn recording_noise(x: &[f64], rate: f64, ceiling_db: f64) -> Vec<Option<f64>> {
    let frame = (HISS_FRAME_S * rate) as usize;
    let span = (NOISE_FLAT_S / HISS_FRAME_S).round() as usize;
    let first = (NOISE_FROM_S / HISS_FRAME_S).round() as usize;
    let stop = x.len().saturating_sub((HISS_GUARD_S * rate) as usize);
    let n = stop.checked_div(frame).unwrap_or(0);
    if frame == 0 || n < first + span {
        return vec![None; BANDS];
    }
    let tm = (span as f64 - 1.0) / 2.0;
    let spread = (0..span)
        .map(|i| ((i as f64 - tm) * HISS_FRAME_S).powi(2))
        .sum::<f64>()
        .max(1e-12);
    (0..BANDS)
        .map(|k| {
            let (lo, hi) = band_hz(k);
            let y = bands::band(&x[..n * frame], rate, lo, hi.min(0.49 * rate))?;
            let p: Vec<f64> = y
                .chunks_exact(frame)
                .map(|c| c.iter().map(|v| v * v).sum::<f64>() / frame as f64)
                .collect();
            let db: Vec<f64> = p.iter().map(|v| 10.0 * (v + 1e-30).log10()).collect();
            let allow = (HISS_SURE * 10.0
                / std::f64::consts::LN_10
                / ((hi - lo) * HISS_FRAME_S).max(1.0).sqrt()
                / spread.sqrt())
            .min(NOISE_FLAT_DB_S);
            (first..=n - span).find_map(|s| {
                let d = &db[s..s + span];
                let mean_db = d.iter().sum::<f64>() / span as f64;
                let slope = d
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (i as f64 - tm) * HISS_FRAME_S * (v - mean_db))
                    .sum::<f64>()
                    / spread;
                let mean = p[s..s + span].iter().sum::<f64>() / span as f64;
                (slope.abs() <= allow && 10.0 * (mean + 1e-30).log10() <= ceiling_db)
                    .then_some(mean)
            })
        })
        .collect()
}

/// A recording's floor in each band: its hiss read from its end, taken out of its cells, or, where
/// none could be, its quietest tenth while it sounds, only marking the cells near it.
struct Floor {
    power: Vec<f64>,
    from_end: bool,
}

impl Floor {
    fn of(x: &[f64], rate: f64, cells: &[Vec<f64>], ceiling_db: Option<f64>) -> Self {
        match hiss(x, rate) {
            Some(mut power) => {
                if let Some(c) = ceiling_db {
                    for (p, n) in power.iter_mut().zip(recording_noise(x, rate, c)) {
                        *p = p.max(n.unwrap_or(0.0));
                    }
                }
                Self {
                    power,
                    from_end: true,
                }
            }
            None => Self {
                power: cells.iter().map(|row| quantile(row, 0.1)).collect(),
                from_end: false,
            },
        }
    }

    /// The sound's own part of a cell, a power; `None` where the cell holds nothing but the floor.
    fn drum(&self, band: usize, p: f64) -> Option<f64> {
        let h = self.power[band];
        if self.from_end {
            (p >= h * 10f64.powf(HISS_CLEAR_DB / 10.0)).then_some(p - h)
        } else {
            (p > h * 10f64.powf(ABOVE_FLOOR_DB / 10.0)).then_some(p)
        }
    }
}

/// The share of `x`'s energy over [`TAIL_HZ`] from [`TAIL_FROM_MS`] on, to `end`, the floor's power
/// over [`TAIL_HZ`] (a mean power) taken out of both sums.
fn tail_share(x: &[f64], rate: f64, end: usize, floor: f64) -> Option<f64> {
    let y = bands::band(&x[..end], rate, TAIL_HZ, 0.45 * rate)?;
    let from = (TAIL_FROM_MS / 1000.0 * rate) as usize;
    let all = (y.iter().map(|v| v * v).sum::<f64>() - floor * end as f64).max(0.0);
    let late =
        (y[from..].iter().map(|v| v * v).sum::<f64>() - floor * (end - from) as f64).max(0.0);
    (all > 0.0).then(|| late / all)
}

/// A floor's power over [`TAIL_HZ`]: the bands centred there and up.
fn floor_over_tail(floor: &Floor) -> f64 {
    if !floor.from_end {
        return 0.0;
    }
    (0..BANDS)
        .filter(|&k| {
            let (lo, hi) = band_hz(k);
            (lo * hi).sqrt() >= TAIL_HZ
        })
        .map(|k| floor.power[k])
        .sum()
}

/// The `q` quantile of `v` (0–1), interpolated between ranks as `numpy.percentile` does.
fn quantile(v: &[f64], q: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let at = q * (s.len() - 1) as f64;
    let (i, f) = (at.floor() as usize, at.fract());
    if i + 1 < s.len() {
        s[i] + f * (s[i + 1] - s[i])
    } else {
        s[i]
    }
}

/// How loud a recording's noise can be, each file in its own scale: dB of a band's mean power, under
/// which a band holding still after the strike is taken for noise ([`recording_noise`]). `None`
/// reads the hiss from the end alone.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NoiseCeiling {
    pub reference_db: Option<f64>,
    pub take_db: Option<f64>,
}

/// Screens the model `candidate` against `reference`, the reference's hiss taken out and the
/// candidate's kept: `None` where either is silent or their rates differ.
#[must_use]
pub fn screen(reference: &Sound, candidate: &Sound, noise: NoiseCeiling) -> Option<Screen> {
    screen_with(reference, candidate, None, noise)
}

/// Screens a second take of the reference against it, both recordings' hiss taken out: the
/// yardstick [`verdict`] reads a model's screen against.
#[must_use]
pub fn screen_take(reference: &Sound, take: &Sound, noise: NoiseCeiling) -> Option<Screen> {
    screen_with(reference, take, Some(noise.take_db), noise)
}

/// `candidate_noise`: `None` for a model (its hiss kept), else a recording's noise ceiling.
fn screen_with(
    reference: &Sound,
    candidate: &Sound,
    candidate_noise: Option<Option<f64>>,
    noise: NoiseCeiling,
) -> Option<Screen> {
    if reference.rate != candidate.rate {
        return None;
    }
    let rate = f64::from(reference.rate);
    let (r32, c32) = (from_onset(reference), from_onset(candidate));
    if r32.is_empty() || c32.is_empty() {
        return None;
    }
    let (body_r, body_c) = (
        prep::body_rms(&r32, reference.rate),
        prep::body_rms(&c32, candidate.rate),
    );
    if body_c <= 0.0 || body_r <= 0.0 {
        return None;
    }
    let gain = f64::from(body_r / body_c);
    let r: Vec<f64> = r32.iter().map(|&v| f64::from(v)).collect();
    let c: Vec<f64> = c32.iter().map(|&v| f64::from(v) * gain).collect();

    let length_ms = sounding_ms(&r, rate)
        .max(sounding_ms(&c, rate))
        .min(LONGEST_MS);
    let frames = (length_ms / HOP_MS) as usize;
    let n = (length_ms / 1000.0 * rate) as usize + reference.rate as usize / 10;
    let fit = |x: &[f64]| {
        let mut y = x[..x.len().min(n)].to_vec();
        y.resize(n, 0.0);
        y
    };
    let pa = cells(&fit(&c), rate, frames);
    let pb = cells(&fit(&r), rate, frames);
    let floor_b = Floor::of(&r, rate, &pb, noise.reference_db);
    // The candidate was scaled by `gain`: its ceiling moves with it.
    let gain_db = 20.0 * gain.log10();
    let floor_a =
        candidate_noise.map(|ceiling| Floor::of(&c, rate, &pa, ceiling.map(|v| v + gain_db)));

    // Each cell as loud as it is heard: its band's A-weighting at its centre added to both sounds,
    // so a difference stays what it is and only how much a cell counts, and whether it is heard,
    // follows the ear (a room's rumble under 100 Hz no longer outweighs a strike's highs).
    let weight: Vec<f64> = (0..BANDS)
        .map(|k| {
            let (lo, hi) = band_hz(k);
            20.0 * a_weight((lo * hi).sqrt()).max(1e-15).log10()
        })
        .collect();
    let db = |p: f64, k: usize| 10.0 * (p + 1e-30).log10() + weight[k];
    // Each cell as (the sound's own part, dB; or the floor's level where it holds nothing else).
    let part = |floor: Option<&Floor>, k: usize, p: f64| match floor {
        Some(f) => f
            .drum(k, p)
            .map_or_else(|| Err(db(f.power[k], k)), |q| Ok(db(q, k))),
        None => Ok(db(p, k)),
    };
    let a: Vec<Vec<Result<f64, f64>>> = (0..BANDS)
        .map(|k| {
            pa[k]
                .iter()
                .map(|&p| part(floor_a.as_ref(), k, p))
                .collect()
        })
        .collect();
    let b: Vec<Vec<Result<f64, f64>>> = (0..BANDS)
        .map(|k| pb[k].iter().map(|&p| part(Some(&floor_b), k, p)).collect())
        .collect();

    let top = b
        .iter()
        .flatten()
        .filter_map(|v| v.ok())
        .fold(f64::MIN, f64::max);
    let low = top - RANGE_DB - 10.0;
    let mut d = vec![vec![0.0; frames]; BANDS];
    let mut heard = vec![vec![false; frames]; BANDS];
    let mut w = vec![vec![0.0; frames]; BANDS];
    for k in 0..BANDS {
        for j in 0..frames {
            // Candidate minus reference, and how loud the cell is heard. Where the reference holds
            // only its floor, the candidate is off only for standing over it; where a recorded
            // candidate does, it lacks at most what stands over its floor.
            let (diff, loud) = match (a[k][j], b[k][j]) {
                (Ok(x), Ok(y)) => (x.max(low) - y.max(low), x.max(y)),
                (Ok(x), Err(floor)) => ((x - floor).max(0.0), x),
                (Err(floor), Ok(y)) => ((floor.max(low) - y.max(low)).min(0.0), y),
                (Err(_), Err(_)) => (0.0, f64::MIN),
            };
            heard[k][j] = loud > top - RANGE_DB;
            d[k][j] = diff;
            w[k][j] = if heard[k][j] {
                10f64.powf(loud / 20.0)
            } else {
                0.0
            };
        }
    }
    let total: f64 = w.iter().flatten().sum();
    if total > 0.0 {
        for v in w.iter_mut().flatten() {
            *v /= total;
        }
    }
    let mut off_cells = vec![vec![0.0; frames]; BANDS];
    for k in 0..BANDS {
        for j in 0..frames {
            if heard[k][j] && d[k][j].abs() > OFF_DB {
                off_cells[k][j] = w[k][j].copysign(d[k][j]);
            }
        }
    }
    let off_share = off_cells.iter().flatten().map(|v| v.abs()).sum();
    let patch = largest_patch(&d, &heard, &w);

    let end = r.len().min(c.len());
    let tail_db = if end >= (2.0 * TAIL_FROM_MS / 1000.0 * rate) as usize {
        let floor_c = floor_a.as_ref().map_or(0.0, floor_over_tail);
        match (
            tail_share(&c, rate, end, floor_c),
            tail_share(&r, rate, end, floor_over_tail(&floor_b)),
        ) {
            (Some(tc), Some(tr)) => Some(10.0 * (tc.max(1e-12) / tr.max(1e-12)).log10()),
            _ => None,
        }
    } else {
        None
    };
    Some(Screen {
        off_share,
        patch,
        tail_db,
        length_ms,
        hiss_from_end: floor_b.from_end,
        off_cells,
    })
}

/// The largest connected patch of off cells, too loud or lacking, four neighbours each.
fn largest_patch(d: &[Vec<f64>], heard: &[Vec<bool>], w: &[Vec<f64>]) -> Option<Patch> {
    let frames = d.first().map_or(0, Vec::len);
    let mut best: Option<(usize, Patch)> = None;
    for too_loud in [true, false] {
        let off = |k: usize, j: usize| {
            heard[k][j]
                && if too_loud {
                    d[k][j] > OFF_DB
                } else {
                    -d[k][j] > OFF_DB
                }
        };
        let mut seen = vec![vec![false; frames]; BANDS];
        for k0 in 0..BANDS {
            for j0 in 0..frames {
                if seen[k0][j0] || !off(k0, j0) {
                    continue;
                }
                seen[k0][j0] = true;
                let mut stack = vec![(k0, j0)];
                let (mut count, mut sum_db, mut loudness) = (0usize, 0.0, 0.0);
                let (mut k_lo, mut k_hi, mut j_lo, mut j_hi) = (k0, k0, j0, j0);
                while let Some((k, j)) = stack.pop() {
                    count += 1;
                    sum_db += d[k][j];
                    loudness += w[k][j];
                    (k_lo, k_hi, j_lo, j_hi) = (k_lo.min(k), k_hi.max(k), j_lo.min(j), j_hi.max(j));
                    let next = [
                        (k.wrapping_sub(1), j),
                        (k + 1, j),
                        (k, j.wrapping_sub(1)),
                        (k, j + 1),
                    ];
                    for (nk, nj) in next {
                        if nk < BANDS && nj < frames && !seen[nk][nj] && off(nk, nj) {
                            seen[nk][nj] = true;
                            stack.push((nk, nj));
                        }
                    }
                }
                if best.as_ref().is_none_or(|(n, _)| count > *n) {
                    best = Some((
                        count,
                        Patch {
                            too_loud,
                            mean_db: sum_db / count as f64,
                            window_ms: (j_lo as f64 * HOP_MS, (j_hi + 1) as f64 * HOP_MS),
                            band_hz: (band_hz(k_lo).0, band_hz(k_hi).1),
                            loudness,
                        },
                    ));
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

/// Why the candidate is obviously off, empty where it is not: much of it [`OFF_DB`] away where the
/// takes are not, one patch carrying much of its loudness, or a tail far under the reference's past
/// the takes' own difference. `takes` is a second take screened against the same reference
/// ([`screen_take`]).
#[must_use]
pub fn verdict(model: &Screen, takes: Option<&Screen>) -> Vec<String> {
    let mut why = Vec::new();
    let ts = takes.map_or(0.0, |t| t.off_share);
    if model.off_share > SHARE_OFF && model.off_share > 2.0 * ts + 0.05 {
        why.push(format!(
            "{:.0}% of it heard {OFF_DB:.0} dB off (the takes {:.0}%)",
            model.off_share * 100.0,
            ts * 100.0
        ));
    }
    if let Some(p) = model.patch.as_ref().filter(|p| p.loudness > PATCH_OFF) {
        why.push(format!(
            "{} by {:.0} dB over {:.0}-{:.0} ms, {:.0}-{:.0} Hz ({:.0}% of its loudness)",
            if p.too_loud { "too loud" } else { "missing" },
            p.mean_db.abs(),
            p.window_ms.0,
            p.window_ms.1,
            p.band_hz.0,
            p.band_hz.1,
            p.loudness * 100.0
        ));
    }
    if let Some(tail) = model.tail_db {
        let yard = takes.and_then(|t| t.tail_db).unwrap_or(0.0);
        if tail < -(yard.abs() + TAIL_OFF_DB) {
            why.push(format!(
                "its tail {tail:+.0} dB under the reference's (two takes {yard:+.0}): it stops short"
            ));
        }
    }
    why
}
