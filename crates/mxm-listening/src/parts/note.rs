//! A pitched note's partials (L5a; the plan's §2 item 11): each partial's frequency, ratio to the
//! fundamental, level at the strike and decay, and the laws they follow together — decay against
//! frequency, AAS's *Material* (`T60 ∝ (f/f₁)^M`), and level against frequency, its *Tone* (dB an
//! octave) — with the ideal object the partials sit nearest ([`crate::objects`]).
//!
//! Sources: the laws and the reason to read decay across frequency rather than as one time are
//! `research:listening/percussion-perception.md` §4.1 and §9 (damping against frequency is the main
//! cue to material) and `research:physical-modelling/physical-modelling-synthesis.md` §6.3 (constant
//! Q is M = −1; linear and quadratic laws; high partials outlasting low ones, M > 0). The partials
//! are the mode analysis's believed modes (`repr::modes`) on bands reaching 20 kHz; each is then
//! followed over the whole note by Hann-windowed demodulation (`repr::demod`), its window long enough
//! that its neighbours fall about 40 dB down, and its decay is a line fitted to its level in dB from
//! the strike until it nears its own band's noise, which a probe beside it measures.

use super::Context;
use super::pitch::WindowModes;
use crate::objects;
use crate::reading::{Reading, Section, Table, Unit};
use crate::repr::demod::Tracker;
use crate::repr::envelope;

/// A note cut from a run starts with the notes before it still ringing, so its onset is the
/// percussive one — the first sample at 1 % of the peak — raised to this many times the loudest
/// sample of the file's first [`TAIL_MS`] (12 dB over what was already ringing). A file that starts
/// in silence keeps the percussive onset exactly.
pub const TAIL_MARGIN: f64 = 4.0;
pub const TAIL_MS: f64 = 5.0;
/// A partial only one window holds is believed above the fundamental when its band, followed from
/// the strike, decays within this factor of the mode analysis's own fit…
pub const VERIFY_FACTOR: f64 = 2.0;
/// …and, from any window, when its band's level at the window's start is within this of the
/// analysis's, dB: one line measured two ways. A synthetic onset's click, smeared through a 400 ms
/// band, decayed like a 0.3 s mode the attack window had fitted 51 dB down, and read 26 dB louder.
pub const VERIFY_DB: f64 = 10.0;
/// A note's fundamental is its most energetic persisting partial over the whole note, unless a
/// persisting partial well below it — between [`FUNDAMENTAL_FAR`] and [`FUNDAMENTAL_BELOW`] of its
/// frequency — holds energy within this far of it, dB; then the lowest such. On Iowa's 531 cut notes
/// this names 530 as their files do. The one is a marimba's B6 at mf, under which a line 6.3× lower
/// (a bar ringing in sympathy: it sits under only two notes of the run, so a set cannot call it the
/// room) holds energy 25 dB down; `--room` names it away. A rule that the lower line must outlast the
/// upper threefold fixed that note and named ten crotales by their octave partial, so it was dropped. A drum's rest pitch takes the lowest mode within
/// 12 dB in its ring window, which gave a crotale's 3.95 kHz partial for its C6; the strike's level
/// cannot tell either: the disc's fundamental starts 33 dB under its loudest partial, as far down as
/// a neighbouring bar ringing in sympathy under a marimba note. Energy tells them apart, because a
/// fundamental outlasts the rest (11.6 s against 0.4 s).
pub const FUNDAMENTAL_WITHIN_DB: f64 = 30.0;
/// No ideal object puts its second partial nearer its first than 1.59× (the membrane's), so a line
/// within this ratio below the most energetic partial is its companion — a resonator's line, a
/// neighbour in sympathy (a marimba's Db5 has one 4 % under it, 24 dB down) — not its fundamental.
pub const FUNDAMENTAL_BELOW: f64 = 1.0 / 1.4;
/// …and no further below it than this: a struck object's most energetic partial sits within a few
/// of its lowest ratios (a crotale's at 5.7×), while a high marimba note's frame resonances, near
/// 110 Hz and ringing five times longer than the note, sit 16–19× under it, and a rumble under a
/// crotale 36×. The strike's level cannot part them: the crotale's fundamental starts 31.6 dB under
/// its loudest partial, the frame lines 35.1 dB. **For struck objects**: a string's brightest
/// harmonic can sit far above its fundamental, and strings take a harmonic comb when they come.
pub const FUNDAMENTAL_FAR: f64 = 1.0 / 8.0;
/// A mode window starting this late (ms) holds no glide's phantoms: a partial only it holds is
/// believed anywhere once its band falls as a decaying line does.
pub const LATE_MS: f64 = 160.0;
/// Lines within this share of each other's frequency are one partial: a bar and its resonator, or a
/// string's two polarisations, split one mode into two lines a few hertz apart.
pub const MERGE_SHARE: f64 = 0.02;
/// A partial's demodulation window, in cycles of the distance to its nearest neighbour and in its own
/// periods, held between the bounds below. Sixteen puts the neighbour about 80 dB down; four (40 dB)
/// was tried first, and a bar's overtones, falling fast, sank into the slow fundamental's leak and
/// read three times their decay. The window's length does not bias a decay: an exponential seen
/// through any window falls at its own rate once the window is inside it.
pub const TRACK_CYCLES: f64 = 16.0;
pub const TRACK_MIN_MS: f64 = 4.0;
pub const TRACK_MAX_MS: f64 = 400.0;
/// The envelope's hop, ms.
pub const TRACK_HOP_MS: f64 = 2.0;
/// A partial's strike level is its envelope's largest value within this long of the onset, ms.
pub const STRIKE_MS: f64 = 150.0;
/// A decay is fitted down to this far above the partial's noise, dB, and needs this much fall.
pub const FLOOR_MARGIN_DB: f64 = 10.0;
pub const MIN_FALL_DB: f64 = 10.0;
/// …and over at most the first this many dB of its fall: lower, a louder neighbour's leak rises
/// against it, and a second, slower stage (a bar's resonator) takes over.
pub const FIT_DOWN_DB: f64 = 40.0;
/// The partials the laws are fitted to lie within this far of the strongest at the strike, dB.
pub const LAW_WITHIN_DB: f64 = 60.0;
/// A partial counts as present within this far of the strongest, dB.
pub const COUNT_WITHIN_DB: f64 = 40.0;
/// Each law needs at least this many partials.
pub const LAW_MIN_PARTIALS: usize = 3;

/// The onset of a note: see [`TAIL_MARGIN`]. `None` for silence.
#[must_use]
pub fn onset(x: &[f32], rate: u32) -> Option<usize> {
    let peak = f64::from(mxm_measure::level::peak(x)?);
    if peak <= 0.0 {
        return None;
    }
    let head = ((TAIL_MS / 1000.0 * f64::from(rate)) as usize).clamp(1, x.len());
    let ringing = x[..head]
        .iter()
        .fold(0.0f64, |m, s| m.max(f64::from(s.abs())));
    let threshold = (f64::from(crate::prep::ONSET_FRACTION) * peak).max(TAIL_MARGIN * ringing);
    if threshold > peak {
        // Loudest at its start: nothing was struck after the file began.
        return crate::prep::onset(x);
    }
    x.iter().position(|s| f64::from(s.abs()) >= threshold)
}

/// One partial of the note.
#[derive(Clone, Debug, PartialEq)]
pub struct Partial {
    pub hz: f64,
    /// The second line of a split partial, as its frequency minus this one's, Hz.
    pub split_hz: Option<f64>,
    /// Its level at the strike, dB against the sound's peak: its fitted decay carried back to the
    /// onset — a mode's initial amplitude, as a modal model is set — or, where no decay could be
    /// fitted, its envelope's largest value within [`STRIKE_MS`] of the onset. The largest value alone
    /// read a fast partial low, because a window centred near the strike still half covers the
    /// silence before it: a free bar's tilt read −4 at C4 and −7 at C5 where −6 was built.
    pub strike_db: f64,
    pub t60_s: Option<f64>,
    /// How far the fitted line falls over the stretch it was fitted to, dB.
    pub fall_db: f64,
    /// Its energy over the whole note, dB against the sound's peak held for a second.
    pub energy_db: f64,
    /// Its decay in two stages, where two lines fit its whole fall far better than one: the knee (s
    /// from the onset) and each stage's T60.
    pub stages: Option<Stages>,
    /// Its level beating, where its decay carries a slow periodic swing: the rate (Hz) and the peak
    /// swing (dB) — a string's two polarisations, or a bar and its resonator.
    pub beat: Option<(f64, f64)>,
}

/// A partial's beating is sought between these rates, Hz: a string's two polarisations sit a few
/// tenths of a hertz to a few hertz apart.
pub const BEAT_HZ: (f64, f64) = (0.3, 15.0);
/// A strum's strings are the onsets within this long of the first, ms…
pub const STRUM_MS: f64 = 400.0;
/// …each a spectral-flux peak at least this share of the largest (a later string enters a spectrum
/// already sounding, and 0.3 lost a chord's third string).
pub const STRUM_SHARE: f64 = 0.15;
/// …and this many times the median flux of the 20 ms before it (see `strum`).
pub const STRUM_OVER_BEFORE: f64 = 4.0;

/// A decay in two stages: a slower second stage is a piano's aftersound or a string's two
/// polarisations; a far faster one, a damper coming down. Read on the energy decay curve, whose knee
/// sits a little before a damper's (0.1 s before one at 1.2 s), and only down to 50 dB of it: a
/// piano sample's note-off 9.5 s in, below that, is not seen. Two **lines**, not two exponentials: a sum
/// of two decays bends between them, and the lines average across the bend — a 0.5 s decay over a
/// 4 s one 20 dB down reads a ratio of about 3.8, not 8.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stages {
    pub knee_s: f64,
    pub early_t60_s: f64,
    pub late_t60_s: f64,
}

/// Two lines are taken over one when they leave at most this share of one line's squared error…
/// (on the energy decay curve: on the envelope, a piano's unison strings beat ±3–4 dB around both
/// lines, an error no second line removes, and neither half nor three quarters let its note-off
/// through)
pub const STAGES_ERROR_SHARE: f64 = 0.5;
/// …each stage falls at least this far, dB…
pub const STAGES_MIN_FALL_DB: f64 = 5.0;
/// …and their decays differ by at least this factor.
pub const STAGES_MIN_RATIO: f64 = 1.5;
/// The energy decay curve is read down to this far, dB.
pub const STAGES_EDC_FLOOR_DB: f64 = 50.0;
/// A fall ends for its stages where the envelope stays under the noise margin this long, ms.
pub const STAGES_BELOW_MS: f64 = 200.0;
/// A damper's decay is read from this long after the curve's knee, s.
pub const DAMPER_SETTLE_S: f64 = 0.15;
/// A second stage this many times faster than the first is a damper coming down (String Studio's
/// *Release*), not a second decay of the string.
pub const DAMPER_RATIO: f64 = 3.0;

/// The sum of squared residuals of a line through `points`.
fn line_error(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    let (slope, intercept) = line(points)?;
    let error = points
        .iter()
        .map(|p| (p.1 - intercept - slope * p.0).powi(2))
        .sum();
    Some((slope, error))
}

/// A decay's two stages, by the breakpoint that leaves two lines the least squared error (see
/// [`STAGES_ERROR_SHARE`]); `points` are `(s, dB)` from the strike down.
fn stages(points: &[(f64, f64)]) -> Option<Stages> {
    let (_, one) = line_error(points)?;
    let n = points.len();
    if n < 20 || one <= 0.0 {
        return None;
    }
    let (at, error, early, late) = (n / 10..n - n / 10)
        .step_by((n / 200).max(1))
        .filter_map(|b| {
            let (se, ee) = line_error(&points[..b])?;
            let (sl, el) = line_error(&points[b..])?;
            Some((b, ee + el, se, sl))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    let span = |a: usize, b: usize| points[b - 1].0 - points[a].0;
    let (fall_early, fall_late) = (-early * span(0, at), -late * span(at, n));
    let ratio = (late / early).abs().max((early / late).abs());
    (error <= STAGES_ERROR_SHARE * one
        && early < 0.0
        && late < 0.0
        && fall_early >= STAGES_MIN_FALL_DB
        && fall_late >= STAGES_MIN_FALL_DB
        && ratio >= STAGES_MIN_RATIO)
        .then(|| Stages {
            knee_s: points[at].0,
            early_t60_s: -60.0 / early,
            late_t60_s: -60.0 / late,
        })
}

/// A partial the mode analysis found, before it is followed.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    /// The frequency of its strongest line, Hz.
    pub hz: f64,
    /// Its second line's frequency minus this one's, where one window holds two.
    pub split_hz: Option<f64>,
    /// Another window holds it too: the mode analysis's rule for a mode believed.
    pub persists: bool,
    /// Its strongest line is in a window starting [`LATE_MS`] or later.
    pub late: bool,
    /// Its strongest line's window start (ms from the onset) and level there (dB against the peak),
    /// as the mode analysis fitted it.
    pub from_ms: f64,
    pub level_db: f64,
    /// Its strongest line's decay as the mode analysis fitted it, s.
    pub t60_s: f64,
}

/// One mode line: Hz, level against the peak (dB), window index, persists, T60 (s).
type Line = (f64, f64, usize, bool, f64);

/// Every window's modes merged into partials, lowest first: lines within [`MERGE_SHARE`] are one
/// partial, at the frequency of its strongest line. A drum's rule believes only a mode another window
/// holds too, because a glide leaves phantom modes that do not persist; a bar does not glide, and its
/// brightest partials (a xylophone's, a glockenspiel's) can die inside the first window, so a partial
/// that does not persist is kept here for [`read`] to verify. So is one only a late window holds: a
/// glide is over within tens of milliseconds, and a vibraphone's Db3 or a crotale's E6 at pp was
/// found only from 400 ms, ringing for 16 and 9 s.
#[must_use]
pub fn partials(windows: &[WindowModes]) -> Vec<Candidate> {
    let mut lines: Vec<Line> = Vec::new();
    for (wi, w) in windows.iter().enumerate() {
        for (m, level) in &w.modes {
            let persists = super::pitch::persistence(windows, w, m) >= 1;
            lines.push((m.hz, *level, wi, persists, m.t60()));
        }
    }
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut groups: Vec<Vec<Line>> = Vec::new();
    for line in lines {
        match groups.last_mut() {
            Some(g) if line.0 <= g[0].0 * (1.0 + MERGE_SHARE) => g.push(line),
            _ => groups.push(vec![line]),
        }
    }
    groups
        .iter()
        .filter_map(|g| {
            let main = g.iter().max_by(|a, b| a.1.total_cmp(&b.1))?;
            // A split: two persisting lines of the group in one window, the earliest that has two.
            let split_hz = (0..windows.len()).find_map(|wi| {
                let mut here: Vec<_> = g.iter().filter(|l| l.2 == wi && l.3).collect();
                here.sort_by(|a, b| b.1.total_cmp(&a.1));
                (here.len() >= 2).then(|| here[1].0 - here[0].0)
            });
            Some(Candidate {
                hz: main.0,
                split_hz,
                persists: g.iter().any(|l| l.3),
                late: windows[main.2].window_ms.0 >= LATE_MS,
                from_ms: windows[main.2].window_ms.0,
                level_db: main.1,
                t60_s: main.4,
            })
        })
        .collect()
}

/// A line fitted by least squares: `(slope, intercept)`; `None` for fewer than two points or no
/// spread in `x`.
fn line(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    let n = points.len() as f64;
    if points.len() < 2 {
        return None;
    }
    let mx = points.iter().map(|p| p.0).sum::<f64>() / n;
    let my = points.iter().map(|p| p.1).sum::<f64>() / n;
    let sxx: f64 = points.iter().map(|p| (p.0 - mx).powi(2)).sum();
    if sxx <= 0.0 {
        return None;
    }
    let sxy: f64 = points.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
    let slope = sxy / sxx;
    Some((slope, my - slope * mx))
}

/// The amplitude envelope at `hz` from the onset, one value per [`TRACK_HOP_MS`], over a Hann window
/// `window_ms` long centred on each point; points whose window runs past the file are left out.
fn envelope_at(c: &Context, hz: f64, window_ms: f64) -> Vec<f64> {
    let hop = ((TRACK_HOP_MS / 1000.0 * c.rate).round() as usize).max(1);
    let span = ((window_ms / 1000.0 * c.rate / hop as f64).round() as usize).max(2);
    let half = (span * hop / 2) as i64;
    let origin = c.onset as i64 - half;
    let after = c.x.len().saturating_sub(c.onset);
    let points = (after.saturating_sub(half as usize)) / hop;
    let Some(t) = Tracker::new(&c.x, c.rate, hz, origin, hop, span, points + span) else {
        return Vec::new();
    };
    (0..points).filter_map(|p| t.amplitude(p)).collect()
}

/// Follows one partial: its strike level, and its decay fitted from the strike down to
/// [`FLOOR_MARGIN_DB`] above the noise a probe beside it reads.
fn follow(c: &Context, hz: f64, neighbour_hz: f64) -> Option<Partial> {
    let gap = (neighbour_hz - hz).abs().max(1e-9);
    let window_ms = (1000.0 * TRACK_CYCLES / gap)
        .max(1000.0 * TRACK_CYCLES / hz)
        .clamp(TRACK_MIN_MS, TRACK_MAX_MS);
    let env = envelope_at(c, hz, window_ms);
    // The probe sits on the window's first null, on the side away from the nearest neighbour.
    let probe_hz = hz - (gap / 2.0).min(0.5 * hz) * (neighbour_hz - hz).signum();
    let probe = envelope_at(c, probe_hz, window_ms);
    let hop_s = TRACK_HOP_MS / 1000.0;
    let strike_points = ((STRIKE_MS / TRACK_HOP_MS) as usize).clamp(1, env.len().max(1));
    let (peak_at, strike) =
        env.iter()
            .take(strike_points)
            .enumerate()
            .fold(
                (0, 0.0f64),
                |(i, m), (j, &a)| if a > m { (j, a) } else { (i, m) },
            );
    let strike_db = envelope::db(strike, c.peak)?;
    let mut noise: Vec<f64> = probe.iter().copied().filter(|a| *a > 0.0).collect();
    noise.sort_by(f64::total_cmp);
    let noise = noise.get(noise.len() / 2).copied().unwrap_or(0.0);
    let limit =
        (noise * 10f64.powf(FLOOR_MARGIN_DB / 20.0)).max(strike * 10f64.powf(-FIT_DOWN_DB / 20.0));
    let end = env
        .iter()
        .enumerate()
        .skip(peak_at)
        .find(|(_, a)| **a < limit)
        .map_or(env.len(), |(i, _)| i);
    let points: Vec<(f64, f64)> = env[peak_at..end]
        .iter()
        .enumerate()
        .filter(|(_, a)| **a > 0.0)
        .map(|(i, a)| ((peak_at + i) as f64 * hop_s, 20.0 * (a / strike).log10()))
        .collect();
    let fitted = line(&points);
    let span_s = points.last().map_or(0.0, |p| p.0) - peak_at as f64 * hop_s;
    let (t60_s, fall_db, at_onset_db) = match fitted {
        Some((slope, intercept)) if slope < 0.0 => {
            let fall = -slope * span_s;
            let valid = fall >= MIN_FALL_DB;
            (
                valid.then_some(-60.0 / slope),
                fall,
                valid.then_some(intercept),
            )
        }
        _ => (None, 0.0, None),
    };
    let strike_db = strike_db + at_onset_db.unwrap_or(0.0);
    // The whole fall, down to where it stays under the noise margin for STAGES_BELOW_MS, for its
    // stages: a piano's unison strings beat, and a null of their beating dips under the margin long
    // before the fall reaches it.
    let floor = noise * 10f64.powf(FLOOR_MARGIN_DB / 20.0);
    let below = ((STAGES_BELOW_MS / TRACK_HOP_MS) as usize).max(1);
    let whole_end = (peak_at..env.len())
        .find(|&i| {
            env[i..(i + below).min(env.len())]
                .iter()
                .all(|a| *a < floor)
        })
        .unwrap_or(env.len());
    // The stages are read on the fall's energy decay curve (Schroeder's backward integration, the
    // noise's power taken out first): smooth where the envelope beats, and a two-slope decay bends in
    // it as it does in a room's. Its last stretch, where the integral runs out, is left out.
    let power: Vec<f64> = env[peak_at..whole_end]
        .iter()
        .map(|a| (a * a - noise * noise).max(0.0))
        .collect();
    let mut remaining: Vec<f64> = power.clone();
    for i in (0..remaining.len().saturating_sub(1)).rev() {
        remaining[i] += remaining[i + 1];
    }
    let total = remaining.first().copied().unwrap_or(0.0);
    let whole: Vec<(f64, f64)> = remaining
        .iter()
        .enumerate()
        .take(remaining.len() * 9 / 10)
        .filter(|(_, e)| **e > 0.0 && total > 0.0)
        .map(|(i, e)| ((peak_at + i) as f64 * hop_s, 10.0 * (e / total).log10()))
        .take_while(|p| p.1 >= -STAGES_EDC_FLOOR_DB)
        .collect();
    // A damper's decay is read on the envelope from DAMPER_SETTLE_S after the curve's knee: the curve
    // runs out within a tenth of a second under a damper (it read a 0.15 s T60 as 0.4 s), and its knee
    // sits before the damper's.
    let stages = stages(&whole).map(|st| {
        if st.early_t60_s < DAMPER_RATIO * st.late_t60_s {
            return st;
        }
        let after: Vec<(f64, f64)> = env[peak_at..whole_end]
            .iter()
            .enumerate()
            .map(|(i, a)| ((peak_at + i) as f64 * hop_s, *a))
            .filter(|(t, a)| *t >= st.knee_s + DAMPER_SETTLE_S && *a > 0.0)
            .map(|(t, a)| (t, 20.0 * (a / strike).log10()))
            .collect();
        match line(&after) {
            Some((slope, _)) if slope < 0.0 && after.len() >= 10 => Stages {
                late_t60_s: -60.0 / slope,
                ..st
            },
            _ => st,
        }
    });
    // Its beating: the level in dB over the fitted fall, its line taken out, read for a slow swing
    // as a held note's vibrato is.
    let beat = super::sustain::modulation(&points, hop_s, BEAT_HZ).and_then(|m| m.1);
    let energy: f64 = env.iter().map(|a| a * a).sum::<f64>() * hop_s;
    Some(Partial {
        hz,
        split_hz: None,
        strike_db,
        t60_s,
        fall_db,
        energy_db: 10.0 * (energy / (c.peak * c.peak)).log10(),
        stages,
        beat,
    })
}

/// A harmonic series is found when this many of its first [`SERIES_FIRST`] harmonics are among the
/// note's partials, or one fewer with the first among them: a string, open or closed at its ends, and
/// a tube. A tuned bar's 1 : 4 : 10 holds two of eight, a free bar's none.
pub const SERIES_FIRST: usize = 8;
pub const SERIES_MIN: usize = 6;
/// A series this long is a string's whatever its first harmonic's level (see `read`).
pub const SERIES_LONG: usize = 10;
/// A series whose odd harmonics sit this far under its even ones on average, dB, is the series an
/// octave up (see `read`).
pub const SERIES_ODD_UNDER_DB: f64 = 15.0;
/// A partial is harmonic `n` within this share of where the series puts it.
pub const SERIES_TOLERANCE: f64 = 0.03;
/// Harmonics are sought up to this number.
pub const SERIES_MAX_N: usize = 40;
/// A harmonic the series lacks between two it holds counts this far under its weakest, dB, when the
/// comb is sought: a zero of the comb is exactly a harmonic too weak to be found.
pub const SERIES_MISSING_DB: f64 = 10.0;
/// The comb is reported when it matches the levels this well (a correlation).
pub const COMB_MIN_MATCH: f64 = 0.5;

/// A harmonic series in a note's partials: a stiff string's, `f_n = n·f0·√(1 + B·n²)` (the stiff-string
/// law, `research:listening/percussion-perception.md` §5.1).
#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    pub f0: f64,
    /// The inharmonicity coefficient B.
    pub b: f64,
    /// Each harmonic found: its number, frequency and strike level (dB).
    pub harmonics: Vec<(usize, f64, f64)>,
}

/// The harmonic deficit is read over the first this many harmonics, up to the series' last…
pub const DEFICIT_ORDERS: usize = 12;
/// …a harmonic missing, or this far under the line through the series' levels, dB.
pub const DEFICIT_DB: f64 = 15.0;
/// The line leaves out the harmonics this far under its first fit before it is fitted again, dB: a
/// clarinet's sunk even half drew a line through all of them halfway down, and none sat 15 dB under
/// it.
pub const DEFICIT_REFIT_DB: f64 = 6.0;

/// The harmonic deficit (the plan's §2 item 3): each harmonic among the first [`DEFICIT_ORDERS`], up to
/// the series' last, that is missing or [`DEFICIT_DB`] under the line through the series' strike
/// levels against log₂ n — a clarinet's even half, a pluck's comb nulls, a filter's notch. Each as
/// `(n, Hz, dB under the line)`, the level absent for a harmonic not found.
#[must_use]
pub fn harmonic_deficit(s: &Series) -> Vec<(usize, f64, Option<f64>)> {
    let top = s
        .harmonics
        .iter()
        .map(|h| h.0)
        .max()
        .unwrap_or(0)
        .min(DEFICIT_ORDERS);
    let points: Vec<(f64, f64)> = s
        .harmonics
        .iter()
        .filter(|h| h.0 <= top)
        .map(|h| ((h.0 as f64).log2(), h.2))
        .collect();
    let Some((slope, icept)) = line(&points) else {
        return Vec::new();
    };
    let kept: Vec<(f64, f64)> = points
        .iter()
        .copied()
        .filter(|p| p.1 >= icept + slope * p.0 - DEFICIT_REFIT_DB)
        .collect();
    let (slope, icept) = line(&kept).unwrap_or((slope, icept));
    (1..=top)
        .filter_map(|n| {
            let at = (n as f64).log2();
            match s.harmonics.iter().find(|h| h.0 == n) {
                Some(h) => {
                    let under = h.2 - (icept + slope * at);
                    (under < -DEFICIT_DB).then_some((n, h.1, Some(under)))
                }
                None => Some((
                    n,
                    n as f64 * s.f0 * (1.0 + s.b * (n * n) as f64).max(0.0).sqrt(),
                    None,
                )),
            }
        })
        .collect()
}

/// The harmonics of `f0` stretched by `b` among `partials` (`(Hz, strike dB)`), each partial used once.
fn harmonics_of(partials: &[(f64, f64)], f0: f64, b: f64, up_to: usize) -> Vec<(usize, f64, f64)> {
    let mut used = vec![false; partials.len()];
    let mut out = Vec::new();
    for n in 1..=up_to {
        let at = n as f64 * f0 * (1.0 + b * (n * n) as f64).max(0.0).sqrt();
        if let Some((i, p)) = partials
            .iter()
            .enumerate()
            .filter(|(i, p)| !used[*i] && (p.0 / at - 1.0).abs() <= SERIES_TOLERANCE)
            .min_by(|a, b| {
                (a.1.0 / at - 1.0)
                    .abs()
                    .total_cmp(&(b.1.0 / at - 1.0).abs())
            })
        {
            used[i] = true;
            out.push((n, p.0, p.1));
        }
    }
    out
}

/// `f0` and `B` from harmonics: `(f_n / n)² = f0² + f0²·B·n²` is a line in `n²`.
fn stiff_string(harmonics: &[(usize, f64, f64)]) -> Option<(f64, f64)> {
    let points: Vec<(f64, f64)> = harmonics
        .iter()
        .map(|&(n, hz, _)| ((n * n) as f64, (hz / n as f64).powi(2)))
        .collect();
    if points.len() < 3 {
        return None;
    }
    let (slope, intercept) = line(&points)?;
    (intercept > 0.0).then(|| (intercept.sqrt(), slope / intercept))
}

/// The note's harmonic series, where it has one (see [`SERIES_MIN`]): each partial's subharmonics
/// down to the lowest partial are candidates, scored by the first harmonics they hold less the
/// partials they leave unexplained below their last — a stiff string's stretched harmonics meet other
/// partials within the tolerance, and a third harmonic read as the fundamental held as many as the
/// true one while leaving eleven partials between its own — and `B` is fitted to the winner and the
/// harmonics sought again along the stretched series.
#[must_use]
pub fn harmonic_series(partials: &[(f64, f64)]) -> Option<Series> {
    let lowest = partials.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let (_, f0) = partials
        .iter()
        .flat_map(|p| (1..=SERIES_FIRST).map(move |k| p.0 / k as f64))
        .filter(|&f0| f0 >= lowest * (1.0 - SERIES_TOLERANCE))
        .map(|f0| (harmonics_of(partials, f0, 0.0, SERIES_FIRST), f0))
        .filter(|(h, _)| {
            h.len() >= SERIES_MIN
                || (h.len() + 1 >= SERIES_MIN && h.first().is_some_and(|x| x.0 == 1))
        })
        .map(|(h, f0)| {
            let top = (SERIES_FIRST as f64 + 0.5) * f0;
            let unexplained = partials
                .iter()
                .filter(|p| p.0 <= top && !h.iter().any(|x| x.1 == p.0))
                .count();
            (h.len() as i64 - unexplained as i64, f0)
        })
        .max_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)))?;
    let first = harmonics_of(partials, f0, 0.0, SERIES_FIRST);
    let (f0, b) = stiff_string(&first).unwrap_or((f0, 0.0));
    let harmonics = harmonics_of(partials, f0, b, SERIES_MAX_N);
    let (f0, b) = stiff_string(&harmonics).unwrap_or((f0, b));
    Some(Series {
        f0,
        b,
        harmonics: harmonics_of(partials, f0, b, SERIES_MAX_N),
    })
}

/// Where along the string it was plucked or struck, as a share of its length (0 to ½): an ideal
/// excitation at β leaves harmonic `n` at `|sin(nπβ)|` of its level (the excitation position's comb,
/// `research:listening/percussion-perception.md` §6), so β is the comb that best matches the
/// harmonics' levels once their overall tilt is taken out of both. Harmonics the series lacks between
/// those it holds count [`SERIES_MISSING_DB`] under the weakest. `None` for fewer than six harmonics or
/// no comb matching [`COMB_MIN_MATCH`].
#[must_use]
pub fn excitation_position(series: &Series) -> Option<f64> {
    let top = series
        .harmonics
        .iter()
        .map(|h| h.0)
        .max()?
        .min(SERIES_MAX_N);
    if series.harmonics.len() < 6 {
        return None;
    }
    let weakest = series
        .harmonics
        .iter()
        .map(|h| h.2)
        .fold(f64::INFINITY, f64::min);
    let levels: Vec<(f64, f64)> = (1..=top)
        .map(|n| {
            let level = series
                .harmonics
                .iter()
                .find(|h| h.0 == n)
                .map_or(weakest - SERIES_MISSING_DB, |h| h.2);
            ((n as f64).log2(), level)
        })
        .collect();
    let detrend = |v: &[(f64, f64)]| -> Option<Vec<f64>> {
        let (slope, intercept) = line(v)?;
        Some(v.iter().map(|p| p.1 - intercept - slope * p.0).collect())
    };
    let measured = detrend(&levels)?;
    let correlation = |a: &[f64], b: &[f64]| -> f64 {
        let n = a.len() as f64;
        let (ma, mb) = (a.iter().sum::<f64>() / n, b.iter().sum::<f64>() / n);
        let cov: f64 = a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum();
        let (va, vb): (f64, f64) = (
            a.iter().map(|x| (x - ma).powi(2)).sum(),
            b.iter().map(|y| (y - mb).powi(2)).sum(),
        );
        if va <= 0.0 || vb <= 0.0 {
            0.0
        } else {
            cov / (va * vb).sqrt()
        }
    };
    let (beta, fit) = (10..=500)
        .map(|k| f64::from(k) / 1000.0)
        .filter_map(|beta| {
            let comb: Vec<(f64, f64)> = (1..=top)
                .map(|n| {
                    let s = (n as f64 * std::f64::consts::PI * beta)
                        .sin()
                        .abs()
                        .max(0.01);
                    ((n as f64).log2(), 20.0 * s.log10())
                })
                .collect();
            detrend(&comb).map(|c| (beta, correlation(&measured, &c)))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    (fit >= COMB_MIN_MATCH).then_some(beta)
}

/// A note's pitch is its fundamental; the virtual pitch is reported beside it as the pitch the ear may
/// take instead. On Iowa's 531 notes the fundamental names 530 as their files do and the virtual pitch
/// 461 (a low bar's upper partials, near the dominance region, can outvote its fundamental), while on
/// VCSL's tubular bells the fundamental names 1 of 40 and the virtual pitch 31 by pitch class (the
/// strike note, a pitch no partial carries; 14 at the named octave, 17 one below).
///
/// Subharmonic summation (Hermes, "Measurement of pitch by subharmonic summation", *JASA* 83, 1988):
/// a candidate pitch scores the partials on its harmonics, harmonic `n` weighted `SHS_H^(n−1)`. Run on
/// the note's partial table rather than a spectrum, each partial weighted by the dominance region and,
/// mildly, by its level: linearly in dB from 1 at the strongest to 0 at [`VIRTUAL_WITHIN_DB`] under
/// it (**chosen**). A bell's strike note is a virtual pitch from its nominal, superquint and octave
/// nominal, about half the nominal, and the level of an audible partial does not significantly move
/// it; the dominant partials lie near 500–1600 Hz (`research:listening/percussion-perception.md` §2.1
/// and §2.5). Presence alone let a tube's weak low modes, 16–24 dB under its loud 4th to 6th, take
/// the pitch.
pub const SHS_H: f64 = 0.84;
pub const SHS_HARMONICS: usize = 15;
/// A partial sits on a harmonic within this share of it.
pub const SHS_TOLERANCE: f64 = 0.03;
pub const VIRTUAL_WITHIN_DB: f64 = 30.0;
/// The dominance region as a weight, a Gaussian in octaves around this centre (Hibbert's reading,
/// "centred near 600 to 800Hz") with this width — **chosen**.
pub const DOMINANCE_HZ: f64 = 700.0;
pub const DOMINANCE_OCTAVES: f64 = 1.5;

/// The pitch the ear takes from `partials` (`(Hz, strike level dB)`): see [`SHS_H`]. `None` without a
/// partial.
#[must_use]
pub fn virtual_pitch(partials: &[(f64, f64)]) -> Option<f64> {
    let strongest = partials
        .iter()
        .map(|p| p.1)
        .fold(f64::NEG_INFINITY, f64::max);
    let present: Vec<(f64, f64)> = partials
        .iter()
        .filter(|p| p.0 > 0.0 && p.1 >= strongest - VIRTUAL_WITHIN_DB)
        .map(|p| {
            let octaves = (p.0 / DOMINANCE_HZ).log2() / DOMINANCE_OCTAVES;
            let level = 1.0 - (strongest - p.1) / VIRTUAL_WITHIN_DB;
            (p.0, (-0.5 * octaves * octaves).exp() * level)
        })
        .collect();
    // For each harmonic of `f0`, the heaviest partial on it and the pitch it implies.
    let matches = |f0: f64| -> Vec<(f64, f64)> {
        (1..=SHS_HARMONICS)
            .filter_map(|n| {
                let at = n as f64 * f0;
                present
                    .iter()
                    .filter(|q| (q.0 / at - 1.0).abs() <= SHS_TOLERANCE)
                    .max_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|q| (SHS_H.powi(n as i32 - 1) * q.1, q.0 / n as f64))
            })
            .collect()
    };
    let score = |f0: f64| matches(f0).iter().map(|m| m.0).sum::<f64>();
    let best = present
        .iter()
        .flat_map(|q| (1..=8).map(move |n| q.0 / f64::from(n)))
        .filter(|&f0| f0 >= 20.0)
        .max_by(|a, b| score(*a).total_cmp(&score(*b)))?;
    // Refined: the harmonics' implied pitches, averaged by their weights.
    let m = matches(best);
    let weight: f64 = m.iter().map(|x| x.0).sum();
    (weight > 0.0).then(|| m.iter().map(|x| x.0 * x.1).sum::<f64>() / weight)
}

/// The onsets at the start of a sound: spectral-flux peaks over third-octave bands (1024-point Hann
/// frames every 256 samples)
/// of at least [`STRUM_SHARE`] of the largest, within [`STRUM_MS`] of the first, no two within 10 ms;
/// their count and the time from the first to the last, ms. A strum's strings come one after another.
fn strum(c: &Context) -> (usize, f64) {
    const FRAME: usize = 1024;
    const STEP: usize = 256;
    // From a frame before the onset: an attack at the onset itself lies in the first frame only, which
    // has no frame before it to rise from, and a strum's first string went uncounted.
    // Silence stands in for what the file does not hold before it: a strike under a frame from the
    // file's start sat in the first frame, rose from nothing, and its beating partials' ripple
    // outweighed it.
    let from = c.onset.saturating_sub(FRAME);
    let pad = FRAME.saturating_sub(c.onset);
    let end = from + ((STRUM_MS + 100.0) / 1000.0 * c.rate) as usize + FRAME;
    let mut padded = vec![0.0; pad];
    padded.extend_from_slice(&c.x[from..end.min(c.x.len())]);
    let x = &padded[..];
    if x.len() < FRAME * 2 {
        return (1, 0.0);
    }
    let window: Vec<f64> = (0..FRAME)
        .map(|n| 0.5 - 0.5 * (std::f64::consts::TAU * n as f64 / FRAME as f64).cos())
        .collect();
    let frames = (x.len() - FRAME) / STEP + 1;
    let bin = c.rate / FRAME as f64;
    let top_hz = (16_000.0f64).min(0.45 * c.rate);
    let mut bands: Vec<(usize, usize)> = Vec::new();
    let mut centre = 50.0f64;
    while centre * 2f64.powf(1.0 / 6.0) <= top_hz {
        let (lo, hi) = (centre / 2f64.powf(1.0 / 6.0), centre * 2f64.powf(1.0 / 6.0));
        let (a, b) = (
            (lo / bin).ceil() as usize,
            ((hi / bin).ceil() as usize).min(FRAME / 2 + 1),
        );
        if b > a {
            bands.push((a, b));
        }
        centre *= 2f64.powf(1.0 / 3.0);
    }
    let mut previous: Option<Vec<f64>> = None;
    let mut flux = vec![0.0; frames];
    for (f, value) in flux.iter_mut().enumerate() {
        let mut re: Vec<f64> = (0..FRAME).map(|n| x[f * STEP + n] * window[n]).collect();
        let mut im = vec![0.0; FRAME];
        if mxm_measure::spectrum::fft(&mut re, &mut im).is_none() {
            return (1, 0.0);
        }
        // Third-octave bands, not bins: a bin between two partials holds both one's and the other's
        // window leakage, which beats at their spacing, and sampled every hop the beat aliased into a
        // ripple of the flux 30 ms long that read a clean pluck as a strum. A band holding both beats
        // no more than their sum.
        let now: Vec<f64> = bands
            .iter()
            .map(|&(a, b)| {
                let power: f64 = (a..b).map(|k| re[k] * re[k] + im[k] * im[k]).sum();
                (1.0 + 1000.0 * power.sqrt() / c.peak).ln()
            })
            .collect();
        if let Some(p) = &previous {
            *value = now.iter().zip(p).map(|(a, b)| (a - b).max(0.0)).sum();
        }
        previous = Some(now);
    }
    let top = flux.iter().copied().fold(0.0, f64::max);
    if top <= 0.0 {
        return (1, 0.0);
    }
    let gap = ((0.01 * c.rate) as usize / STEP).max(1);
    // A new string rises out of a quiet flux (a decaying note's flux counts only rises); a beating
    // note's level swings make ripples amid their own ripple, so an onset must also stand
    // STRUM_OVER_BEFORE times over the median of the 20 ms before it (50 ms took in the string before,
    // 30 ms earlier in a strum).
    let before = ((0.02 * c.rate) as usize / STEP).max(2);
    let quiet_before = |f: usize| {
        let mut v: Vec<f64> = flux[f.saturating_sub(before)..f].to_vec();
        v.sort_by(f64::total_cmp);
        v.get(v.len() / 2).copied().unwrap_or(0.0)
    };
    let peaks: Vec<usize> = (1..frames.saturating_sub(1))
        .filter(|&f| {
            flux[f] >= STRUM_SHARE * top
                && flux[f] >= flux[f - 1]
                && flux[f] > flux[f + 1]
                && flux[f] >= STRUM_OVER_BEFORE * quiet_before(f)
        })
        .collect();
    let mut onsets: Vec<usize> = Vec::new();
    for f in peaks {
        match onsets.last() {
            Some(&last) if f - last < gap => {
                if flux[f] > flux[last] {
                    *onsets.last_mut().expect("just matched") = f;
                }
            }
            _ => onsets.push(f),
        }
    }
    let first = onsets.first().copied().unwrap_or(0);
    let within = (STRUM_MS / 1000.0 * c.rate) as usize / STEP;
    onsets.retain(|&f| f - first <= within);
    let spread = onsets
        .last()
        .map_or(0.0, |&l| ((l - first) * STEP) as f64 / c.rate * 1000.0);
    (onsets.len().max(1), spread)
}

/// The equal-tempered note nearest `hz` at A = 440 Hz, and how far `hz` sits from it, cents.
#[must_use]
pub fn nearest_note(hz: f64) -> Option<(String, f64)> {
    const NAMES: [&str; 12] = [
        "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
    ];
    if hz.is_nan() || hz <= 0.0 {
        return None;
    }
    let from_a4 = 1200.0 * (hz / 440.0).log2();
    let steps = (from_a4 / 100.0).round();
    let midi = 69 + steps as i64;
    let name = format!(
        "{}{}",
        NAMES[midi.rem_euclid(12) as usize],
        midi.div_euclid(12) - 1
    );
    Some((name, from_a4 - 100.0 * steps))
}

/// The note section: the partials from the note's own mode windows, the fundamental (the rest pitch
/// `f1` stands in when no partial persists), and the laws. `room` are lines a set found recurring in
/// its files without being any note's — under a run of high marimba notes, the frame's resonances —
/// which are no partial of this one. `expect` is the note the caller says it is, Hz: the fundamental
/// is then its most energetic partial within a semitone of it. A `sustained` note (a bow, a breath)
/// takes no weaker line below its most energetic partial for its fundamental: that rule is a struck
/// object's, whose fundamental outlasts the rest, while a held note holds every line as long, and an
/// overblown flute's A5 holds its A4 fingering's resonance 30 dB down for the whole note.
#[must_use]
pub fn read(
    c: &Context,
    windows: &[WindowModes],
    f1: Option<f64>,
    room: &[f64],
    expect: Option<f64>,
    sustained: bool,
) -> Section {
    let src_tuning = "the fundamental (the most energetic partial, or one 1.4 to 8 times below it within 30 dB of its energy) against equal temperament, A = 440 Hz";
    let src_law = "research:listening/percussion-perception.md §4.1, §9 and research:physical-modelling/physical-modelling-synthesis.md §6.3: AAS Material, a line through ln T60 against ln (f/f₁)";
    let src_tilt = "AAS Tone: a line through each partial's strike level against log₂ (f/f₁)";
    let src_ideal = "research:physical-modelling/physical-modelling-synthesis.md §6.2, the nearest object's ratios";
    let src_t60 = "a line through the partial's level from its strike over its first 40 dB of fall, or until 10 dB above its band's noise";
    let src_split = "two lines of one partial in one mode window: a bar and its resonator, or two polarisations";
    let Some(f1) = f1 else {
        let why = "no fundamental: no believed mode";
        let section = Section::new(
            "note",
            "The note: partials, material and tone",
            vec![
                Reading::absent(
                    "note.fundamental",
                    "The fundamental",
                    Unit::Hertz,
                    why,
                    src_tuning,
                ),
                Reading::absent(
                    "note.tuning",
                    "Pitch against equal temperament (A = 440 Hz)",
                    Unit::Cents,
                    why,
                    src_tuning,
                ),
                Reading::absent(
                    "note.virtual_pitch",
                    "The pitch the ear takes (virtual pitch)",
                    Unit::Hertz,
                    why,
                    src_tuning,
                ),
                Reading::absent(
                    "note.virtual_tuning",
                    "The virtual pitch against equal temperament (A = 440 Hz)",
                    Unit::Cents,
                    why,
                    src_tuning,
                ),
                Reading::absent(
                    "note.t60",
                    "The fundamental's decay (T60)",
                    Unit::Seconds,
                    why,
                    src_t60,
                ),
                Reading::absent(
                    "note.decay_law",
                    "Decay against frequency, M in T60 ∝ (f/f₁)^M",
                    Unit::Plain,
                    why,
                    src_law,
                ),
                Reading::absent(
                    "note.tilt",
                    "Level against frequency at the strike",
                    Unit::DecibelsPerOctave,
                    why,
                    src_tilt,
                ),
                Reading::absent(
                    "note.partials",
                    "Partials within 40 dB of the strongest",
                    Unit::Plain,
                    why,
                    src_ideal,
                ),
                Reading::absent(
                    "note.ideal_cents",
                    "The partials' distance from the nearest ideal object",
                    Unit::Cents,
                    why,
                    src_ideal,
                ),
                Reading::absent(
                    "note.split",
                    "The fundamental split into two lines",
                    Unit::Hertz,
                    why,
                    src_split,
                ),
            ],
        );
        return section;
    };
    let found: Vec<Candidate> = partials(windows)
        .into_iter()
        .filter(|k| {
            !room.iter().any(|r| {
                (r - k.hz).abs() <= crate::set::SAME_LINE_HZ.max(super::pitch::ROOM_FRACTION * r)
            })
        })
        .collect();
    let freqs: Vec<f64> = found.iter().map(|p| p.hz).collect();
    let followed: Vec<(&Candidate, Partial)> = found
        .iter()
        .enumerate()
        .filter_map(|(i, candidate)| {
            let hz = candidate.hz;
            // The nearest other partial, or the partial's own image at −hz when it stands alone.
            let neighbour = freqs
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, &f)| f)
                .min_by(|a, b| (a - hz).abs().total_cmp(&(b - hz).abs()))
                .unwrap_or(-hz);
            let mut p = follow(c, hz, neighbour)?;
            p.split_hz = candidate.split_hz;
            p.strike_db.is_finite().then_some((candidate, p))
        })
        .collect();
    // A partial only one window holds is kept above the fundamental when its band decays as the
    // attack window's fit says (that window can hold a contact's phantoms), and anywhere when a late
    // window found it and its band falls as a line does. A late window cannot fit a long decay (a
    // crotale's E6 at pp: 9.4 s from 600 ms of it, 3.7 s from its band over the whole note); what
    // it must rule out is a steady line, and a steady line does not fall.
    let level_agrees = |k: &Candidate, p: &Partial| {
        p.t60_s.is_some_and(|t| {
            let there = p.strike_db - 60.0 * k.from_ms / 1000.0 / t;
            (there - k.level_db).abs() <= VERIFY_DB
        })
    };
    let verified = |k: &Candidate, p: &Partial| {
        level_agrees(k, p)
            && p.t60_s
                .is_some_and(|t| (t / k.t60_s).ln().abs() <= VERIFY_FACTOR.ln())
    };
    let believed = |k: &Candidate, p: &Partial| k.persists || (k.late && level_agrees(k, p));
    // The fundamental (see FUNDAMENTAL_WITHIN_DB); the rest pitch when nothing is believed.
    let most = followed
        .iter()
        .filter(|(k, p)| believed(k, p))
        .map(|(_, p)| p)
        .max_by(|a, b| a.energy_db.total_cmp(&b.energy_db));
    let f1 = most.map_or(f1, |m| {
        followed
            .iter()
            .filter(|(k, p)| {
                !sustained
                    && believed(k, p)
                    && p.hz <= m.hz * FUNDAMENTAL_BELOW
                    && p.hz >= m.hz * FUNDAMENTAL_FAR
                    && p.energy_db >= m.energy_db - FUNDAMENTAL_WITHIN_DB
            })
            .map(|(_, p)| p.hz)
            .fold(m.hz, f64::min)
    });
    // The caller's note, where a partial answers to it.
    let semitone = 2f64.powf(1.0 / 12.0);
    let answering = expect.and_then(|e| {
        followed
            .iter()
            .filter(|(k, p)| {
                p.hz >= e / semitone && p.hz <= e * semitone && (believed(k, p) || verified(k, p))
            })
            .map(|(_, p)| p)
            .max_by(|a, b| a.energy_db.total_cmp(&b.energy_db))
            .map(|p| p.hz)
    });
    let f1 = answering.unwrap_or(f1);
    let unanswered = expect.filter(|_| answering.is_none());
    // A string's fundamental is its series' first harmonic, where the series holds it: a low piano
    // string's second or third harmonic can outweigh its first. The series must hold the note's own
    // fundamental among its harmonics: a xylophone's dense field of upper partials holds six of eight
    // harmonics of a pitch above its fundamental, and a string's fundamental is its lowest partial.
    let series = harmonic_series(
        &followed
            .iter()
            .filter(|(k, p)| believed(k, p) || verified(k, p))
            .map(|(_, p)| (p.hz, p.strike_db))
            .collect::<Vec<_>>(),
    )
    .filter(|s| s.harmonics.iter().any(|h| (h.1 - f1).abs() < 1e-9))
    // …and either hold SERIES_LONG harmonics or a first harmonic within FUNDAMENTAL_WITHIN_DB of the
    // most energetic partial's energy: frame lines under a marimba's D4 at pp formed a short series of
    // their own around it from a faint line, while a piano's lowest C holds 27 harmonics over a
    // fundamental more than 30 dB down.
    // …and not be the series an octave up with a faint odd half: an overblown flute's A5 carries its
    // A4 fingering's resonances 20–30 dB down, which with A5's own harmonics make a long series at
    // A4 whose odd harmonics all sit far under its even ones.
    .filter(|s| {
        let mean = |odd: bool| {
            let v: Vec<f64> = s
                .harmonics
                .iter()
                .filter(|h| h.0 <= 2 * SERIES_FIRST && (h.0 % 2 == 1) == odd)
                .map(|h| h.2)
                .collect();
            (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
        };
        !matches!((mean(true), mean(false)), (Some(o), Some(e)) if o < e - SERIES_ODD_UNDER_DB)
    })
    .filter(|s| {
        if s.harmonics.len() >= SERIES_LONG {
            return true;
        }
        let most = followed
            .iter()
            .map(|(_, p)| p.energy_db)
            .fold(f64::NEG_INFINITY, f64::max);
        s.harmonics.first().is_some_and(|h| {
            followed
                .iter()
                .any(|(_, p)| p.hz == h.1 && p.energy_db >= most - FUNDAMENTAL_WITHIN_DB)
        })
    });
    let first_harmonic = series
        .as_ref()
        .and_then(|s| s.harmonics.first().filter(|h| h.0 == 1).map(|h| h.1));
    let f1 = if answering.is_some() {
        f1
    } else {
        first_harmonic.unwrap_or(f1)
    };
    let all: Vec<Partial> = followed
        .into_iter()
        .filter(|(k, p)| {
            believed(k, p) || (p.hz > f1 * (1.0 + MERGE_SHARE) && verified(k, p)) || p.hz == f1
        })
        .map(|(_, p)| p)
        .collect();
    let fundamental = all
        .iter()
        .find(|p| (p.hz / f1 - 1.0).abs() <= 1e-9)
        .cloned();
    let f1_db = fundamental.as_ref().map(|p| p.strike_db);
    let strongest = all
        .iter()
        .map(|p| p.strike_db)
        .fold(f64::NEG_INFINITY, f64::max);
    // The note's partials: the fundamental and what lies above it.
    let above: Vec<&Partial> = all
        .iter()
        .filter(|p| p.hz >= f1 * (1.0 - MERGE_SHARE))
        .collect();
    let within = |db: f64| {
        above
            .iter()
            .copied()
            .filter(move |p| p.strike_db >= strongest - db)
    };
    let law_points: Vec<(f64, f64)> = within(LAW_WITHIN_DB)
        .filter_map(|p| p.t60_s.map(|t| ((p.hz / f1).ln(), t.ln())))
        .collect();
    let decay_law = (law_points.len() >= LAW_MIN_PARTIALS)
        .then(|| line(&law_points).map(|l| l.0))
        .flatten();
    let tilt_points: Vec<(f64, f64)> = within(LAW_WITHIN_DB)
        .map(|p| ((p.hz / f1).log2(), p.strike_db))
        .collect();
    let tilt = (tilt_points.len() >= LAW_MIN_PARTIALS)
        .then(|| line(&tilt_points).map(|l| l.0))
        .flatten();
    let upper: Vec<(f64, f64)> = within(COUNT_WITHIN_DB)
        .filter(|p| p.hz > f1 * (1.0 + MERGE_SHARE))
        .map(|p| (p.hz / f1, 10f64.powf(p.strike_db / 10.0)))
        .collect();
    let objects = objects::all();
    let best = objects::fits(&upper).into_iter().next();
    let count = within(COUNT_WITHIN_DB).count();
    let (name, cents) = nearest_note(f1).map_or((String::new(), None), |(n, c)| (n, Some(c)));
    let heard = virtual_pitch(
        &above
            .iter()
            .map(|p| (p.hz, p.strike_db))
            .collect::<Vec<_>>(),
    );
    let heard_as = heard.and_then(nearest_note);

    let mut readings = vec![
        Reading::new(
            "note.fundamental",
            "The fundamental",
            Some(f1),
            Unit::Hertz,
            src_tuning,
        ),
        Reading::new(
            "note.tuning",
            "Pitch against equal temperament (A = 440 Hz)",
            cents,
            Unit::Cents,
            src_tuning,
        ),
        Reading::new(
            "note.t60",
            "The fundamental's decay (T60)",
            fundamental.as_ref().and_then(|p| p.t60_s),
            Unit::Seconds,
            src_t60,
        ),
    ];
    readings.push(match decay_law {
        Some(m) => Reading::new(
            "note.decay_law",
            "Decay against frequency, M in T60 ∝ (f/f₁)^M",
            Some(m),
            Unit::Plain,
            src_law,
        ),
        None => Reading::absent(
            "note.decay_law",
            "Decay against frequency, M in T60 ∝ (f/f₁)^M",
            Unit::Plain,
            "fewer than three partials ring long enough to fit",
            src_law,
        ),
    });
    readings.push(match tilt {
        Some(t) => Reading::new(
            "note.tilt",
            "Level against frequency at the strike",
            Some(t),
            Unit::DecibelsPerOctave,
            src_tilt,
        ),
        None => Reading::absent(
            "note.tilt",
            "Level against frequency at the strike",
            Unit::DecibelsPerOctave,
            "fewer than three partials",
            src_tilt,
        ),
    });
    readings.push(Reading::new(
        "note.partials",
        "Partials within 40 dB of the strongest",
        Some(count as f64),
        Unit::Plain,
        src_ideal,
    ));
    readings.push(match &best {
        Some(fit) => Reading::new(
            "note.ideal_cents",
            "The partials' distance from the nearest ideal object",
            Some(fit.cents),
            Unit::Cents,
            src_ideal,
        ),
        None => Reading::absent(
            "note.ideal_cents",
            "The partials' distance from the nearest ideal object",
            Unit::Cents,
            "no partial above the fundamental",
            src_ideal,
        ),
    });
    let src_virtual = "subharmonic summation over the partials (Hermes 1988), by presence and the dominance region: research:listening/percussion-perception.md §2.1, §2.5";
    readings.push(Reading::new(
        "note.virtual_pitch",
        "The pitch the ear takes (virtual pitch)",
        heard,
        Unit::Hertz,
        src_virtual,
    ));
    readings.push(Reading::new(
        "note.virtual_tuning",
        "The virtual pitch against equal temperament (A = 440 Hz)",
        heard_as.as_ref().map(|h| h.1),
        Unit::Cents,
        src_virtual,
    ));
    let src_stages = "two lines through the fundamental's energy decay curve (Schroeder), the breakpoint where they fit best, taken when they leave at most half one line's error and their decays differ 1.5-fold";
    let stage = fundamental.as_ref().and_then(|p| p.stages);
    readings.push(match stage {
        Some(st) => Reading::new(
            "note.two_stage",
            "The fundamental's second stage against its first (T60 ratio)",
            Some(st.late_t60_s / st.early_t60_s),
            Unit::Ratio,
            src_stages,
        ),
        None => Reading::absent(
            "note.two_stage",
            "The fundamental's second stage against its first (T60 ratio)",
            Unit::Ratio,
            "one stage",
            src_stages,
        ),
    });
    let damper = stage.filter(|st| st.early_t60_s >= DAMPER_RATIO * st.late_t60_s);
    readings.push(match damper {
        Some(st) => Reading::new(
            "note.release_at",
            "The damper comes down (note-off)",
            Some(st.knee_s),
            Unit::Seconds,
            src_stages,
        ),
        None => Reading::absent(
            "note.release_at",
            "The damper comes down (note-off)",
            Unit::Seconds,
            "no damper: no second stage three times faster",
            src_stages,
        ),
    });
    readings.push(match damper {
        Some(st) => Reading::new(
            "note.release_t60",
            "The decay under the damper (T60)",
            Some(st.late_t60_s),
            Unit::Seconds,
            src_stages,
        ),
        None => Reading::absent(
            "note.release_t60",
            "The decay under the damper (T60)",
            Unit::Seconds,
            "no damper: no second stage three times faster",
            src_stages,
        ),
    });
    let src_beat = "the fundamental's level over its fall, its line taken out, and the peak of its spectrum between 0.3 and 15 Hz";
    let beat = fundamental.as_ref().and_then(|p| p.beat);
    readings.push(Reading::new(
        "note.beat_rate",
        "The fundamental's beating (two polarisations, a bar and its resonator)",
        beat.map(|b| b.0),
        Unit::Hertz,
        src_beat,
    ));
    readings.push(Reading::new(
        "note.beat_depth",
        "The fundamental's beating, peak swing",
        beat.map(|b| b.1),
        Unit::Decibels,
        src_beat,
    ));
    let (onsets, spread) = strum(c);
    let src_strum = "spectral-flux peaks of at least 0.15 of the largest within 400 ms of the first (Bello et al. 2005)";
    readings.push(Reading::new(
        "note.onsets",
        "Onsets at the start (several for a strum)",
        Some(onsets as f64),
        Unit::Plain,
        src_strum,
    ));
    readings.push(Reading::new(
        "note.onset_spread",
        "From the first onset to the last",
        (onsets > 1).then_some(spread),
        Unit::Milliseconds,
        src_strum,
    ));
    let src_series = "the stiff-string law f_n = n·f0·√(1 + B·n²) fitted to the harmonics (research:listening/percussion-perception.md §5.1)";
    let src_comb = "the excitation position's comb |sin(nπβ)| matched to the harmonics' levels, tilt removed (research:listening/percussion-perception.md §6)";
    let not_series = "not a harmonic series: fewer than six of its first eight harmonics";
    readings.push(match &series {
        Some(s) => Reading::new(
            "note.harmonics",
            "Harmonics found (a string's or a tube's series)",
            Some(s.harmonics.len() as f64),
            Unit::Plain,
            src_series,
        ),
        None => Reading::absent(
            "note.harmonics",
            "Harmonics found (a string's or a tube's series)",
            Unit::Plain,
            not_series,
            src_series,
        ),
    });
    readings.push(match &series {
        Some(s) => Reading::new(
            "note.inharmonicity",
            "Inharmonicity, B in f_n = n·f0·√(1 + B·n²)",
            Some(s.b),
            Unit::Plain,
            src_series,
        ),
        None => Reading::absent(
            "note.inharmonicity",
            "Inharmonicity, B in f_n = n·f0·√(1 + B·n²)",
            Unit::Plain,
            not_series,
            src_series,
        ),
    });
    // A pickup's harmonics (Lounge Lizard's pickup: its nonlinearity and its symmetry): the second and
    // third harmonics against the first, and the even harmonics against the odd, 2 to 7.
    let src_pickup = "the series' harmonic levels at the strike: the second and third against the first, and the mean of the even (2, 4, 6) against the odd (3, 5, 7)";
    let level_of = |n: usize| {
        series
            .as_ref()
            .and_then(|s| s.harmonics.iter().find(|h| h.0 == n).map(|h| h.2))
    };
    let against_first = |n: usize| level_of(n).zip(level_of(1)).map(|(a, b)| a - b);
    let mean = |ns: &[usize]| {
        let v: Vec<f64> = ns.iter().filter_map(|&n| level_of(n)).collect();
        (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
    };
    readings.push(Reading::new(
        "note.h2",
        "The second harmonic against the first",
        against_first(2),
        Unit::Decibels,
        src_pickup,
    ));
    readings.push(Reading::new(
        "note.h3",
        "The third harmonic against the first",
        against_first(3),
        Unit::Decibels,
        src_pickup,
    ));
    readings.push(Reading::new(
        "note.even_odd",
        "Even harmonics against odd",
        mean(&[2, 4, 6]).zip(mean(&[3, 5, 7])).map(|(e, o)| e - o),
        Unit::Decibels,
        src_pickup,
    ));
    let src_deficit = "the harmonics among the first twelve (up to the series' last) missing, or 15 dB under a line through the series' strike levels against log₂ n, refitted without those 6 dB under it (the plan's §2 item 3)";
    let deficit = series.as_ref().map(harmonic_deficit);
    readings.push(Reading::new(
        "note.deficit",
        "Harmonics missing from the series, or sunk under it",
        deficit.as_ref().map(|d| d.len() as f64),
        Unit::Plain,
        src_deficit,
    ));
    let position = series.as_ref().and_then(excitation_position);
    readings.push(match position {
        Some(beta) => Reading::new(
            "note.excitation_position",
            "Where it was plucked or struck, as a share of the length",
            Some(beta),
            Unit::Plain,
            src_comb,
        ),
        None => Reading::absent(
            "note.excitation_position",
            "Where it was plucked or struck, as a share of the length",
            Unit::Plain,
            if series.is_some() {
                "no comb in the harmonics' levels"
            } else {
                not_series
            },
            src_comb,
        ),
    });
    readings.push(match fundamental.as_ref().and_then(|p| p.split_hz) {
        Some(s) => Reading::new(
            "note.split",
            "The fundamental split into two lines",
            Some(s.abs()),
            Unit::Hertz,
            src_split,
        ),
        None => Reading::absent(
            "note.split",
            "The fundamental split into two lines",
            Unit::Hertz,
            "one line",
            src_split,
        ),
    });

    let object = best.as_ref().map(|f| &objects[f.object]);
    let title = format!(
        "Partials of {name} {}{}{}",
        cents.map_or(String::new(), |c| format!("{c:+.0} c")),
        match &heard_as {
            Some((n, c)) if *n != name => format!(", heard as {n} {c:+.0} c"),
            _ => String::new(),
        } + &unanswered
            .and_then(nearest_note)
            .map_or(String::new(), |(n, _)| {
                format!(", and no partial within a semitone of the expected {n}")
            }),
        match (object, &best) {
            (Some(o), Some(f)) if f.score <= objects::FIT_CENTS => format!(
                ", like the {} ({:.0} c off, {:.0} % of its ratios in range unmet)",
                o.name,
                f.cents,
                100.0 * f.empty
            ),
            (Some(o), Some(f)) => format!(
                ", like no ideal object (the nearest, the {}, {:.0} c off, {:.0} % of its ratios in range unmet)",
                o.name,
                f.cents,
                100.0 * f.empty
            ),
            _ => String::new(),
        }
    );
    let rows = all
        .iter()
        .map(|p| {
            // Lines below the fundamental (a neighbouring bar ringing in sympathy, the room) have
            // no place in the object.
            let ideal = object
                .filter(|_| p.hz >= f1 * (1.0 - MERGE_SHARE))
                .and_then(|o| objects::nearest_ratio(o, p.hz / f1));
            vec![
                Some(p.hz),
                Some(p.hz / f1),
                f1_db.map(|f| p.strike_db - f),
                p.t60_s,
                Some(p.fall_db),
                p.split_hz,
                ideal.map(|i| i.0),
                ideal.map(|i| i.1),
                p.stages.map(|st| st.knee_s),
                p.stages.map(|st| st.late_t60_s),
            ]
        })
        .collect();
    let mut section = Section::new("note", "The note: partials, material and tone", readings);
    if let Some(d) = deficit.as_ref().filter(|d| !d.is_empty()) {
        section.tables.push(Table {
            id: "note.deficit",
            title: "The harmonic deficit: harmonics missing, or sunk under the series' line".into(),
            columns: vec![
                ("Harmonic", Unit::Plain),
                ("Frequency", Unit::Hertz),
                ("Under the line", Unit::Decibels),
            ],
            rows: d
                .iter()
                .map(|&(n, hz, under)| vec![Some(n as f64), Some(hz), under])
                .collect(),
            window_ms: None,
            source: src_deficit,
        });
    }
    section.tables.push(Table {
        id: "note.partials",
        title,
        columns: vec![
            ("Frequency", Unit::Hertz),
            ("Ratio to the fundamental", Unit::Ratio),
            ("Strike level against the fundamental", Unit::Decibels),
            ("T60", Unit::Seconds),
            ("Fall fitted", Unit::Decibels),
            ("Split", Unit::Hertz),
            ("Nearest ideal ratio", Unit::Ratio),
            ("Off it", Unit::Cents),
            ("Second stage from", Unit::Seconds),
            ("Its T60", Unit::Seconds),
        ],
        rows,
        window_ms: None,
        source: src_t60,
    });
    section
}
