//! An impulse response: how a system answers one sample. Its frequency response (gain, a filter's
//! peak, Q and edges), its taps and repeats (a delay's time, feedback and darkening), and, where it
//! decays like a space, the descriptors `mxm-classic-verb-fit` reads (the plan's §2 items 5 and 9).
//!
//! An impulse response comes from a recorded response file (`describe --family impulse`) or from
//! `respond`, which deconvolves a response to an impulse, a sweep or noise by its stimulus.

use crate::reading::{Reading, Resolution, Section, Table, Unit};
use crate::repr::envelope;

/// The magnitude is smoothed over this fraction of an octave around each point of its grid, and the
/// grid is spaced the same.
pub const SMOOTH_OCTAVES: f64 = 1.0 / 24.0;
/// A filter's edges and its peak's bandwidth are read this far below their reference, dB.
pub const EDGE_DB: f64 = 3.0;
/// An end of the band whose first octave moves less than this is a passband, dB; where neither end is
/// one, the response is a band-pass and its edges are read from its peak.
pub const FLAT_DB: f64 = 1.5;
/// A peak less than this over the ends is no peak, dB.
pub const PEAK_MIN_DB: f64 = 1.0;
/// A tap is a peak of the response's energy (over ±0.25 ms) that is the largest within this many ms…
pub const TAP_APART_MS: f64 = 2.0;
/// …no more than this far below the strongest, dB…
pub const TAP_WITHIN_DB: f64 = 50.0;
/// …and this many times the median energy within ±20 ms: it stands out of what is around it, which a
/// dense reverberant tail's peaks do not.
pub const TAP_OVER_MEDIAN: f64 = 20.0;
/// Repeats are taps spaced evenly, each spacing within this share of their median.
pub const REPEAT_SPACING_SHARE: f64 = 0.05;

/// A magnitude response on a logarithmic grid.
#[derive(Clone, Debug, PartialEq)]
pub struct Magnitude {
    pub hz: Vec<f64>,
    pub db: Vec<f64>,
}

impl Magnitude {
    /// The level at `hz`, interpolated along the grid in log frequency; `None` outside it.
    #[must_use]
    pub fn at(&self, hz: f64) -> Option<f64> {
        let i = self.hz.iter().position(|&f| f >= hz)?;
        if i == 0 {
            return (self.hz[0] - hz).abs().lt(&1e-9).then_some(self.db[0]);
        }
        let (f0, f1) = (self.hz[i - 1].ln(), self.hz[i].ln());
        let t = (hz.ln() - f0) / (f1 - f0);
        Some(self.db[i - 1] + t * (self.db[i] - self.db[i - 1]))
    }
}

/// The magnitude of a one-sided power spectrum `power[k]` (bins `bin_hz` apart) on a
/// [`SMOOTH_OCTAVES`] grid from `lo_hz` to `hi_hz`, each point the mean power over its span, dB.
#[must_use]
pub fn smooth(power: &[f64], bin_hz: f64, lo_hz: f64, hi_hz: f64) -> Option<Magnitude> {
    if power.is_empty() || bin_hz <= 0.0 || lo_hz <= 0.0 || hi_hz <= lo_hz {
        return None;
    }
    let steps = ((hi_hz / lo_hz).log2() / SMOOTH_OCTAVES).floor() as usize;
    let half = 2f64.powf(SMOOTH_OCTAVES / 2.0);
    let (mut hz, mut db) = (Vec::new(), Vec::new());
    for s in 0..=steps {
        let f = lo_hz * 2f64.powf(s as f64 * SMOOTH_OCTAVES);
        let a = ((f / half / bin_hz).ceil() as usize).min(power.len() - 1);
        let b = ((f * half / bin_hz).floor() as usize).min(power.len() - 1);
        let p = if b >= a {
            power[a..=b].iter().sum::<f64>() / (b - a + 1) as f64
        } else {
            power[((f / bin_hz).round() as usize).min(power.len() - 1)]
        };
        if p > 0.0 && p.is_finite() {
            hz.push(f);
            db.push(10.0 * p.log10());
        }
    }
    (hz.len() > 2).then_some(Magnitude { hz, db })
}

/// The magnitude response of the impulse response `h`, over at least a second of spectrum.
#[must_use]
pub fn magnitude(h: &[f64], rate: f64, lo_hz: f64, hi_hz: f64) -> Option<Magnitude> {
    let n = h.len().max(rate as usize).next_power_of_two();
    let mut re = vec![0.0; n];
    let mut im = vec![0.0; n];
    re[..h.len()].copy_from_slice(h);
    mxm_measure::spectrum::fft(&mut re, &mut im)?;
    let power: Vec<f64> = (0..=n / 2).map(|k| re[k] * re[k] + im[k] * im[k]).collect();
    smooth(&power, rate / n as f64, lo_hz, hi_hz)
}

/// Where the grid crosses `level` walking from point `from` in steps of `dir` (±1), interpolated in log
/// frequency; `None` if it never does.
fn crossing(m: &Magnitude, from: usize, dir: isize, level: f64, above: bool) -> Option<f64> {
    let mut i = from as isize;
    loop {
        let j = i + dir;
        if j < 0 || j as usize >= m.hz.len() {
            return None;
        }
        let (a, b) = (m.db[i as usize], m.db[j as usize]);
        let crossed = if above { b >= level } else { b < level };
        if crossed {
            let t = ((level - a) / (b - a)).clamp(0.0, 1.0);
            let (fa, fb) = (m.hz[i as usize].ln(), m.hz[j as usize].ln());
            return Some((fa + t * (fb - fa)).exp());
        }
        i = j;
    }
}

/// The readings of a frequency response: gain at 1 kHz, the peak, its Q, and the edges, with the
/// third-octave levels as a table. `latency_ms` is reported when given.
#[must_use]
pub fn response_section(m: &Magnitude, latency_ms: Option<f64>, source: &'static str) -> Section {
    let n = m.hz.len();
    let (lo, hi) = (m.hz[0], m.hz[n - 1]);
    let mean_over = |a: f64, b: f64| {
        let v: Vec<f64> = (0..n)
            .filter(|&i| m.hz[i] >= a && m.hz[i] <= b)
            .map(|i| m.db[i])
            .collect();
        (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
    };
    let low_end = mean_over(lo, 2.0 * lo).unwrap_or(m.db[0]);
    let high_end = mean_over(hi / 2.0, hi).unwrap_or(m.db[n - 1]);
    // An end is a passband where its first octave is flat, and the passband's level is what the edges
    // are read against. Where neither end is flat the response is a band-pass, read from its peak.
    let flat = |a: f64, b: f64| {
        m.at(a)
            .zip(m.at(b))
            .is_some_and(|(x, y)| (x - y).abs() < FLAT_DB)
    };
    let (low_flat, high_flat) = (flat(lo, 2.0 * lo), flat(hi / 2.0, m.hz[n - 1]));
    let ends = low_end.max(high_end);
    // The peak, away from the band's ends.
    let peak = (0..n)
        .filter(|&i| m.hz[i] >= lo * 2f64.sqrt() && m.hz[i] <= hi / 2f64.sqrt())
        .max_by(|&a, &b| m.db[a].total_cmp(&m.db[b]));
    let passband = match (low_flat, high_flat) {
        (true, true) => Some(ends),
        (true, false) => Some(low_end),
        (false, true) => Some(high_end),
        (false, false) => None,
    };
    let peak_over = peak.map(|i| m.db[i] - passband.unwrap_or(ends));
    let has_peak = peak_over.is_some_and(|d| d >= PEAK_MIN_DB);
    let reference = passband.or(peak.map(|i| m.db[i])).unwrap_or(ends);
    let q = peak.filter(|_| has_peak).and_then(|i| {
        let level = m.db[i] - EDGE_DB;
        let a = crossing(m, i, -1, level, false)?;
        let b = crossing(m, i, 1, level, false)?;
        Some(m.hz[i] / (b - a))
    });
    let low_edge = (low_end < reference - EDGE_DB)
        .then(|| crossing(m, 0, 1, reference - EDGE_DB, true))
        .flatten();
    let high_edge = (high_end < reference - EDGE_DB)
        .then(|| crossing(m, n - 1, -1, reference - EDGE_DB, true))
        .flatten();
    let resolution = Resolution {
        window_ms: 0.0,
        bin_hz: None,
        span: Some(SMOOTH_OCTAVES / 2.0),
    };
    let band = |r: Reading| r.band(lo, hi).resolution(resolution);
    let mut readings = Vec::new();
    if let Some(ms) = latency_ms {
        readings.push(Reading::new(
            "response.latency",
            "Latency: the peak of the response's first arrival",
            Some(ms),
            Unit::Milliseconds,
            source,
        ));
    }
    readings.extend([
        band(Reading::new(
            "response.gain",
            "Gain at 1 kHz",
            m.at(1000.0),
            Unit::Decibels,
            source,
        )),
        band(Reading::new(
            "response.peak_hz",
            "The response's peak",
            peak.filter(|_| has_peak).map(|i| m.hz[i]),
            Unit::Hertz,
            source,
        )),
        band(Reading::new(
            "response.peak_db",
            "The peak over the passband (a band-pass's: over the louder end)",
            peak_over.filter(|_| has_peak),
            Unit::Decibels,
            source,
        )),
        band(Reading::new(
            "response.q",
            "The peak's Q: its frequency over its width 3 dB down",
            q,
            Unit::Plain,
            source,
        )),
        band(Reading::new(
            "response.low_edge",
            "Low edge: 3 dB under the passband",
            low_edge,
            Unit::Hertz,
            source,
        )),
        band(Reading::new(
            "response.high_edge",
            "High edge: 3 dB under the passband",
            high_edge,
            Unit::Hertz,
            source,
        )),
    ]);
    let mut section = Section::new("response", "Frequency response", readings);
    let mut rows = Vec::new();
    let mut centre = 25.0f64;
    while centre <= 16_000.0 {
        if centre >= lo && centre <= hi {
            rows.push(vec![Some(centre), m.at(centre)]);
        }
        centre *= 2f64.powf(1.0 / 3.0);
    }
    section.tables.push(Table {
        id: "response.thirds",
        title: "The response at third-octave centres (smoothed over 1/24 octave)".into(),
        columns: vec![("Centre", Unit::Hertz), ("Level", Unit::Decibels)],
        rows,
        window_ms: None,
        source,
    });
    section
}

/// The response's latency, from sample `origin` (lag zero), ms: the peak of its first arrival — the
/// largest sample within 0.5 ms of the first one within 20 dB of the response's largest. A band-limited
/// arrival rings a few samples ahead of its peak, and the first sample alone read a plain wire 0.06 ms
/// early.
#[must_use]
pub fn latency_ms(h: &[f64], rate: f64, origin: usize) -> Option<f64> {
    let top = h.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    (top > 0.0).then(|| {
        let first = h.iter().position(|v| v.abs() >= 0.1 * top).unwrap_or(0);
        let end = (first + (0.0005 * rate) as usize + 1).min(h.len());
        let at = (first..end)
            .max_by(|&a, &b| h[a].abs().total_cmp(&h[b].abs()))
            .unwrap_or(first);
        (at as f64 - origin as f64) / rate * 1000.0
    })
}

/// One tap: time from the response's start, ms; energy against the strongest tap, dB; and the energy
/// above 2 kHz against the energy below, dB.
type Tap = (f64, f64, Option<f64>);

/// The response's taps (see [`TAP_APART_MS`]).
fn taps(h: &[f64], rate: f64) -> Vec<Tap> {
    let n = h.len();
    if n < 8 {
        return Vec::new();
    }
    let half = ((0.000_25 * rate) as usize).max(1);
    let mut prefix = vec![0.0; n + 1];
    for (i, v) in h.iter().enumerate() {
        prefix[i + 1] = prefix[i] + v * v;
    }
    let energy: Vec<f64> = (0..n)
        .map(|i| prefix[(i + half + 1).min(n)] - prefix[i.saturating_sub(half)])
        .collect();
    let top = energy.iter().copied().fold(0.0, f64::max);
    if top <= 0.0 {
        return Vec::new();
    }
    let apart = ((TAP_APART_MS / 1000.0 * rate) as usize).max(1);
    let around = (0.02 * rate) as usize;
    let floor = top * 10f64.powf(-TAP_WITHIN_DB / 10.0);
    let mut found: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < n {
        let e = energy[i];
        if e >= floor && (i == 0 || energy[i - 1] < e) {
            let (a, b) = (i.saturating_sub(apart), (i + apart + 1).min(n));
            if energy[a..b].iter().all(|&v| v <= e) {
                let (c, d) = (i.saturating_sub(around), (i + around + 1).min(n));
                let mut near: Vec<f64> = energy[c..d].to_vec();
                near.sort_by(f64::total_cmp);
                if e >= TAP_OVER_MEDIAN * near[near.len() / 2] {
                    // The tap's own sample: the largest within the energy window.
                    let (p, q) = (i.saturating_sub(half), (i + half + 1).min(n));
                    let at = (p..q)
                        .max_by(|&x, &y| h[x].abs().total_cmp(&h[y].abs()))
                        .unwrap_or(i);
                    found.push(at);
                    i += apart;
                    continue;
                }
            }
        }
        i += 1;
    }
    found.dedup();
    let spacing = found.windows(2).map(|w| w[1] - w[0]).min().unwrap_or(n);
    let reach = (spacing / 2).min((0.01 * rate) as usize).max(16);
    found
        .iter()
        .map(|&at| {
            let e = energy[at];
            (
                at as f64 / rate * 1000.0,
                10.0 * (e / top).log10(),
                tilt(h, at, reach, rate),
            )
        })
        .collect()
}

/// The energy above 2 kHz against the energy below it, around sample `at` (±`reach`, Hann), dB.
fn tilt(h: &[f64], at: usize, reach: usize, rate: f64) -> Option<f64> {
    if rate < 8000.0 {
        return None;
    }
    let a = at.saturating_sub(reach);
    let b = (at + reach).min(h.len());
    let seg = &h[a..b];
    let s = crate::repr::spectrum::power_spectrum(seg, rate, 4096)?;
    let low: f64 = s.bins(100.0, 2000.0).map(|k| s.power[k]).sum();
    let high: f64 = s
        .bins(2000.0, (0.45 * rate).min(16_000.0))
        .map(|k| s.power[k])
        .sum();
    (low > 0.0 && high > 0.0).then(|| 10.0 * (high / low).log10())
}

/// The longest run of evenly spaced taps (each spacing within [`REPEAT_SPACING_SHARE`] of the run's
/// first), as indices into `taps`; empty for fewer than three taps in a run.
fn repeats(taps: &[Tap]) -> std::ops::Range<usize> {
    let mut best = 0..0;
    for start in 0..taps.len().saturating_sub(2) {
        let d = taps[start + 1].0 - taps[start].0;
        if d <= 0.0 {
            continue;
        }
        let mut end = start + 2;
        while end < taps.len() {
            let s = taps[end].0 - taps[end - 1].0;
            if (s - d).abs() > REPEAT_SPACING_SHARE * d {
                break;
            }
            end += 1;
        }
        if end - start >= 3 && end - start > best.len() {
            best = start..end;
        }
    }
    best
}

/// The delay section: the taps, and where they repeat evenly, the delay time, the feedback per repeat
/// and how each repeat darkens. Times are from sample `origin`, lag zero.
#[must_use]
pub fn delay_section(h: &[f64], rate: f64, origin: usize) -> Section {
    let src = "peaks of the impulse response's energy over ±0.25 ms, each the largest within 2 ms, within 50 dB of the strongest and 20 times the median within ±20 ms";
    let shift = origin as f64 / rate * 1000.0;
    let found: Vec<Tap> = taps(h, rate)
        .into_iter()
        .map(|t| (t.0 - shift, t.1, t.2))
        .collect();
    let run = repeats(&found);
    let run_taps = &found[run.clone()];
    let spacing = (run.len() >= 3).then(|| {
        let mut s: Vec<f64> = run_taps.windows(2).map(|w| w[1].0 - w[0].0).collect();
        s.sort_by(f64::total_cmp);
        s[s.len() / 2]
    });
    let per_repeat = |f: &dyn Fn(&Tap) -> Option<f64>| {
        let points: Vec<(f64, f64)> = run_taps
            .iter()
            .enumerate()
            .filter_map(|(k, t)| f(t).map(|v| (k as f64, v)))
            .collect();
        (points.len() >= 3)
            .then(|| envelope::line_fit(&points).map(|l| l.0))
            .flatten()
    };
    let feedback = per_repeat(&|t| Some(t.1));
    let darkening = per_repeat(&|t| t.2);
    let readings = vec![
        Reading::new(
            "delay.taps",
            "Taps: discrete peaks standing out of the response",
            Some(found.len() as f64),
            Unit::Plain,
            src,
        ),
        Reading::new(
            "delay.first",
            "The first tap",
            found.first().map(|t| t.0),
            Unit::Milliseconds,
            src,
        ),
        Reading::new(
            "delay.time",
            "Delay time: the spacing of the evenly spaced repeats",
            spacing,
            Unit::Milliseconds,
            src,
        ),
        Reading::new(
            "delay.repeats",
            "Evenly spaced repeats",
            (run.len() >= 3).then_some(run.len() as f64),
            Unit::Plain,
            src,
        ),
        Reading::new(
            "delay.feedback",
            "Feedback: each repeat's level against the one before (a line through them)",
            feedback,
            Unit::Decibels,
            src,
        ),
        Reading::new(
            "delay.darkening",
            "Each repeat's change in the energy above 2 kHz against below",
            darkening,
            Unit::Decibels,
            src,
        ),
    ];
    let mut section = Section::new("delay", "Taps and repeats", readings);
    section.tables.push(Table {
        id: "delay.taps",
        title: "Taps".into(),
        columns: vec![
            ("Time", Unit::Milliseconds),
            ("Level", Unit::Decibels),
            ("Above 2 kHz against below", Unit::Decibels),
        ],
        rows: found
            .iter()
            .map(|t| vec![Some(t.0), Some(t.1), t.2])
            .collect(),
        window_ms: None,
        source: src,
    });
    section
}

/// The space section from `mxm-classic-verb-fit`'s analysis of one or two channels, or why it refused.
///
/// # Errors
/// The analysis's refusal, as text.
pub fn space_section(channels: &[Vec<f64>], rate: f64) -> Result<Section, String> {
    let owned: Vec<Vec<f32>> = channels
        .iter()
        .map(|c| c.iter().map(|&v| v as f32).collect())
        .collect();
    let slices: Vec<&[f32]> = owned.iter().map(Vec::as_slice).collect();
    let a = mxm_classic_verb_fit::analyse(&slices, rate as f32).map_err(|e| e.to_string())?;
    let src = "mxm-classic-verb-fit: Lundeby's method and Schroeder integration per octave band (research:effects/feedback-delay-network-reverb.md §9)";
    let mut readings = Vec::new();
    for band in &a.bands {
        let c = f64::from(band.centre_hz);
        let (lo, hi) = (c / 2f64.sqrt(), c * 2f64.sqrt());
        let d = band.decay();
        readings.push(
            Reading::new(
                "space.t30",
                "Reverberation time (T30, or T20 where the decay does not reach −35 dB)",
                d.and_then(|d| d.t30_s.or(d.t20_s)).map(f64::from),
                Unit::Seconds,
                src,
            )
            .band(lo, hi),
        );
        readings.push(
            Reading::new(
                "space.edt",
                "Early decay time",
                d.and_then(|d| d.edt_s).map(f64::from),
                Unit::Seconds,
                src,
            )
            .band(lo, hi),
        );
    }
    readings.extend([
        Reading::new(
            "space.pre_delay",
            "Pre-delay: the direct sound to the first reflection",
            a.pre_delay_s.map(|s| f64::from(s) * 1000.0),
            Unit::Milliseconds,
            src,
        ),
        Reading::new(
            "space.drr",
            "Direct-to-reverberant ratio",
            a.onset.drr_db.map(f64::from),
            Unit::Decibels,
            src,
        ),
        Reading::new(
            "space.mixing_time",
            "Mixing time: where the echo density reaches a Gaussian field's",
            a.echo_density.mixing_time_s.map(|s| f64::from(s) * 1000.0),
            Unit::Milliseconds,
            src,
        ),
        Reading::new(
            "space.iacc",
            "Late-field interchannel cross-correlation",
            a.width.measured().map(|c| f64::from(c.iacc)),
            Unit::Plain,
            src,
        ),
        Reading::new(
            "space.confidence",
            "The analysis's confidence (0 to 1)",
            Some(f64::from(a.validity.confidence)),
            Unit::Plain,
            src,
        ),
    ]);
    Ok(Section::new(
        "space",
        "Space (as a reverberation)",
        readings,
    ))
}

/// Everything an impulse response says, channel by channel where it has two: the response and taps
/// of their mean, and the space of both. Sample `origin` is lag zero. Notes say why the space was not
/// read.
#[must_use]
pub fn read(
    channels: &[Vec<f64>],
    rate: f64,
    band_hz: (f64, f64),
    origin: usize,
) -> (Vec<Section>, Vec<String>) {
    let mut notes = Vec::new();
    let Some(first) = channels.first() else {
        return (Vec::new(), notes);
    };
    let mono: Vec<f64> = (0..first.len())
        .map(|i| {
            channels
                .iter()
                .map(|c| c.get(i).copied().unwrap_or(0.0))
                .sum::<f64>()
                / channels.len() as f64
        })
        .collect();
    let src = "the impulse response's spectrum over at least a second, smoothed over 1/24 octave";
    let mut sections = Vec::new();
    match magnitude(&mono, rate, band_hz.0, band_hz.1) {
        Some(m) => sections.push(response_section(&m, latency_ms(&mono, rate, origin), src)),
        None => notes.push("the response is silent: no frequency response".into()),
    }
    sections.push(delay_section(&mono, rate, origin));
    match space_section(channels, rate) {
        Ok(s) => sections.push(s),
        Err(why) => notes.push(format!("not read as a space: {why}")),
    }
    (sections, notes)
}
