//! Cutting a run of struck notes into single notes: a chromatic scale played one note at a time, as
//! the Iowa MIS mallet recordings are, with each note left to ring before the next.
//!
//! The run's name says which notes it holds and in what order (`C4B4` is C4 up to B4), and the cut
//! uses both. A strike is a rise of the whole spectrum at once, so the **candidates** are the peaks of
//! the **spectral flux** — each frame's summed rise in log magnitude over every bin, a 1024-point Hann
//! frame every 256 samples (Bello et al., *A tutorial on onset detection in music signals*, IEEE TSAP
//! 2005, the flux family) — no two within 50 ms. Each note then takes the candidate where **its own
//! band** rises most: the level at the note's frequency just after the candidate against just before,
//! by complex demodulation over a window long enough to part neighbouring semitones. The notes take
//! their candidates in scale order, no two closer than a fair share of the run apart, the sum of their
//! rises largest (a dynamic programme). Each note is cut from 20 ms before its strike to 20 ms before
//! the next.
//!
//! Two simpler cuts were tried on the Iowa runs and failed. A level-rise detector: notes ringing
//! together beat, their sum dips close to silence and climbs back as sharply as a strike, and a
//! vibraphone's tremolo does the same more slowly. The strongest flux peaks alone: a pianissimo strike
//! (the vibraphone's E4 in `pp.C4B4`) is quieter in the flux than handling noise, and a low thump near
//! 50 Hz (in the marimba's `pp.C5B5`) outranks a quiet note.

use crate::sound::Sound;

/// The spectral flux's frame and hop, samples.
const FRAME: usize = 1024;
const STEP: usize = 256;
/// Log compression of each bin's magnitude before its rise is taken: `ln(1 + C·|X|)`, the magnitude
/// on the run normalised to its loudest frame's peak bin (**chosen**: quiet bins still count, a
/// silent one does not). A note band's level is compressed the same way, on its own loudest window.
const COMPRESSION: f64 = 1000.0;
/// No two candidate strikes closer than this, s: a strike's flux has one peak inside it.
const CANDIDATE_GAP_S: f64 = 0.05;
/// No two notes' strikes closer than this share of the run's length per note.
const SPACING_SHARE: f64 = 0.3;
/// A note band's Hann window, before and after a candidate: this many cycles of the semitone
/// difference, so the neighbouring semitones fall near its fourth null, about 40 dB down; held
/// between the bounds below, s.
const BAND_WINDOW_CYCLES: f64 = 4.0;
const BAND_WINDOW_MIN_S: f64 = 0.02;
const BAND_WINDOW_MAX_S: f64 = 1.2;
/// A strike is placed at the largest flux near the band's choice whose band rise is at least this
/// share of the choice's: a note's rise is flat for a stretch around its strike, and a tremolo dipping
/// just after the strike tilts it early.
const PLACE_SHARE: f64 = 0.5;
/// Each note starts this long before its attack, s.
const LEAD_S: f64 = 0.02;
/// A cut whose strongest peak near its name sits further than this from it is named or cut wrong:
/// half a semitone. Tuning to A = 442–443 Hz reads 8–12 c, and a missed strike reads about ±100.
pub const NAME_TOLERANCE_CENTS: f64 = 50.0;

/// The names of the notes a run holds, from a range such as `C4B4`, `C8E8`, `F4B4` or a single `C7`;
/// `None` when the name holds no range.
#[must_use]
pub fn range_names(name: &str) -> Option<Vec<String>> {
    const NAMES: [&str; 12] = [
        "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
    ];
    let midi = |s: &str| -> Option<(i32, usize)> {
        // A note name and octave at the start of `s`, and how many bytes it took.
        for len in [2, 1] {
            let (pc, rest) = (s.get(..len)?, s.get(len..)?);
            if let Some(i) = NAMES.iter().position(|n| *n == pc) {
                let digits: String = rest
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .take(1)
                    .collect();
                let octave: i32 = digits.parse().ok()?;
                return Some((12 * (octave + 1) + i as i32, len + 1));
            }
        }
        None
    };
    // The range is the last dot-separated field that parses.
    for field in name.split('.').rev() {
        let Some((a, used)) = midi(field) else {
            continue;
        };
        let b = if used < field.len() {
            midi(&field[used..]).map(|(b, _)| b)
        } else {
            Some(a)
        };
        let Some(b) = b.filter(|b| *b >= a) else {
            continue;
        };
        return Some(
            (a..=b)
                .map(|m| format!("{}{}", NAMES[(m % 12) as usize], m / 12 - 1))
                .collect(),
        );
    }
    None
}

/// The spectral flux of `x`, one value per hop: frame `f`'s rise is placed at sample
/// `f·STEP + FRAME/2`, where the samples it added begin. `None` for a run shorter than a frame, or
/// silent.
fn flux(x: &[f32]) -> Option<Vec<f64>> {
    if x.len() < FRAME {
        return None;
    }
    let window: Vec<f64> = (0..FRAME)
        .map(|n| 0.5 - 0.5 * (std::f64::consts::TAU * n as f64 / FRAME as f64).cos())
        .collect();
    let frames = (x.len() - FRAME) / STEP + 1;
    let mut spectra: Vec<Vec<f64>> = Vec::with_capacity(frames);
    let mut top = 0.0f64;
    for f in 0..frames {
        let mut re: Vec<f64> = (0..FRAME)
            .map(|n| f64::from(x[f * STEP + n]) * window[n])
            .collect();
        let mut im = vec![0.0; FRAME];
        mxm_measure::spectrum::fft(&mut re, &mut im)?;
        let mag: Vec<f64> = (0..=FRAME / 2).map(|k| re[k].hypot(im[k])).collect();
        top = mag.iter().copied().fold(top, f64::max);
        spectra.push(mag);
    }
    if top <= 0.0 {
        return None;
    }
    let scale = COMPRESSION / top;
    let mut previous: Vec<f64> = vec![0.0; FRAME / 2 + 1];
    let mut flux = vec![0.0; frames];
    for (f, mag) in spectra.iter().enumerate() {
        let now: Vec<f64> = mag.iter().map(|m| (1.0 + scale * m).ln()).collect();
        if f > 0 {
            flux[f] = now
                .iter()
                .zip(&previous)
                .map(|(a, b)| (a - b).max(0.0))
                .sum();
        }
        previous = now;
    }
    Some(flux)
}

/// How much the level at `hz` rises at each hop: the compressed level over the `span` hops after
/// against the `span` hops before, the run silent before its first sample and after its last (an
/// Iowa file can start 30 ms before its strike). The level is a Hann-windowed demodulation
/// ([`crate::repr::demod`]).
fn band_rise(x: &[f32], rate: u32, hz: f64, frames: usize, span: usize) -> Vec<f64> {
    let mut rise = vec![0.0; frames];
    // Window p runs from FRAME/2 + (p − span)·STEP for span hops: window f + span starts at hop f,
    // window f ends there.
    let origin = (FRAME / 2) as i64 - (span * STEP) as i64;
    let Some(track) = crate::repr::demod::Tracker::new(
        x,
        f64::from(rate),
        hz,
        origin,
        STEP,
        span,
        frames + 2 * span,
    ) else {
        return rise;
    };
    let level = |p: usize| track.amplitude(p).unwrap_or(0.0);
    let top = (0..=frames + span).map(level).fold(0.0, f64::max);
    if top <= 0.0 {
        return rise;
    }
    let scale = COMPRESSION / top;
    for (f, r) in rise.iter_mut().enumerate() {
        *r = (1.0 + scale * level(f + span)).ln() - (1.0 + scale * level(f)).ln();
    }
    rise
}

/// The sample index of each note's strike in `x`, in time order, one per frequency in `hz` (the run's
/// notes in the order played); empty when the run is too short or silent, or holds fewer candidate
/// strikes than notes.
#[must_use]
pub fn attacks(x: &[f32], rate: u32, hz: &[f64]) -> Vec<usize> {
    let count = hz.len();
    let Some(flux) = flux(x).filter(|_| count > 0) else {
        return Vec::new();
    };
    let frames = flux.len();
    let per_s = f64::from(rate) / STEP as f64;
    // Candidates: the flux's largest value within the gap either side, the first of equals.
    let gap = ((CANDIDATE_GAP_S * per_s) as usize).max(1);
    let candidates: Vec<usize> = (0..frames)
        .filter(|&f| {
            let (lo, hi) = (f.saturating_sub(gap), (f + gap).min(frames - 1));
            flux[f] > 0.0 && (lo..=hi).all(|g| flux[g] < flux[f] || (flux[g] == flux[f] && g >= f))
        })
        .collect();
    if candidates.len() < count {
        return Vec::new();
    }
    let spacing = ((SPACING_SHARE * frames as f64 / count as f64) as usize).max(1);
    // spans[i]: note i's band window, hops; score[i][j]: its band rise at candidate j.
    let spans: Vec<usize> = hz
        .iter()
        .map(|&f0| {
            let semitone = f0 * (2f64.powf(1.0 / 12.0) - 1.0);
            let seconds =
                (BAND_WINDOW_CYCLES / semitone).clamp(BAND_WINDOW_MIN_S, BAND_WINDOW_MAX_S);
            ((seconds * per_s).round() as usize).max(1)
        })
        .collect();
    let score: Vec<Vec<f64>> = hz
        .iter()
        .zip(&spans)
        .map(|(&f0, &span)| {
            let rise = band_rise(x, rate, f0, frames, span);
            candidates.iter().map(|&c| rise[c].max(0.0)).collect()
        })
        .collect();
    let Some(chosen) = order(&candidates, &score, spacing) else {
        return Vec::new();
    };
    let m = candidates.len();
    chosen
        .iter()
        .enumerate()
        .map(|(i, &j)| {
            // The band chose the strike; the flux places it: the largest flux within half the note's
            // window whose rise is still at least half the chosen one.
            let (at, rise) = (candidates[j], score[i][j]);
            let place = (0..m)
                .filter(|&k| {
                    candidates[k].abs_diff(at) <= spans[i] / 2 && score[i][k] >= PLACE_SHARE * rise
                })
                .max_by(|&a, &b| flux[candidates[a]].total_cmp(&flux[candidates[b]]))
                .unwrap_or(j);
            candidates[place] * STEP + FRAME / 2
        })
        .collect()
}

/// The candidate each note takes, in scale order: the largest sum of the notes' scores with no two
/// notes closer than `spacing` hops (a dynamic programme); `None` when the notes cannot all be placed.
fn order(candidates: &[usize], score: &[Vec<f64>], spacing: usize) -> Option<Vec<usize>> {
    let count = score.len();
    let m = candidates.len();
    if count == 0 || m < count {
        return None;
    }
    // best[i][j]: the largest sum for notes 0..=i with note i at candidate j; from[i][j]: note i − 1's
    // candidate behind it.
    let mut best = vec![vec![f64::NEG_INFINITY; m]; count];
    let mut from = vec![vec![usize::MAX; m]; count];
    best[0].clone_from(&score[0]);
    for i in 1..count {
        // The best of note i − 1 over the candidates at least `spacing` before j, kept as j moves on.
        let (mut k, mut run_best, mut run_at) = (0, f64::NEG_INFINITY, usize::MAX);
        for j in 0..m {
            while k < m && candidates[k] + spacing <= candidates[j] {
                if best[i - 1][k] > run_best {
                    (run_best, run_at) = (best[i - 1][k], k);
                }
                k += 1;
            }
            if run_best.is_finite() {
                best[i][j] = run_best + score[i][j];
                from[i][j] = run_at;
            }
        }
    }
    let mut j = (0..m)
        .filter(|&j| best[count - 1][j].is_finite())
        .max_by(|&a, &b| best[count - 1][a].total_cmp(&best[count - 1][b]))?;
    let mut chosen = vec![0; count];
    for i in (0..count).rev() {
        chosen[i] = j;
        j = from[i][j];
    }
    Some(chosen)
}

/// Harmonics whose bands a held note's start is read on.
pub const HELD_HARMONICS: usize = 4;

/// The sample index of each held note's start in `x`, as [`attacks`] but for notes a bow or a breath
/// holds: a bow starts softly and a held note's strongest line jumps between harmonics, so the flux
/// gives no candidates to trust. Every 50 ms is a candidate, a note's score is the rise of its first
/// [`HELD_HARMONICS`] harmonics' bands together, and the start is the candidate chosen.
#[must_use]
pub fn attacks_held(x: &[f32], rate: u32, hz: &[f64]) -> Vec<usize> {
    let count = hz.len();
    if count == 0 || x.len() < FRAME {
        return Vec::new();
    }
    let frames = (x.len() - FRAME) / STEP + 1;
    let per_s = f64::from(rate) / STEP as f64;
    let gap = ((CANDIDATE_GAP_S * per_s) as usize).max(1);
    let candidates: Vec<usize> = (0..frames).step_by(gap).collect();
    let spacing = ((SPACING_SHARE * frames as f64 / count as f64) as usize).max(1);
    let nyquist = 0.45 * f64::from(rate);
    let score: Vec<Vec<f64>> = hz
        .iter()
        .map(|&f0| {
            let semitone = f0 * (2f64.powf(1.0 / 12.0) - 1.0);
            let seconds =
                (BAND_WINDOW_CYCLES / semitone).clamp(BAND_WINDOW_MIN_S, BAND_WINDOW_MAX_S);
            let span = ((seconds * per_s).round() as usize).max(1);
            let rises: Vec<Vec<f64>> = (1..=HELD_HARMONICS)
                .map(|k| k as f64 * f0)
                .filter(|&hz| hz < nyquist)
                .map(|hz| band_rise(x, rate, hz, frames, span))
                .collect();
            candidates
                .iter()
                .map(|&c| rises.iter().map(|r| r[c].max(0.0)).sum())
                .collect()
        })
        .collect();
    let Some(chosen) = order(&candidates, &score, spacing) else {
        return Vec::new();
    };
    chosen
        .iter()
        .map(|&j| candidates[j] * STEP + FRAME / 2)
        .collect()
}

/// A run cut into its notes, named from its range; `Err` when the name holds no range or fewer strikes
/// are found than it names.
///
/// # Errors
/// As above, with the reason.
pub fn notes(run: &Sound) -> Result<Vec<Sound>, String> {
    cut(run, false)
}

/// A run of held notes (bowed, blown) cut as [`notes`] cuts a struck run, their starts found by
/// [`attacks_held`].
///
/// # Errors
/// As [`notes`].
pub fn notes_held(run: &Sound) -> Result<Vec<Sound>, String> {
    cut(run, true)
}

fn cut(run: &Sound, held: bool) -> Result<Vec<Sound>, String> {
    let names =
        range_names(&run.name).ok_or_else(|| format!("{}: no note range in the name", run.name))?;
    let hz: Vec<f64> = names.iter().filter_map(|n| note_hz(n)).collect();
    let at = if held {
        attacks_held(&run.samples, run.rate, &hz)
    } else {
        attacks(&run.samples, run.rate, &hz)
    };
    if at.len() < names.len() {
        return Err(format!(
            "{}: {} strikes found where the name holds {}",
            run.name,
            at.len(),
            names.len()
        ));
    }
    let lead = (LEAD_S * f64::from(run.rate)) as usize;
    Ok(at
        .iter()
        .enumerate()
        .map(|(i, &a)| {
            let from = a.saturating_sub(lead);
            let to = at
                .get(i + 1)
                .map_or(run.samples.len(), |&b| b.saturating_sub(lead));
            Sound::new(
                format!("{} {}", run.name, names[i]),
                run.rate,
                run.samples[from..to.max(from)].to_vec(),
            )
        })
        .collect())
}

/// A note name's equal-tempered frequency, A4 = 440 Hz (`C4`, `Db6`, …); `None` for anything else.
#[must_use]
pub fn note_hz(name: &str) -> Option<f64> {
    const NAMES: [&str; 12] = [
        "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
    ];
    let split = name.find(|c: char| c.is_ascii_digit() || c == '-')?;
    let pc = NAMES.iter().position(|n| *n == &name[..split])?;
    let octave: i32 = name[split..].parse().ok()?;
    let midi = 12 * (octave + 1) + pc as i32;
    Some(440.0 * 2f64.powf(f64::from(midi - 69) / 12.0))
}

/// How far, in cents, the strongest spectral peak within three semitones of `expected_hz` sits from it
/// over the note's first second: a cut that named its note right reads within a few tens of cents; a
/// missed or doubled strike shifts every later name by a semitone and reads about ±100.
#[must_use]
pub fn check_cents(note: &Sound, expected_hz: f64) -> Option<f64> {
    let rate = f64::from(note.rate);
    let n = (rate as usize).min(note.samples.len());
    if n < 1024 || expected_hz <= 0.0 {
        return None;
    }
    let size = (4 * n).next_power_of_two().max(1 << 16);
    let mut re = vec![0.0; size];
    let mut im = vec![0.0; size];
    for (i, v) in note.samples[..n].iter().enumerate() {
        re[i] = f64::from(*v) * (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos());
    }
    mxm_measure::spectrum::fft(&mut re, &mut im)?;
    let bin = rate / size as f64;
    let lo = ((expected_hz * 2f64.powf(-3.0 / 12.0)) / bin) as usize;
    let hi = (((expected_hz * 2f64.powf(3.0 / 12.0)) / bin) as usize).min(size / 2 - 1);
    let k = (lo.max(1)..=hi).max_by(|&a, &b| re[a].hypot(im[a]).total_cmp(&re[b].hypot(im[b])))?;
    Some(1200.0 * (k as f64 * bin / expected_hz).log2())
}
