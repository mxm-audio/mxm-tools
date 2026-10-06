//! A synth voice (the plan's §2 item 10): what a held note from an oscillator and a filter does that
//! an acoustic note does not — aliasing, and how audible it is under the harmonics' masking; the
//! harmonics by order; DC; its envelope's stages; clicks at note-on and note-off; zipper sidebands
//! from a stepped control; a detuned unison's beating; the filter's peak on the harmonic series; and a
//! line outside the series (a filter singing on its own, a second oscillator).
//!
//! Sources: aliasing as the energy off the harmonic grid (Lehtonen, Pekonen & Välimäki, JASA 2012,
//! who judge its audibility against the masked threshold), the masked threshold from Schroeder,
//! Atal & Hall's spreading function (JASA 1979) with Johnston's tonal-masker offset (IEEE JSAC 1988),
//! the Bark scale after Zwicker & Terhardt (JASA 1980) and the threshold in quiet after Terhardt
//! (1979), at the listener's stated level (a full-scale sine at 94 dB SPL); the spectrum through
//! Nuttall's four-term window with a continuous first derivative (IEEE ASSP 1981): sidelobes at
//! −93 dB falling 18 dB an octave. Blackman–Harris's fall only 6 dB an octave, and their sum over a
//! rich wave's thousands of bins held an additive saw's aliasing at −66 dB.

use super::Context;
use crate::reading::{Reading, Section, Table, Unit};
use crate::repr::envelope;
use crate::repr::lines::Lines;

/// A full-scale sine is heard at this level, dB SPL (the plan's §8).
pub const FULL_SCALE_SPL: f64 = 94.0;
/// A harmonic's mask reaches this many of the window's own bins either side (its main lobe; the
/// padded spectrum's bins are finer, and a mask four of those wide left the lobe's skirts as −57 dB of
/// aliasing)…
pub const MASK_BINS: f64 = 4.0;
/// …and this share of the harmonic's frequency more, for a unison's spread or a slow drift.
pub const MASK_SHARE: f64 = 0.003;
/// The highest order in the harmonic table.
pub const MAX_ORDER: usize = 16;
/// The attack ends where the envelope first comes within this of its maximum, dB.
pub const ATTACK_WITHIN_DB: f64 = 0.5;
/// A decay stage needs the peak this far over the sustain, dB.
pub const MIN_DECAY_DB: f64 = 1.0;
/// A line off the series stands this far over the off-grid median…
pub const LINE_OVER_DB: f64 = 20.0;
/// …and within this of the strongest harmonic, dB.
pub const LINE_WITHIN_DB: f64 = 30.0;
/// A zipper line stands this far over the median of the envelope's spectrum, dB.
pub const ZIPPER_OVER_DB: f64 = 20.0;
/// Zipper sidebands this far under the envelope are none: rounding, not a stepped control, dB.
pub const ZIPPER_FLOOR_DB: f64 = -90.0;
/// A peak on the harmonic series stands this far over the line through the harmonics' levels, dB.
pub const PEAK_OVER_DB: f64 = 3.0;

/// A Nuttall-windowed magnitude spectrum: `|X|` per bin, the bin spacing, and the factor that turns a
/// peak's `|X|` into a sine's amplitude.
struct Spectrum {
    mag: Vec<f64>,
    bin: f64,
    /// The window's own resolution, `rate / length`: its main lobe is four of these either side,
    /// however finely the zero-padding samples it.
    resolution: f64,
    to_amp: f64,
}

fn spectrum(seg: &[f64], rate: f64) -> Option<Spectrum> {
    let n = (2 * seg.len()).next_power_of_two();
    let a = [0.355_768, 0.487_396, 0.144_232, 0.012_604];
    let m = seg.len().max(2) as f64 - 1.0;
    let mut re = vec![0.0; n];
    let mut im = vec![0.0; n];
    let mut sum = 0.0;
    for (k, v) in seg.iter().enumerate() {
        let x = std::f64::consts::TAU * k as f64 / m;
        let w = a[0] - a[1] * x.cos() + a[2] * (2.0 * x).cos() - a[3] * (3.0 * x).cos();
        re[k] = v * w;
        sum += w;
    }
    mxm_measure::spectrum::fft(&mut re, &mut im)?;
    let mag = (0..=n / 2).map(|k| re[k].hypot(im[k])).collect();
    Some(Spectrum {
        mag,
        bin: rate / n as f64,
        resolution: rate / seg.len().max(1) as f64,
        to_amp: 2.0 / sum.max(f64::MIN_POSITIVE),
    })
}

/// The peak of `s` nearest `hz` within ±`reach` Hz, placed by a parabola on log magnitude:
/// `(frequency, amplitude)`.
fn peak_near(s: &Spectrum, hz: f64, reach: f64) -> Option<(f64, f64)> {
    let lo = ((hz - reach) / s.bin).floor().max(1.0) as usize;
    let hi = (((hz + reach) / s.bin).ceil() as usize).min(s.mag.len() - 2);
    let k = (lo..=hi).max_by(|&a, &b| s.mag[a].total_cmp(&s.mag[b]))?;
    let (a, b, c) = (
        s.mag[k - 1].max(1e-300).ln(),
        s.mag[k].max(1e-300).ln(),
        s.mag[k + 1].max(1e-300).ln(),
    );
    let d = a - 2.0 * b + c;
    let shift = if d.abs() > 0.0 {
        (0.5 * (a - c) / d).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    Some(((k as f64 + shift) * s.bin, s.mag[k] * s.to_amp))
}

/// The fundamental refined on the harmonic grid: each harmonic's peak, found where the fundamental so
/// far predicts it, and the fundamental re-fitted through them (least squares on `f_k = k·f0`,
/// weighted by amplitude). A fundamental read to 0.1 % puts the 40th harmonic 4 % off its guess.
fn refine(s: &Spectrum, f0: f64, rate: f64) -> Option<f64> {
    let mut f = f0;
    let top = (1..=8)
        .filter_map(|k| peak_near(s, k as f64 * f, 0.25 * f).map(|p| p.1))
        .fold(0.0, f64::max);
    if top <= 0.0 {
        return None;
    }
    let (mut num, mut den) = (0.0, 0.0);
    let mut k = 1usize;
    while (k as f64) * f < 0.45 * rate && k <= 200 {
        if let Some((hz, amp)) = peak_near(s, k as f64 * f, (0.25 * f).max(3.0 * s.bin)) {
            if amp >= top * 1e-3 {
                let kf = k as f64;
                num += amp * kf * hz;
                den += amp * kf * kf;
                f = num / den;
            }
        }
        k += 1;
    }
    (den > 0.0).then_some(f)
}

/// The Bark scale (Zwicker & Terhardt 1980).
fn bark(hz: f64) -> f64 {
    13.0 * (0.000_76 * hz).atan() + 3.5 * (hz / 7500.0).powi(2).atan()
}

/// The threshold in quiet, dB SPL (Terhardt 1979).
fn quiet(hz: f64) -> f64 {
    let k = hz.max(20.0) / 1000.0;
    3.64 * k.powf(-0.8) - 6.5 * (-0.6 * (k - 3.3).powi(2)).exp() + 1e-3 * k.powi(4)
}

/// A masker at `z_m` Bark and `level` dB SPL's threshold at `z` Bark: Schroeder's spreading function
/// less Johnston's tonal-masker offset.
fn masked(level: f64, z_m: f64, z: f64) -> f64 {
    let dz = z - z_m + 0.474;
    let spread = 15.81 + 7.5 * dz - 17.5 * (1.0 + dz * dz).sqrt();
    level - (14.5 + z_m) + spread
}

/// What the harmonic grid says: the aliasing (off-grid energy against the harmonics', dB), its most
/// audible part over the masked threshold (dB), the harmonics' amplitudes by order, and the strongest
/// off-grid line.
struct Grid {
    aliasing: Option<f64>,
    audible: Option<f64>,
    orders: Vec<(usize, f64, f64)>,
    line: Option<(f64, f64)>,
}

fn grid(s: &Spectrum, f0: f64, rate: f64) -> Grid {
    let top_hz = 0.5 * rate;
    let mut harmonic = vec![false; s.mag.len()];
    let mut orders = Vec::new();
    let mut k = 1usize;
    while (k as f64) * f0 < top_hz {
        let hz = k as f64 * f0;
        let reach = MASK_BINS * s.resolution + MASK_SHARE * hz;
        let (a, b) = (
            ((hz - reach) / s.bin).floor().max(0.0) as usize,
            (((hz + reach) / s.bin).ceil() as usize).min(s.mag.len() - 1),
        );
        for h in &mut harmonic[a..=b] {
            *h = true;
        }
        let amp = (a..=b).map(|i| s.mag[i]).fold(0.0, f64::max) * s.to_amp;
        orders.push((k, hz, amp));
        k += 1;
    }
    let dc_bins = (20.0 / s.bin).ceil() as usize;
    let (mut on, mut off) = (0.0, 0.0);
    for (i, m) in s.mag.iter().enumerate().skip(dc_bins) {
        if harmonic[i] {
            on += m * m;
        } else {
            off += m * m;
        }
    }
    let aliasing = (on > 0.0 && off > 0.0).then(|| 10.0 * (off / on).log10());
    // The masked threshold at each off-grid peak: the harmonics as tonal maskers, power-summed with
    // the threshold in quiet.
    let spl = |amp: f64| FULL_SCALE_SPL + 20.0 * amp.max(1e-12).log10();
    let maskers: Vec<(f64, f64)> = orders
        .iter()
        .filter(|o| o.2 > 0.0)
        .map(|o| (bark(o.1), spl(o.2)))
        .collect();
    let mut audible: Option<f64> = None;
    let mut off_peaks: Vec<(f64, f64)> = Vec::new();
    for (i, w) in s.mag.windows(3).enumerate().skip(dc_bins.max(1) - 1) {
        let i = i + 1;
        if harmonic[i] || w[1] < w[0] || w[1] < w[2] || w[1] <= 0.0 {
            continue;
        }
        let hz = i as f64 * s.bin;
        let amp = s.mag[i] * s.to_amp;
        off_peaks.push((hz, amp));
        let z = bark(hz);
        let power: f64 = maskers
            .iter()
            .map(|&(zm, l)| 10f64.powf(masked(l, zm, z) / 10.0))
            .sum::<f64>()
            + 10f64.powf(quiet(hz) / 10.0);
        let excess = spl(amp) - 10.0 * power.log10();
        audible = Some(audible.map_or(excess, |a: f64| a.max(excess)));
    }
    let strongest = orders.iter().map(|o| o.2).fold(0.0, f64::max);
    let mut mags: Vec<f64> = s
        .mag
        .iter()
        .enumerate()
        .skip(dc_bins)
        .filter(|(i, _)| !harmonic[*i])
        .map(|(_, m)| *m * s.to_amp)
        .collect();
    mags.sort_by(f64::total_cmp);
    let median = mags.get(mags.len() / 2).copied().unwrap_or(0.0);
    let line = off_peaks
        .iter()
        .copied()
        .filter(|p| {
            p.1 >= median * 10f64.powf(LINE_OVER_DB / 20.0)
                && p.1 >= strongest * 10f64.powf(-LINE_WITHIN_DB / 20.0)
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .and_then(|p| peak_near(s, p.0, 2.0 * s.bin));
    Grid {
        aliasing,
        audible,
        orders,
        line,
    }
}

/// The level of a window of `x` every millisecond, dB against the loudest: `(s from the onset, dB)`.
fn levels(c: &Context, window: usize) -> Vec<(f64, f64)> {
    let hop = ((0.001 * c.rate) as usize).max(1);
    let x = c.from_onset();
    let mut out = Vec::new();
    let mut i = 0;
    while i + window <= x.len() {
        let r = envelope::rms(&x[i..i + window]).unwrap_or(0.0);
        out.push(((i + window / 2) as f64 / c.rate, r));
        i += hop;
    }
    let top = out.iter().map(|p| p.1).fold(0.0, f64::max);
    out.into_iter()
        .map(|(t, r)| {
            (
                t,
                if r > 0.0 && top > 0.0 {
                    20.0 * (r / top).log10()
                } else {
                    -300.0
                },
            )
        })
        .collect()
}

/// The envelope's stages: attack (ms), decay time constant (ms), sustain (dB against the peak) and
/// release (ms from note-off to 60 dB under the level there).
fn stages(env: &[(f64, f64)], off: Option<f64>) -> [Option<f64>; 4] {
    let held: Vec<(f64, f64)> = env
        .iter()
        .copied()
        .filter(|p| off.is_none_or(|o| p.0 <= o))
        .collect();
    if held.len() < 10 {
        return [None; 4];
    }
    let max = held.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let attack_at = held.iter().position(|p| p.1 >= max - ATTACK_WITHIN_DB);
    let attack = attack_at.map(|i| held[i].0 * 1000.0);
    let tail = &held[held.len() * 7 / 10..];
    let mut t: Vec<f64> = tail.iter().map(|p| p.1).collect();
    t.sort_by(f64::total_cmp);
    let sustain = t.get(t.len() / 2).copied();
    let decay = attack_at.zip(sustain).and_then(|(i, s)| {
        let peak = held[i..]
            .iter()
            .map(|p| p.1)
            .fold(f64::NEG_INFINITY, f64::max);
        if peak - s < MIN_DECAY_DB {
            return None;
        }
        let (p, sl) = (10f64.powf(peak / 20.0), 10f64.powf(s / 20.0));
        let target = sl + (p - sl) / std::f64::consts::E;
        let at = held[i..].iter().position(|q| q.1 == peak)? + i;
        held[at..]
            .iter()
            .find(|q| 10f64.powf(q.1 / 20.0) <= target)
            .map(|q| (q.0 - held[at].0) * 1000.0)
    });
    let release = off.and_then(|o| {
        let after: Vec<(f64, f64)> = env.iter().copied().filter(|p| p.0 >= o).collect();
        let start = after.first()?.1;
        if let Some(q) = after.iter().find(|q| q.1 <= start - 60.0) {
            return Some((q.0 - o) * 1000.0);
        }
        // Not 60 dB down by the end: the line through the first 40 dB of the fall, extrapolated.
        let fall: Vec<(f64, f64)> = after
            .iter()
            .copied()
            .take_while(|q| q.1 > start - 40.0)
            .collect();
        let (slope, _) = envelope::line_fit(&fall)?;
        (slope < 0.0 && start - fall.last()?.1 >= 10.0).then(|| -60.0 / slope * 1000.0)
    });
    [attack, decay, sustain, release]
}

/// A click at an event: the largest energy of the signal's second difference over 2 ms frames within
/// [−2, +10] ms of it, against the largest over a steady stretch, dB.
fn click(x: &[f64], rate: f64, at: usize, steady: (usize, usize)) -> Option<f64> {
    let frame = ((0.002 * rate) as usize).max(4);
    let energy = |a: usize, b: usize| {
        let mut best = 0.0f64;
        let mut i = a.max(2);
        while i + frame <= b.min(x.len()) {
            let e: f64 = (i..i + frame)
                .map(|n| (x[n] - 2.0 * x[n - 1] + x[n - 2]).powi(2))
                .sum();
            best = best.max(e);
            i += frame / 2;
        }
        best
    };
    let reference = energy(steady.0, steady.1);
    let event = energy(at.saturating_sub(frame), at + 5 * frame);
    (reference > 0.0 && event > 0.0).then(|| 10.0 * (event / reference).log10())
}

/// The analytic band of the partial at `hz`, ±0.45 of the fundamental, over `seg` less its first and
/// last 5 %, as its envelope.
fn band_envelope(seg: &[f64], rate: f64, hz: f64, f0: f64) -> Option<Vec<f64>> {
    let n = (2 * seg.len()).next_power_of_two();
    let mut re = vec![0.0; n];
    let mut im = vec![0.0; n];
    // Tapered over its first and last 5 % (a Tukey window), and only the interior returned: a
    // rectangular segment's sidelobes, cut at the band's edge, read as a line 0.45 of the fundamental
    // off it, 63 dB down, on a clean saw.
    let edge = seg.len() / 20;
    for (i, v) in seg.iter().enumerate() {
        let d = i.min(seg.len() - 1 - i);
        let w = if d < edge {
            0.5 - 0.5 * (std::f64::consts::PI * d as f64 / edge as f64).cos()
        } else {
            1.0
        };
        re[i] = v * w;
    }
    mxm_measure::spectrum::fft(&mut re, &mut im)?;
    let bin = rate / n as f64;
    let (lo, hi) = (hz - 0.45 * f0, hz + 0.45 * f0);
    for k in 0..n {
        let f = k as f64 * bin;
        let g = if k <= n / 2 && f >= lo && f <= hi {
            2.0
        } else {
            0.0
        };
        re[k] *= g;
        im[k] *= g;
    }
    mxm_measure::spectrum::ifft(&mut re, &mut im)?;
    Some(
        (edge..seg.len() - edge)
            .map(|i| re[i].hypot(im[i]))
            .collect(),
    )
}

/// Zipper sidebands: the strongest line between 50 Hz and 0.45 of the fundamental in the spectrum of
/// the strongest partial's envelope, [`ZIPPER_OVER_DB`] over the median: its rate (Hz) and its level
/// against the envelope's mean (dB). A control stepped every 64 samples at 48 kHz steps at 750 Hz.
fn zipper(env: &[f64], rate: f64, f0: f64) -> Option<(f64, f64)> {
    let mean = env.iter().sum::<f64>() / env.len().max(1) as f64;
    if mean <= 0.0 {
        return None;
    }
    // What moves slower than 10 ms taken out (a moving average subtracted, over the interior where it
    // is whole): a smooth sweep's ramp otherwise stood 20 dB over the median at the range's low end.
    let half = ((0.005 * rate) as usize).max(1);
    if env.len() <= 4 * half {
        return None;
    }
    let mut prefix = vec![0.0; env.len() + 1];
    for (i, v) in env.iter().enumerate() {
        prefix[i + 1] = prefix[i] + v;
    }
    let centred: Vec<f64> = (half..env.len() - half)
        .map(|i| env[i] - (prefix[i + half + 1] - prefix[i - half]) / (2 * half + 1) as f64)
        .collect();
    let s = spectrum(&centred, rate)?;
    let lo = (50.0 / s.bin).ceil() as usize;
    let hi = ((0.45 * f0 / s.bin).floor() as usize).min(s.mag.len() - 2);
    if hi <= lo + 4 {
        return None;
    }
    let k = (lo..=hi).max_by(|&a, &b| s.mag[a].total_cmp(&s.mag[b]))?;
    let mut sorted: Vec<f64> = s.mag[lo..=hi].to_vec();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[sorted.len() / 2];
    if s.mag[k] < median * 10f64.powf(ZIPPER_OVER_DB / 20.0) {
        return None;
    }
    let (hz, amp) = peak_near(&s, k as f64 * s.bin, 2.0 * s.bin)?;
    let level = 20.0 * (amp / mean).log10();
    (level >= ZIPPER_FLOOR_DB).then_some((hz, level))
}

/// A detuned unison: the fundamental's and the second harmonic's level beating; a unison's pairs beat
/// at the detuning times the order, so the second must beat at twice the first. The fundamental's
/// beat rate (Hz) and the detuning it means (cents).
fn unison(seg: &[f64], rate: f64, f0: f64) -> Option<(f64, f64)> {
    let track = |k: f64| -> Option<f64> {
        let env = band_envelope(seg, rate, k * f0, f0)?;
        let hop = ((0.005 * rate) as usize).max(1);
        let points: Vec<(f64, f64)> = (0..env.len())
            .step_by(hop)
            .filter(|&i| env[i] > 0.0)
            .map(|i| (i as f64 / rate, 20.0 * env[i].log10()))
            .collect();
        super::sustain::modulation(&points, hop as f64 / rate, (0.2, 40.0))
            .and_then(|m| m.1)
            .map(|m| m.0)
    };
    let first = track(1.0)?;
    let second = track(2.0)?;
    ((second / first - 2.0).abs() <= 0.3).then(|| (first, 1200.0 * (1.0 + first / f0).log2()))
}

/// A peak on the harmonic series: the harmonic standing most over a Theil–Sen line through the
/// harmonics' levels against log frequency — a resonant filter's peak on a rich wave.
fn series_peak(orders: &[(usize, f64, f64)]) -> Option<(f64, f64)> {
    let top = orders.iter().map(|o| o.2).fold(0.0, f64::max);
    let points: Vec<(f64, f64)> = orders
        .iter()
        .filter(|o| o.2 > top * 1e-4)
        .map(|o| (o.1.log2(), 20.0 * o.2.log10()))
        .collect();
    if points.len() < 6 {
        return None;
    }
    let mut slopes = Vec::new();
    for i in 0..points.len() {
        for j in i + 1..points.len() {
            slopes.push((points[j].1 - points[i].1) / (points[j].0 - points[i].0));
        }
    }
    slopes.sort_by(f64::total_cmp);
    let slope = slopes[slopes.len() / 2];
    let mut icepts: Vec<f64> = points.iter().map(|p| p.1 - slope * p.0).collect();
    icepts.sort_by(f64::total_cmp);
    let icept = icepts[icepts.len() / 2];
    points
        .iter()
        .map(|p| (2f64.powf(p.0), p.1 - (icept + slope * p.0)))
        .filter(|p| p.1 >= PEAK_OVER_DB)
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

/// The voice section, following the fundamental `f0` (Hz), with the note-off `off_s` seconds after
/// the onset where it is known.
#[must_use]
pub fn read(c: &Context, f0: Option<f64>, off_s: Option<f64>) -> Section {
    let src_grid = "a Nuttall-windowed spectrum of the held stretch: the harmonics' bins (±4 of the window's bins and 0.3 %) against the rest above 20 Hz; audibility against the harmonics' masked threshold at 94 dB SPL full scale (Schroeder et al. 1979, Johnston 1988)";
    let src_env = "the RMS over whole periods (at least two and 5 ms) every millisecond: attack to within 0.5 dB of the maximum, the decay's time constant to the sustain (its median over the held note's last 30 %), and the release from note-off to 60 dB down";
    let src_click = "the second difference's energy over 2 ms frames within −2 to +10 ms of the event, against its largest over the held stretch";
    let x = c.from_onset();
    let rate = c.rate;
    let held_end = off_s.map_or(x.len(), |o| ((o * rate) as usize).min(x.len()));
    let (a, b) = (held_end / 5, held_end.saturating_sub(held_end / 20));
    let seg = (b > a + (0.1 * rate) as usize).then(|| &x[a..b]);
    let spec = seg.and_then(|s| spectrum(s, rate));
    let f = f0
        .zip(spec.as_ref())
        .and_then(|(f, s)| refine(s, f, rate))
        .or(f0);
    let g = f.zip(spec.as_ref()).map(|(f, s)| grid(s, f, rate));
    let dc = seg
        .and_then(|s| Lines::new(s.len()).mean(s))
        .and_then(|m| (m.abs() > 1e-7).then(|| 20.0 * m.abs().log10()));
    let fundamental = g.as_ref().and_then(|g| g.orders.first().map(|o| o.2));
    let thd = g.as_ref().zip(fundamental).and_then(|(g, a1)| {
        (a1 > 0.0).then(|| {
            let rest: f64 = g.orders.iter().skip(1).take(9).map(|o| o.2 * o.2).sum();
            rest.sqrt() / a1 * 100.0
        })
    });
    // A whole number of periods, at least two and 5 ms: a window holding 2.2 periods of a sine rippled
    // by a few per cent, and a ripple's crest late in a slow decay was taken for its peak.
    let window = f.map_or((0.005 * rate) as usize, |f| {
        let periods = (0.005 * f).ceil().max(2.0);
        (periods * rate / f).round() as usize
    });
    let env = levels(c, window);
    let [attack, decay, sustain, release] = stages(&env, off_s);
    let steady = (c.onset + a, c.onset + b);
    let on_click = (b > a)
        .then(|| click(&c.x, rate, c.onset, steady))
        .flatten();
    let off_click = off_s
        .filter(|_| b > a)
        .and_then(|o| click(&c.x, rate, c.onset + (o * rate) as usize, steady));
    let strongest = g.as_ref().and_then(|g| {
        g.orders
            .iter()
            .filter(|o| o.1 < 0.45 * rate)
            .max_by(|p, q| p.2.total_cmp(&q.2))
            .map(|o| o.1)
    });
    let zip = seg
        .zip(f)
        .zip(strongest)
        .and_then(|((s, f), hz)| band_envelope(s, rate, hz, f).and_then(|e| zipper(&e, rate, f)));
    let uni = seg.zip(f).and_then(|(s, f)| unison(s, rate, f));
    let peak = g.as_ref().and_then(|g| series_peak(&g.orders));
    let readings = vec![
        Reading::new(
            "voice.aliasing",
            "Aliasing: energy off the harmonic grid against on it",
            g.as_ref().and_then(|g| g.aliasing),
            Unit::Decibels,
            src_grid,
        ),
        Reading::new(
            "voice.alias_audible",
            "The most audible off-grid part over its masked threshold (positive: heard)",
            g.as_ref().and_then(|g| g.audible),
            Unit::Decibels,
            src_grid,
        ),
        Reading::new(
            "voice.line_hz",
            "A line off the series: the filter singing, or a second oscillator",
            g.as_ref().and_then(|g| g.line).map(|l| l.0),
            Unit::Hertz,
            src_grid,
        ),
        Reading::new(
            "voice.thd",
            "Harmonic distortion: orders 2 to 10 against the fundamental",
            thd,
            Unit::Percent,
            src_grid,
        ),
        Reading::new(
            "voice.peak_hz",
            "A peak on the harmonic series (a resonant filter)",
            peak.map(|p| p.0),
            Unit::Hertz,
            "the harmonic standing most over a Theil–Sen line through the harmonics' levels against log frequency",
        ),
        Reading::new(
            "voice.peak_db",
            "The peak over the series' line",
            peak.map(|p| p.1),
            Unit::Decibels,
            "the harmonic standing most over a Theil–Sen line through the harmonics' levels against log frequency",
        ),
        Reading::new("voice.dc", "DC", dc, Unit::DecibelsFullScale, src_grid),
        Reading::new(
            "voice.attack",
            "Attack",
            attack,
            Unit::Milliseconds,
            src_env,
        ),
        Reading::new(
            "voice.decay",
            "Decay: time constant to the sustain",
            decay,
            Unit::Milliseconds,
            src_env,
        ),
        Reading::new(
            "voice.sustain",
            "Sustain against the peak",
            sustain,
            Unit::Decibels,
            src_env,
        ),
        Reading::new(
            "voice.release",
            "Release: note-off to 60 dB down",
            release,
            Unit::Milliseconds,
            src_env,
        ),
        Reading::new(
            "voice.on_click",
            "Click at note-on over the held texture",
            on_click,
            Unit::Decibels,
            src_click,
        ),
        Reading::new(
            "voice.off_click",
            "Click at note-off over the held texture",
            off_click,
            Unit::Decibels,
            src_click,
        ),
        Reading::new(
            "voice.zipper_hz",
            "Zipper: a stepped control's rate in the envelope",
            zip.map(|z| z.0),
            Unit::Hertz,
            "the spectrum of the strongest partial's envelope between 50 Hz and 0.45 of the fundamental",
        ),
        Reading::new(
            "voice.zipper",
            "Zipper sidebands against the envelope",
            zip.map(|z| z.1),
            Unit::Decibels,
            "the spectrum of the strongest partial's envelope between 50 Hz and 0.45 of the fundamental",
        ),
        Reading::new(
            "voice.unison_beat",
            "A detuned unison's beating (the second harmonic's twice as fast)",
            uni.map(|u| u.0),
            Unit::Hertz,
            "the fundamental's and the second harmonic's analytic bands, their levels' spectra between 0.2 and 40 Hz",
        ),
        Reading::new(
            "voice.unison_detune",
            "The unison's detuning",
            uni.map(|u| u.1),
            Unit::Cents,
            "the fundamental's and the second harmonic's analytic bands, their levels' spectra between 0.2 and 40 Hz",
        ),
    ];
    let mut section = Section::new("voice", "A synth voice", readings);
    if let (Some(g), Some(a1)) = (g.as_ref(), fundamental.filter(|a| *a > 0.0)) {
        section.tables.push(Table {
            id: "voice.orders",
            title: "Harmonics by order, against the fundamental".into(),
            columns: vec![
                ("Order", Unit::Plain),
                ("Frequency", Unit::Hertz),
                ("Level", Unit::Decibels),
            ],
            rows: g
                .orders
                .iter()
                .take(MAX_ORDER)
                .map(|o| {
                    vec![
                        Some(o.0 as f64),
                        Some(o.1),
                        (o.2 > 0.0).then(|| 20.0 * (o.2 / a1).log10()),
                    ]
                })
                .collect(),
            window_ms: Some((a as f64 / rate * 1000.0, b as f64 / rate * 1000.0)),
            source: src_grid,
        });
    }
    section
}
