//! Resynthesis from the listener's own readings — the owner's self-test (the plan's revision 9): "if you
//! could resynthesize the sound, then your measurements must have been correct."
//!
//! A sound is rebuilt from what the listener measured: its first [`CONTACT_MS`] kept as recorded (the
//! stick's contact is not free vibration, and no mode model holds it); from there the modes of each mode
//! window (`parts::pitch`), crossfaded window to window; and everything the modes do not explain — the
//! residual — rebuilt as noise, octave by octave, following the residual's own 2 ms envelope, so the
//! wires' clustering survives, not only their level. `compare` then reads the rebuild against the
//! original: whatever it finds is something the readings do not hold.
//!
//! **Techniques:** sinusoids plus a noise residual (Serra & Smith, *Spectral Modeling Synthesis*, CMJ
//! 1990), here with damped modes from subband ESPRIT (`repr::modes`) and octave-band envelopes for the
//! residual; written from those descriptions.

use crate::family::Family;
use crate::parts::{Context, pitch};
use crate::repr::{bands, envelope};
use crate::sound::Sound;

/// The contact kept as recorded, ms from the onset.
pub const CONTACT_MS: f64 = 10.0;
/// The crossfade between mode windows, and into the rebuilt part after the contact, ms.
pub const CROSSFADE_MS: f64 = 5.0;
/// The residual's envelope frame, ms: short enough to keep the wires' 1–20 ms clustering.
pub const ENVELOPE_MS: f64 = 2.0;

/// The rebuild and its parts, sample for sample: the modes, the noise, and the whole.
#[derive(Clone, Debug)]
pub struct Rebuilt {
    pub modal: Vec<f64>,
    pub noise: Vec<f64>,
    pub sound: Sound,
}

/// The sound rebuilt from its readings; `None` for a sound the listener cannot measure.
#[must_use]
pub fn resynthesize(sound: &Sound, room_lines: &[f64]) -> Option<Sound> {
    rebuild(sound, room_lines).map(|r| r.sound)
}

/// The rebuild with its parts, for finding which part a self-test's difference comes from: a
/// percussive hit's.
#[must_use]
pub fn rebuild(sound: &Sound, room_lines: &[f64]) -> Option<Rebuilt> {
    rebuild_with(sound, room_lines, None)
}

/// [`rebuild`] as the family reads the sound: a pitched, held or synth note from a note's analysis —
/// its bands to 20 kHz and its steady partials kept, from a note's onset — so the rebuild holds what
/// the note's readings hold; the drum analysis stops at 2 kHz and keeps only decaying modes, and a
/// note's upper partials came back as noise.
#[must_use]
pub fn rebuild_with(sound: &Sound, room_lines: &[f64], family: Option<Family>) -> Option<Rebuilt> {
    let mut c = Context::new(&sound.samples, sound.rate)?;
    let note = matches!(
        family,
        Some(Family::Note | Family::Sustained | Family::Voice)
    );
    let windows = if note {
        if let Some(at) = crate::parts::note::onset(&c.raw, sound.rate) {
            c.onset = at;
        }
        pitch::analyse_note(&c)
    } else {
        pitch::analyse(&c)
    };
    let _ = room_lines;
    let n = c.x.len();
    let rate = c.rate;
    let contact = c.at(CONTACT_MS).min(n);
    let fade = envelope::samples(CROSSFADE_MS / 1000.0, rate).max(1);

    // The modal part: modes linked window to window into tracks (the same mode within
    // [`SAME_TRACK`]), each track one continuous sinusoid — its amplitude geometric between the
    // windows' estimates and decaying past the last, its phase the cubic that meets each window's
    // measured frequency and phase at its start — as sinusoidal modelling tracks partials (McAulay &
    // Quatieri, IEEE TASSP 1986). Crossfading each window's mode set instead made a gliding mode jump at
    // every window's edge; a phase merely integrated from the first window drifted from the recording's,
    // and the residual (recording minus modes) carried the ring back as noise.
    // The rebuilt parts start under the contact's crossfade, so the fade blends two sounds rather than
    // the recording into silence: a strong mode switched on at full level made a step, and the step
    // was a 50–100 Hz excess the self-test reported.
    let from = contact.saturating_sub(fade);
    let mut modal = vec![0.0; n];
    // Each window's modes are read at its start (the amplitude's reference); a track starting in the
    // first window sounds from `from`, extrapolated back along its decay.
    let starts: Vec<usize> = windows.iter().map(|w| c.at(w.window_ms.0).min(n)).collect();
    let ends: Vec<usize> = windows.iter().map(|w| c.at(w.window_ms.1).min(n)).collect();
    for track in tracks(&windows) {
        let (first, _) = track[0];
        let (last, _) = *track.last().expect("a track has a mode");
        let begin = if first == 0 {
            from.min(starts[0])
        } else {
            starts[first].max(from)
        };
        // A track the next window lacks because its mode decayed under that window's floor keeps
        // decaying to the end; one the next window should have held (a gliding mode moved on) fades
        // out past its own. Cutting a decayed mode off bent a note's fastest partials' fall, and the
        // line through it put their strike 2 dB high.
        let (_, lm) = *track.last().expect("a track has a mode");
        let decayed_away = last + 1 < windows.len() && {
            let t = (starts[last + 1].saturating_sub(starts[last])) as f64 / rate;
            let there = lm.amplitude * (-lm.decay * t).exp();
            envelope::db(there, c.peak).is_none_or(|d| d < pitch::LEVEL_FLOOR_DB)
        };
        let ends_early = last + 1 < windows.len() && !decayed_away;
        let finish = if ends_early {
            (ends[last] + fade).min(n)
        } else {
            n
        };
        for (j, out) in modal.iter_mut().enumerate().take(finish).skip(begin) {
            // Which anchors bracket this sample.
            let k = track
                .iter()
                .rposition(|&(w, _)| starts[w] <= j)
                .unwrap_or(0);
            let (wk, mk) = track[k];
            let (amp, phase) = if j < starts[wk] {
                // Before the first window: back along its decay, at its frequency.
                let tau = (starts[wk] - j) as f64 / rate;
                (
                    mk.amplitude * (mk.decay * tau).exp(),
                    mk.phase - std::f64::consts::TAU * mk.hz * tau,
                )
            } else if k + 1 < track.len() {
                let (wn, mn) = track[k + 1];
                let span = (starts[wn] - starts[wk]).max(1) as f64 / rate;
                let t = (j - starts[wk]) as f64 / rate;
                let u = t / span;
                let amp = if mk.amplitude > 0.0 && mn.amplitude > 0.0 {
                    mk.amplitude * (mn.amplitude / mk.amplitude).powf(u)
                } else {
                    mk.amplitude
                };
                (amp, cubic_phase(&mk, &mn, span, t))
            } else {
                let t = (j - starts[wk]) as f64 / rate;
                (
                    mk.amplitude * (-mk.decay * t).exp(),
                    mk.phase + std::f64::consts::TAU * mk.hz * t,
                )
            };
            // A track that appears after the first window fades in; one that ends before the last fades
            // out past its window.
            let mut weight = 1.0;
            if first > 0 && j < begin + fade {
                weight *= raised((j - begin) as f64 / fade as f64);
            }
            if ends_early && j + fade > finish {
                weight *= raised((finish - j) as f64 / fade as f64);
            }
            *out += weight * amp * phase.cos();
        }
    }

    // The residual, rebuilt as noise following its octave envelopes: each band's white noise shaped by
    // a gain eased between frame centres (a gain stepped every frame splattered energy into the
    // neighbouring bands), band-passed again so the shaping stays in its band, and then corrected once
    // against its target, since neighbouring bands' skirts overlap.
    // The residual fades in with the crossfade: a residual cut in at full level carried the modes'
    // start as a step, and the band filters smeared the step into 50–140 Hz noise.
    let residual: Vec<f64> = (0..n)
        .map(|j| {
            if j >= from {
                raised((j - from) as f64 / fade as f64) * (c.x[j] - modal[j])
            } else {
                0.0
            }
        })
        .collect();
    let white = white_noise(n);
    let frame = envelope::samples(ENVELOPE_MS / 1000.0, rate).max(1);
    let edges = noise_bands(rate);
    let mut targets = Vec::new();
    let mut gains = Vec::new();
    for &(lo, hi) in &edges {
        let (Some(r), Some(z)) = (
            bands::band(&residual, rate, lo, hi),
            bands::band(&white, rate, lo, hi),
        ) else {
            targets.push(Vec::new());
            gains.push(Vec::new());
            continue;
        };
        let target = envelope::windowed_rms(&r, frame);
        let own = envelope::windowed_rms(&z, frame);
        gains.push(
            target
                .iter()
                .zip(&own)
                .map(|(t, o)| if *o > 0.0 { t / o } else { 0.0 })
                .collect::<Vec<f64>>(),
        );
        targets.push(target);
    }
    let shaped = |gains: &[Vec<f64>]| -> Vec<Vec<f64>> {
        edges
            .iter()
            .zip(gains)
            .map(|(&(lo, hi), g)| {
                if g.is_empty() {
                    return Vec::new();
                }
                let z = bands::band(&white, rate, lo, hi).unwrap_or_default();
                let modulated: Vec<f64> = (0..n)
                    .map(|j| {
                        if j >= from {
                            z.get(j).copied().unwrap_or(0.0) * eased(g, j, frame)
                        } else {
                            0.0
                        }
                    })
                    .collect();
                bands::band(&modulated, rate, lo, hi).unwrap_or_default()
            })
            .collect()
    };
    let sum = |parts: &[Vec<f64>]| -> Vec<f64> {
        let mut out = vec![0.0; n];
        for p in parts {
            for (o, v) in out.iter_mut().zip(p) {
                *o += v;
            }
        }
        out
    };
    let first = sum(&shaped(&gains));
    for ((&(lo, hi), g), target) in edges.iter().zip(gains.iter_mut()).zip(&targets) {
        let Some(got) = bands::band(&first, rate, lo, hi) else {
            continue;
        };
        let got = envelope::windowed_rms(&got, frame);
        for ((gk, t), o) in g.iter_mut().zip(target).zip(&got) {
            if *o > 0.0 {
                *gk *= (t / o).clamp(1.0 / CORRECTION_LIMIT, CORRECTION_LIMIT);
            }
        }
    }
    let noise = sum(&shaped(&gains));

    // The recorded contact, crossfaded into the rebuilt part.
    let out: Vec<f32> = (0..n)
        .map(|j| {
            let rebuilt = modal[j] + noise[j];
            let v = if j < from {
                c.x[j]
            } else if j >= contact {
                rebuilt
            } else {
                let w = raised((j + fade - contact) as f64 / fade as f64);
                (1.0 - w) * c.x[j] + w * rebuilt
            };
            v as f32
        })
        .collect();
    Some(Rebuilt {
        modal,
        noise,
        sound: Sound::new(format!("{} (rebuilt)", sound.name), sound.rate, out),
    })
}

/// Two estimates in neighbouring windows are the same mode when within this fraction of each other
/// (a glide moves a drum's mode tens of cents between windows).
pub const SAME_TRACK: f64 = 0.03;

/// Modes linked window to window, strongest first: each track a list of (window, mode).
pub fn tracks(windows: &[pitch::WindowModes]) -> Vec<Vec<(usize, crate::repr::modes::Mode)>> {
    let mut open: Vec<Vec<(usize, crate::repr::modes::Mode)>> = Vec::new();
    let mut done = Vec::new();
    for (i, w) in windows.iter().enumerate() {
        let mut used = vec![false; w.modes.len()];
        let mut next = Vec::new();
        for mut t in open.drain(..) {
            let last = t.last().expect("a track has a mode").1;
            let best = w
                .modes
                .iter()
                .enumerate()
                .filter(|(j, (m, _))| !used[*j] && (m.hz / last.hz - 1.0).abs() <= SAME_TRACK)
                .min_by(|a, b| {
                    (a.1.0.hz - last.hz)
                        .abs()
                        .total_cmp(&(b.1.0.hz - last.hz).abs())
                })
                .map(|(j, _)| j);
            match best {
                Some(j) => {
                    used[j] = true;
                    t.push((i, w.modes[j].0));
                    next.push(t);
                }
                None => done.push(t),
            }
        }
        for (j, (m, _)) in w.modes.iter().enumerate() {
            if !used[j] {
                next.push(vec![(i, *m)]);
            }
        }
        open = next;
    }
    done.extend(open);
    done
}

/// The noise bands, Hz: thirds of an octave from 20 Hz to the top of the audio band. Octaves spread
/// what the modes leave of a line across twice the width, burying the weaker modes beside it.
fn noise_bands(rate: f64) -> Vec<(f64, f64)> {
    let top = 20_000.0f64.min(0.45 * rate);
    let step = 2f64.powf(1.0 / NOISE_BANDS_PER_OCTAVE);
    let mut out = Vec::new();
    let mut lo = 20.0;
    while lo < top {
        let hi = (lo * step).min(top);
        if hi / lo > 1.05 {
            out.push((lo, hi));
        }
        lo *= step;
    }
    out
}

/// How finely the residual's noise follows its spectrum (**chosen**).
pub const NOISE_BANDS_PER_OCTAVE: f64 = 3.0;

/// The most one correction pass may change a noise band's gain in one frame (**chosen**: 20 dB).
pub const CORRECTION_LIMIT: f64 = 10.0;

/// The phase at `t` seconds after mode `a`'s window start, on the cubic that leaves `a` with its
/// frequency and phase and meets `b`'s `span` seconds later, unwrapped to the smoothest (McAulay &
/// Quatieri 1986, their maximally smooth phase).
fn cubic_phase(
    a: &crate::repr::modes::Mode,
    b: &crate::repr::modes::Mode,
    span: f64,
    t: f64,
) -> f64 {
    let tau = std::f64::consts::TAU;
    let (w0, w1) = (tau * a.hz, tau * b.hz);
    let m = ((a.phase + w0 * span - b.phase + 0.5 * (w1 - w0) * span) / tau).round();
    let e = b.phase - a.phase - w0 * span + tau * m;
    let alpha = 3.0 * e / (span * span) - (w1 - w0) / span;
    let beta = -2.0 * e / (span * span * span) + (w1 - w0) / (span * span);
    a.phase + w0 * t + alpha * t * t + beta * t * t * t
}

/// A frame gain at sample `j`, eased linearly between frame centres.
fn eased(g: &[f64], j: usize, frame: usize) -> f64 {
    let pos = (j as f64 + 0.5) / frame as f64 - 0.5;
    if pos <= 0.0 {
        return g.first().copied().unwrap_or(0.0);
    }
    let k = pos.floor() as usize;
    if k + 1 >= g.len() {
        return g.last().copied().unwrap_or(0.0);
    }
    let u = pos - k as f64;
    g[k] + u * (g[k + 1] - g[k])
}

/// A raised-cosine ramp from 0 at 0 to 1 at 1, clamped.
fn raised(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    0.5 - 0.5 * (std::f64::consts::PI * x).cos()
}

/// Deterministic white noise, uniform in −1..1.
fn white_noise(n: usize) -> Vec<f64> {
    let mut s = 0x0DDB_A11C_0FFE_E123u64;
    (0..n)
        .map(|_| {
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            (s.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 52) as f64 - 1.0
        })
        .collect()
}
