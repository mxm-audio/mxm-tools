//! The strip along the bottom: the sound through time, three lanes on one axis of ms from the onset.
//! The spectrogram above — each column the frames that fall in it, their loudest; the waveform in the
//! middle — the samples themselves, each column's lowest and highest, scaled to the sound's peak; and
//! the envelope below, in dB against its loudest moment — with the attack, body and tail bracketed
//! where their readings put them. Hovering reads the curve out under the cursor: a curve point, never
//! a new number.

use egui::epaint::Mesh;
use egui::{Align2, Painter, Pos2, Rect, Stroke, pos2, vec2};
use mxm_listening::curves::Curves;

use crate::scene::{Scene, number};
use crate::theme::{
    AMBER, CYAN, DIM, GRID, PANEL, TEXT, brackets, dashed, fade, glow_line, heat, mono,
};

/// The envelope's floor, dB.
const FLOOR_DB: f64 = -72.0;

/// The waveform as drawn: each pixel column's lowest and highest sample, from the onset over the
/// strip's span, and the sound's peak reading for its label.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wave {
    pub columns: Vec<(f32, f32)>,
    /// `level.peak`, dBFS.
    pub peak_dbfs: Option<f64>,
}

/// The lanes' area within the strip's `rect`: the axis every lane shares.
#[must_use]
pub fn inner(rect: Rect) -> Rect {
    rect.shrink2(vec2(44.0, 16.0))
}

/// Each of `columns` columns' lowest and highest sample of `samples` (at `rate` Hz), from the onset
/// at `onset_s` over `span_ms`. Display only: the samples, pooled.
#[must_use]
pub fn wave_columns(
    samples: &[f32],
    rate: u32,
    onset_s: f64,
    span_ms: f64,
    columns: usize,
) -> Vec<(f32, f32)> {
    let rate = f64::from(rate.max(1));
    let start = ((onset_s * rate) as usize).min(samples.len());
    let end = (start + (span_ms / 1000.0 * rate) as usize).min(samples.len());
    let n = end - start;
    if n == 0 || columns == 0 {
        return Vec::new();
    }
    (0..columns)
        .map(|c| {
            let a = start + c * n / columns;
            let b = (start + (c + 1) * n / columns)
                .max(a + 1)
                .min(end.max(a + 1));
            let slice = &samples[a.min(samples.len() - 1)..b.min(samples.len())];
            slice
                .iter()
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &v| {
                    (lo.min(v), hi.max(v))
                })
        })
        .collect()
}

/// Draws the strip in `rect`; `hover` is the cursor.
pub fn draw(
    painter: &Painter,
    rect: Rect,
    scene: Option<&Scene>,
    curves: Option<&Curves>,
    wave: Option<&Wave>,
    hover: Option<Pos2>,
    playhead_ms: Option<f64>,
) {
    painter.rect_filled(rect, 2.0, fade(PANEL, 0.9));
    brackets(painter, rect, 10.0, Stroke::new(1.0, fade(CYAN, 0.6)));
    let inner = inner(rect);
    let (upper, middle, lower) = {
        let a = inner.top() + inner.height() * 0.34;
        let b = inner.top() + inner.height() * 0.72;
        (
            Rect::from_min_max(inner.left_top(), pos2(inner.right(), a - 4.0)),
            Rect::from_min_max(pos2(inner.left(), a + 4.0), pos2(inner.right(), b - 4.0)),
            Rect::from_min_max(pos2(inner.left(), b + 4.0), inner.right_bottom()),
        )
    };
    painter.text(
        pos2(rect.left() + 8.0, middle.center().y),
        Align2::LEFT_CENTER,
        "WAVE",
        mono(9.0),
        DIM,
    );
    painter.text(
        pos2(rect.left() + 8.0, upper.center().y),
        Align2::LEFT_CENTER,
        "Hz",
        mono(9.0),
        DIM,
    );
    painter.text(
        pos2(rect.left() + 8.0, lower.center().y),
        Align2::LEFT_CENTER,
        "dB",
        mono(9.0),
        DIM,
    );

    // The waveform: the samples themselves, each column's lowest to highest, scaled to their peak.
    if let Some(wave) = wave.filter(|w| !w.columns.is_empty()) {
        let peak = wave
            .columns
            .iter()
            .fold(0.0f32, |m, (lo, hi)| m.max(lo.abs()).max(hi.abs()))
            .max(1e-9);
        let mid = middle.center().y;
        painter.line_segment(
            [pos2(middle.left(), mid), pos2(middle.right(), mid)],
            Stroke::new(1.0, fade(GRID, 0.8)),
        );
        let w = middle.width() / wave.columns.len() as f32;
        let half = middle.height() / 2.0;
        let mut mesh = Mesh::default();
        for (c, (lo, hi)) in wave.columns.iter().enumerate() {
            if !lo.is_finite() || !hi.is_finite() {
                continue;
            }
            let x = middle.left() + c as f32 * w;
            let top = mid - hi / peak * half;
            let bottom = mid - lo / peak * half;
            mesh.add_colored_rect(
                Rect::from_min_max(
                    pos2(x, top.min(bottom)),
                    pos2(x + w.max(1.0), bottom.max(top) + 0.5),
                ),
                fade(CYAN, 0.85),
            );
        }
        painter.add(egui::Shape::mesh(mesh));
        if let Some(db) = wave.peak_dbfs {
            painter.text(
                pos2(middle.right(), middle.top()),
                Align2::RIGHT_TOP,
                format!("scaled to its peak, {db:.1} dBFS"),
                mono(8.0),
                DIM,
            );
        }
    }

    let Some(curves) = curves else {
        painter.text(
            inner.center(),
            Align2::CENTER_CENTER,
            "NO TRACE",
            mono(12.0),
            DIM,
        );
        return;
    };
    let span = span_ms(curves);
    if span <= 0.0 {
        return;
    }
    let x_of = |ms: f64| inner.left() + (ms / span).clamp(0.0, 1.0) as f32 * inner.width();

    // The spectrogram.
    if let Some(s) = &curves.spectrogram {
        // A column per frame when the frames are fewer than the pixels, else the loudest of each
        // column's frames: never gaps between frames, never more cells than pixels.
        let columns = (inner.width().max(1.0) as usize).min(s.times_ms.len().max(1));
        let bands = s.bands_hz.len().max(1);
        let mut cells = vec![vec![None::<f64>; bands]; columns];
        for (t, row) in s.times_ms.iter().zip(&s.cells_db) {
            let k = ((t / span) * (columns as f64 - 1.0)).round() as usize;
            if k >= columns {
                continue;
            }
            for (j, v) in row.iter().enumerate() {
                if let Some(v) = v {
                    let cell = &mut cells[k][j];
                    *cell = Some(cell.map_or(*v, |c: f64| c.max(*v)));
                }
            }
        }
        let mut mesh = Mesh::default();
        let (w, h) = (
            upper.width() / columns as f32,
            upper.height() / bands as f32,
        );
        for (k, column) in cells.iter().enumerate() {
            for (j, cell) in column.iter().enumerate() {
                let Some(db) = cell.filter(|db| *db > -70.0) else {
                    continue;
                };
                let x = upper.left() + k as f32 * w;
                let y = upper.bottom() - (j + 1) as f32 * h;
                mesh.add_colored_rect(
                    Rect::from_min_size(pos2(x, y), vec2(w + 0.5, h + 0.5)),
                    heat(db, -70.0),
                );
            }
        }
        painter.add(egui::Shape::mesh(mesh));
    }

    // The envelope and its grid.
    for db in [-20.0, -40.0, -60.0] {
        let y = y_of(lower, db);
        dashed(
            painter,
            pos2(lower.left(), y),
            pos2(lower.right(), y),
            2.0,
            5.0,
            Stroke::new(1.0, GRID),
        );
        painter.text(
            pos2(lower.left() - 6.0, y),
            Align2::RIGHT_CENTER,
            format!("{db:.0}"),
            mono(8.0),
            DIM,
        );
    }
    if let Some(env) = &curves.envelope {
        let mut runs: Vec<Vec<Pos2>> = vec![Vec::new()];
        for (ms, db) in &env.points {
            match db {
                Some(db) => runs
                    .last_mut()
                    .expect("one run")
                    .push(pos2(x_of(*ms), y_of(lower, *db))),
                None => runs.push(Vec::new()),
            }
        }
        for run in runs.into_iter().filter(|r| r.len() > 1) {
            glow_line(painter, run, false, AMBER, 1.2);
        }
    }

    // The stages, where their readings put them.
    if let Some(scene) = scene {
        let s = &scene.stages;
        let marks = [
            (0.0, s.attack_ms, "ATTACK"),
            (s.attack_ms.unwrap_or(0.0), s.body_ms, "BODY"),
            (s.body_ms.unwrap_or(0.0), s.tail_ms, "TAIL"),
        ];
        for (from, to, name) in marks {
            let Some(to) = to else {
                continue;
            };
            let (a, b) = (x_of(from), x_of(to));
            dashed(
                painter,
                pos2(b, inner.top()),
                pos2(b, inner.bottom()),
                3.0,
                3.0,
                Stroke::new(1.0, fade(AMBER, 0.5)),
            );
            let y = rect.top() + 8.0;
            painter.line_segment(
                [pos2(a + 2.0, y), pos2(b - 2.0, y)],
                Stroke::new(1.5, fade(AMBER, 0.7)),
            );
            if b - a > 36.0 {
                painter.text(
                    pos2((a + b) / 2.0, y + 1.0),
                    Align2::CENTER_TOP,
                    name,
                    mono(9.0),
                    AMBER,
                );
            }
        }
    }

    // The time axis.
    for k in 0..=5 {
        let ms = span * f64::from(k) / 5.0;
        painter.text(
            pos2(x_of(ms), inner.bottom() + 3.0),
            Align2::CENTER_TOP,
            format!("{} ms", number(ms)),
            mono(8.0),
            DIM,
        );
    }

    // Where the sound plays, ms from the onset.
    if let Some(ms) = playhead_ms.filter(|ms| *ms >= 0.0 && *ms <= span) {
        let x = x_of(ms);
        glow_line(
            painter,
            vec![pos2(x, inner.top()), pos2(x, inner.bottom())],
            false,
            AMBER,
            1.5,
        );
        painter.text(
            pos2(x + 5.0, inner.bottom() - 2.0),
            Align2::LEFT_BOTTOM,
            format!("▶ {} ms", number(ms)),
            mono(10.0),
            AMBER,
        );
    }

    // The curve under the cursor.
    if let Some(p) = hover.filter(|p| inner.contains(*p)) {
        painter.line_segment(
            [pos2(p.x, inner.top()), pos2(p.x, inner.bottom())],
            Stroke::new(1.0, fade(TEXT, 0.5)),
        );
        let ms = f64::from((p.x - inner.left()) / inner.width()) * span;
        let level = curves.envelope.as_ref().and_then(|e| {
            e.points
                .iter()
                .min_by(|a, b| (a.0 - ms).abs().total_cmp(&(b.0 - ms).abs()))
                .and_then(|p| p.1)
        });
        let text = match level {
            Some(db) => format!("{} ms · {db:.1} dB", number(ms)),
            None => format!("{} ms", number(ms)),
        };
        painter.text(
            pos2(p.x + 6.0, inner.top()),
            Align2::LEFT_TOP,
            text,
            mono(10.0),
            TEXT,
        );
    }
}

fn y_of(rect: Rect, db: f64) -> f32 {
    let t = ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0) as f32;
    rect.bottom() - t * rect.height()
}

/// How far the strip reaches, ms from the onset: the longest curve.
#[must_use]
pub fn span_ms(c: &Curves) -> f64 {
    let env = c
        .envelope
        .as_ref()
        .and_then(|e| e.points.last())
        .map_or(0.0, |p| p.0);
    let spec = c
        .spectrogram
        .as_ref()
        .and_then(|s| s.times_ms.last().copied())
        .unwrap_or(0.0);
    env.max(spec)
}
