//! Set 2, wide-vibrato: held notes of 1 to 4 s whose vibrato is ±50, ±100 or ±150 cents at 4.5 to
//! 7 Hz, its rate and depth wandering 10 to 20 % over the note, and the level louder at the upper
//! turn (or the lower) by 3, 6 or 10 dB. Some notes start straight and grow their vibrato after
//! 0.3 s. The truth is the midline. File 01 holds them; file 02, the control, the same notes with
//! even levels on both turns. Additive: the tone's power is held to its midline's, so the turns'
//! levels differ by the case's asymmetry alone (measured back into the sidecar).

use super::*;
use std::f64::consts::TAU;

pub const INSTRUMENTS: [&str; 3] = ["soprano", "tenor", "violin"];

fn range(instrument: &str) -> (f64, f64) {
    match instrument {
        "soprano" => (60.0, 81.0),
        "tenor" => (50.0, 67.0),
        "violin" => (62.0, 88.0),
        other => panic!("no vibrato range for {other}"),
    }
}

#[derive(Clone, Copy, Debug)]
struct Held {
    midi: f64,
    depth: f64,
    rate: f64,
    rate_wander: f64,
    depth_wander: f64,
    /// Louder at the upper turn by this much (negative: at the lower), dB.
    asymmetry: f64,
    /// The vibrato starts this long after the onset (0: from the start).
    grow_after: f64,
    length: f64,
}

/// A held note with vibrato. The same `seed` gives the same note, whatever its asymmetry.
fn held(profile: &Profile, h: &Held, rate: f64, seed: u64, control: bool) -> Case {
    let mut rng = Rng::stream(seed, "note");
    let mut case = Case::new(
        if control {
            "vibrato, even turns"
        } else {
            "wide vibrato"
        },
        "additive",
        control,
    );
    let (hz, detune) = centre(&mut rng, h.midi);
    let duration = h.length + profile.release * 1.3;
    let n = (duration * rate).round() as usize;
    let jit = jitter(&mut rng, n, rate, 1.5);
    // The wanders: slow curves, kept within ±1.5 of their scale.
    let rate_curve = slow_noise(&mut rng, n, rate, 0.7);
    let depth_curve = slow_noise(&mut rng, n, rate, 0.7);
    let phase0 = rng.range(0.0, TAU);
    // The phase is the integral of the wandering rate.
    let mut phase = Vec::with_capacity(n);
    let mut p = phase0;
    for &w in &rate_curve {
        phase.push(p);
        p += TAU * h.rate * (1.0 + h.rate_wander * w.clamp(-1.5, 1.5)) / rate;
    }
    let level = db_to_gain(-6.0);
    let grow = |t: f64| -> f64 {
        if h.grow_after == 0.0 {
            smoothstep((t - 0.05) / 0.15)
        } else {
            smoothstep((t - h.grow_after) / 0.5)
        }
    };
    let cents_at = |i: usize, t: f64| -> f64 {
        let depth = h.depth * (1.0 + h.depth_wander * depth_curve[i].clamp(-1.5, 1.5)) * grow(t);
        depth * phase[i].sin()
    };
    let asym = if control { 0.0 } else { h.asymmetry };
    let mut line = Line::from_fn(rate, duration, |t| {
        let i = ((t * rate) as usize).min(n - 1);
        let swing = phase[i].sin() * grow(t);
        let amp = level
            * db_to_gain(asym / 2.0 * swing)
            * attack(t, profile.attack)
            * release(t - h.length, profile.release);
        (
            hz * 2f64.powf((cents_at(i, t) + jit[i]) / 1200.0),
            amp,
            amp,
            0.7,
        )
    });
    line.centre_hz = Some(vec![hz; n]);
    let onset = line
        .amp
        .iter()
        .position(|&a| a >= 0.01 * level)
        .unwrap_or(0) as f64
        / rate;
    let end = h.length + profile.release;
    additive_case(&mut case, profile, &line, rate, &mut rng, 0.0, move |t| {
        t >= onset && t < end
    });
    let measured = turn_levels(
        &case.audio,
        rate,
        &phase,
        onset + h.grow_after + 0.5,
        h.length - 0.1,
    );
    case.notes.push(Note {
        onset,
        end,
        hz,
        params: obj(vec![("detune_cents", detune.into())]),
    });
    case.params = obj(vec![
        ("note_s", h.length.into()),
        ("vibrato_cents", h.depth.into()),
        ("vibrato_hz", h.rate.into()),
        ("rate_wander", h.rate_wander.into()),
        ("depth_wander", h.depth_wander.into()),
        (
            "louder_turn",
            (if asym > 0.0 {
                "upper"
            } else if asym < 0.0 {
                "lower"
            } else {
                "even"
            })
            .into(),
        ),
        ("turn_level_difference_db", asym.abs().into()),
        ("measured_upper_minus_lower_db", measured.into()),
        (
            "measured_how",
            "RMS within 1/8 cycle of each upper turn against each lower, so 90 % of a peak-to-peak asymmetry".into(),
        ),
        ("vibrato_starts_after_s", h.grow_after.into()),
        (
            "level",
            "tone power held to the midline's; the turns differ by the asymmetry alone".into(),
        ),
    ]);
    case
}

/// The level at the upper turns minus the lower, dB: RMS within ±1/8 cycle of each turn.
fn turn_levels(audio: &[f64], rate: f64, phase: &[f64], from: f64, to: f64) -> f64 {
    let (mut up, mut nu, mut down, mut nd) = (0.0, 0usize, 0.0, 0usize);
    let a = (from * rate) as usize;
    let b = ((to * rate) as usize).min(audio.len()).min(phase.len());
    for i in a..b {
        let s = phase[i].sin();
        if s > (TAU / 8.0).cos() {
            up += audio[i] * audio[i];
            nu += 1;
        } else if s < -(TAU / 8.0).cos() {
            down += audio[i] * audio[i];
            nd += 1;
        }
    }
    if nu == 0 || nd == 0 {
        return f64::NAN;
    }
    10.0 * ((up / nu as f64) / (down / nd as f64)).log10()
}

pub fn files(ctx: &mut Context, instrument: &str) -> Result<Vec<Take>> {
    let profile = additive::profile(instrument);
    let (lo, hi) = range(instrument);
    let seed = seed_of(&format!("wide-vibrato-{instrument}"));
    let mut draw = Rng::stream(seed, "notes");
    let mut notes = Vec::new();
    let mut k = 0;
    for &depth in &[50.0, 100.0, 150.0] {
        for &level in &[3.0, 6.0, 10.0] {
            let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
            notes.push((depth, sign * level, 0.0));
            k += 1;
        }
    }
    // Three that start straight and grow their vibrato after 0.3 s.
    for (j, &depth) in [50.0, 100.0, 150.0].iter().enumerate() {
        notes.push((depth, [6.0, -10.0, 3.0][j], 0.3));
    }
    let helds: Vec<Held> = notes
        .iter()
        .map(|&(depth, asymmetry, grow_after)| Held {
            midi: draw.range(lo, hi).round(),
            depth,
            rate: draw.range(4.5, 7.0),
            rate_wander: draw.range(0.10, 0.20),
            depth_wander: draw.range(0.10, 0.20),
            asymmetry,
            grow_after,
            length: draw.range(1.0, 4.0),
        })
        .collect();
    let mut order: Vec<usize> = (0..helds.len()).collect();
    // Shuffled, so depths and asymmetries don't come in a run.
    for i in (1..order.len()).rev() {
        let j = (draw.unit() * (i + 1) as f64) as usize;
        order.swap(i, j.min(i));
    }
    let mut takes = Vec::new();
    for (nn, control) in [(1, false), (2, true)] {
        let cases = order
            .iter()
            .map(|&i| {
                held(
                    &profile,
                    &helds[i],
                    ctx.rate,
                    Rng::stream(seed, &format!("held{i}")).unit().to_bits(),
                    control,
                )
            })
            .collect();
        takes.push(take(
            "wide-vibrato",
            instrument,
            nn,
            ctx.rate,
            cases,
            &mut Rng::stream(seed, "gaps"),
            obj(vec![(
                "pairing",
                Json::from(if control {
                    "the notes of file 01, in the same order, with even levels on both turns"
                } else {
                    "file 02 holds these notes with even levels: the control"
                }),
            )]),
        ));
    }
    Ok(takes)
}
