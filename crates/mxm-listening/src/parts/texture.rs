//! Noise and texture: how much buzz there is against the body, how long the wires keep going, whether
//! their slaps come in clusters, how sharp each slap is, the slow rhythm of the buzz, and a fast
//! periodicity in the attack's envelope.
//!
//! Sources: the snare's fine-tuning (`docs/drum-model-fitting.md` §7, *Acoustic drum models in
//! particular*: "the original has a longer wire decay", "the slapping is … a little too pronounced")
//! and `ab_residuals.py`'s envelope periodicity. Every texture measure of one render is a **sample**:
//! a single draw of a random quantity, which the guide found spanning 0.3–1.1 dB as one setting moved.
//! For reference, steady noise of the same band reads a swing near 1.7 dB, autocorrelations near 0,
//! kurtosis 2.75 ± 0.07 (not 3: dividing out the 5 ms envelope takes some of it) and a median 2 ms
//! crest near 2.5.

use super::Context;
use crate::reading::{Reading, Section, Unit};
use crate::repr::{bands, envelope, spectrum};

/// The wires' band, Hz.
pub const WIRE_BAND: (f64, f64) = (2000.0, 8000.0);
/// The rattle's octaves, Hz: where the wires sound, below the hiss above 6.4 kHz.
pub const RATTLE_BAND: (f64, f64) = (1600.0, 6400.0);
/// Where the late wire level is read, ms.
pub const LATE_MS: [f64; 3] = [300.0, 450.0, 600.0];
/// Autocorrelation lags of the 1 ms wire level, ms.
pub const LAGS_MS: [usize; 5] = [1, 3, 5, 10, 20];
/// The modulation bands of the wire level, Hz.
pub const RHYTHM_HZ: [(f64, f64); 3] = [(4.0, 16.0), (16.0, 64.0), (64.0, 200.0)];

#[must_use]
pub fn read(c: &Context) -> Section {
    let mut readings = Vec::new();

    // Buzz share: energy above 1.6 kHz against below 400 Hz over 30–150 ms.
    let level = c.level_db(30.0, 150.0);
    let early = c
        .window(30.0, 150.0)
        .filter(|_| c.covers(150.0))
        .and_then(|w| spectrum::power_spectrum(w, c.rate, 1 << 15));
    let share = early.as_ref().and_then(|s| {
        let high: f64 = s.power[s.bins(1600.0, 20_000.0_f64.min(0.49 * c.rate))]
            .iter()
            .sum();
        let low: f64 = s.power[s.bins(30.0, 400.0)].iter().sum();
        (high > 0.0 && low > 0.0).then(|| 10.0 * (high / low).log10())
    });
    // The rattle's share: the wires' own octaves, 1.6–6.4 kHz, against the whole sound. The buzz share
    // lumps in everything above 1.6 kHz, so a model with too much top and too little rattle read level
    // with its recording; the owner heard the recording's rattle as more pronounced (2026-09-27).
    let rattle = early.as_ref().and_then(|s| {
        let band: f64 = s.power[s.bins(RATTLE_BAND.0, RATTLE_BAND.1.min(0.49 * c.rate))]
            .iter()
            .sum();
        let whole: f64 = s.power[s.bins(20.0, 20_000.0_f64.min(0.49 * c.rate))]
            .iter()
            .sum();
        (band > 0.0 && whole > 0.0).then(|| 10.0 * (band / whole).log10())
    });
    readings.push(
        Reading::new(
            "texture.rattle",
            "Rattle's share (1.6–6.4 kHz against the whole sound)",
            rattle,
            Unit::Decibels,
            "the owner, 2026-09-27: \"the rattle from the springs is a little more pronounced on the original\"",
        )
        .window(30.0, 150.0)
        .band(RATTLE_BAND.0, RATTLE_BAND.1.min(0.49 * c.rate))
        .floor(level),
    );
    readings.push(
        Reading::new(
            "texture.buzz_share",
            "Buzz against body (above 1.6 kHz against below 400 Hz)",
            share,
            Unit::Decibels,
            "guide §7, the snare's layer trend",
        )
        .window(30.0, 150.0)
        .floor(level),
    );

    let top = WIRE_BAND.1.min(0.45 * c.rate);
    let x = c.from_onset();
    let wires = bands::band(x, c.rate, WIRE_BAND.0, top);
    let Some(wires) = wires else {
        readings.push(Reading::absent(
            "texture.late_wire_level",
            "Wire band late level",
            Unit::Decibels,
            "the rate is too low for the wire band",
            "guide §7",
        ));
        return Section::new("texture", "Noise and texture", readings);
    };

    let w10 = envelope::samples(0.01, c.rate).max(1);
    for ms in LATE_MS {
        let from = envelope::samples(ms / 1000.0, c.rate);
        let level = envelope::rms_at(&wires, from, w10).and_then(|l| envelope::db(l, c.peak));
        let r = Reading::new(
            "texture.late_wire_level",
            "Wire band late level",
            level,
            Unit::Decibels,
            "guide §7: 2–8 kHz, 10 ms RMS against the peak",
        )
        .window(ms, ms + 10.0)
        .band(WIRE_BAND.0, top);
        readings.push(if envelope::rms_at(&wires, from, w10).is_none() {
            Reading {
                validity: crate::reading::Validity::Absent("the file ends before this window"),
                ..r
            }
        } else if level.is_some_and(|l| l < crate::reading::NUMERICAL_FLOOR_DB) {
            Reading {
                validity: crate::reading::Validity::Absent("numerically silent"),
                value: None,
                ..r
            }
        } else {
            r
        });
    }

    // Burstiness: the 1 ms wire level in dB over 150–600 ms, its decay removed by a line; its swing
    // and its autocorrelation.
    let w1 = envelope::samples(0.001, c.rate).max(1);
    let late_level = c.level_db(150.0, 600.0);
    let db_series = level_series(
        &wires,
        envelope::samples(0.15, c.rate),
        envelope::samples(0.6, c.rate),
        w1,
    );
    let resid = db_series.as_ref().map(|d| detrend(d, 1));
    let swing = resid.as_ref().map(|r| std(r));
    readings.push(
        Reading::new(
            "texture.burst_swing",
            "Wire level swing (1 ms, decay removed)",
            swing,
            Unit::Decibels,
            "guide §7 burstiness; steady noise ≈ 1.7 dB",
        )
        .window(150.0, 600.0)
        .band(WIRE_BAND.0, top)
        .floor(late_level)
        .sample(),
    );
    for lag in LAGS_MS {
        let ac = resid.as_ref().and_then(|r| autocorrelation(r, lag));
        readings.push(
            Reading::new(
                "texture.burst_autocorrelation",
                "Wire level autocorrelation (clustering)",
                ac,
                Unit::Plain,
                "guide §7 burstiness; steady noise ≈ 0",
            )
            .window(lag as f64, lag as f64)
            .band(WIRE_BAND.0, top)
            .floor(late_level)
            .sample(),
        );
    }

    // Slap sharpness: the band over its own 5 ms RMS envelope — kurtosis and the median 2 ms crest.
    // Floored on the band's own level: a band at rounding level has no slaps to measure.
    let w5 = envelope::samples(0.005, c.rate).max(1);
    let normalised = normalise(&wires, w5);
    let band_level = |a: f64, b: f64| -> Option<f64> {
        let (i, j) = (
            envelope::samples(a / 1000.0, c.rate),
            envelope::samples(b / 1000.0, c.rate),
        );
        envelope::rms(wires.get(i..j.min(wires.len()))?).and_then(|l| envelope::db(l, c.peak))
    };
    for (a, b) in [(30.0, 150.0), (150.0, 400.0)] {
        let level = c.level_db(a, b).map(|l| {
            if band_level(a, b).is_some_and(|bl| bl < crate::reading::NUMERICAL_FLOOR_DB) {
                f64::NEG_INFINITY
            } else {
                l
            }
        });
        let seg = normalised.get(
            envelope::samples(a / 1000.0, c.rate)
                ..envelope::samples(b / 1000.0, c.rate).min(normalised.len()),
        );
        let (kurt, crest) = match seg {
            Some(s) if s.len() > 100 && c.covers(b) => (
                kurtosis(s),
                median_crest(s, envelope::samples(0.002, c.rate).max(2)),
            ),
            _ => (None, None),
        };
        readings.push(
            Reading::new(
                "texture.slap_kurtosis",
                "Slap sharpness (kurtosis)",
                kurt,
                Unit::Plain,
                "guide §7; white noise reads 2.75 ± 0.07 here (its 5 ms envelope divided out), a rattle of distinct slaps above it",
            )
            .window(a, b)
            .floor(level)
            .sample(),
        );
        readings.push(
            Reading::new(
                "texture.slap_crest",
                "Slap sharpness (median 2 ms crest)",
                crest,
                Unit::Ratio,
                "guide §7; steady noise ≈ 2.5",
            )
            .window(a, b)
            .floor(level)
            .sample(),
        );
    }

    // The buzz's slow rhythm: the modulation spectrum of the 1 ms wire level over 40–400 ms.
    let rhythm_level = c.level_db(40.0, 400.0);
    let series = level_series(
        &wires,
        envelope::samples(0.04, c.rate),
        envelope::samples(0.4, c.rate),
        w1,
    );
    let rhythm = series.map(|d| modulation_bands(&detrend(&d, 2), 1000.0));
    for (k, (lo, hi)) in RHYTHM_HZ.into_iter().enumerate() {
        readings.push(
            Reading::new(
                "texture.rhythm",
                "Wire level modulation (the buzz's rhythm)",
                rhythm.as_ref().and_then(|r| r[k]),
                Unit::Decibels,
                "guide §7 (\"the slapping is … too pronounced\"): swing carried in the band",
            )
            .window(40.0, 400.0)
            .band(lo, hi)
            .floor(rhythm_level)
            .sample(),
        );
    }

    // A fast periodicity in the attack's envelope (`ab_residuals.py`): 0.25 ms RMS over 150 ms.
    let w = envelope::samples(0.00025, c.rate).max(1);
    let env = envelope::windowed_rms(&x[..envelope::samples(0.15, c.rate).min(x.len())], w);
    let per = periodicity(&env, (0.001 / 0.00025) as usize, (0.02 / 0.00025) as usize);
    readings.push(
        Reading::new(
            "texture.envelope_periodicity",
            "Envelope periodicity (1–20 ms)",
            per,
            Unit::Plain,
            "ab_residuals.py `periodicity`",
        )
        .window(0.0, 150.0),
    );

    Section::new("texture", "Noise and texture", readings)
}

/// The level in dB of `w`-sample windows of `x` from `a` to `b`; `None` if too short.
fn level_series(x: &[f64], a: usize, b: usize, w: usize) -> Option<Vec<f64>> {
    let b = b.min(x.len());
    if b <= a || (b - a) / w < 16 {
        return None;
    }
    Some(
        x[a..b]
            .chunks_exact(w)
            .map(|c| 10.0 * (c.iter().map(|v| v * v).sum::<f64>() / w as f64 + 1e-30).log10())
            .collect(),
    )
}

/// `y` minus its least-squares polynomial of `degree` (1 or 2) in the index.
fn detrend(y: &[f64], degree: usize) -> Vec<f64> {
    let n = y.len();
    let cols = degree + 1;
    let mut a = crate::numeric::Matrix::zeros(n, cols);
    for i in 0..n {
        let t = i as f64 / n as f64;
        for d in 0..cols {
            a.set(i, d, t.powi(d as i32));
        }
    }
    match crate::numeric::least_squares(&a, y) {
        Some(coef) => y
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let t = i as f64 / n as f64;
                v - coef
                    .iter()
                    .enumerate()
                    .map(|(d, c)| c * t.powi(d as i32))
                    .sum::<f64>()
            })
            .collect(),
        None => y.to_vec(),
    }
}

fn mean(y: &[f64]) -> f64 {
    y.iter().sum::<f64>() / y.len().max(1) as f64
}

fn std(y: &[f64]) -> f64 {
    let m = mean(y);
    (y.iter().map(|v| (v - m).powi(2)).sum::<f64>() / y.len().max(1) as f64).sqrt()
}

/// The correlation coefficient of `y` with itself `lag` steps on.
fn autocorrelation(y: &[f64], lag: usize) -> Option<f64> {
    if lag == 0 || y.len() <= lag + 2 {
        return None;
    }
    let (a, b) = (&y[..y.len() - lag], &y[lag..]);
    let (ma, mb) = (mean(a), mean(b));
    let num: f64 = a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum();
    let da: f64 = a.iter().map(|x| (x - ma).powi(2)).sum();
    let db: f64 = b.iter().map(|y| (y - mb).powi(2)).sum();
    (da > 0.0 && db > 0.0).then(|| num / (da * db).sqrt())
}

/// `x` divided by its centred `w`-sample RMS envelope.
fn normalise(x: &[f64], w: usize) -> Vec<f64> {
    let mut prefix = vec![0.0; x.len() + 1];
    for (i, v) in x.iter().enumerate() {
        prefix[i + 1] = prefix[i] + v * v;
    }
    let half = w / 2;
    x.iter()
        .enumerate()
        .map(|(i, v)| {
            let a = i.saturating_sub(half);
            let b = (i + half + 1).min(x.len());
            let rms = ((prefix[b] - prefix[a]) / (b - a) as f64).sqrt();
            if rms > 0.0 { v / rms } else { 0.0 }
        })
        .collect()
}

fn kurtosis(s: &[f64]) -> Option<f64> {
    let m2 = s.iter().map(|v| v * v).sum::<f64>() / s.len() as f64;
    let m4 = s.iter().map(|v| v.powi(4)).sum::<f64>() / s.len() as f64;
    (m2 > 0.0).then(|| m4 / (m2 * m2))
}

fn median_crest(s: &[f64], block: usize) -> Option<f64> {
    let mut crests: Vec<f64> = s
        .chunks_exact(block)
        .filter_map(|b| {
            let rms = (b.iter().map(|v| v * v).sum::<f64>() / b.len() as f64).sqrt();
            (rms > 0.0).then(|| b.iter().fold(0.0f64, |m, v| m.max(v.abs())) / rms)
        })
        .collect();
    if crests.is_empty() {
        return None;
    }
    crests.sort_by(f64::total_cmp);
    Some(crests[crests.len() / 2])
}

/// The standard deviation `y` carries in each of [`RHYTHM_HZ`], sampled at `rate`: its total standard
/// deviation times the square root of the band's share of its (Hann-windowed) power.
fn modulation_bands(y: &[f64], rate: f64) -> [Option<f64>; 3] {
    let s = spectrum::power_spectrum(y, rate, 4096);
    let total_std = std(y);
    let mut out = [None; 3];
    if let Some(s) = s {
        let total: f64 = s.power[1..].iter().sum();
        if total > 0.0 {
            for (k, (lo, hi)) in RHYTHM_HZ.into_iter().enumerate() {
                let band: f64 = s.power[s.bins(lo, hi)].iter().sum();
                out[k] = Some(total_std * (band / total).sqrt());
            }
        }
    }
    out
}

/// The largest autocorrelation of `env` (mean removed) over lags `lo..=hi`.
fn periodicity(env: &[f64], lo: usize, hi: usize) -> Option<f64> {
    if env.len() <= hi + 2 {
        return None;
    }
    let m = mean(env);
    let e: Vec<f64> = env.iter().map(|v| v - m).collect();
    let zero: f64 = e.iter().map(|v| v * v).sum();
    if zero <= 0.0 {
        return None;
    }
    (lo..=hi)
        .map(|lag| {
            e[..e.len() - lag]
                .iter()
                .zip(&e[lag..])
                .map(|(a, b)| a * b)
                .sum::<f64>()
                / zero
        })
        .reduce(f64::max)
}
