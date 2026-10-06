//! The centre of the HUD: the pitch.
//!
//! **The note ring** puts every partial where a musician would: around the ring by pitch class (C at
//! the top), outward by octave, so a harmonic series stacks and a bell scatters. The crosshair locks on
//! the family's pitch — a hit's rest pitch, a note's fundamental — and says `NO LOCK` and why when
//! there is none. **The radar** turns the spectrogram round: time around the ring from the top,
//! frequency outward. The sweep spins fast while the listener works and idles once it is done; the
//! rings turn for the look only. Every number drawn is a reading, a table cell or a curve point.

use egui::{Align2, Painter, Pos2, Rect, Stroke, vec2};
use mxm_listening::curves::Curves;
use mxm_listening::parts::note::nearest_note;

use crate::scene::{Lock, Scene, number};
use crate::theme::{
    AMBER, BRIGHT, CYAN, DIM, GRID, TEXT, arc, brackets, dashed, dashed_ring, fade, glow_circle,
    glow_line, glow_text, heat, light, mono, radial, sweep, ticks,
};

/// What the centre shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// Pitch class around, octave outward.
    Ring,
    /// Time around, frequency outward.
    Radar,
}

/// C0, the note ring's centre, Hz.
const C0_HZ: f64 = 16.351_597_831_287_414;
/// The note ring spans ten octaves, C0 to C10.
const OCTAVES: f64 = 10.0;
/// The radar spans 20 Hz to 20 kHz.
const RADAR_HZ: (f64, f64) = (20.0, 20_000.0);
/// The ring's inner edge, as a share of its radius: the readout sits inside.
const INNER: f32 = 0.30;

/// The scope's geometry.
#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub centre: Pos2,
    pub radius: f32,
}

impl Geometry {
    fn inner(self) -> f32 {
        self.radius * INNER
    }

    /// Where `hz` sits on the note ring.
    #[must_use]
    pub fn on_ring(self, hz: f64) -> Pos2 {
        let semis = 12.0 * (hz / C0_HZ).log2();
        let octave = (semis / 12.0).clamp(0.0, OCTAVES);
        let r = self.inner() + (octave / OCTAVES) as f32 * (self.radius - self.inner());
        let a = pitch_angle(semis.rem_euclid(12.0));
        self.centre + r * vec2(a.cos(), a.sin())
    }

    /// The frequency under a point on the note ring.
    #[must_use]
    pub fn ring_hz(self, p: Pos2) -> Option<f64> {
        let d = p - self.centre;
        let r = d.length();
        if r < self.inner() || r > self.radius {
            return None;
        }
        let octave = f64::from((r - self.inner()) / (self.radius - self.inner())) * OCTAVES;
        let mut class = (f64::from(d.y.atan2(d.x)) + std::f64::consts::FRAC_PI_2)
            .rem_euclid(std::f64::consts::TAU)
            / std::f64::consts::TAU
            * 12.0;
        // The octave is carried by the radius, the class by the angle: take the class's octave.
        if class >= 12.0 {
            class -= 12.0;
        }
        let whole = (octave - class / 12.0).round();
        Some(C0_HZ * 2f64.powf(whole + class / 12.0))
    }

    /// The radar's radius for `hz`.
    fn radar_r(self, hz: f64) -> f32 {
        let t = ((hz / RADAR_HZ.0).log10() / (RADAR_HZ.1 / RADAR_HZ.0).log10()).clamp(0.0, 1.0);
        self.inner() + t as f32 * (self.radius - self.inner())
    }
}

/// The screen angle of a pitch class, 0–12, C at the top, clockwise.
fn pitch_angle(class: f64) -> f32 {
    (class / 12.0 * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2) as f32
}

const CLASSES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Everything the centre needs to draw one frame.
pub struct Frame<'a> {
    pub scene: Option<&'a Scene>,
    pub curves: Option<&'a Curves>,
    /// The listener is working.
    pub busy: bool,
    /// What the centre says while there is no scene: `AWAITING SIGNAL`, `LISTENING 1.24 s`.
    pub status: &'a str,
    pub view: View,
    /// Seconds, for the motion.
    pub time: f64,
    pub hover: Option<Pos2>,
    /// While a sound plays, where it is as a share of the span drawn: the radar's sweep follows it.
    pub playhead: Option<f32>,
}

/// Draws the centre; returns what lies under the cursor, as coordinates.
pub fn draw(painter: &Painter, g: Geometry, f: &Frame<'_>) -> Option<String> {
    let t = f.time as f32;
    let (c, r) = (g.centre, g.radius);
    radial(painter, c, r * 1.05, CYAN, 0.10);

    // The rings that turn for the look.
    dashed_ring(
        painter,
        c,
        r + 7.0,
        90,
        0.5,
        t * 0.05,
        Stroke::new(1.0, fade(CYAN, 0.35)),
    );
    dashed_ring(
        painter,
        c,
        r + 16.0,
        3,
        0.22,
        -t * 0.12,
        Stroke::new(2.0, fade(CYAN, 0.55)),
    );
    ticks(painter, c, r, 72, 6, 4.0, fade(CYAN, 0.45));
    glow_circle(painter, c, r, fade(CYAN, 0.7), 1.0);
    glow_circle(painter, c, g.inner(), fade(CYAN, 0.35), 0.8);

    match f.view {
        View::Ring => ring_grid(painter, g),
        View::Radar => radar_grid(painter, g, f),
    }

    // The sweep: fast while the listener works, idle after — and on the radar, where the sound plays.
    match (f.view, f.playhead) {
        (View::Radar, Some(share)) => {
            let at = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * share;
            sweep(painter, c, r, at, 0.5, AMBER, 0.6);
        }
        _ => {
            let turn = if f.busy { t * 4.0 } else { t * 0.6 };
            sweep(
                painter,
                c,
                r,
                turn,
                1.1,
                CYAN,
                if f.busy { 0.55 } else { 0.22 },
            );
        }
    }

    if f.view == View::Ring
        && let Some(scene) = f.scene
    {
        blips(painter, g, scene, t);
    }
    lock(painter, g, f, t);

    // Coordinates under the cursor: a conversion of where it points, never a reading.
    let p = f.hover?;
    let hz = match f.view {
        View::Ring => g.ring_hz(p)?,
        View::Radar => {
            let d = (p - c).length();
            if d < g.inner() || d > r {
                return None;
            }
            let u = f64::from((d - g.inner()) / (r - g.inner()));
            RADAR_HZ.0 * (RADAR_HZ.1 / RADAR_HZ.0).powf(u)
        }
    };
    painter.line_segment(
        [p - vec2(8.0, 0.0), p + vec2(8.0, 0.0)],
        Stroke::new(1.0, fade(TEXT, 0.6)),
    );
    painter.line_segment(
        [p - vec2(0.0, 8.0), p + vec2(0.0, 8.0)],
        Stroke::new(1.0, fade(TEXT, 0.6)),
    );
    let (note, cents) = nearest_note(hz)?;
    Some(format!("{} Hz · {note} {cents:+.0} c", number(hz)))
}

fn ring_grid(painter: &Painter, g: Geometry) {
    let c = g.centre;
    for octave in 1..10 {
        let r = g.inner() + octave as f32 / OCTAVES as f32 * (g.radius - g.inner());
        dashed_ring(painter, c, r, 120, 0.35, 0.0, Stroke::new(1.0, GRID));
    }
    for (k, name) in CLASSES.iter().enumerate() {
        let a = pitch_angle(k as f64);
        let d = vec2(a.cos(), a.sin());
        let natural = !name.contains('#');
        painter.line_segment(
            [c + d * g.inner(), c + d * g.radius],
            Stroke::new(1.0, if natural { GRID } else { fade(GRID, 0.5) }),
        );
        painter.text(
            c + d * (g.radius + 26.0),
            Align2::CENTER_CENTER,
            *name,
            mono(if natural { 13.0 } else { 10.0 }),
            if natural { fade(CYAN, 0.9) } else { DIM },
        );
    }
    // Octave numbers along the C spoke.
    for octave in [2, 4, 6, 8] {
        let r = g.inner() + octave as f32 / OCTAVES as f32 * (g.radius - g.inner());
        painter.text(
            c + vec2(6.0, -r),
            Align2::LEFT_CENTER,
            format!("C{octave}"),
            mono(9.0),
            DIM,
        );
    }
}

fn radar_grid(painter: &Painter, g: Geometry, f: &Frame<'_>) {
    let c = g.centre;
    if let Some(s) = f.curves.and_then(|c| c.spectrogram.as_ref()) {
        polar_spectrogram(painter, g, s);
    }
    for hz in [50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10_000.0] {
        let r = g.radar_r(hz);
        dashed_ring(
            painter,
            c,
            r,
            120,
            0.35,
            0.0,
            Stroke::new(1.0, fade(GRID, 0.8)),
        );
        // Labels on the decades and their halves only; every ring labelled crowds the centre.
        if [100.0, 1000.0, 10_000.0, 500.0, 5000.0].contains(&hz) {
            let label = if hz >= 1000.0 {
                format!("{}k", hz / 1000.0)
            } else {
                format!("{hz}")
            };
            painter.text(
                c + vec2(4.0, -r),
                Align2::LEFT_BOTTOM,
                label,
                mono(9.0),
                DIM,
            );
        }
    }
    // Time spokes, labelled in ms from the onset.
    let span = f
        .curves
        .and_then(|c| c.spectrogram.as_ref())
        .and_then(|s| s.times_ms.last().copied());
    for k in 0..8 {
        let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * k as f32 / 8.0;
        let d = vec2(a.cos(), a.sin());
        dashed(
            painter,
            c + d * g.inner(),
            c + d * g.radius,
            3.0,
            5.0,
            Stroke::new(1.0, GRID),
        );
        if let Some(span) = span {
            painter.text(
                c + d * (g.radius + 30.0),
                Align2::CENTER_CENTER,
                format!("{} ms", number(span * k as f64 / 8.0)),
                mono(10.0),
                DIM,
            );
        }
    }
}

/// The spectrogram drawn round the radar: each angular slice the loudest of its frames.
fn polar_spectrogram(painter: &Painter, g: Geometry, s: &mxm_listening::curves::Spectrogram) {
    let Some(&span) = s.times_ms.last() else {
        return;
    };
    if span <= 0.0 {
        return;
    }
    // A slice per frame when the frames are few, else the loudest of each slice's frames.
    let slices = s.times_ms.len().clamp(1, 240);
    let mut cells = vec![vec![None::<f64>; s.bands_hz.len()]; slices];
    for (t, row) in s.times_ms.iter().zip(&s.cells_db) {
        let k = ((t / span) * (slices as f64 - 1.0)).round() as usize;
        for (j, v) in row.iter().enumerate() {
            if let Some(v) = v {
                let cell = &mut cells[k.min(slices - 1)][j];
                *cell = Some(cell.map_or(*v, |c: f64| c.max(*v)));
            }
        }
    }
    let mut mesh = egui::epaint::Mesh::default();
    let step = std::f32::consts::TAU / slices as f32;
    for (k, column) in cells.iter().enumerate() {
        let a0 = -std::f32::consts::FRAC_PI_2 + step * k as f32;
        let (d0, d1) = (
            vec2(a0.cos(), a0.sin()),
            vec2((a0 + step).cos(), (a0 + step).sin()),
        );
        for (j, cell) in column.iter().enumerate() {
            let Some(db) = cell.filter(|db| *db > -70.0) else {
                continue;
            };
            let (lo, hi) = s.bands_hz[j];
            let (r0, r1) = (g.radar_r(lo), g.radar_r(hi));
            let colour = light(heat(db, -70.0), 0.85);
            let at = |d: egui::Vec2, r: f32| g.centre + d * r;
            let i = mesh.vertices.len() as u32;
            for p in [at(d0, r0), at(d1, r0), at(d1, r1), at(d0, r1)] {
                mesh.colored_vertex(p, colour);
            }
            mesh.add_triangle(i, i + 1, i + 2);
            mesh.add_triangle(i, i + 2, i + 3);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// The partials or modes, each a blip: bigger the louder, brighter the longer it rings.
fn blips(painter: &Painter, g: Geometry, scene: &Scene, t: f32) {
    for (k, b) in scene.blips.iter().enumerate() {
        let p = g.on_ring(b.hz);
        let size = b.level_db.map_or(2.5, |db| {
            2.0 + ((db + 48.0) / 48.0).clamp(0.0, 1.0) as f32 * 5.0
        });
        let ring = b.ring_s.map_or(0.4, |s| (s / 2.0).clamp(0.15, 1.0) as f32);
        let pulse = 0.85 + 0.15 * (t * 2.3 + k as f32).sin();
        radial(painter, p, size * 4.0, CYAN, 0.35 * ring * pulse);
        painter.circle_filled(p, size, fade(BRIGHT, 0.6 + 0.4 * ring));
    }
}

/// The crosshair: locked on the pitch, or searching and saying why it cannot lock.
fn lock(painter: &Painter, g: Geometry, f: &Frame<'_>, t: f32) {
    let c = g.centre;
    let locked = f.scene.and_then(|s| match &s.lock {
        Lock::Locked {
            hz,
            note,
            cents,
            what,
            ..
        } => Some((*hz, note.as_str(), *cents, *what)),
        Lock::NoLock { .. } => None,
    });
    // Where the reticle sits: on the pitch once locked (the ring's own place for it, or the radar's
    // radius for its frequency), else wandering while the listener searches.
    let wander = || {
        let a = t * 0.7;
        let r = g.inner() + (g.radius - g.inner()) * (0.55 + 0.25 * (t * 0.43).sin());
        c + r * vec2(a.cos(), a.sin())
    };
    let target = match (locked, f.view) {
        (Some((hz, ..)), View::Ring) => g.on_ring(hz),
        (Some((hz, ..)), View::Radar) => c + vec2(0.0, -g.radar_r(hz)),
        (None, _) => wander(),
    };
    let colour = if locked.is_some() {
        CYAN
    } else {
        fade(TEXT, 0.35)
    };
    let size = if locked.is_some() {
        13.0
    } else {
        17.0 + 3.0 * (t * 3.0).sin()
    };
    brackets(
        painter,
        Rect::from_center_size(target, vec2(size, size) * 2.0),
        6.0,
        Stroke::new(1.5, colour),
    );
    if locked.is_some() {
        glow_line(
            painter,
            arc(target, size * 1.6, t * 1.5, t * 1.5 + 1.2, 12),
            false,
            CYAN,
            1.0,
        );
        dashed(
            painter,
            c,
            target,
            4.0,
            4.0,
            Stroke::new(1.0, fade(CYAN, 0.5)),
        );
    }

    // The readout in the middle.
    match (f.scene, locked) {
        (Some(_), Some((hz, note, cents, what))) => {
            glow_text(
                painter,
                c - vec2(0.0, 14.0),
                Align2::CENTER_CENTER,
                note,
                mono(40.0),
                BRIGHT,
            );
            painter.text(
                c + vec2(0.0, 18.0),
                Align2::CENTER_CENTER,
                format!("{cents:+.0} c · {} Hz", number(hz)),
                mono(14.0),
                CYAN,
            );
            painter.text(
                c + vec2(0.0, 36.0),
                Align2::CENTER_CENTER,
                what.to_uppercase(),
                mono(10.0),
                DIM,
            );
        }
        (Some(scene), None) => {
            glow_text(
                painter,
                c - vec2(0.0, 10.0),
                Align2::CENTER_CENTER,
                "NO LOCK",
                mono(26.0),
                TEXT,
            );
            if let Lock::NoLock { why } = &scene.lock {
                let wrapped = wrap(why, 26);
                painter.text(
                    c + vec2(0.0, 16.0),
                    Align2::CENTER_TOP,
                    wrapped,
                    mono(10.0),
                    DIM,
                );
            }
        }
        (None, _) => {
            let blink = if f.busy && (t * 2.0).fract() < 0.5 {
                "▮"
            } else {
                " "
            };
            glow_text(
                painter,
                c,
                Align2::CENTER_CENTER,
                &format!("{}{blink}", f.status),
                mono(15.0),
                if f.busy { CYAN } else { DIM },
            );
        }
    }
}

/// Wraps `s` at spaces to lines of at most `width` characters.
fn wrap(s: &str, width: usize) -> String {
    let mut out = String::new();
    let mut line = 0;
    for word in s.split(' ') {
        if line > 0 && line + 1 + word.chars().count() > width {
            out.push('\n');
            line = 0;
        } else if line > 0 {
            out.push(' ');
            line += 1;
        }
        out.push_str(word);
        line += word.chars().count();
    }
    out
}
