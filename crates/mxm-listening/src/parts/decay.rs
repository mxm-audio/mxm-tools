//! The decay: how fast the hit falls, whether it falls in one stage or two (a fast first stage and a
//! long ring, the "gated snare" fault), how loud it still is late, and how the file ends.
//!
//! Sources: `docs/drum-model-fitting.md` §4 (`t20`/`t40`) and §7; `ab_metrics.py`, `ab_residuals.py`
//! (the −20 to −40 dB fall) and the late-ring measures the corpus fit gained in the model-drums plan's
//! revision 19.

use super::Context;
use crate::reading::{Reading, Resolution, Section, Unit};
use crate::repr::envelope;

/// Where the late level is read, ms from the onset.
pub const LATE_MS: [f64; 4] = [300.0, 500.0, 800.0, 1200.0];

#[must_use]
pub fn read(c: &Context) -> Section {
    let mut readings = Vec::new();
    let x = c.from_onset();

    // The decay envelope: a sliding RMS every 1 ms over a window of one period of the ring (2–20 ms).
    // `ab_metrics.py` used fixed 2 ms windows; on a 220 Hz tone a 2 ms window covers less than half
    // a cycle and its level swings by several dB with the phase, which moved t20 by 3 % there and
    // would move it more on a kick. **A correction to that definition, not an equivalence.**
    let (env, hop_ms, window_ms) = decay_envelope(c);
    let resolution = Resolution {
        window_ms,
        bin_hz: None,
        span: None,
    };
    let loudest = env.iter().enumerate().fold(
        (0, 0.0f64),
        |(i, m), (j, &e)| if e > m { (j, e) } else { (i, m) },
    );
    // A fall counts once the envelope stays under the level for [`STAYS_MS`]: a dip in a noisy tail
    // (a snare's wires) is not the decay. The first sliding window under the level read a snare's
    // t20 at 31 ms where `ab_metrics.py` read 54.
    let stay = (STAYS_MS / hop_ms).round().max(1.0) as usize;
    let below = |from: usize, top: f64, db: f64| -> Option<usize> {
        let level = top * 10f64.powf(-db / 20.0);
        (from..env.len()).find(|&i| env[i..(i + stay).min(env.len())].iter().all(|&e| e < level))
    };
    for (id, label, db) in [
        ("decay.t20", "Fall to −20 dB", 20.0),
        ("decay.t40", "Fall to −40 dB", 40.0),
    ] {
        let t = below(loudest.0, loudest.1, db).map(|i| (i - loudest.0) as f64 * hop_ms);
        let r = Reading::new(
            id,
            label,
            t,
            Unit::Milliseconds,
            "guide §4 `t20`/`t40`, from the loudest window, on a period-long envelope",
        )
        .resolution(resolution);
        readings.push(if t.is_none() {
            Reading {
                validity: crate::reading::Validity::Absent("the sound never falls that far"),
                ..r
            }
        } else {
            r
        });
    }

    // The fall from −20 to −40 dB: a tail that drops abruptly after a normal start shows here.
    let fall = match (
        below(loudest.0, loudest.1, 20.0),
        below(loudest.0, loudest.1, 40.0),
    ) {
        (Some(a), Some(b)) => Some((b - a) as f64 * hop_ms),
        _ => None,
    };
    readings.push(
        Reading::new(
            "decay.fall_20_40",
            "Time from −20 dB to −40 dB",
            fall,
            Unit::Milliseconds,
            "ab_residuals.py `fall_ms`, on the same envelope",
        )
        .resolution(resolution),
    );

    // The first stage's slope, from the loudest window to −20 dB, and the late ring's, 300–1200 ms:
    // two very different slopes are two stages.
    let early = below(loudest.0, loudest.1, 20.0).and_then(|end| {
        let pts: Vec<(f64, f64)> = (loudest.0..=end)
            .filter_map(|i| {
                envelope::db(env[i], loudest.1).map(|d| (i as f64 * hop_ms / 1000.0, d))
            })
            .collect();
        envelope::line_fit(&pts).map(|l| l.0)
    });
    readings.push(
        Reading::new(
            "decay.early_slope",
            "First-stage decay rate",
            early,
            Unit::DecibelsPerSecond,
            "a line through the envelope from its loudest window to −20 dB",
        )
        .resolution(resolution),
    );

    // The late ring against the loudest 10 ms in the first 30 ms (5 ms steps).
    let w10 = envelope::samples(0.01, c.rate).max(1);
    let hop = envelope::samples(0.005, c.rate).max(1);
    let reference = (0..)
        .map(|k| k * hop)
        .take_while(|&a| a < envelope::samples(0.03, c.rate))
        .filter_map(|a| envelope::rms_at(x, a, w10))
        .fold(0.0f64, f64::max);
    let w50 = envelope::samples(0.05, c.rate).max(1);
    for ms in LATE_MS {
        let a = envelope::samples(ms / 1000.0, c.rate);
        let level = envelope::rms_at(x, a, w50).and_then(|l| envelope::db(l, reference));
        let r = Reading::new(
            "decay.late_level",
            "Late level",
            level,
            Unit::Decibels,
            "the model-drums plan's revision 19: 50 ms RMS against the loudest 10 ms",
        )
        .window(ms, ms + 50.0);
        readings.push(if envelope::rms_at(x, a, w50).is_none() {
            Reading {
                validity: crate::reading::Validity::Absent("the file ends before this window"),
                ..r
            }
        } else {
            r
        });
    }
    let late_pts: Vec<(f64, f64)> = (0..)
        .map(|k| 0.3 + k as f64 * 0.01)
        .take_while(|&t| t < 1.2)
        .filter_map(|t| {
            let l = envelope::rms_at(x, envelope::samples(t, c.rate), w10)?;
            envelope::db(l, reference)
                .filter(|d| d.is_finite())
                .map(|d| (t, d))
        })
        .collect();
    let late_slope = if late_pts.len() >= 6 {
        envelope::line_fit(&late_pts).map(|l| l.0)
    } else {
        None
    };
    readings.push(
        Reading::new(
            "decay.late_slope",
            "Late ring decay rate",
            late_slope,
            Unit::DecibelsPerSecond,
            "the model-drums plan's revision 19: a line through 10 ms levels, 0.3–1.2 s",
        )
        .window(300.0, 1200.0),
    );

    // How the file ends.
    let end_level =
        envelope::rms(&c.x[c.x.len().saturating_sub(w10)..]).and_then(|l| envelope::db(l, c.peak));
    readings.push(Reading::new(
        "decay.end_level",
        "Level of the file's last 10 ms",
        end_level,
        Unit::Decibels,
        "against the peak; a file ending loud ends on a cut",
    ));
    let last_sound = c.x.iter().rposition(|&s| s != 0.0).unwrap_or(0);
    let silent_tail = c.x.len() - 1 - last_sound;
    let silence = if silent_tail > w10 {
        Reading::new(
            "decay.digital_silence",
            "Falls to digital silence at",
            Some(c.ms(last_sound.saturating_sub(c.onset))),
            Unit::Milliseconds,
            "the last non-zero sample; a model that parks early goes silent here",
        )
    } else {
        Reading::absent(
            "decay.digital_silence",
            "Falls to digital silence at",
            Unit::Milliseconds,
            "the file never falls to digital silence",
            "the last non-zero sample",
        )
    };
    readings.push(silence);

    Section::new("decay", "Decay", readings)
}

/// The shortest and longest decay-envelope window, ms.
const WINDOW_MS: (f64, f64) = (2.0, 20.0);
/// How long the envelope must stay under a level for the fall to count, ms (**chosen**).
pub const STAYS_MS: f64 = 10.0;

/// A sliding RMS every 1 ms from the onset, over one period of the strongest line below 2 kHz in the
/// 50 ms after the peak — from its spectrum, because zero crossings count a snare's wires, not its
/// ring — clamped to [`WINDOW_MS`]. A bright hit gets the 2 ms floor; a low drum a window its level
/// does not ripple in. `(envelope, hop ms, window ms)`.
pub(crate) fn decay_envelope(c: &Context) -> (Vec<f64>, f64, f64) {
    let x = c.from_onset();
    let from = c.peak_at.saturating_sub(c.onset);
    let span = envelope::samples(0.05, c.rate).min(x.len().saturating_sub(from));
    let seg = &x[from..from + span];
    let line = crate::repr::spectrum::power_spectrum(seg, c.rate, 1 << 15).and_then(|s| {
        s.bins(20.0, 2000.0)
            .max_by(|a, b| s.power[*a].total_cmp(&s.power[*b]))
            .map(|k| s.hz(k))
    });
    let period_ms = match line {
        Some(hz) if hz > 0.0 => 1000.0 / hz,
        _ => WINDOW_MS.1,
    };
    let window_ms = period_ms.clamp(WINDOW_MS.0, WINDOW_MS.1);
    let w = envelope::samples(window_ms / 1000.0, c.rate).max(1);
    let hop = envelope::samples(0.001, c.rate).max(1);
    let mut prefix = vec![0.0; x.len() + 1];
    for (i, v) in x.iter().enumerate() {
        prefix[i + 1] = prefix[i] + v * v;
    }
    let env = (0..)
        .map(|k| k * hop)
        .take_while(|&a| a + w <= x.len())
        .map(|a| ((prefix[a + w] - prefix[a]) / w as f64).max(0.0).sqrt())
        .collect();
    (env, hop as f64 / c.rate * 1000.0, window_ms)
}
