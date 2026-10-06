//! What an effect does to a stimulus it was played (the plan's §2 item 9): distortion by order and
//! DC on a steady sine, the sine's pitch and level wobble, intermodulation on two sines, a
//! compressor's static curve and its attack and release on level steps, noise, runaway and
//! self-oscillation on silence, a chorus's or flanger's moving delay on noise, a gate's or a tail's
//! hold on bursts, and a stereo output's width, correlation and balance.
//!
//! Every reading here knows what was played — the stimulus is rendered again from its sidecar — so
//! nothing is guessed from the response alone: the tone's frequency, the step levels, where each burst
//! starts.

use crate::numeric::linalg::{Matrix, least_squares};
use crate::reading::{Reading, Section, Table, Unit};
use crate::repr::demod::Tracker;
use crate::repr::envelope;
use crate::repr::lines::Lines;
use crate::stimulus::{Kind, Stimulus};

/// The highest harmonic order read.
pub const MAX_ORDER: usize = 10;
/// A steady stretch leaves out this share of the signal at its start (at least 100 ms) and half as
/// much at its end, where a fade or a compressor's settling sits.
pub const SETTLE_SHARE: f64 = 0.1;
/// A tone's pitch and level wobble are sought between these rates, Hz.
pub const WOBBLE_HZ: (f64, f64) = (0.3, 20.0);
/// A moving delay's rate is sought between these rates, Hz.
pub const SWEEP_HZ: (f64, f64) = (0.05, 20.0);
/// The attack and release are the times the gain takes to go this share of the way, in dB, from its
/// level before a step to its settled level after — a one-pole gain in dB's time constant.
pub const TIME_CONSTANT_SHARE: f64 = 1.0 - 1.0 / std::f64::consts::E;
/// A gain moving less than this across a step has no attack or release to read, dB.
pub const MIN_GAIN_CHANGE_DB: f64 = 1.0;
/// A compressor's slope above its threshold is read as none within this of one.
pub const MIN_SLOPE_CHANGE: f64 = 0.03;
/// A line on silence stands this far over the median of the spectrum, dB.
pub const LINE_OVER_MEDIAN_DB: f64 = 20.0;
/// A burst's hangover is the time its response takes to fall this far below its level during the
/// burst, dB.
pub const HANGOVER_DB: f64 = 40.0;

/// The steady stretch of the signal, as sample indices into the response.
fn steady(s: &Stimulus, len: usize) -> Option<(usize, usize)> {
    let rate = f64::from(s.rate);
    let n = s.signal_len();
    let skip = ((SETTLE_SHARE * n as f64) as usize).max((0.1 * rate) as usize);
    let end_skip = ((SETTLE_SHARE / 2.0 * n as f64) as usize).max((0.05 * rate) as usize);
    let a = s.start() + skip;
    let b = (s.start() + n).saturating_sub(end_skip).min(len);
    (b > a + (0.1 * rate) as usize).then_some((a, b))
}

fn db(v: f64) -> Option<f64> {
    (v > 0.0 && v.is_finite()).then(|| 20.0 * v.log10())
}

/// The distortion section on a steady sine: gain, harmonics by order, THD and DC, and the tone's
/// wobble in pitch and level.
#[must_use]
pub fn sine_section(y: &[f64], s: &Stimulus, hz: f64) -> Section {
    let rate = f64::from(s.rate);
    let peak = 10f64.powf(s.level_dbfs / 20.0);
    let src = "the response's lines at the tone's frequency and its multiples, read through a four-term Blackman–Harris window over the steady stretch (Harris 1978)";
    let Some((a, b)) = steady(s, y.len()) else {
        return Section::new(
            "distortion",
            "Distortion",
            vec![Reading::absent(
                "effect.gain",
                "Gain at the tone",
                Unit::Decibels,
                "the steady stretch is under 100 ms",
                src,
            )],
        );
    };
    let seg = &y[a..b];
    let lines = Lines::new(seg.len());
    let a1 = lines.amplitude(seg, rate, hz).unwrap_or(0.0);
    let orders: Vec<(usize, f64)> = (2..=MAX_ORDER)
        .filter(|&k| (k as f64) * hz < 0.45 * rate)
        .filter_map(|k| lines.amplitude(seg, rate, k as f64 * hz).map(|v| (k, v)))
        .collect();
    let thd = (a1 > 0.0).then(|| orders.iter().map(|o| o.1 * o.1).sum::<f64>().sqrt() / a1 * 100.0);
    let order_db = |k: usize| {
        orders
            .iter()
            .find(|o| o.0 == k)
            .and_then(|o| db(o.1 / a1.max(f64::MIN_POSITIVE)))
    };
    let dc = lines
        .mean(seg)
        .and_then(|m| db(m.abs()).filter(|d| *d > -140.0));
    let window = |r: Reading| {
        r.window(
            (a - s.start()) as f64 / rate * 1000.0,
            (b - s.start()) as f64 / rate * 1000.0,
        )
    };
    let mut readings = vec![
        window(Reading::new(
            "effect.gain",
            "Gain at the tone",
            db(a1 / peak),
            Unit::Decibels,
            src,
        )),
        window(Reading::new(
            "effect.thd",
            "Total harmonic distortion (orders 2 to 10)",
            thd,
            Unit::Percent,
            src,
        )),
        window(Reading::new(
            "effect.h2",
            "The second harmonic against the tone",
            order_db(2),
            Unit::Decibels,
            src,
        )),
        window(Reading::new(
            "effect.h3",
            "The third harmonic against the tone",
            order_db(3),
            Unit::Decibels,
            src,
        )),
        window(Reading::new(
            "effect.dc",
            "DC",
            dc,
            Unit::DecibelsFullScale,
            src,
        )),
    ];
    readings.extend(wobble(seg, rate, hz));
    let mut section = Section::new(
        "distortion",
        "Distortion and wobble on a steady sine",
        readings,
    );
    section.tables.push(Table {
        id: "effect.orders",
        title: "Harmonics by order, against the tone".into(),
        columns: vec![
            ("Order", Unit::Plain),
            ("Frequency", Unit::Hertz),
            ("Level", Unit::Decibels),
        ],
        rows: orders
            .iter()
            .map(|&(k, v)| {
                vec![
                    Some(k as f64),
                    Some(k as f64 * hz),
                    db(v / a1.max(f64::MIN_POSITIVE)),
                ]
            })
            .collect(),
        window_ms: None,
        source: src,
    });
    section
}

/// The tone's pitch (cents) and level (dB) tracked every 5 ms: the band from half to one and a half
/// times the tone kept as an analytic signal, its carrier taken out.
fn tone_track(seg: &[f64], rate: f64, hz: f64) -> Vec<(f64, f64, f64)> {
    let n = (2 * seg.len()).next_power_of_two();
    let mut re = vec![0.0; n];
    let mut im = vec![0.0; n];
    re[..seg.len()].copy_from_slice(seg);
    if mxm_measure::spectrum::fft(&mut re, &mut im).is_none() {
        return Vec::new();
    }
    let bin = rate / n as f64;
    for k in 0..n {
        let f = k as f64 * bin;
        let keep = k <= n / 2 && f >= 0.5 * hz && f <= 1.5 * hz;
        let g = if keep { 2.0 } else { 0.0 };
        re[k] *= g;
        im[k] *= g;
    }
    if mxm_measure::spectrum::ifft(&mut re, &mut im).is_none() {
        return Vec::new();
    }
    let hop = ((0.005 * rate) as usize).max(1);
    let edge = seg.len() / 20;
    let w = std::f64::consts::TAU * hz / rate;
    let base = |i: usize| {
        let (s, c) = (w * i as f64).sin_cos();
        // z·e^(−iωn): the carrier taken out.
        (re[i] * c + im[i] * s, im[i] * c - re[i] * s)
    };
    let mut out = Vec::new();
    let mut i = edge;
    while i + hop < seg.len() - edge {
        let (p, q) = (base(i), base(i + hop));
        let dphi = mxm_measure::spectrum::princarg(q.1.atan2(q.0) - p.1.atan2(p.0));
        let f = hz + dphi * rate / (std::f64::consts::TAU * hop as f64);
        let level = re[i].hypot(im[i]);
        if f > 0.0 && level > 0.0 {
            out.push((
                i as f64 / rate,
                1200.0 * (f / hz).log2(),
                20.0 * level.log10(),
            ));
        }
        i += hop;
    }
    out
}

fn wobble(seg: &[f64], rate: f64, hz: f64) -> Vec<Reading> {
    let src = "the tone's band as an analytic signal every 5 ms, its line taken out and the peak of its spectrum between 0.3 and 20 Hz";
    let track = tone_track(seg, rate, hz);
    let cents: Vec<(f64, f64)> = track.iter().map(|p| (p.0, p.1)).collect();
    let level: Vec<(f64, f64)> = track.iter().map(|p| (p.0, p.2)).collect();
    let pitch = super::sustain::modulation(&cents, 0.005, WOBBLE_HZ).and_then(|m| m.1);
    let amp = super::sustain::modulation(&level, 0.005, WOBBLE_HZ).and_then(|m| m.1);
    vec![
        Reading::new(
            "effect.pitch_wobble_rate",
            "The tone's pitch wobble: rate",
            pitch.map(|p| p.0),
            Unit::Hertz,
            src,
        ),
        Reading::new(
            "effect.pitch_wobble",
            "The tone's pitch wobble: peak",
            pitch.map(|p| p.1),
            Unit::Cents,
            src,
        ),
        Reading::new(
            "effect.level_wobble_rate",
            "The tone's level wobble: rate",
            amp.map(|p| p.0),
            Unit::Hertz,
            src,
        ),
        Reading::new(
            "effect.level_wobble",
            "The tone's level wobble: peak",
            amp.map(|p| p.1),
            Unit::Decibels,
            src,
        ),
    ]
}

/// The intermodulation section on two sines: second- and third-order products against the tones, and
/// SMPTE's figure where the second tone is at least eight times the first.
#[must_use]
pub fn dual_section(y: &[f64], s: &Stimulus, hz: f64, hz2: f64) -> Section {
    let rate = f64::from(s.rate);
    let src = "the products m·f1 + n·f2 (|m| + |n| of 2 or 3) read through a four-term Blackman–Harris window over the steady stretch; SMPTE's figure is the sidebands f2 ± f1 and f2 ± 2·f1 against f2 (IEC 60268-3's modulation method)";
    let Some((a, b)) = steady(s, y.len()) else {
        return Section::new("intermodulation", "Intermodulation", Vec::new());
    };
    let seg = &y[a..b];
    let lines = Lines::new(seg.len());
    let amp = |f: f64| lines.amplitude(seg, rate, f).unwrap_or(0.0);
    let (a1, a2) = (amp(hz), amp(hz2));
    let tones = (a1 * a1 + a2 * a2).sqrt();
    let mut seen: Vec<f64> = Vec::new();
    let (mut second, mut third) = (0.0, 0.0);
    for m in -3i32..=3 {
        for n in -3i32..=3 {
            let order = m.abs() + n.abs();
            if m == 0 || n == 0 || !(2..=3).contains(&order) {
                continue;
            }
            let f = (f64::from(m) * hz + f64::from(n) * hz2).abs();
            let clash = |g: f64| (1..=MAX_ORDER).any(|k| (f - k as f64 * g).abs() < 5.0);
            if f < 5.0
                || f >= 0.45 * rate
                || clash(hz)
                || clash(hz2)
                || seen.iter().any(|g| (g - f).abs() < 1.0)
            {
                continue;
            }
            seen.push(f);
            let p = amp(f);
            if order == 2 {
                second += p * p;
            } else {
                third += p * p;
            }
        }
    }
    let ratio_db = |e: f64| (tones > 0.0).then(|| db(e.sqrt() / tones)).flatten();
    let smpte = (hz2 >= 8.0 * hz && a2 > 0.0).then(|| {
        let side: f64 = [hz2 - hz, hz2 + hz, hz2 - 2.0 * hz, hz2 + 2.0 * hz]
            .iter()
            .filter(|f| **f > 0.0 && **f < 0.45 * rate)
            .map(|&f| amp(f).powi(2))
            .sum();
        side.sqrt() / a2 * 100.0
    });
    Section::new(
        "intermodulation",
        "Intermodulation on two sines",
        vec![
            Reading::new(
                "effect.imd2",
                "Second-order products against the tones",
                ratio_db(second),
                Unit::Decibels,
                src,
            ),
            Reading::new(
                "effect.imd3",
                "Third-order products against the tones",
                ratio_db(third),
                Unit::Decibels,
                src,
            ),
            Reading::new(
                "effect.imd",
                "Intermodulation distortion (SMPTE)",
                smpte,
                Unit::Percent,
                src,
            ),
        ],
    )
}

/// The dB envelope of the tone at `hz` over a two-cycle Hann window every quarter cycle:
/// `(centre sample, level dB)`.
fn tone_envelope(y: &[f64], rate: f64, hz: f64) -> Vec<(f64, f64)> {
    let hop = ((rate / hz / 4.0).round() as usize).max(1);
    let span = 8;
    let count = y.len() / hop;
    let Some(t) = Tracker::new(y, rate, hz, 0, hop, span, count) else {
        return Vec::new();
    };
    (0..t.windows())
        .filter_map(|p| {
            let a = t.amplitude(p)?;
            (a > 0.0).then(|| ((p * hop + span * hop / 2) as f64, 20.0 * a.log10()))
        })
        .collect()
}

/// A compressor's static curve and times on level steps.
#[must_use]
pub fn steps_section(y: &[f64], s: &Stimulus, hz: f64) -> Section {
    let rate = f64::from(s.rate);
    let src = "each step's tone read over its last 40 % (Blackman–Harris); the curve a knee fitted by least squares, slope one below it; the times the gain's 63 % way to its settled level after the step up and the step down that move it most, from where the response's own level jumps (a two-cycle Hann window every quarter cycle)";
    let steps = s.steps();
    let lag = coarse_lag(&s.render(), y, rate);
    let settled: Vec<(f64, Option<f64>)> = steps
        .iter()
        .map(|&(a, b, level)| {
            let from = a + lag + (b - a) * 3 / 5;
            let to = (b + lag).min(y.len());
            let out = (to > from + 16)
                .then(|| {
                    let seg = &y[from..to];
                    Lines::new(seg.len()).amplitude(seg, rate, hz).and_then(db)
                })
                .flatten();
            (level, out)
        })
        .collect();
    // The static curve: the gain g = out − in against in, fitted as g0 below a threshold T and
    // g0 + slope·(in − T) above it.
    let points: Vec<(f64, f64)> = settled
        .iter()
        .filter_map(|&(i, o)| o.map(|o| (i, o - i)))
        .collect();
    let fit = knee(&points);
    let (threshold, ratio, makeup) = match fit {
        Some((t, g0, slope)) if slope.abs() >= MIN_SLOPE_CHANGE => {
            (Some(t), Some(1.0 / (1.0 + slope).max(0.01)), Some(g0))
        }
        Some((_, g0, _)) => (None, Some(1.0), Some(g0)),
        None => (None, None, None),
    };
    // The times, at the largest step up and the largest step down.
    let env = tone_envelope(y, rate, hz);
    let gain_after = |k: usize| settled[k].1.map(|o| o - settled[k].0);
    // The step up and the step down across which the gain moves most.
    let biggest = |up: bool| {
        (1..steps.len())
            .filter(|&k| (steps[k].2 > steps[k - 1].2) == up && steps[k].2 != steps[k - 1].2)
            .filter_map(|k| Some((k, (gain_after(k)? - gain_after(k - 1)?).abs())))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(k, _)| k)
    };
    let time = |k: Option<usize>| -> Option<f64> {
        let k = k?;
        let (g_start, g_end) = (gain_after(k - 1)?, gain_after(k)?);
        if (g_end - g_start).abs() < MIN_GAIN_CHANGE_DB {
            return None;
        }
        let (in_before, in_after) = (steps[k - 1].2, steps[k].2);
        let out_before = in_before + g_start;
        let out_jump = in_after + g_start;
        let half =
            20.0 * ((10f64.powf(out_before / 20.0) + 10f64.powf(out_jump / 20.0)) / 2.0).log10();
        let expected = (steps[k].0 + lag) as f64;
        let search = 0.01 * rate;
        let up = in_after > in_before;
        let instant = env
            .windows(2)
            .find(|w| {
                w[1].0 >= expected - search
                    && w[1].0 <= expected + search
                    && if up {
                        w[0].1 < half && w[1].1 >= half
                    } else {
                        w[0].1 > half && w[1].1 <= half
                    }
            })
            .map(|w| {
                let t = (half - w[0].1) / (w[1].1 - w[0].1);
                w[0].0 + t * (w[1].0 - w[0].0)
            })?;
        let window = 2.0 * rate / hz;
        let target = g_start + TIME_CONSTANT_SHARE * (g_end - g_start);
        let end = (steps[k].1 + lag) as f64;
        env.iter()
            .filter(|p| p.0 >= instant + window / 2.0 && p.0 < end)
            .find(|p| {
                let g = p.1 - in_after;
                if g_end < g_start {
                    g <= target
                } else {
                    g >= target
                }
            })
            .map(|p| (p.0 - instant) / rate * 1000.0)
    };
    let attack = time(biggest(true));
    let release = time(biggest(false));
    let mut section = Section::new(
        "dynamics",
        "Dynamics on level steps",
        vec![
            Reading::new(
                "effect.threshold",
                "Threshold: where the gain starts to fall",
                threshold,
                Unit::DecibelsFullScale,
                src,
            ),
            Reading::new(
                "effect.ratio",
                "Ratio above the threshold (input dB per output dB)",
                ratio,
                Unit::Plain,
                src,
            ),
            Reading::new(
                "effect.makeup",
                "Gain below the threshold",
                makeup,
                Unit::Decibels,
                src,
            ),
            Reading::new(
                "effect.attack",
                "Attack: 63 % of the gain's way after the largest step up",
                attack,
                Unit::Milliseconds,
                src,
            ),
            Reading::new(
                "effect.release",
                "Release: 63 % of the gain's way after the largest step down",
                release,
                Unit::Milliseconds,
                src,
            ),
        ],
    );
    section.tables.push(Table {
        id: "effect.curve",
        title: "The static curve".into(),
        columns: vec![
            ("Input", Unit::DecibelsFullScale),
            ("Output", Unit::DecibelsFullScale),
            ("Gain", Unit::Decibels),
        ],
        rows: settled
            .iter()
            .map(|&(i, o)| vec![Some(i), o, o.map(|o| o - i)])
            .collect(),
        window_ms: None,
        source: src,
    });
    section
}

/// A knee fitted to `(in, gain)`: `(threshold, gain below, slope above)`, the threshold searched every
/// 0.5 dB across the inputs and each candidate's two parameters found by least squares.
fn knee(points: &[(f64, f64)]) -> Option<(f64, f64, f64)> {
    if points.len() < 3 {
        return None;
    }
    let lo = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let hi = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    let mut best: Option<(f64, f64, f64, f64)> = None;
    let mut t = lo;
    while t <= hi {
        let rows: Vec<Vec<f64>> = points
            .iter()
            .map(|p| vec![1.0, (p.0 - t).max(0.0)])
            .collect();
        let b: Vec<f64> = points.iter().map(|p| p.1).collect();
        if let Some(m) = Matrix::from_rows(&rows) {
            if let Some(x) = least_squares(&m, &b) {
                let err: f64 = points
                    .iter()
                    .map(|p| (x[0] + x[1] * (p.0 - t).max(0.0) - p.1).powi(2))
                    .sum();
                if best.is_none_or(|b| err < b.3 - 1e-9) {
                    best = Some((t, x[0], x[1], err));
                }
            }
        }
        t += 0.5;
    }
    best.map(|b| (b.0, b.1, b.2))
}

/// The lag of `y` behind `x` by cross-correlation, samples, within a second; zero where it cannot be
/// read.
#[must_use]
pub fn coarse_lag(x: &[f64], y: &[f64], rate: f64) -> usize {
    let n = (x.len() + y.len()).next_power_of_two();
    let mut xr = vec![0.0; n];
    let mut xi = vec![0.0; n];
    let mut yr = vec![0.0; n];
    let mut yi = vec![0.0; n];
    xr[..x.len()].copy_from_slice(x);
    yr[..y.len()].copy_from_slice(y);
    if mxm_measure::spectrum::fft(&mut xr, &mut xi).is_none()
        || mxm_measure::spectrum::fft(&mut yr, &mut yi).is_none()
    {
        return 0;
    }
    let mut re: Vec<f64> = (0..n).map(|k| yr[k] * xr[k] + yi[k] * xi[k]).collect();
    let mut im: Vec<f64> = (0..n).map(|k| yi[k] * xr[k] - yr[k] * xi[k]).collect();
    if mxm_measure::spectrum::ifft(&mut re, &mut im).is_none() {
        return 0;
    }
    let max = (rate as usize).min(n - 1);
    (0..=max)
        .max_by(|&a, &b| re[a].abs().total_cmp(&re[b].abs()))
        .unwrap_or(0)
}

/// Noise, runaway and self-oscillation on silence, and DC.
#[must_use]
pub fn silence_section(y: &[f64], s: &Stimulus) -> Section {
    let rate = f64::from(s.rate);
    let src = "the response to silence: its RMS, a line through its level every 100 ms, and its strongest spectral line over the last second";
    let from = s.start().min(y.len());
    let seg = &y[from..];
    let floor = envelope::rms(seg).and_then(db);
    let block = ((0.1 * rate) as usize).max(1);
    let levels: Vec<(f64, f64)> = seg
        .chunks(block)
        .enumerate()
        .filter(|(_, c)| c.len() == block)
        .filter_map(|(k, c)| envelope::rms(c).and_then(db).map(|d| (k as f64 * 0.1, d)))
        .collect();
    let runaway = envelope::line_fit(&levels).map(|l| l.0);
    let tail = &seg[seg.len().saturating_sub(rate as usize)..];
    let line = strongest_line(tail, rate);
    let dc = Lines::new(seg.len())
        .mean(seg)
        .and_then(|m| db(m.abs()).filter(|d| *d > -140.0));
    Section::new(
        "silence",
        "Noise, runaway and self-oscillation on silence",
        vec![
            Reading::new(
                "effect.noise_floor",
                "Noise floor (RMS)",
                floor,
                Unit::DecibelsFullScale,
                src,
            ),
            Reading::new(
                "effect.runaway",
                "Runaway: the level's change per second",
                runaway,
                Unit::DecibelsPerSecond,
                src,
            ),
            Reading::new(
                "effect.line_hz",
                "A line on silence: self-oscillation or hum",
                line.map(|l| l.0),
                Unit::Hertz,
                src,
            ),
            Reading::new(
                "effect.line_level",
                "The line's level",
                line.map(|l| l.1),
                Unit::DecibelsFullScale,
                src,
            ),
            Reading::new("effect.dc", "DC", dc, Unit::DecibelsFullScale, src),
        ],
    )
}

/// The strongest spectral line between 20 Hz and 0.45 of the rate standing [`LINE_OVER_MEDIAN_DB`]
/// over the median: its frequency (parabolic on log power) and amplitude, dBFS.
#[must_use]
pub fn strongest_line(x: &[f64], rate: f64) -> Option<(f64, f64)> {
    let s = crate::repr::spectrum::power_spectrum(x, rate, 1 << 16)?;
    let range = s.bins(20.0, 0.45 * rate);
    let k = range
        .clone()
        .max_by(|&a, &b| s.power[a].total_cmp(&s.power[b]))?;
    let mut sorted: Vec<f64> = range.clone().map(|i| s.power[i]).collect();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[sorted.len() / 2];
    if s.power[k] <= 0.0 || s.power[k] < median * 10f64.powf(LINE_OVER_MEDIAN_DB / 10.0) {
        return None;
    }
    let hz = if k > range.start && k + 1 < range.end {
        let (a, b, c) = (
            s.power[k - 1].max(1e-300).ln(),
            s.power[k].ln(),
            s.power[k + 1].max(1e-300).ln(),
        );
        let d = a - 2.0 * b + c;
        let shift = if d.abs() > 0.0 {
            0.5 * (a - c) / d
        } else {
            0.0
        };
        s.hz(k) + shift.clamp(-0.5, 0.5) * s.bin_hz
    } else {
        s.hz(k)
    };
    let amp = Lines::new(x.len()).amplitude(x, rate, hz)?;
    Some((hz, db(amp)?))
}

/// Bursts: the gain during them, the first against the rest, and the hangover after each.
#[must_use]
pub fn bursts_section(y: &[f64], s: &Stimulus, hz: f64, cycles: f64, every_s: f64) -> Section {
    let rate = f64::from(s.rate);
    let src = "each burst's tone over its middle half (Blackman–Harris) against the stimulus's; the hangover from each burst's end until the response's RMS over 1 ms stays 40 dB under its level during the burst";
    let peak = 10f64.powf(s.level_dbfs / 20.0);
    let lag = coarse_lag(&s.render(), y, rate);
    let every = ((every_s * rate).round() as usize).max(1);
    let on = ((cycles / hz) * rate).round() as usize;
    let count = s.signal_len() / every;
    let mut gains = Vec::new();
    let mut hangovers = Vec::new();
    let ms = ((0.001 * rate) as usize).max(1);
    for k in 0..count {
        let a = s.start() + k * every + lag;
        let (m0, m1) = (a + on / 4, a + 3 * on / 4);
        if m1 > y.len() || m1 <= m0 + 8 {
            break;
        }
        let seg = &y[m0..m1];
        let Some(level) = Lines::new(seg.len()).amplitude(seg, rate, hz) else {
            continue;
        };
        gains.push(db(level / peak));
        let during = envelope::rms(seg).unwrap_or(0.0);
        let limit = during * 10f64.powf(-HANGOVER_DB / 20.0);
        // From the burst's end until the response stays under the limit up to the next burst; none
        // where it is still over it there.
        let (end, next) = (a + on, (a + every).min(y.len()));
        let mut t = end;
        let mut last_over: Option<usize> = None;
        let mut settled = true;
        while t + ms <= next {
            let over = envelope::rms(&y[t..t + ms]).unwrap_or(0.0) >= limit;
            if over {
                last_over = Some(t);
            }
            settled = !over;
            t += ms;
        }
        hangovers.push(
            settled.then(|| last_over.map_or(0.0, |l| (l + ms - end) as f64 / rate * 1000.0)),
        );
    }
    let median = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        (!v.is_empty()).then(|| v[v.len() / 2])
    };
    let mut g: Vec<f64> = gains.iter().flatten().copied().collect();
    let gain = median(&mut g);
    let first = gains
        .first()
        .copied()
        .flatten()
        .zip(gain)
        .map(|(f, m)| f - m);
    let mut h: Vec<f64> = hangovers.iter().flatten().copied().collect();
    let hangover = if h.len() * 2 >= hangovers.len().max(1) {
        median(&mut h)
    } else {
        None
    };
    Section::new(
        "bursts",
        "Tone bursts",
        vec![
            Reading::new(
                "effect.burst_gain",
                "Gain during a burst (median)",
                gain,
                Unit::Decibels,
                src,
            ),
            Reading::new(
                "effect.burst_first",
                "The first burst's gain against the rest",
                first,
                Unit::Decibels,
                src,
            ),
            Reading::new(
                "effect.hangover",
                "Hangover: from a burst's end until it stays 40 dB under it",
                hangover,
                Unit::Milliseconds,
                src,
            ),
        ],
    )
}

/// The magnitude-squared coherence of `y` with `x` (Welch: 4096-point Hann segments, half
/// overlapping), averaged over 100 Hz to 10 kHz: one for a linear, time-invariant system with no noise.
#[must_use]
pub fn coherence(x: &[f64], y: &[f64], rate: f64) -> Option<f64> {
    const N: usize = 4096;
    let len = x.len().min(y.len());
    if len < N {
        return None;
    }
    let w = crate::repr::spectrum::hann(N);
    let mut sxx = vec![0.0; N / 2 + 1];
    let mut syy = vec![0.0; N / 2 + 1];
    let mut sxy = vec![(0.0, 0.0); N / 2 + 1];
    let mut a = 0;
    while a + N <= len {
        let mut xr: Vec<f64> = (0..N).map(|i| x[a + i] * w[i]).collect();
        let mut xi = vec![0.0; N];
        let mut yr: Vec<f64> = (0..N).map(|i| y[a + i] * w[i]).collect();
        let mut yi = vec![0.0; N];
        mxm_measure::spectrum::fft(&mut xr, &mut xi)?;
        mxm_measure::spectrum::fft(&mut yr, &mut yi)?;
        for k in 0..=N / 2 {
            sxx[k] += xr[k] * xr[k] + xi[k] * xi[k];
            syy[k] += yr[k] * yr[k] + yi[k] * yi[k];
            sxy[k].0 += yr[k] * xr[k] + yi[k] * xi[k];
            sxy[k].1 += yi[k] * xr[k] - yr[k] * xi[k];
        }
        a += N / 2;
    }
    let bin = rate / N as f64;
    let values: Vec<f64> = (0..=N / 2)
        .filter(|&k| {
            let f = k as f64 * bin;
            (100.0..=10_000.0).contains(&f) && sxx[k] > 0.0 && syy[k] > 0.0
        })
        .map(|k| (sxy[k].0 * sxy[k].0 + sxy[k].1 * sxy[k].1) / (sxx[k] * syy[k]))
        .collect();
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

/// A moving delay, frame by frame: `(centre s, lag ms)` of the strongest cross-correlation peak of
/// `y` against `x` that is not a lag held throughout (the dry path, a fixed tap). Frames of about
/// 5 ms end to end, lags up to 40 ms, the peak placed by a parabola. A frame is short because the
/// delay moves within it and smears its peak: a chorus swept ±2 ms at 0.8 Hz moves 10 samples in
/// 20 ms, which sank its peak under the dry path's, and 2.5 in 5 ms.
fn delay_track(x: &[f64], y: &[f64], rate: f64, from: usize, to: usize) -> Vec<(f64, f64)> {
    let frame = ((0.005 * rate) as usize).next_power_of_two();
    let hop = frame;
    let max_lag = (0.04 * rate) as usize;
    let size = (frame + max_lag).next_power_of_two();
    let mut frames: Vec<(f64, Vec<(f64, f64)>)> = Vec::new();
    let mut t = from.max(max_lag);
    while t + frame <= to.min(y.len()).min(x.len()) {
        let mut ar = vec![0.0; size];
        let mut ai = vec![0.0; size];
        let mut br = vec![0.0; size];
        let mut bi = vec![0.0; size];
        ar[..frame].copy_from_slice(&y[t..t + frame]);
        br[..frame + max_lag].copy_from_slice(&x[t - max_lag..t + frame]);
        if mxm_measure::spectrum::fft(&mut ar, &mut ai).is_none()
            || mxm_measure::spectrum::fft(&mut br, &mut bi).is_none()
        {
            return Vec::new();
        }
        // conj(A)·B: c[k] = Σ a[n]·b[n + k], and the lag l is c[max_lag − l].
        let mut cr: Vec<f64> = (0..size).map(|k| ar[k] * br[k] + ai[k] * bi[k]).collect();
        let mut ci: Vec<f64> = (0..size).map(|k| ar[k] * bi[k] - ai[k] * br[k]).collect();
        if mxm_measure::spectrum::ifft(&mut cr, &mut ci).is_none() {
            return Vec::new();
        }
        let r: Vec<f64> = (0..=max_lag).map(|l| cr[max_lag - l].abs()).collect();
        let top = r.iter().copied().fold(0.0, f64::max);
        let mut peaks: Vec<(f64, f64)> = (1..max_lag)
            .filter(|&l| r[l] >= 0.2 * top && r[l] >= r[l - 1] && r[l] > r[l + 1])
            .map(|l| {
                let (a, b, c) = (r[l - 1], r[l], r[l + 1]);
                let d = a - 2.0 * b + c;
                let shift = if d.abs() > 0.0 {
                    (0.5 * (a - c) / d).clamp(-0.5, 0.5)
                } else {
                    0.0
                };
                (l as f64 + shift, b / top.max(f64::MIN_POSITIVE))
            })
            .collect();
        if r[0] >= 0.2 * top && r[0] > r[1] {
            peaks.push((0.0, r[0] / top.max(f64::MIN_POSITIVE)));
        }
        frames.push(((t + frame / 2) as f64 / rate, peaks));
        t += hop;
    }
    if frames.is_empty() {
        return Vec::new();
    }
    // Lags held throughout: present (±1 sample) in 70 % of frames.
    let mut counts: std::collections::BTreeMap<i64, usize> = std::collections::BTreeMap::new();
    for (_, peaks) in &frames {
        let mut seen: Vec<i64> = peaks.iter().map(|p| p.0.round() as i64).collect();
        seen.sort_unstable();
        seen.dedup();
        for l in seen {
            *counts.entry(l).or_default() += 1;
        }
    }
    let held: Vec<i64> = counts
        .keys()
        .copied()
        .filter(|&l| {
            let near: usize = (l - 1..=l + 1)
                .map(|m| counts.get(&m).copied().unwrap_or(0))
                .sum();
            near * 10 >= frames.len() * 7
        })
        .collect();
    frames
        .iter()
        .filter_map(|(t, peaks)| {
            peaks
                .iter()
                .filter(|p| p.1 >= 0.3 && held.iter().all(|&h| (p.0 - h as f64).abs() > 2.0))
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|p| (*t, p.0 / rate * 1000.0))
        })
        .collect()
}

/// A track fitted by least squares to a line and a sinusoid at `hz` and its third harmonic:
/// `(mean, amplitude, phase rad, third's amplitude)`.
fn fit_sinusoid(points: &[(f64, f64)], hz: f64) -> Option<(f64, f64, f64, f64)> {
    let w = std::f64::consts::TAU * hz;
    let rows: Vec<Vec<f64>> = points
        .iter()
        .map(|p| {
            let t = p.0;
            vec![
                1.0,
                t,
                (w * t).sin(),
                (w * t).cos(),
                (3.0 * w * t).sin(),
                (3.0 * w * t).cos(),
            ]
        })
        .collect();
    let b: Vec<f64> = points.iter().map(|p| p.1).collect();
    let x = least_squares(&Matrix::from_rows(&rows)?, &b)?;
    let mean = points.iter().map(|p| p.1).sum::<f64>() / points.len() as f64;
    Some((mean, x[2].hypot(x[3]), x[3].atan2(x[2]), x[4].hypot(x[5])))
}

/// A moving delay's rate, refined from the track's spectrum by the fit's amplitude over ±one bin.
fn sweep_rate(points: &[(f64, f64)], hop_s: f64) -> Option<f64> {
    let (_, m, _) = super::sustain::modulation(points, hop_s, SWEEP_HZ)?;
    let rough = m?.0;
    let bin = 1.0 / (4 * points.len()).next_power_of_two() as f64 / hop_s;
    (0..=40)
        .map(|k| rough + (k as f64 - 20.0) / 20.0 * bin)
        .filter(|&f| f > 0.0)
        .filter_map(|f| fit_sinusoid(points, f).map(|fit| (f, fit.1)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(f, _)| f)
}

/// Gaps in an evenly hopped track filled by straight lines.
fn fill(points: &[(f64, f64)], hop_s: f64) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::new();
    for &p in points {
        if let Some(&last) = out.last() {
            let steps = ((p.0 - last.0) / hop_s).round() as usize;
            for k in 1..steps {
                let t = k as f64 / steps as f64;
                out.push((last.0 + t * (p.0 - last.0), last.1 + t * (p.1 - last.1)));
            }
        }
        out.push(p);
    }
    out
}

/// A chorus's or flanger's moving delay on noise: its rate, depth, mean, shape, and the right
/// channel's phase against the left's; and the coherence that says whether the system is linear and
/// time-invariant.
#[must_use]
pub fn noise_section(channels: &[Vec<f64>], s: &Stimulus) -> Section {
    let rate = f64::from(s.rate);
    let x = s.render();
    let mono: Vec<f64> = mean_channel(channels);
    let src = "the cross-correlation of the response with the noise over 5 ms frames, its strongest peak away from lags held throughout; the track fitted to a sinusoid at the peak of its spectrum";
    let from = s.start() + (0.1 * rate) as usize;
    let to = s.start() + s.signal_len();
    let hop_s = ((0.005 * rate) as usize).next_power_of_two() as f64 / rate;
    // Each channel's track; the right's is fitted at the left's rate, or a rate a little off turns
    // their phases apart across the seconds between the fit's origin and the track.
    let reading = |ch: &[f64], at: Option<f64>| -> Option<(f64, (f64, f64, f64, f64))> {
        let track = delay_track(&x, ch, rate, from, to);
        let frames = ((to - from) as f64 / rate / hop_s) as usize;
        if track.len() * 2 < frames.max(1) {
            return None;
        }
        let filled = fill(&track, hop_s);
        let hz = match at {
            Some(hz) => hz,
            None => sweep_rate(&filled, hop_s)?,
        };
        Some((hz, fit_sinusoid(&filled, hz)?))
    };
    let left = channels.first().and_then(|c| reading(c, None));
    let right = left
        .zip(channels.get(1))
        .and_then(|(l, c)| reading(c, Some(l.0)));
    let phase = left.zip(right).map(|(l, r)| {
        let d = (r.1.2 - l.1.2).to_degrees();
        (d + 180.0).rem_euclid(360.0) - 180.0
    });
    Section::new(
        "movement",
        "A moving delay (chorus, flanger) on noise",
        vec![
            Reading::new(
                "response.coherence",
                "Coherence with the noise, 100 Hz to 10 kHz (one: linear and time-invariant)",
                coherence(&x, &mono, rate),
                Unit::Plain,
                "the magnitude-squared coherence, Welch's method over 4096-point Hann segments",
            ),
            Reading::new(
                "effect.mod_rate",
                "The delay's sweep rate",
                left.map(|l| l.0),
                Unit::Hertz,
                src,
            ),
            Reading::new(
                "effect.mod_depth",
                "The delay's sweep depth (peak)",
                left.map(|l| l.1.1),
                Unit::Milliseconds,
                src,
            ),
            Reading::new(
                "effect.mod_delay",
                "The swept delay's mean",
                left.map(|l| l.1.0),
                Unit::Milliseconds,
                src,
            ),
            Reading::new(
                "effect.mod_shape",
                "The sweep's third harmonic against its first (a triangle's is −19 dB, a sine's far lower)",
                left.and_then(|l| db(l.1.3 / l.1.1)),
                Unit::Decibels,
                src,
            ),
            Reading::new(
                "effect.mod_phase",
                "The right channel's sweep against the left's",
                phase,
                Unit::Plain,
                "the two channels' fitted sinusoids at the left's rate: degrees",
            ),
        ],
    )
}

/// The mean of the channels.
#[must_use]
pub fn mean_channel(channels: &[Vec<f64>]) -> Vec<f64> {
    let len = channels.iter().map(Vec::len).max().unwrap_or(0);
    (0..len)
        .map(|i| {
            channels
                .iter()
                .map(|c| c.get(i).copied().unwrap_or(0.0))
                .sum::<f64>()
                / channels.len().max(1) as f64
        })
        .collect()
}

/// A stereo output's correlation, width and balance, from `from` on.
#[must_use]
pub fn stereo_section(channels: &[Vec<f64>], from: usize) -> Option<Section> {
    let (l, r) = (channels.first()?, channels.get(1)?);
    let n = l.len().min(r.len());
    let from = from.min(n);
    let (mut ll, mut rr, mut lr, mut mid, mut side) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for i in from..n {
        let (a, b) = (l[i], r[i]);
        ll += a * a;
        rr += b * b;
        lr += a * b;
        mid += (a + b) * (a + b);
        side += (a - b) * (a - b);
    }
    let src = "the two output channels over the response: Pearson's correlation at zero lag, side against mid energy, left against right energy";
    let ratio = |a: f64, b: f64| (a > 0.0 && b > 0.0).then(|| 10.0 * (a / b).log10());
    Some(Section::new(
        "stereo",
        "Stereo output",
        vec![
            Reading::new(
                "stereo.correlation",
                "Correlation of left and right (1 mono, 0 unrelated, −1 opposed)",
                (ll > 0.0 && rr > 0.0).then(|| lr / (ll * rr).sqrt()),
                Unit::Plain,
                src,
            ),
            Reading::new(
                "stereo.width",
                "Width: side against mid",
                ratio(side, mid),
                Unit::Decibels,
                src,
            ),
            Reading::new(
                "stereo.balance",
                "Balance: left against right",
                ratio(ll, rr),
                Unit::Decibels,
                src,
            ),
        ],
    ))
}

/// The distortion, intermodulation, dynamics, silence or bursts section for a stimulus of one of
/// those kinds; `None` for the kinds read elsewhere.
#[must_use]
pub fn section_for(mono: &[f64], s: &Stimulus) -> Option<Section> {
    Some(match &s.kind {
        Kind::Sine { hz } => sine_section(mono, s, *hz),
        Kind::DualSine { hz, hz2, .. } => dual_section(mono, s, *hz, *hz2),
        Kind::Steps { hz, .. } => steps_section(mono, s, *hz),
        Kind::Silence => silence_section(mono, s),
        Kind::Bursts {
            hz,
            cycles,
            every_s,
        } => bursts_section(mono, s, *hz, *cycles, *every_s),
        _ => return None,
    })
}
