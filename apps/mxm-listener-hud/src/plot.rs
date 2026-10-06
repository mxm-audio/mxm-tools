//! Draws the inspector's figures (`figures`): axes, curves, marks, bars, heat maps, gauges and table
//! plots, in the HUD's look. Each figure is a widget named for what it shows; hovering one explains the
//! reading or the table row nearest the pointer, with its number. Positions are the readings' own
//! values, windows and bands; a single value sits on its unit's scale (`figures::scale`).

use egui::{Align2, Color32, Pos2, Rect, Response, RichText, Sense, Stroke, Ui, pos2, vec2};
use mxm_listening::glossary;
use mxm_listening::reading::{Reading, Section, Table, Unit, Validity};

use crate::figures::{Axis, Figure, Mark, PlotKind, scale};
use crate::glyphs::Knowledge;
use crate::scene::number;
use crate::theme::{
    AMBER, BRIGHT, CYAN, DIM, GRID, PANEL, TEXT, dashed, fade, glow_line, heat, mono, radial,
};

/// What every figure of one part needs.
pub struct Ctx<'a> {
    pub section: &'a Section,
    pub know: &'a Knowledge,
    /// The sound's family's name: the thresholds' context.
    pub family: &'a str,
    /// The part's role colour.
    pub colour: Color32,
    /// Where the sound plays, ms from the onset.
    pub playhead_ms: Option<f64>,
}

/// The dB range a context figure spans, against the loudest.
const FLOOR_DB: f64 = -72.0;

/// Draws one figure at the full width available.
pub fn draw(ui: &mut Ui, figure: &Figure, cx: &Ctx<'_>) {
    match figure {
        Figure::Context {
            title,
            axis,
            curve,
            marks,
        } => context(ui, title, *axis, curve, marks, cx),
        Figure::Durations { readings } => durations(ui, readings, cx),
        Figure::Series {
            label,
            unit,
            axis,
            steps,
            ..
        } => series(ui, label, *unit, *axis, steps, cx),
        Figure::Heat {
            label,
            unit,
            windows,
            bands,
            cells,
            ..
        } => heat_map(ui, label, *unit, windows, bands, cells, cx),
        Figure::Gauges { readings } => {
            for &i in readings {
                gauge(ui, i, cx);
            }
        }
        Figure::Plot { table, kind } => plot(ui, &cx.section.tables[*table], kind, cx),
        Figure::Numbers { table } => numbers(ui, &cx.section.tables[*table]),
    }
    ui.add_space(6.0);
}

/// A figure's frame: allocates `height`, names the widget, draws the panel and the title; returns the
/// plotting area and the response.
fn frame(ui: &mut Ui, height: f32, name: &str, title: &str) -> (Rect, Response, egui::Painter) {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let label = name.to_string();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, label.clone()));
    let painter = ui.painter_at(rect.expand(2.0));
    painter.rect_filled(rect, 2.0, fade(PANEL, 0.7));
    painter.rect_stroke(rect, 2.0, Stroke::new(1.0, GRID), egui::StrokeKind::Inside);
    painter.text(
        rect.left_top() + vec2(6.0, 4.0),
        Align2::LEFT_TOP,
        title,
        mono(10.0),
        TEXT,
    );
    let area = Rect::from_min_max(rect.min + vec2(40.0, 26.0), rect.max - vec2(10.0, 16.0));
    (area, response, painter)
}

/// An axis over `area`'s width: `x_of` and the ticks drawn under it.
struct XAxis {
    lo: f64,
    hi: f64,
    log: bool,
    area: Rect,
}

impl XAxis {
    fn new(axis: Axis, lo: f64, hi: f64, area: Rect) -> Self {
        let log = axis == Axis::Hz;
        let (lo, hi) = if log {
            (lo.max(10.0), hi.max(lo.max(10.0) * 2.0))
        } else {
            (lo, if hi > lo { hi } else { lo + 1.0 })
        };
        Self { lo, hi, log, area }
    }

    fn x(&self, v: f64) -> f32 {
        let t = if self.log {
            (v.max(self.lo).ln() - self.lo.ln()) / (self.hi.ln() - self.lo.ln())
        } else {
            (v - self.lo) / (self.hi - self.lo)
        };
        self.area.left() + t.clamp(0.0, 1.0) as f32 * self.area.width()
    }

    fn value(&self, x: f32) -> f64 {
        let t = f64::from((x - self.area.left()) / self.area.width()).clamp(0.0, 1.0);
        if self.log {
            (self.lo.ln() + t * (self.hi.ln() - self.lo.ln())).exp()
        } else {
            self.lo + t * (self.hi - self.lo)
        }
    }

    fn ticks(&self, painter: &egui::Painter, unit: &str) {
        let values: Vec<f64> = if self.log && self.lo < 10.0 {
            [0.1, 1.0, 10.0, 100.0, 1000.0, 10_000.0, 100_000.0]
                .into_iter()
                .filter(|v| *v >= self.lo && *v <= self.hi)
                .collect()
        } else if self.log {
            [
                20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10_000.0, 20_000.0,
            ]
            .into_iter()
            .filter(|v| *v >= self.lo && *v <= self.hi)
            .collect()
        } else {
            (0..4)
                .map(|k| self.lo + (self.hi - self.lo) * f64::from(k) / 4.0)
                .collect()
        };
        let values: Vec<f64> = values
            .into_iter()
            .filter(|v| !self.log || *v <= self.hi / 1.6)
            .collect();
        for v in values {
            let x = self.x(v);
            painter.line_segment(
                [pos2(x, self.area.top()), pos2(x, self.area.bottom())],
                Stroke::new(1.0, fade(GRID, 0.6)),
            );
            let text = if self.log && self.lo >= 10.0 && v >= 1000.0 {
                format!("{}k", v / 1000.0)
            } else {
                tick(v)
            };
            painter.text(
                pos2(x, self.area.bottom() + 2.0),
                Align2::CENTER_TOP,
                text,
                mono(8.0),
                DIM,
            );
        }
        painter.text(
            pos2(self.area.right(), self.area.bottom() + 2.0),
            Align2::RIGHT_TOP,
            unit,
            mono(8.0),
            DIM,
        );
    }
}

/// A vertical scale over `area`'s height.
fn y_of(area: Rect, lo: f64, hi: f64, v: f64) -> f32 {
    let t = if hi > lo { (v - lo) / (hi - lo) } else { 0.5 };
    area.bottom() - t.clamp(0.0, 1.0) as f32 * area.height()
}

fn y_ticks(painter: &egui::Painter, area: Rect, lo: f64, hi: f64, unit: &str) {
    for k in 0..=2 {
        let v = lo + (hi - lo) * f64::from(k) / 2.0;
        let y = y_of(area, lo, hi, v);
        dashed(
            painter,
            pos2(area.left(), y),
            pos2(area.right(), y),
            2.0,
            4.0,
            Stroke::new(1.0, fade(GRID, 0.7)),
        );
        painter.text(
            pos2(area.left() - 4.0, y),
            Align2::RIGHT_CENTER,
            tick(v),
            mono(8.0),
            DIM,
        );
    }
    painter.text(
        pos2(area.left() - 4.0, area.top() - 10.0),
        Align2::RIGHT_TOP,
        unit,
        mono(8.0),
        DIM,
    );
}

fn playhead(painter: &egui::Painter, x: &XAxis, ms: Option<f64>) {
    if let Some(ms) = ms.filter(|ms| *ms >= x.lo && *ms <= x.hi) {
        let px = x.x(ms);
        glow_line(
            painter,
            vec![pos2(px, x.area.top()), pos2(px, x.area.bottom())],
            false,
            AMBER,
            1.2,
        );
    }
}

/// The part in context: the envelope or the spectrum, the part's readings marked on it.
fn context(
    ui: &mut Ui,
    title: &str,
    axis: Axis,
    curve: &[(f64, Option<f64>)],
    marks: &[Mark],
    cx: &Ctx<'_>,
) {
    let name = format!("{} {title}", cx.section.part.to_uppercase());
    let (area, response, painter) = frame(ui, 190.0, &name, &name);
    let (lo, hi) = match axis {
        Axis::Hz => (20.0, curve.last().map_or(20_000.0, |p| p.0)),
        _ => {
            let end = marks
                .iter()
                .filter_map(|m| match m {
                    Mark::Segment { to_ms, .. } => Some(*to_ms),
                    _ => None,
                })
                .fold(curve.last().map_or(100.0, |p| p.0), f64::max);
            (0.0, end)
        }
    };
    let x = XAxis::new(axis, lo, hi, area);
    x.ticks(&painter, if axis == Axis::Hz { "Hz" } else { "ms" });
    y_ticks(&painter, area, FLOOR_DB, 0.0, "dB");

    // The curve.
    let mut runs: Vec<Vec<Pos2>> = vec![Vec::new()];
    for (v, db) in curve {
        match db {
            Some(db) if *v >= x.lo && *v <= x.hi => runs
                .last_mut()
                .expect("a run")
                .push(pos2(x.x(*v), y_of(area, FLOOR_DB, 0.0, *db))),
            _ => runs.push(Vec::new()),
        }
    }
    let curve_colour = if axis == Axis::Hz { CYAN } else { AMBER };
    for run in runs.into_iter().filter(|r| r.len() > 1) {
        glow_line(&painter, run, false, fade(curve_colour, 0.8), 1.0);
    }

    // The stems: a table's rows, on their own scale.
    let stems: Vec<(usize, usize, f64, f64)> = marks
        .iter()
        .filter_map(|m| match m {
            Mark::Stem { table, row, hz, db } => Some((*table, *row, *hz, *db)),
            _ => None,
        })
        .collect();
    let (s_lo, s_hi) = stems
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), s| {
            (a.min(s.3), b.max(s.3))
        });
    let s_lo = s_lo.min(s_hi - 30.0);
    for &(_, _, hz, db) in &stems {
        let px = x.x(hz);
        let top = y_of(area, s_lo, s_hi, db);
        painter.line_segment(
            [pos2(px, area.bottom()), pos2(px, top)],
            Stroke::new(1.0, fade(BRIGHT, 0.35)),
        );
        painter.circle_filled(pos2(px, top), 2.0, fade(BRIGHT, 0.7));
    }

    // The readings.
    let mut near: Option<(f32, String)> = None;
    let pointer = response.hover_pos();
    fn consider(near: &mut Option<(f32, String)>, d: f32, text: String) {
        if near.as_ref().is_none_or(|(best, _)| d < *best) {
            *near = Some((d, text));
        }
    }
    let mut label_row = 0;
    for m in marks {
        match m {
            Mark::At { reading, hz } => {
                let r = &cx.section.readings[*reading];
                let px = x.x(*hz);
                dashed(
                    &painter,
                    pos2(px, area.top()),
                    pos2(px, area.bottom()),
                    3.0,
                    2.0,
                    Stroke::new(1.3, cx.colour),
                );
                painter.text(
                    pos2(px + 3.0, area.top() + 2.0 + 10.0 * (label_row % 4) as f32),
                    Align2::LEFT_TOP,
                    short(r.label),
                    mono(8.0),
                    cx.colour,
                );
                label_row += 1;
                if let Some(p) = pointer {
                    consider(&mut near, (p.x - px).abs(), cx.know.explain(r, cx.family));
                }
            }
            Mark::Segment {
                reading,
                from_ms,
                to_ms,
                db,
            } => {
                let r = &cx.section.readings[*reading];
                let (a, b) = (x.x(*from_ms), x.x(*to_ms));
                let y = y_of(area, FLOOR_DB, 0.0, *db);
                glow_line(
                    &painter,
                    vec![pos2(a, y), pos2(b, y)],
                    false,
                    cx.colour,
                    1.6,
                );
                for end in [a, b] {
                    painter.line_segment(
                        [pos2(end, y - 4.0), pos2(end, y + 4.0)],
                        Stroke::new(1.0, cx.colour),
                    );
                }
                if let Some(p) = pointer
                    && p.x >= a - 4.0
                    && p.x <= b + 4.0
                {
                    consider(&mut near, (p.y - y).abs(), cx.know.explain(r, cx.family));
                }
            }
            Mark::Stem { .. } => {}
        }
    }
    if let Some(p) = pointer
        && near.as_ref().is_none_or(|(d, _)| *d > 12.0)
    {
        for &(t, row, hz, _) in &stems {
            let d = (p.x - x.x(hz)).abs();
            if d < 6.0 {
                consider(&mut near, d, row_text(&cx.section.tables[t], row));
            }
        }
    }
    if axis == Axis::Ms {
        playhead(&painter, &x, cx.playhead_ms);
    }
    let text = match (near, pointer) {
        (Some((d, text)), _) if d <= 12.0 => text,
        (_, Some(p)) if area.contains(p) => {
            let v = x.value(p.x);
            let db = curve
                .iter()
                .min_by(|a, b| (a.0 - v).abs().total_cmp(&(b.0 - v).abs()))
                .and_then(|c| c.1);
            let unit = if axis == Axis::Hz { "Hz" } else { "ms" };
            match db {
                Some(db) => format!("{} {unit} · {db:.1} dB", number(v)),
                None => format!("{} {unit}", number(v)),
            }
        }
        _ => glossary::meaning(if axis == Axis::Hz {
            "curve.spectrum"
        } else {
            "curve.envelope"
        })
        .unwrap_or("")
        .to_string(),
    };
    response.on_hover_text(text);
}

/// Readings that are times, each a bar from the axis's start on one logarithmic time scale.
fn durations(ui: &mut Ui, readings: &[usize], cx: &Ctx<'_>) {
    let rows = readings.len() as f32;
    let (area, response, painter) = frame(ui, 44.0 + 24.0 * rows, "DURATIONS", "DURATIONS");
    let ms = |r: &Reading| {
        r.value.map(|v| {
            if r.unit == Unit::Seconds {
                v * 1000.0
            } else {
                v
            }
        })
    };
    let hi = readings
        .iter()
        .filter_map(|&i| ms(&cx.section.readings[i]))
        .fold(10.0, f64::max)
        * 1.5;
    let x = XAxis {
        lo: 0.1,
        hi,
        log: true,
        area,
    };
    x.ticks(&painter, "ms, log");
    let mut hovered = None;
    for (k, &i) in readings.iter().enumerate() {
        let r = &cx.section.readings[i];
        let y = area.top() + 14.0 + 24.0 * k as f32;
        let Some(v) = ms(r) else {
            continue;
        };
        let end = x.x(v);
        painter.text(
            pos2(area.left(), y - 5.0),
            Align2::LEFT_BOTTOM,
            short(r.label),
            mono(8.0),
            DIM,
        );
        painter.text(
            pos2(area.right(), y - 5.0),
            Align2::RIGHT_BOTTOM,
            format!("{} {}", number(r.value.unwrap_or(v)), r.unit.symbol()),
            mono(9.0),
            TEXT,
        );
        glow_line(
            &painter,
            vec![pos2(area.left(), y), pos2(end, y)],
            false,
            cx.colour,
            3.0,
        );
        if response.hover_pos().is_some_and(|p| (p.y - y).abs() < 12.0) {
            hovered = Some(cx.know.explain(r, cx.family));
        }
    }
    response.on_hover_text(hovered.unwrap_or_else(|| {
        "Each time a reading measures, as a bar on one logarithmic time scale.".into()
    }));
}

/// One reading over its windows, or across its bands.
fn series(
    ui: &mut Ui,
    label: &str,
    unit: Unit,
    axis: Axis,
    steps: &[(usize, f64, f64)],
    cx: &Ctx<'_>,
) {
    let (area, response, painter) = frame(ui, 110.0, label, label);
    let values: Vec<f64> = steps
        .iter()
        .filter_map(|(i, _, _)| cx.section.readings[*i].value)
        .collect();
    let (mut lo, mut hi) = values
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
            (a.min(*v), b.max(*v))
        });
    if !lo.is_finite() {
        (lo, hi) = (0.0, 1.0);
    }
    if hi - lo < 1e-9 {
        (lo, hi) = (lo - 1.0, hi + 1.0);
    }
    let pad = (hi - lo) * 0.15;
    let floor = if values.iter().all(|v| *v >= 0.0) {
        0.0
    } else {
        f64::NEG_INFINITY
    };
    let (lo, hi) = ((lo - pad).max(floor), hi + pad);
    let x0 = steps.iter().map(|s| s.1).fold(f64::INFINITY, f64::min);
    let x1 = steps.iter().map(|s| s.2).fold(f64::NEG_INFINITY, f64::max);
    let x = XAxis::new(axis, if axis == Axis::Ms { 0.0 } else { x0 }, x1, area);
    x.ticks(&painter, if axis == Axis::Hz { "Hz" } else { "ms" });
    y_ticks(&painter, area, lo, hi, unit.symbol());
    let zero = (lo < 0.0 && hi > 0.0).then(|| y_of(area, lo, hi, 0.0));
    let mut hovered = None;
    for &(i, a, b) in steps {
        let r = &cx.section.readings[i];
        let (xa, xb) = (x.x(a), x.x(b));
        match r.value {
            Some(v) => {
                let y = y_of(area, lo, hi, v);
                if axis == Axis::Hz {
                    let base = zero.unwrap_or(area.bottom());
                    painter.rect_filled(
                        Rect::from_two_pos(pos2(xa + 1.0, y), pos2(xb - 1.0, base)),
                        1.0,
                        fade(cx.colour, 0.35),
                    );
                }
                glow_line(
                    &painter,
                    vec![pos2(xa, y), pos2(xb, y)],
                    false,
                    cx.colour,
                    1.6,
                );
                painter.circle_filled(pos2((xa + xb) / 2.0, y), 2.2, BRIGHT);
            }
            None => {
                painter.text(
                    pos2((xa + xb) / 2.0, area.bottom() - 2.0),
                    Align2::CENTER_BOTTOM,
                    "×",
                    mono(10.0),
                    DIM,
                );
            }
        }
        if response
            .hover_pos()
            .is_some_and(|p| p.x >= xa.min(xb) && p.x <= xa.max(xb))
        {
            hovered = Some(cx.know.explain(r, cx.family));
        }
    }
    if axis == Axis::Ms {
        playhead(&painter, &x, cx.playhead_ms);
    }
    response.on_hover_text(hovered.unwrap_or_else(|| label.to_string()));
}

/// One reading over windows and bands: a cell each, coloured by its value over the figure's range.
#[allow(clippy::too_many_arguments)]
fn heat_map(
    ui: &mut Ui,
    label: &str,
    unit: Unit,
    windows: &[(f64, f64)],
    bands: &[(f64, f64)],
    cells: &[Vec<Option<usize>>],
    cx: &Ctx<'_>,
) {
    let rows = bands.len() as f32;
    let (area, response, painter) = frame(ui, 44.0 + 11.0 * rows.min(12.0), label, label);
    let values: Vec<f64> = cells
        .iter()
        .flatten()
        .flatten()
        .filter_map(|&i| cx.section.readings[i].value)
        .collect();
    let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let (w, h) = (
        area.width() / windows.len().max(1) as f32,
        area.height() / bands.len().max(1) as f32,
    );
    let mut hovered = None;
    for (b, row) in cells.iter().enumerate() {
        for (k, cell) in row.iter().enumerate() {
            let rect = Rect::from_min_size(
                pos2(
                    area.left() + w * k as f32,
                    area.bottom() - h * (b + 1) as f32,
                ),
                vec2(w - 1.0, h - 1.0),
            );
            let Some(i) = cell else {
                continue;
            };
            let r = &cx.section.readings[*i];
            if let Some(v) = r.value {
                let t = if hi > lo { (v - lo) / (hi - lo) } else { 1.0 };
                painter.rect_filled(rect, 0.0, heat(-60.0 * (1.0 - t), -60.0));
            } else {
                painter.rect_stroke(rect, 0.0, Stroke::new(1.0, GRID), egui::StrokeKind::Inside);
            }
            if response.hover_pos().is_some_and(|p| rect.contains(p)) {
                hovered = Some(cx.know.explain(r, cx.family));
            }
        }
    }
    for (b, band) in bands.iter().enumerate() {
        if b % 2 == 0 {
            painter.text(
                pos2(area.left() - 4.0, area.bottom() - h * (b as f32 + 0.5)),
                Align2::RIGHT_CENTER,
                number(band.0),
                mono(7.0),
                DIM,
            );
        }
    }
    for (k, win) in windows.iter().enumerate().filter(|(k, _)| k % 2 == 0) {
        painter.text(
            pos2(area.left() + w * (k as f32 + 0.5), area.bottom() + 2.0),
            Align2::CENTER_TOP,
            number(win.0),
            mono(7.0),
            DIM,
        );
    }
    painter.text(
        pos2(area.right(), area.top() - 12.0),
        Align2::RIGHT_TOP,
        if lo.is_finite() {
            format!("{} … {} {}", number(lo), number(hi), unit.symbol())
        } else {
            "no values".into()
        },
        mono(8.0),
        DIM,
    );
    response.on_hover_text(
        hovered.unwrap_or_else(|| format!("{label}: rows are bands (Hz), columns windows (ms)")),
    );
}

/// One reading with one value, on its unit's scale.
fn gauge(ui: &mut Ui, i: usize, cx: &Ctx<'_>) {
    let r = &cx.section.readings[i];
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::hover());
    let name = r.label.to_string();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, name.clone()));
    let painter = ui.painter_at(rect.expand(2.0));
    painter.text(
        rect.left_top(),
        Align2::LEFT_TOP,
        short(r.label),
        mono(9.0),
        TEXT,
    );
    let track = Rect::from_min_max(
        pos2(rect.left(), rect.top() + 16.0),
        pos2(rect.right() - 96.0, rect.top() + 28.0),
    );
    let value_at = pos2(rect.right(), track.center().y);
    let dim = matches!(r.validity, Validity::BelowFloor);
    match (r.value, r.value.and_then(|v| scale(r.unit, v))) {
        (Some(v), Some((lo, hi, log))) => {
            let t = if log {
                (v.max(lo).ln() - lo.ln()) / (hi.ln() - lo.ln())
            } else {
                (v - lo) / (hi - lo)
            };
            let x = track.left() + t.clamp(0.0, 1.0) as f32 * track.width();
            painter.line_segment(
                [track.left_center(), track.right_center()],
                Stroke::new(1.0, GRID),
            );
            for end in [track.left(), track.right()] {
                painter.line_segment(
                    [pos2(end, track.top()), pos2(end, track.bottom())],
                    Stroke::new(1.0, GRID),
                );
            }
            painter.text(
                track.left_bottom() + vec2(0.0, 1.0),
                Align2::LEFT_TOP,
                number(lo),
                mono(7.0),
                DIM,
            );
            painter.text(
                track.right_bottom() + vec2(0.0, 1.0),
                Align2::RIGHT_TOP,
                number(hi),
                mono(7.0),
                DIM,
            );
            let colour = if dim { fade(cx.colour, 0.4) } else { cx.colour };
            glow_line(
                &painter,
                vec![track.left_center(), pos2(x, track.center().y)],
                false,
                fade(colour, 0.6),
                2.0,
            );
            radial(&painter, pos2(x, track.center().y), 10.0, colour, 0.5);
            painter.circle_filled(pos2(x, track.center().y), 3.5, BRIGHT);
            if !(lo..=hi).contains(&v) {
                painter.text(
                    pos2(x, track.top() - 2.0),
                    Align2::CENTER_BOTTOM,
                    if v < lo { "◂" } else { "▸" },
                    mono(9.0),
                    AMBER,
                );
            }
        }
        (Some(_), None) => {}
        (None, _) => {
            let why = match &r.validity {
                Validity::Absent(why) => format!("NO SIGNAL — {why}"),
                _ => "NO SIGNAL".into(),
            };
            painter.text(
                track.left_center(),
                Align2::LEFT_CENTER,
                why,
                mono(8.0),
                DIM,
            );
        }
    }
    let mut value = match r.value {
        Some(v) => format!("{} {}", number(v), r.unit.symbol())
            .trim_end()
            .to_string(),
        None => String::new(),
    };
    if r.validity == Validity::Sample {
        value.push_str(" ≈");
    }
    painter.text(value_at, Align2::RIGHT_CENTER, value, mono(12.0), BRIGHT);
    response.on_hover_text(cx.know.explain(r, cx.family));
}

/// A table as a plot.
fn plot(ui: &mut Ui, table: &Table, kind: &PlotKind, cx: &Ctx<'_>) {
    let window = table
        .window_ms
        .map(|(a, b)| format!(" · {}–{} ms", number(a), number(b)))
        .unwrap_or_default();
    let title = format!("{}{window}", table.title);
    match kind {
        PlotKind::Stems { hz, db } => {
            let (area, response, painter) = frame(ui, 150.0, &table.title, &title);
            let points: Vec<(usize, f64, f64)> = table
                .rows
                .iter()
                .enumerate()
                .filter_map(|(k, row)| Some((k, (*row.get(*hz)?)?, (*row.get(*db)?)?)))
                .collect();
            let x_lo = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
            let x_hi = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
            let y_lo = points
                .iter()
                .map(|p| p.2)
                .fold(f64::INFINITY, f64::min)
                .min(-40.0);
            let y_hi = points
                .iter()
                .map(|p| p.2)
                .fold(f64::NEG_INFINITY, f64::max)
                .max(0.0);
            let x = XAxis::new(
                Axis::Hz,
                (x_lo / 1.5).max(10.0),
                (x_hi * 1.5).max(100.0),
                area,
            );
            x.ticks(&painter, "Hz");
            y_ticks(&painter, area, y_lo, y_hi, table.columns[*db].1.symbol());
            let mut hovered = None;
            for &(k, f, l) in &points {
                let px = x.x(f);
                let top = y_of(area, y_lo, y_hi, l);
                glow_line(
                    &painter,
                    vec![pos2(px, area.bottom()), pos2(px, top)],
                    false,
                    cx.colour,
                    1.2,
                );
                painter.circle_filled(pos2(px, top), 3.0, BRIGHT);
                if response.hover_pos().is_some_and(|p| (p.x - px).abs() < 5.0) {
                    hovered = Some(row_text(table, k));
                }
            }
            response.on_hover_text(hovered.unwrap_or_else(|| table_meaning(table)));
        }
        PlotKind::Lanes { x: xc, ys } => {
            let lanes = ys.len().max(1) as f32;
            let (area, response, painter) = frame(ui, 36.0 + 46.0 * lanes, &table.title, &title);
            let xs: Vec<f64> = table.rows.iter().filter_map(|r| r[*xc]).collect();
            let (x_lo, x_hi) = xs
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
                    (a.min(*v), b.max(*v))
                });
            if !x_lo.is_finite() {
                numbers(ui, table);
                return;
            }
            let axis = match table.columns[*xc].1 {
                Unit::Hertz => Axis::Hz,
                Unit::Milliseconds => Axis::Ms,
                other => Axis::Linear(other),
            };
            let x = XAxis::new(axis, x_lo, x_hi, area);
            x.ticks(&painter, table.columns[*xc].1.symbol());
            let lane_h = area.height() / lanes;
            let mut hovered = None;
            for (n, &c) in ys.iter().enumerate() {
                let lane = Rect::from_min_size(
                    pos2(area.left(), area.top() + lane_h * n as f32),
                    vec2(area.width(), lane_h - 6.0),
                );
                let vals: Vec<(f64, f64)> = table
                    .rows
                    .iter()
                    .filter_map(|r| Some((r[*xc]?, r[c]?)))
                    .collect();
                let lo = vals.iter().map(|v| v.1).fold(f64::INFINITY, f64::min);
                let hi = vals.iter().map(|v| v.1).fold(f64::NEG_INFINITY, f64::max);
                let (lo, hi) = if hi - lo < 1e-9 {
                    (lo - 1.0, hi + 1.0)
                } else {
                    (lo, hi)
                };
                let (name, unit) = table.columns[c];
                let label = painter.layout_no_wrap(
                    format!("{name} ({}) {}…{}", unit.symbol(), number(lo), number(hi)),
                    mono(8.0),
                    DIM,
                );
                let at = lane.left_top() + vec2(4.0, 0.0);
                painter.rect_filled(
                    Rect::from_min_size(at, label.size()).expand(1.0),
                    1.0,
                    fade(PANEL, 0.85),
                );
                painter.galley(at, label, DIM);
                let line: Vec<Pos2> = vals
                    .iter()
                    .map(|(a, b)| pos2(x.x(*a), y_of(lane, lo, hi, *b)))
                    .collect();
                let colour = [cx.colour, CYAN, AMBER, BRIGHT][n % 4];
                if line.len() > 1 {
                    glow_line(&painter, line, false, colour, 1.2);
                } else if let Some(p) = line.first() {
                    painter.circle_filled(*p, 2.5, colour);
                }
                if let Some(p) = response.hover_pos().filter(|p| lane.contains(*p)) {
                    let at = x.value(p.x);
                    if let Some(k) = table
                        .rows
                        .iter()
                        .enumerate()
                        .filter_map(|(k, r)| r[*xc].map(|v| (k, (v - at).abs())))
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|(k, _)| k)
                    {
                        hovered = Some(row_text(table, k));
                    }
                }
            }
            if axis == Axis::Ms {
                playhead(&painter, &x, cx.playhead_ms);
            }
            response.on_hover_text(hovered.unwrap_or_else(|| table_meaning(table)));
        }
    }
}

/// A table no plot fits: its numbers.
pub fn numbers(ui: &mut Ui, table: &Table) {
    ui.label(RichText::new(&table.title).font(mono(10.0)).color(TEXT))
        .on_hover_text(table_meaning(table));
    egui::Grid::new(("numbers", table.id, table.title.as_str()))
        .striped(true)
        .num_columns(table.columns.len())
        .show(ui, |ui| {
            for (name, unit) in &table.columns {
                let symbol = unit.symbol();
                let head = if symbol.is_empty() {
                    (*name).to_string()
                } else {
                    format!("{name} ({symbol})")
                };
                ui.label(RichText::new(head).font(mono(9.0)).color(DIM));
            }
            ui.end_row();
            for row in &table.rows {
                for cell in row {
                    ui.label(
                        RichText::new(cell.map(number).unwrap_or_else(|| "—".into()))
                            .font(mono(9.0))
                            .color(TEXT),
                    );
                }
                ui.end_row();
            }
        });
}

/// An axis tick's number, shorter than a reading's.
fn tick(v: f64) -> String {
    let a = v.abs();
    if a >= 10.0 || a == 0.0 {
        format!("{v:.0}")
    } else if a >= 1.0 {
        format!("{v:.1}")
    } else {
        number(v)
    }
}

/// One table row, every column with its number: a hover's text.
fn row_text(table: &Table, row: usize) -> String {
    let cells = &table.rows[row];
    let mut out = vec![table.title.clone()];
    for ((name, unit), cell) in table.columns.iter().zip(cells) {
        out.push(match cell {
            Some(v) => format!("{name}: {} {}", number(*v), unit.symbol()),
            None => format!("{name}: —"),
        });
    }
    out.join("\n")
}

fn table_meaning(table: &Table) -> String {
    format!(
        "{}\n\n{}",
        glossary::meaning(table.id).unwrap_or(&table.title),
        table.id
    )
}

/// A label shortened to fit a figure: up to its first parenthesis or colon.
fn short(label: &str) -> String {
    let cut = label.find(['(', ':']).unwrap_or(label.len());
    let s = label[..cut].trim_end();
    if s.chars().count() > 34 {
        format!("{}…", s.chars().take(33).collect::<String>())
    } else {
        s.to_string()
    }
}
