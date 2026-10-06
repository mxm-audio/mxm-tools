//! Windowed power spectra of one segment, and the statistics the guide reads off them.
//!
//! The transform is `mxm_measure::spectrum::fft`. A segment is Hann-windowed (the symmetric window
//! the Python tools used) and zero-padded to a power of two at least `pad_to` long; every spectrum
//! carries its window length and bin spacing, because its resolution is those, not its pixels.

use mxm_measure::spectrum::{a_weight, fft};

use crate::reading::Resolution;

/// A one-sided power spectrum.
#[derive(Clone, Debug, PartialEq)]
pub struct Spectrum {
    /// Power per bin, bins `0..=n/2`.
    pub power: Vec<f64>,
    pub bin_hz: f64,
    pub window_ms: f64,
}

impl Spectrum {
    #[must_use]
    pub fn resolution(&self) -> Resolution {
        Resolution {
            window_ms: self.window_ms,
            bin_hz: Some(self.bin_hz),
            span: None,
        }
    }

    #[must_use]
    pub fn hz(&self, bin: usize) -> f64 {
        bin as f64 * self.bin_hz
    }

    /// Bins with centre frequency in `[lo_hz, hi_hz)`.
    #[must_use]
    pub fn bins(&self, lo_hz: f64, hi_hz: f64) -> std::ops::Range<usize> {
        let lo = (lo_hz / self.bin_hz).ceil().max(0.0) as usize;
        let hi = ((hi_hz / self.bin_hz).ceil() as usize).min(self.power.len());
        lo.min(hi)..hi
    }
}

/// The symmetric Hann window of `n` points (zero at both ends), as `numpy.hanning`.
#[must_use]
pub fn hann(n: usize) -> Vec<f64> {
    if n < 2 {
        return vec![1.0; n];
    }
    (0..n)
        .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (n - 1) as f64).cos())
        .collect()
}

/// The Hann-windowed power spectrum of `segment`, zero-padded to the next power of two at least
/// `pad_to` long. `None` for an empty or non-finite segment.
#[must_use]
pub fn power_spectrum(segment: &[f64], rate: f64, pad_to: usize) -> Option<Spectrum> {
    if segment.is_empty() || segment.iter().any(|v| !v.is_finite()) || rate.is_nan() || rate <= 0.0
    {
        return None;
    }
    let n = segment.len().max(pad_to).next_power_of_two();
    let window = hann(segment.len());
    let mut re = vec![0.0; n];
    let mut im = vec![0.0; n];
    for (i, (s, w)) in segment.iter().zip(&window).enumerate() {
        re[i] = s * w;
    }
    fft(&mut re, &mut im)?;
    let power = (0..=n / 2).map(|k| re[k] * re[k] + im[k] * im[k]).collect();
    Some(Spectrum {
        power,
        bin_hz: rate / n as f64,
        window_ms: segment.len() as f64 / rate * 1000.0,
    })
}

/// The power-weighted mean frequency below `max_hz`; `None` for no power.
#[must_use]
pub fn centroid(s: &Spectrum, max_hz: f64) -> Option<f64> {
    let (mut num, mut den) = (0.0, 0.0);
    for k in s.bins(0.0, max_hz) {
        num += s.hz(k) * s.power[k];
        den += s.power[k];
    }
    (den > 0.0).then(|| num / den)
}

/// The frequency below which `fraction` of the power between `lo_hz` and `hi_hz` lies.
#[must_use]
pub fn rolloff(s: &Spectrum, lo_hz: f64, hi_hz: f64, fraction: f64) -> Option<f64> {
    let bins = s.bins(lo_hz, hi_hz);
    let total: f64 = s.power[bins.clone()].iter().sum();
    if total.is_nan() || total <= 0.0 {
        return None;
    }
    let mut acc = 0.0;
    for k in bins {
        acc += s.power[k];
        if acc >= fraction * total {
            return Some(s.hz(k));
        }
    }
    None
}

/// The frequency at half the A-weighted power between `lo_hz` and `hi_hz` (`ab_resonance.py`'s
/// median): where the weighted energy sits, as the ear weights it.
#[must_use]
pub fn weighted_median(s: &Spectrum, lo_hz: f64, hi_hz: f64) -> Option<f64> {
    let bins = s.bins(lo_hz, hi_hz);
    let weighted: Vec<(f64, f64)> = bins
        .map(|k| {
            let g = a_weight(s.hz(k));
            (s.hz(k), s.power[k] * g * g)
        })
        .collect();
    let total: f64 = weighted.iter().map(|w| w.1).sum();
    if total.is_nan() || total <= 0.0 {
        return None;
    }
    let mut acc = 0.0;
    for (hz, p) in weighted {
        acc += p;
        if acc >= 0.5 * total {
            return Some(hz);
        }
    }
    None
}

/// The resonance of a spectrum between `lo_hz` and `hi_hz` (`ab_resonance.py`): the dB spectrum is
/// smoothed by a moving mean over `f·(1 ± span)`; its peak is the resonance, and its Q is that
/// frequency over the distance between the −3 dB points found walking outward. `(peak_hz, q)`; the Q
/// is `None` where a −3 dB point falls outside the band.
#[must_use]
pub fn resonance(s: &Spectrum, lo_hz: f64, hi_hz: f64, span: f64) -> Option<(f64, Option<f64>)> {
    let bins: Vec<usize> = s.bins(lo_hz.max(s.bin_hz), hi_hz).collect();
    if bins.is_empty() {
        return None;
    }
    let db: Vec<f64> = s
        .power
        .iter()
        .map(|p| 10.0 * (p.max(1e-300)).log10())
        .collect();
    // Prefix sums for the moving mean.
    let mut prefix = vec![0.0; db.len() + 1];
    for (i, v) in db.iter().enumerate() {
        prefix[i + 1] = prefix[i] + v;
    }
    let smooth: Vec<(usize, f64)> = bins
        .iter()
        .map(|&k| {
            let f = s.hz(k);
            let a = ((f * (1.0 - span)) / s.bin_hz).floor().max(0.0) as usize;
            let b = (((f * (1.0 + span)) / s.bin_hz).ceil() as usize + 1).min(db.len());
            let a = a.min(b.saturating_sub(1));
            (k, (prefix[b] - prefix[a]) / (b - a) as f64)
        })
        .collect();
    let (peak_i, &(peak_k, peak_db)) = smooth
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.1.total_cmp(&b.1.1))?;
    let walk = |step: isize| -> Option<f64> {
        let mut i = peak_i as isize;
        loop {
            i += step;
            if i < 0 || i as usize >= smooth.len() {
                return None;
            }
            if smooth[i as usize].1 <= peak_db - 3.0 {
                return Some(s.hz(smooth[i as usize].0));
            }
        }
    };
    let peak_hz = s.hz(peak_k);
    let q = match (walk(-1), walk(1)) {
        (Some(a), Some(b)) if b > a => Some(peak_hz / (b - a)),
        _ => None,
    };
    Some((peak_hz, q))
}
