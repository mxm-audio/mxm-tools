//! Levels and envelopes over windows.

/// The RMS of a slice; `None` if it is empty.
#[must_use]
pub fn rms(x: &[f64]) -> Option<f64> {
    if x.is_empty() {
        return None;
    }
    Some((x.iter().map(|s| s * s).sum::<f64>() / x.len() as f64).sqrt())
}

/// The RMS of non-overlapping windows of `w` samples; a short last window is kept.
#[must_use]
pub fn windowed_rms(x: &[f64], w: usize) -> Vec<f64> {
    if w == 0 {
        return Vec::new();
    }
    x.chunks(w).filter_map(rms).collect()
}

/// The RMS over `[from, from + w)`; `None` where the window leaves the signal.
#[must_use]
pub fn rms_at(x: &[f64], from: usize, w: usize) -> Option<f64> {
    if w == 0 || from + w > x.len() {
        return None;
    }
    rms(&x[from..from + w])
}

/// A level ratio in decibels (amplitude). `None` for a non-positive reference; a silent level is −∞,
/// which a reading reports as absent.
#[must_use]
pub fn db(level: f64, reference: f64) -> Option<f64> {
    if reference.is_nan() || reference <= 0.0 || !level.is_finite() {
        return None;
    }
    Some(mxm_measure::convert::amplitude_db(level / reference))
}

/// Seconds to samples at `rate`, rounded down.
#[must_use]
pub fn samples(seconds: f64, rate: f64) -> usize {
    (seconds * rate).max(0.0) as usize
}

/// A least-squares straight line through `(t, y)` points: `(slope, intercept)`; `None` for fewer than
/// two distinct `t`.
#[must_use]
pub fn line_fit(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    let n = points.len() as f64;
    if points.len() < 2 {
        return None;
    }
    let mt = points.iter().map(|p| p.0).sum::<f64>() / n;
    let my = points.iter().map(|p| p.1).sum::<f64>() / n;
    let sxx: f64 = points.iter().map(|p| (p.0 - mt).powi(2)).sum();
    if sxx <= 0.0 {
        return None;
    }
    let sxy: f64 = points.iter().map(|p| (p.0 - mt) * (p.1 - my)).sum();
    let slope = sxy / sxx;
    Some((slope, my - slope * mt))
}
