//! Pitch and partials: the hit's modes in four windows after the contact, the rest pitch, the attack's
//! pitch against it, the glide, and how clearly pitched the ring is.
//!
//! Sources: the mode analysis is `repr::modes` (`research:listening/modal-estimation.md`); the rest-pitch
//! rule (the **lowest** partial within 12 dB of the strongest, not the loudest one), partials per window
//! and the owner's phrases "higher pitched", "the tone of the drum skin" and "the attack is lower
//! pitched than the tail" are `docs/drum-model-fitting.md` §6 and §7; pitch salience and head share are
//! the corpus fit's clarity measures (the model-drums plan's revision 18).

use super::Context;
use crate::reading::{Reading, Section, Table, Unit};
use crate::repr::modes::{self, Mode};
use crate::repr::{envelope, spectrum};

/// The mode windows, ms from the onset: the first starts after a stick's contact (3–8 ms) and is short,
/// because a glide inside a window leaves a phantom mode; the later ones lengthen as the modes settle.
pub const WINDOWS_MS: [(f64, f64); 4] =
    [(10.0, 60.0), (60.0, 160.0), (160.0, 400.0), (400.0, 1000.0)];
/// A mode more than this far below the sound's peak is not listed.
pub const LEVEL_FLOOR_DB: f64 = -60.0;
/// The rest pitch is the lowest mode within this many dB of the strongest.
pub const REST_WITHIN_DB: f64 = 12.0;
/// A mode in another window is the same mode when within this fraction of its frequency.
pub const SAME_MODE: f64 = 0.08;
/// A mode is seen again in another window when within this fraction of its frequency. A mode is
/// believed when it persists (`research:listening/modal-estimation.md` §9): a glide leaves phantoms
/// that do not.
pub const PERSISTS: f64 = 0.03;
/// Where the glide is read, ms (window centres), and the track's window and search span.
pub const GLIDE_AT_MS: [f64; 4] = [20.0, 40.0, 70.0, 100.0];
pub const TRACK_WINDOW_MS: f64 = 40.0;
pub const TRACK_SPAN: f64 = 0.06;

/// The line nearest `hz` tracked from 20 to 400 ms in [`TRACK_WINDOW_MS`] Hann windows every 10 ms:
/// the strongest bin within ±[`TRACK_SPAN`], refined by a parabola through the log magnitudes.
/// `(centre ms, hz)`.
#[must_use]
pub fn line_track(c: &Context, hz: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let half = TRACK_WINDOW_MS / 2.0;
    let mut centre = 20.0;
    while centre <= 400.0 {
        if !c.covers(centre + half) {
            break;
        }
        let track = c
            .window(centre - half, centre + half)
            .and_then(|w| spectrum::power_spectrum(w, c.rate, 1 << 16))
            .and_then(|s| {
                let bins = s.bins(hz * (1.0 - TRACK_SPAN), hz * (1.0 + TRACK_SPAN));
                let k = bins
                    .clone()
                    .max_by(|a, b| s.power[*a].total_cmp(&s.power[*b]))?;
                if k == 0 || k + 1 >= s.power.len() || s.power[k] <= 0.0 {
                    return None;
                }
                let (a, b, g) = (
                    s.power[k - 1].max(1e-300).ln(),
                    s.power[k].ln(),
                    s.power[k + 1].max(1e-300).ln(),
                );
                let d = a - 2.0 * b + g;
                let offset = if d < 0.0 { 0.5 * (a - g) / d } else { 0.0 };
                Some(s.hz(k) + offset * s.bin_hz)
            });
        if let Some(f) = track {
            out.push((centre, f));
        }
        centre += 10.0;
    }
    out
}

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    Some(v[v.len() / 2])
}

/// Salience windows, ms (the corpus fit's clarity windows).
pub const SALIENCE_MS: [(f64, f64); 3] = [(20.0, 80.0), (80.0, 200.0), (200.0, 400.0)];

/// The modes found in one window, strongest first, with their level against the sound's peak.
#[derive(Clone, Debug)]
pub struct WindowModes {
    pub window_ms: (f64, f64),
    pub modes: Vec<(Mode, f64)>,
}

/// Every window's modes on a percussive hit's bands. Windows that run past the file are left out.
#[must_use]
pub fn analyse(c: &Context) -> Vec<WindowModes> {
    analyse_with(c, &modes::percussion_bands(c.rate))
}

/// Every window's modes on a pitched note's bands, up to 20 kHz, a steady partial kept as a mode
/// ([`modes::STEADY_GROWTH`]): a held note's partials neither decay nor grow.
#[must_use]
pub fn analyse_note(c: &Context) -> Vec<WindowModes> {
    analyse_inner(c, &modes::note_bands(c.rate), true)
}

/// Every window's modes on the given bands (`(lo, hi, keep_lo, keep_hi)`, Hz), decaying modes only.
#[must_use]
pub fn analyse_with(c: &Context, bands: &[(f64, f64, f64, f64)]) -> Vec<WindowModes> {
    analyse_inner(c, bands, false)
}

fn analyse_inner(c: &Context, bands: &[(f64, f64, f64, f64)], steady: bool) -> Vec<WindowModes> {
    let mut out = Vec::new();
    for (a, b) in WINDOWS_MS {
        if !c.covers(a + 20.0) {
            break;
        }
        let start = c.at(a);
        let len = c.at(b).min(c.x.len()).saturating_sub(start);
        let mut all: Vec<Mode> = Vec::new();
        for &(lo, hi, keep_lo, keep_hi) in bands {
            let Some(band) = modes::analyse_band_with(&c.x, c.rate, start, len, lo, hi, steady)
            else {
                continue;
            };
            all.extend(
                band.modes
                    .into_iter()
                    .filter(|m| m.hz >= keep_lo && m.hz < keep_hi),
            );
        }
        // The bands found the modes; the window's own samples say how strong each is.
        modes::refit(&c.x, c.rate, start, len, &mut all);
        let mut found: Vec<(Mode, f64)> = all
            .into_iter()
            .map(|m| {
                (
                    m,
                    envelope::db(m.amplitude, c.peak).unwrap_or(f64::NEG_INFINITY),
                )
            })
            .filter(|(_, level)| *level >= LEVEL_FLOOR_DB)
            .collect();
        found.sort_by(|x, y| y.1.total_cmp(&x.1));
        out.push(WindowModes {
            window_ms: (a, b),
            modes: found,
        });
    }
    out
}

/// How many other windows hold a mode within [`PERSISTS`] of `m`.
#[must_use]
pub fn persistence(windows: &[WindowModes], w: &WindowModes, m: &Mode) -> usize {
    windows
        .iter()
        .filter(|o| o.window_ms != w.window_ms)
        .filter(|o| {
            o.modes
                .iter()
                .any(|(q, _)| (q.hz / m.hz - 1.0).abs() <= PERSISTS)
        })
        .count()
}

/// A mode within this of a room line is the room's: 2 Hz (`set::SAME_LINE_HZ`) or 1.5 %, whichever is
/// wider — an early window's estimate of a sympathetic line sits a few hertz from the late one's
/// (206–207 Hz against a room line at 203.9 Hz on the snare).
pub const ROOM_FRACTION: f64 = 0.015;

/// The mode is one of the room's lines.
#[must_use]
pub fn is_room(m: &Mode, room: &[f64]) -> bool {
    room.iter()
        .any(|r| (r - m.hz).abs() <= crate::set::SAME_LINE_HZ.max(ROOM_FRACTION * r))
}

/// The rest pitch of one window: the lowest **believed** mode within [`REST_WITHIN_DB`] of the
/// strongest drum mode — believed when it recurs in another window, or is the strongest itself — and
/// never a room line. A weak line seen once (a 75 Hz rumble 11 dB under a snare's ring) is not the
/// drum's pitch, and neither is a line the room adds to every drum of a set.
#[must_use]
pub fn rest_mode(windows: &[WindowModes], w: &WindowModes, room: &[f64]) -> Option<Mode> {
    let drum: Vec<&(Mode, f64)> = w.modes.iter().filter(|(m, _)| !is_room(m, room)).collect();
    let strongest = drum.first()?;
    drum.iter()
        .filter(|(m, l)| {
            *l >= strongest.1 - REST_WITHIN_DB
                && (m == &strongest.0 || persistence(windows, w, m) >= 1)
        })
        .map(|(m, _)| *m)
        .min_by(|a, b| a.hz.total_cmp(&b.hz))
}

/// The ring's window: 160–400 ms, or the latest earlier one that has modes.
#[must_use]
pub fn ring_window(windows: &[WindowModes]) -> Option<&WindowModes> {
    windows
        .iter()
        .filter(|w| !w.modes.is_empty())
        .find(|w| w.window_ms.0 >= 160.0)
        .or_else(|| windows.iter().rev().find(|w| !w.modes.is_empty()))
}

/// The sound's rest pitch: the rest mode of its ring window, room lines kept out.
#[must_use]
pub fn rest(windows: &[WindowModes], room: &[f64]) -> Option<Mode> {
    ring_window(windows).and_then(|w| rest_mode(windows, w, room))
}

/// Analyses and reads in one call.
#[must_use]
pub fn read(c: &Context) -> Section {
    section(c, &analyse(c), &[])
}

/// The pitch section from windows already analysed; `room` are the room's lines, Hz.
#[must_use]
pub fn section(c: &Context, windows: &[WindowModes], room: &[f64]) -> Section {
    let mut readings = Vec::new();
    let ring = ring_window(windows);
    let rest = rest(windows, room);
    let source_rest = "guide §6: the lowest believed mode within 12 dB of the strongest";
    let mut r = Reading::new(
        "pitch.rest",
        "Rest pitch",
        rest.map(|m| m.hz),
        Unit::Hertz,
        source_rest,
    );
    if let Some(w) = ring {
        r = r.window(w.window_ms.0, w.window_ms.1);
    }
    readings.push(r);
    let strongest = ring.and_then(|w| w.modes.iter().map(|x| x.0).find(|m| !is_room(m, room)));
    let mut r = Reading::new(
        "pitch.strongest",
        "Strongest mode",
        strongest.map(|m| m.hz),
        Unit::Hertz,
        "repr::modes",
    );
    if let Some(w) = ring {
        r = r.window(w.window_ms.0, w.window_ms.1);
    }
    readings.push(r);
    if let Some(m) = rest {
        readings.push(
            Reading::new(
                "pitch.rest_t60",
                "Rest pitch's decay (T60)",
                Some(m.t60() * 1000.0),
                Unit::Milliseconds,
                "repr::modes",
            )
            .window(
                ring.map_or(0.0, |w| w.window_ms.0),
                ring.map_or(0.0, |w| w.window_ms.1),
            ),
        );
    }

    // The attack's pitch: the strongest mode in the first window, against the rest pitch.
    if let (Some(first), Some(rest)) = (windows.first(), rest) {
        if let Some((m, _)) = first.modes.iter().find(|(m, _)| !is_room(m, room)) {
            readings.push(
                Reading::new(
                    "pitch.attack",
                    "The attack's strongest mode",
                    Some(m.hz),
                    Unit::Hertz,
                    "repr::modes; guide §7 (\"the attack is lower pitched than the tail\")",
                )
                .window(first.window_ms.0, first.window_ms.1),
            );
            readings.push(
                Reading::new(
                    "pitch.attack_against_rest",
                    "The attack's strongest mode against the rest pitch",
                    Some(mxm_measure::convert::cents(m.hz / rest.hz)),
                    Unit::Cents,
                    "guide §7: a low mode lingering early reads as an attack pitched below the tail",
                )
                .window(first.window_ms.0, first.window_ms.1),
            );
        }
    }

    // The glide: the rest mode's line tracked through 40 ms windows. Matching modes across the mode
    // windows cannot see it — a glide inside a window becomes a phantom mode (research §9) — so this
    // is the guide's two-mode measure: the line's frequency at 20, 40, 70 and 100 ms against its
    // median over 180–260 ms.
    if let Some(rest) = rest {
        let track = line_track(c, rest.hz);
        let settled: Vec<f64> = track
            .iter()
            .filter(|(t, _)| (180.0..=260.0).contains(t))
            .map(|x| x.1)
            .collect();
        let settled = median(settled);
        for at in GLIDE_AT_MS {
            let here = track
                .iter()
                .find(|(t, _)| (t - at).abs() < 0.5)
                .map(|x| x.1);
            readings.push(
                Reading::new(
                    "pitch.glide",
                    "The rest pitch's line here, against settled",
                    match (here, settled) {
                        (Some(h), Some(s)) => Some(mxm_measure::convert::cents(h / s)),
                        _ => None,
                    },
                    Unit::Cents,
                    "the guide's two-mode measure: a 40 ms line track against its 180–260 ms median; positive is sharp early",
                )
                .window(at - 20.0, at + 20.0)
                .resolution(crate::reading::Resolution {
                    window_ms: TRACK_WINDOW_MS,
                    bin_hz: None,
                    span: Some(TRACK_SPAN),
                }),
            );
        }
    }

    // How clearly pitched: the strongest peak over the median across half to three times the rest
    // pitch, and that band's energy against everything above it to 10 kHz.
    for (a, b) in SALIENCE_MS {
        let level = c.level_db(a, b);
        let s = if c.covers(b) {
            c.window(a, b)
                .and_then(|w| spectrum::power_spectrum(w, c.rate, 1 << 16))
        } else {
            None
        };
        let (salience, share) = match (&s, rest) {
            (Some(s), Some(rest)) => salience(s, rest.hz),
            _ => (None, None),
        };
        readings.push(
            Reading::new(
                "pitch.salience",
                "Pitch clarity (peak over median)",
                salience,
                Unit::Decibels,
                "the corpus fit's clarity, over ½–3× the rest pitch",
            )
            .window(a, b)
            .floor(level),
        );
        readings.push(
            Reading::new(
                "pitch.head_share",
                "Energy near the pitch against above it",
                share,
                Unit::Decibels,
                "the corpus fit's head share: ½–3× the rest pitch against 3× to 10 kHz",
            )
            .window(a, b)
            .floor(level),
        );
    }

    let mut section = Section::new("pitch", "Pitch and modes", readings);
    for w in windows {
        let rows = w
            .modes
            .iter()
            .map(|(m, level)| {
                let seen = persistence(windows, w, m);
                vec![
                    Some(m.hz),
                    rest.map(|r| m.hz / r.hz),
                    Some(*level),
                    Some(m.t60() * 1000.0),
                    Some(seen as f64),
                    Some(if is_room(m, room) { 1.0 } else { 0.0 }),
                ]
            })
            .collect();
        section.tables.push(Table {
            id: "pitch.modes",
            title: format!("Modes, {:.0}–{:.0} ms", w.window_ms.0, w.window_ms.1),
            columns: vec![
                ("Frequency", Unit::Hertz),
                ("Ratio to rest pitch", Unit::Ratio),
                ("Level at window start", Unit::Decibels),
                ("T60", Unit::Milliseconds),
                ("Also in other windows", Unit::Plain),
                ("Room line", Unit::Plain),
            ],
            rows,
            window_ms: Some(w.window_ms),
            source: "repr::modes: subband ESPRIT, ESTER order, level against the sound's peak",
        });
    }
    section
}

/// Pitch salience and head share for a rest pitch `f0`.
fn salience(s: &spectrum::Spectrum, f0: f64) -> (Option<f64>, Option<f64>) {
    let band = s.bins(0.5 * f0, 3.0 * f0);
    let mut values: Vec<f64> = s.power[band.clone()].to_vec();
    if values.len() < 3 {
        return (None, None);
    }
    let peak = values.iter().copied().fold(0.0f64, f64::max);
    values.sort_by(f64::total_cmp);
    let median = values[values.len() / 2];
    let salience = (median > 0.0).then(|| 10.0 * (peak / median).log10());
    let near: f64 = s.power[band].iter().sum();
    let above: f64 = s.power[s.bins(3.0 * f0, 10_000.0)].iter().sum();
    let share = (near > 0.0 && above > 0.0).then(|| 10.0 * (near / above).log10());
    (salience, share)
}
