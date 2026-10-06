//! Differences no finding explains: both sounds on an auditory-style spectrogram, compared patch by
//! patch, independently of every named reading. A patch that differs audibly where no audible finding
//! covers it is reported, so a gap in the analysers shows instead of hiding (the plan's §3).
//!
//! **Technique:** a short-time power spectrum pooled into bands equally spaced on the ERB-number scale
//! (Glasberg & Moore 1990: `E(f) = 21.4·log₁₀(1 + 0.00437 f)`), each sound in dB against its own
//! loudest cell, then patches — time segments × groups of ERB bands — compared by their mean level
//! difference over the cells either sound makes audible. A simpler relative of the patch comparisons
//! in ViSQOL and Zimtohrli (`research:listening/comparing-sounds.md`), chosen because it names where a
//! difference is, which a single similarity score cannot.

use crate::compare::Finding;
use crate::repr::spectrum;
use crate::sound::Sound;

/// Time segments, ms from each sound's onset.
pub const SEGMENTS_MS: [(f64, f64); 6] = [
    (0.0, 20.0),
    (20.0, 60.0),
    (60.0, 150.0),
    (150.0, 400.0),
    (400.0, 1000.0),
    (1000.0, 2000.0),
];
/// Band groups, Hz (each several ERB bands).
pub const GROUPS_HZ: [(f64, f64); 7] = [
    (50.0, 150.0),
    (150.0, 400.0),
    (400.0, 1000.0),
    (1000.0, 2500.0),
    (2500.0, 5000.0),
    (5000.0, 10_000.0),
    (10_000.0, 16_000.0),
];
/// A patch differing by this much on average is a difference (**chosen**: a level step about twice
/// the level threshold, on a coarse and noisy map).
pub const PATCH_DB: f64 = 3.0;
/// Cells below this against their sound's loudest do not count (**chosen**).
pub const AUDIBLE_DB: f64 = -60.0;

/// A region where the sounds differ.
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub window_ms: (f64, f64),
    pub band_hz: (f64, f64),
    /// Candidate minus reference, dB, averaged over the patch's audible cells.
    pub difference_db: f64,
    /// Where inside the window the frames themselves differ by [`PATCH_DB`], ms: the span a finding
    /// must overlap to explain the region (a reading at 500 ms does not explain a burst at 600).
    pub where_ms: (f64, f64),
    /// Some audible finding covers this region.
    pub explained: bool,
}

/// ERB-number of a frequency.
pub(crate) fn erb_number(hz: f64) -> f64 {
    21.4 * (1.0 + 0.00437 * hz).log10()
}

/// The frequency of an ERB-number: [`erb_number`]'s inverse.
pub(crate) fn erb_hz(erb: f64) -> f64 {
    (10f64.powf(erb / 21.4) - 1.0) / 0.00437
}

/// Each frame's centre, ms from the onset, and its power in each group.
pub(crate) type Frames = Vec<(f64, Vec<f64>)>;

/// The frames of `sound` from its percussive onset, in [`GROUPS_HZ`], and its loudest cell.
fn frames(sound: &Sound) -> (Frames, f64) {
    let onset = crate::prep::onset(&sound.samples).unwrap_or(0);
    frames_in(sound, onset, &GROUPS_HZ)
}

/// The frames of `sound` from `onset`, each band's power pooled with equal weight per ERB, and its
/// loudest cell: [`regions`]' map, and the spectrogram `curves` draws.
pub(crate) fn frames_in(sound: &Sound, onset: usize, bands: &[(f64, f64)]) -> (Frames, f64) {
    let rate = f64::from(sound.rate);
    let x: Vec<f64> = sound.samples.iter().map(|&v| f64::from(v)).collect();
    let n = frame_len(rate);
    let hop = n / 4;
    // Frames' band powers: for each frame, the power in each group (pooled from ERB-spaced bands).
    let mut frames: Vec<(f64, Vec<f64>)> = Vec::new();
    let mut start = onset;
    while start + n <= x.len() {
        if let Some(s) = spectrum::power_spectrum(&x[start..start + n], rate, n) {
            let powers: Vec<f64> = bands
                .iter()
                .map(|&(lo, hi)| {
                    let hi = hi.min(0.49 * rate);
                    if hi <= lo {
                        return 0.0;
                    }
                    // Equal weight per ERB within the group: each bin weighted by its ERB-number width.
                    s.bins(lo, hi)
                        .map(|k| {
                            let f = s.hz(k);
                            let w = erb_number(f + 0.5 * s.bin_hz)
                                - erb_number((f - 0.5 * s.bin_hz).max(0.0));
                            s.power[k] * w / s.bin_hz.max(1e-12)
                        })
                        .sum()
                })
                .collect();
            let centre_ms = (start - onset) as f64 / rate * 1000.0 + n as f64 / rate * 500.0;
            frames.push((centre_ms, powers));
        }
        start += hop;
    }
    let loudest = frames
        .iter()
        .flat_map(|f| f.1.iter().copied())
        .fold(0.0f64, f64::max);
    (frames, loudest)
}

/// A frame's length in samples at `rate`: about 10.7 ms, a power of two; frames hop a quarter of it.
pub(crate) fn frame_len(rate: f64) -> usize {
    ((0.0107 * rate) as usize).next_power_of_two()
}

/// Level in dB (against the sound's loudest) of every patch, `[segment][group]`; `None` where a patch
/// is outside the sound or silent.
fn patches(frames: &Frames, loudest: f64) -> Vec<Vec<Option<f64>>> {
    SEGMENTS_MS
        .iter()
        .map(|&(a, b)| {
            (0..GROUPS_HZ.len())
                .map(|g| {
                    let cells: Vec<f64> = frames
                        .iter()
                        .filter(|(t, _)| *t >= a && *t < b)
                        .map(|(_, p)| p[g])
                        .collect();
                    if cells.is_empty() || loudest <= 0.0 {
                        return None;
                    }
                    let mean = cells.iter().sum::<f64>() / cells.len() as f64;
                    (mean > 0.0).then(|| 10.0 * (mean / loudest).log10())
                })
                .collect()
        })
        .collect()
}

/// Whether an audible finding covers a region: its window overlaps the region's, and its band does
/// (a finding with no band covers every band; with no window, every time).
fn covered(region: &Region, findings: &[&Finding]) -> bool {
    findings.iter().any(|f| {
        let time = f
            .window_ms
            .is_none_or(|(a, b)| a <= region.where_ms.1 && b.max(a + 1.0) >= region.where_ms.0);
        let band = f
            .band_hz
            .is_none_or(|(lo, hi)| lo < region.band_hz.1 && hi > region.band_hz.0);
        time && band
    })
}

/// The regions where two sounds differ by [`PATCH_DB`] or more, each marked explained or not by the
/// audible findings.
#[must_use]
pub fn regions(reference: &Sound, candidate: &Sound, findings: &[&Finding]) -> Vec<Region> {
    let (fa, la) = frames(reference);
    let (fb, lb) = frames(candidate);
    let a = patches(&fa, la);
    let b = patches(&fb, lb);
    let cell = |p: f64, loudest: f64| 10.0 * (p.max(1e-30) / loudest.max(1e-30)).log10();
    let mut out = Vec::new();
    for (s, &(t0, t1)) in SEGMENTS_MS.iter().enumerate() {
        for (g, &(lo, hi)) in GROUPS_HZ.iter().enumerate() {
            let (Some(ra), Some(cb)) = (a[s][g], b[s][g]) else {
                continue;
            };
            if ra < AUDIBLE_DB && cb < AUDIBLE_DB {
                continue;
            }
            let d = cb - ra;
            if d.abs() >= PATCH_DB {
                // The frames inside the patch that differ by the patch threshold themselves, either
                // sound's cell audible; the whole window when none does alone.
                let differing: Vec<f64> = fa
                    .iter()
                    .zip(&fb)
                    .filter(|(x, _)| x.0 >= t0 && x.0 < t1)
                    .filter_map(|(x, y)| {
                        let (ca, cb) = (cell(x.1[g], la), cell(y.1[g], lb));
                        ((ca >= AUDIBLE_DB || cb >= AUDIBLE_DB) && (cb - ca).abs() >= PATCH_DB)
                            .then_some(x.0)
                    })
                    .collect();
                let where_ms = match (differing.first(), differing.last()) {
                    (Some(&first), Some(&last)) => (first, last),
                    _ => (t0, t1),
                };
                let mut region = Region {
                    window_ms: (t0, t1),
                    band_hz: (lo, hi),
                    difference_db: d,
                    where_ms,
                    explained: false,
                };
                region.explained = covered(&region, findings);
                out.push(region);
            }
        }
    }
    out
}
