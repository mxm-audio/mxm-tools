//! Preparation: decoding to mono, the onset, the trim, and the K-weighted body loudness two sounds
//! are level-matched on.
//!
//! **These are the drum A/B page's own definitions** (`docs/drum-model-fitting.md` §2), moved here
//! operation for operation so the page and every report read a sound the same way. The plan's first
//! migration stage (`plans/plan-mxm-listening.md` §1) requires the page to render bit-identically
//! through them, so the arithmetic, its order and its `f32`/`f64` boundaries are part of the contract:
//! a change here is a correction to every page and report, measured and approved, never a tidy-up.

use std::path::Path;

use mxm_audio_file_decode::{AtLimit, Keep, Limits};
pub use mxm_audio_file_decode::{Codec, Container};

/// The onset threshold for a percussive hit: the first sample at 1 % of the peak (−40 dB). Every
/// Python tool but one used it; one definition, used by every analyser (the guide's trap list).
pub const ONSET_FRACTION: f32 = 0.01;

/// The most frames [`decode_mono`] keeps: 30 s at 48 kHz.
pub const HIT_MAX_FRAMES: usize = 48_000 * 30;

/// What a decode kept of a file, beside its samples: what a caller says rather than hides — that a
/// stereo file was folded to mono, or a long one cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kept {
    /// Channels the file holds.
    pub source_channels: usize,
    /// Channels averaged into the mono sum.
    pub channels: usize,
    /// The file held audio past the frame limit, which was not kept.
    pub cut: bool,
    pub codec: Codec,
    pub container: Container,
}

/// Decodes a file and averages its channels to mono (up to eight channels and 30 s at 48 kHz).
///
/// The limit counts frames, not seconds, and decoding **stops silently** at it: a file at 96 kHz keeps
/// its first 15 s. A hit is far shorter; a long file goes through [`decode_mono_whole`].
pub fn decode_mono(path: &Path) -> Result<(Vec<f32>, u32), String> {
    decode_mono_up_to(path, HIT_MAX_FRAMES)
        .map(|(mono, rate, _)| (mono, rate))
        .map_err(|e| format!("{e:?}"))
}

/// [`decode_mono`], sample for sample, with what the decode kept of the file. The error is the
/// decoder's own sentence, which never names the path.
///
/// # Errors
/// When the file cannot be decoded.
pub fn decode_mono_kept(path: &Path) -> Result<(Vec<f32>, u32, Kept), String> {
    decode_mono_up_to(path, HIT_MAX_FRAMES).map_err(|e| e.to_string())
}

/// The most frames [`decode_mono_whole`] keeps: ten minutes at 96 kHz.
pub const WHOLE_MAX_FRAMES: usize = 96_000 * 600;

/// Decodes a whole file to mono by [`decode_mono`]'s arithmetic, for a run of notes that lasts
/// minutes; `Err` rather than a silent cut when the file is longer than [`WHOLE_MAX_FRAMES`].
///
/// # Errors
/// When the file cannot be decoded, or holds more than [`WHOLE_MAX_FRAMES`] frames.
pub fn decode_mono_whole(path: &Path) -> Result<(Vec<f32>, u32), String> {
    let (mono, rate, kept) =
        decode_mono_up_to(path, WHOLE_MAX_FRAMES).map_err(|e| format!("{e:?}"))?;
    if kept.cut {
        return Err(format!(
            "{}: longer than {WHOLE_MAX_FRAMES} frames",
            path.display()
        ));
    }
    Ok((mono, rate))
}

/// Decodes a whole file keeping its channels (at most two, and [`WHOLE_MAX_FRAMES`] frames), for a
/// response read against its stimulus: a stereo effect's two outputs are two readings.
///
/// # Errors
/// When the file cannot be decoded, holds more than two channels, or more than
/// [`WHOLE_MAX_FRAMES`] frames.
pub fn decode_channels(path: &Path) -> Result<(Vec<Vec<f32>>, u32), String> {
    let limits = Limits::new(WHOLE_MAX_FRAMES, AtLimit::Stop, Keep::AllUpTo(8));
    let decoded =
        mxm_audio_file_decode::decode_file(path, &limits).map_err(|e| format!("{e:?}"))?;
    if decoded.more_existed {
        return Err(format!(
            "{}: longer than {WHOLE_MAX_FRAMES} frames",
            path.display()
        ));
    }
    let n = decoded.channels.max(1);
    if n > 2 {
        return Err(format!(
            "{}: {n} channels; a response has one or two",
            path.display()
        ));
    }
    let channels = (0..n)
        .map(|c| {
            decoded
                .interleaved
                .iter()
                .skip(c)
                .step_by(n)
                .copied()
                .collect()
        })
        .collect();
    Ok((channels, decoded.sample_rate))
}

/// The mono sum of at most `max_frames` frames, its rate, and what the decode kept.
fn decode_mono_up_to(
    path: &Path,
    max_frames: usize,
) -> Result<(Vec<f32>, u32, Kept), mxm_audio_file_decode::Error> {
    let limits = Limits::new(max_frames, AtLimit::Stop, Keep::AllUpTo(8));
    let decoded = mxm_audio_file_decode::decode_file(path, &limits)?;
    let mono = decoded
        .interleaved
        .chunks(decoded.channels.max(1))
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect();
    let kept = Kept {
        source_channels: decoded.source_channels,
        channels: decoded.channels,
        cut: decoded.more_existed,
        codec: decoded.codec,
        container: decoded.container,
    };
    Ok((mono, decoded.sample_rate, kept))
}

/// The largest absolute sample; zero for an empty buffer. The page's definition, kept as it is —
/// `mxm_measure::level::peak` is the ruler that reports a non-finite sample as absent, and the
/// analysers use that.
pub fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0, |m, s| m.max(s.abs()))
}

/// The first sample at [`ONSET_FRACTION`] of the peak; `None` for silence.
pub fn onset(x: &[f32]) -> Option<usize> {
    let p = peak(x);
    if p == 0.0 || !p.is_finite() {
        return None;
    }
    x.iter().position(|s| s.abs() >= ONSET_FRACTION * p)
}

/// Drops leading silence to 5 ms before the onset and the tail after the last sample within 70 dB of
/// the peak, keeping 50 ms of it, so both sides start together and neither ends in minutes of zero.
/// A 1 ms fade in and a 5 ms fade out: neither end may step, whatever the file brought.
pub fn trim(x: &[f32], rate: u32) -> Vec<f32> {
    let p = peak(x);
    if p == 0.0 {
        return x.to_vec();
    }
    let onset = x.iter().position(|s| s.abs() >= 0.01 * p).unwrap_or(0);
    let start = onset.saturating_sub(rate as usize / 200);
    let last = x
        .iter()
        .rposition(|s| s.abs() >= p * 10f32.powf(-70.0 / 20.0))
        .unwrap_or(x.len() - 1);
    let end = (last + rate as usize / 20).min(x.len());
    let mut out = x[start..end].to_vec();
    let n = out.len();
    let rise = (rate as usize / 1000).min(n);
    for (i, sample) in out[..rise].iter_mut().enumerate() {
        *sample *= (i as f32 + 0.5) / rise as f32;
    }
    let fall = (rate as usize / 200).min(n);
    for (i, sample) in out[n - fall..].iter_mut().enumerate() {
        *sample *= 1.0 - (i as f32 + 0.5) / fall as f32;
    }
    out
}

/// A biquad's feed-forward `b` and feedback `a` coefficients (`a₀ = 1`).
pub type Biquad = ([f64; 3], [f64; 2]);

/// ITU-R BS.1770 K-weighting's two stages at `rate`: the high shelf, then the high-pass, designed for
/// any sample rate with the analogue-matched parameters `libebur128` publishes (the standard tabulates
/// 48 kHz only).
pub fn k_weighting(rate: u32) -> [Biquad; 2] {
    let fs = f64::from(rate);
    let shelf = {
        let (f0, gain_db, q) = (1681.974450955533, 3.999843853973347, 0.7071752369554196);
        let k = (std::f64::consts::PI * f0 / fs).tan();
        let vh = 10f64.powf(gain_db / 20.0);
        let vb = vh.powf(0.4996667741545416);
        let a0 = 1.0 + k / q + k * k;
        (
            [
                (vh + vb * k / q + k * k) / a0,
                2.0 * (k * k - vh) / a0,
                (vh - vb * k / q + k * k) / a0,
            ],
            [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
        )
    };
    let high_pass = {
        let (f0, q) = (38.13547087602444, 0.5003270373238773);
        let k = (std::f64::consts::PI * f0 / fs).tan();
        let a0 = 1.0 + k / q + k * k;
        (
            [1.0, -2.0, 1.0],
            [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
        )
    };
    [shelf, high_pass]
}

/// The K-weighted signal, in `f64`.
pub fn k_weighted(x: &[f32], rate: u32) -> Vec<f64> {
    let mut y: Vec<f64> = x.iter().map(|&s| f64::from(s)).collect();
    for (b, a) in k_weighting(rate) {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        for s in &mut y {
            let x0 = *s;
            let y0 = b[0] * x0 + b[1] * x1 + b[2] * x2 - a[0] * y1 - a[1] * y2;
            (x2, x1, y2, y1) = (x1, x0, y1, y0);
            *s = y0;
        }
    }
    y
}

/// Loudness over the loud body of the hit, what two drums are matched on before anyone compares
/// them: K-weighted (BS.1770) power in 5 ms windows over the first second, counting only windows
/// within 20 dB of the loudest, as an RMS amplitude. Plain energy let a recording's sub-audio leak,
/// hum or file-end artefact count as body, and let a model whose energy sat where the ear is less
/// sensitive sound quieter at "equal" level; the owner heard both as "the original is louder".
pub fn body_rms(x: &[f32], rate: u32) -> f32 {
    let window = (rate / 200) as usize;
    let weighted = k_weighted(&x[..x.len().min(rate as usize)], rate);
    let windows: Vec<f64> = weighted
        .chunks(window)
        .map(|w| w.iter().map(|s| s * s).sum::<f64>() / w.len() as f64)
        .collect();
    let loudest = windows.iter().copied().fold(0.0, f64::max);
    let body: Vec<f64> = windows
        .iter()
        .copied()
        .filter(|w| *w >= 0.01 * loudest)
        .collect();
    (body.iter().sum::<f64>() / body.len().max(1) as f64).sqrt() as f32
}
