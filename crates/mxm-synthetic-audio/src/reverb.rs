//! Set 9's room: a mono impulse response of exponentially decaying noise, its decay set by
//! RT60 in three bands, after Moorer, *About this reverberation business*, Computer Music Journal
//! 3(2), 1979 (noise with an exponential envelope as a room's late field), with a pre-delay and a
//! few early reflections. The direct sound is in it, so a file convolved with it is the dry file
//! in a room. The ground truth stays the dry notes'.

use crate::dsp::{Biquad, Rng, db_to_gain, smoothstep, white};
use crate::json::{Json, obj};

/// The reverberant energy against the direct sound's, dB, for an RT60: a longer room is a
/// larger one, heard from further back.
fn direct_to_reverberant(rt60: f64) -> f64 {
    4.0 - 3.5 * (rt60 / 0.5).log2()
}

/// The impulse response for `rt60` s (the mid band's; the lows decay 1.2 times as long, the highs
/// 0.6 times), and what it is.
pub fn impulse(rt60: f64, rate: f64, seed: u64) -> (Vec<f64>, Json) {
    let mut rng = Rng::stream(seed, "room");
    let predelay = 0.008 + 0.004 * rt60;
    let length = 1.6 * rt60 * 1.2 + predelay + 0.1;
    let n = (length * rate).round() as usize;
    let mut late = vec![0.0; n];
    let bands = [
        (Biquad::lowpass(rate, 500.0, 0.707), 1.2 * rt60),
        (Biquad::bandpass(rate, 1400.0, 0.5), rt60),
        (Biquad::highpass(rate, 4000.0, 0.707), 0.6 * rt60),
    ];
    for (filter, rt) in bands {
        let mut noise = white(&mut rng, n);
        let mut f = filter;
        f.run_all(&mut noise);
        for (i, v) in noise.iter().enumerate() {
            let t = i as f64 / rate;
            if t >= predelay {
                // -60 dB at `rt`: exp(-6.908 t / rt).
                late[i] +=
                    v * (-6.908 * (t - predelay) / rt).exp() * smoothstep((t - predelay) / 0.02);
            }
        }
    }
    // Early reflections: eight taps in the 60 ms after the pre-delay, falling, of either sign.
    let mut early = vec![0.0; n];
    for k in 0..8 {
        let t = predelay + rng.range(0.0, 0.06);
        let i = (t * rate) as usize;
        let sign = if rng.unit() < 0.5 { -1.0 } else { 1.0 };
        early[i] += sign * db_to_gain(-4.0 - 1.2 * k as f64 - rng.range(0.0, 3.0));
    }
    // Scale the reflected part to the direct-to-reverberant ratio; the direct sound is 1.
    let energy: f64 = late.iter().chain(&early).map(|v| v * v).sum();
    let wanted = db_to_gain(-direct_to_reverberant(rt60)).powi(2);
    let scale = (wanted / energy.max(1e-300)).sqrt();
    let mut ir: Vec<f64> = late
        .iter()
        .zip(&early)
        .map(|(l, e)| (l + e) * scale)
        .collect();
    ir[0] += 1.0;
    let params = obj(vec![
        ("reverb_rt60_s", rt60.into()),
        ("rt60_s_low_mid_high", Json::from(vec![1.2 * rt60, rt60, 0.6 * rt60])),
        ("band_edges_hz", Json::from(vec![500.0, 4000.0])),
        ("predelay_ms", (predelay * 1000.0).into()),
        ("direct_to_reverberant_db", direct_to_reverberant(rt60).into()),
        ("impulse", "decaying noise in three bands, eight early reflections, the direct sound (Moorer 1979)".into()),
        ("truth", "the dry notes".into()),
    ]);
    (ir, params)
}

/// `audio` in the room: convolved with the impulse, longer by its length.
pub fn apply(audio: &[f64], rt60: f64, rate: f64, seed: u64) -> (Vec<f64>, Json) {
    let (ir, params) = impulse(rt60, rate, seed);
    (crate::dsp::convolve(audio, &ir), params)
}
