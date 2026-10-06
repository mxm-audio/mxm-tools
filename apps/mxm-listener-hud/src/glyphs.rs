//! The parts of the sound, orbiting the scope, and the inspector that opens when you zoom into one.
//!
//! Each part is a glyph in its role's colour and shape, at a fixed place on the orbit (the place is a
//! cue as much as the colour), bright as the share of its readings that hold a value. **The mouse wheel
//! zooms**: over a glyph, up goes one level deeper and down one back — zoom 1 its key readings as
//! callouts, zoom 2 every reading with where and how finely it was measured, zoom 3 its tables and
//! sources. A click opens zoom 1 or closes. Every row explains itself on hover.

use egui::{Align2, Pos2, Rect, RichText, Sense, Stroke, Ui, vec2};
use mxm_listening::audibility::{Kind, Thresholds, Vocabulary};
use mxm_listening::curves::Curves;
use mxm_listening::glossary;
use mxm_listening::reading::{Reading, Report, Validity};

use crate::scene::{Part, Role, Scene, number, with_unit};
use crate::theme::{
    DIM, PANEL, TEXT, brackets, dashed, fade, glow_line, mono, radial, role_colour, role_shape,
};
use crate::{figures, plot};

/// Where the owner has zoomed in: a part and how deep.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Focus {
    pub part: &'static str,
    /// 1 to [`DEEPEST`].
    pub level: u8,
}

pub const DEEPEST: u8 = 3;

/// Wheel travel for one zoom step, points.
const WHEEL_STEP: f32 = 30.0;

/// What explains a reading: the glossary, the thresholds (the owner's over the literature's) and the
/// owner's words.
pub struct Knowledge {
    pub thresholds: Thresholds,
    pub owner_note: Option<String>,
    pub vocabulary: Vocabulary,
}

impl Knowledge {
    /// The literature's thresholds with the owner's from `.listening/` laid over, as `listen` does.
    #[must_use]
    pub fn load() -> Self {
        let (thresholds, owner_note) = Thresholds::with_owner(std::path::Path::new(".listening"));
        Self {
            thresholds,
            owner_note,
            vocabulary: Vocabulary::owner(),
        }
    }

    /// A reading's full explanation: what it means, where it was measured, the smallest difference
    /// heard, and the owner's words for a change in it.
    #[must_use]
    pub fn explain(&self, r: &Reading, family: &str) -> String {
        let mut out = Vec::new();
        out.push(glossary::meaning(r.id).unwrap_or(r.label).to_string());
        let mut place = Vec::new();
        if let Some((a, b)) = r.window_ms {
            place.push(format!("{}–{} ms from the onset", number(a), number(b)));
        }
        if let Some((a, b)) = r.band_hz {
            place.push(format!("{}–{} Hz", number(a), number(b)));
        }
        if let Some(res) = r.resolution {
            let mut s = format!("a {} ms window", number(res.window_ms));
            if let Some(bin) = res.bin_hz {
                s.push_str(&format!(", {} Hz bins", number(bin)));
            }
            place.push(s);
        }
        if !place.is_empty() {
            out.push(format!("Measured: {}.", place.join(", ")));
        }
        if let Some(t) = self.thresholds.for_reading_in(r.id, r.unit, Some(family)) {
            let size = match t.kind {
                Kind::Absolute => format!("{} {}", number(t.value), r.unit.symbol()),
                Kind::Relative => format!("{:.0} %", t.value * 100.0),
            };
            out.push(format!(
                "Heard from a change of about {size} ({}).",
                t.source
            ));
        }
        let words: Vec<String> = self
            .vocabulary
            .for_id(r.id)
            .into_iter()
            .map(|m| {
                let way = match m.direction {
                    1 => "more: ",
                    -1 => "less: ",
                    _ => "",
                };
                format!("{way}\"{}\"", m.phrase)
            })
            .collect();
        if !words.is_empty() {
            out.push(format!("Your words: {}.", words.join("; ")));
        }
        out.push(format!("Source: {}.", r.source));
        out.push(r.id.to_string());
        out.join("\n\n")
    }
}

/// A glyph's place on the orbit, radians from 3 o'clock, clockwise: fixed for each part, so where a
/// glyph sits says what it is. Parts the table does not name share what is left.
#[must_use]
pub fn place(part: &str, others: usize) -> f32 {
    // Clock degrees, 0 at 12 o'clock, clockwise: time on the left, the ring at the top, character on
    // the right, noise below, the recording chain bottom left.
    let clock = match part {
        "pitch" | "response" => 0.0,
        "sustain" => 28.0,
        "voice" => 52.0,
        "tone" => 78.0,
        "words" => 102.0,
        "perception" | "space" => 126.0,
        "texture" => 154.0,
        "modulation" => 180.0,
        "artefacts" => 206.0,
        "decay" | "delay" => 234.0,
        "level" => 258.0,
        "attack" => 282.0,
        "tonality" => 308.0,
        "note" => 332.0,
        _ => 190.0 + 18.0 * others as f32,
    };
    (clock - 90.0_f32).to_radians()
}

/// Draws the orbit — an ellipse `orbit` wide and tall around the scope — and handles its zoom.
#[allow(clippy::too_many_arguments)]
pub fn orbit(
    ui: &mut Ui,
    centre: Pos2,
    radius: f32,
    orbit: egui::Vec2,
    scene: &Scene,
    focus: &mut Option<Focus>,
    time: f64,
    playhead_ms: Option<f64>,
) -> Vec<(&'static str, Pos2)> {
    let t = time as f32;
    let painter = ui.painter().clone();
    let mut others = 0;
    let mut places = Vec::with_capacity(scene.parts.len());
    for part in &scene.parts {
        let a = place(part.part, others);
        if !matches!(
            part.part,
            "pitch"
                | "response"
                | "sustain"
                | "voice"
                | "tone"
                | "words"
                | "perception"
                | "space"
                | "texture"
                | "modulation"
                | "artefacts"
                | "decay"
                | "delay"
                | "level"
                | "attack"
                | "tonality"
                | "note"
        ) {
            others += 1;
        }
        let d = vec2(a.cos(), a.sin());
        let at = centre + vec2(d.x * orbit.x, d.y * orbit.y);
        places.push((part.part, at));
        let colour = role_colour(part.role);
        let focused = focus.as_ref().is_some_and(|f| f.part == part.part);
        let rect = Rect::from_center_size(at, vec2(56.0, 56.0));
        let id = ui.id().with(("glyph", part.part));
        let response = ui.interact(rect, id, Sense::click()).on_hover_text(format!(
            "{} — {} readings, {} tables.\nScroll up to zoom in, down to zoom out; click to open.",
            part.title, part.readings, part.tables
        ));
        // Its accessible name: what a screen reader says, and what a test finds it by.
        let name = format!("{} part", part.part.to_uppercase());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name.clone())
        });

        // Zoom: the wheel over the glyph, a click to open or close.
        if response.hovered() {
            let wheel = ui.input(|i| {
                i.raw
                    .events
                    .iter()
                    .map(|e| match e {
                        egui::Event::MouseWheel { unit, delta, .. } => match unit {
                            egui::MouseWheelUnit::Point => delta.y,
                            egui::MouseWheelUnit::Line => delta.y * WHEEL_STEP,
                            egui::MouseWheelUnit::Page => delta.y * WHEEL_STEP * 4.0,
                        },
                        _ => 0.0,
                    })
                    .sum::<f32>()
            });
            if wheel != 0.0 {
                let travel_id = id.with("wheel");
                let travel = ui.data_mut(|d| *d.get_temp_mut_or_default::<f32>(travel_id) + wheel);
                let steps = (travel / WHEEL_STEP).trunc();
                ui.data_mut(|d| d.insert_temp(travel_id, travel - steps * WHEEL_STEP));
                if steps != 0.0 {
                    zoom(focus, part.part, steps as i32);
                }
            }
        }
        if response.clicked() {
            if focused {
                *focus = None;
            } else {
                *focus = Some(Focus {
                    part: part.part,
                    level: 1,
                });
            }
        }

        // A faint line from the scope's edge out to the glyph.
        dashed(
            &painter,
            centre + d * (radius + 44.0),
            at - d * 30.0,
            3.0,
            4.0,
            Stroke::new(1.0, fade(colour, 0.25)),
        );
        // Lit while the sound plays through a window one of its key readings was measured in.
        let lit = playhead_ms.is_some_and(|ms| {
            part.key
                .iter()
                .any(|k| k.window_ms.is_some_and(|(a, b)| ms >= a && ms <= b))
        });
        glyph(
            &painter,
            at,
            d,
            part,
            colour,
            focused || lit,
            response.hovered() || lit,
            t,
        );
    }
    places
}

/// Where the callout for a glyph at `at` goes, `size` large, within `bounds`: beside the glyph on the
/// side towards the scope's centre, so the glyph stays in sight, and level with it where the bounds
/// allow. Returns the panel and the point on its edge the leader line meets.
#[must_use]
pub fn callout_place(at: Pos2, centre: Pos2, size: egui::Vec2, bounds: Rect) -> (Rect, Pos2) {
    const GAP: f32 = 72.0;
    let rightwards = at.x <= centre.x;
    let x = if rightwards {
        at.x + GAP
    } else {
        at.x - GAP - size.x
    };
    let x = x.clamp(
        bounds.left() + 4.0,
        (bounds.right() - size.x - 4.0).max(bounds.left()),
    );
    let y = (at.y - size.y / 2.0).clamp(
        bounds.top() + 4.0,
        (bounds.bottom() - size.y - 4.0).max(bounds.top()),
    );
    let panel = Rect::from_min_size(egui::pos2(x, y), size);
    let edge_x = if rightwards {
        panel.left()
    } else {
        panel.right()
    };
    let edge = egui::pos2(
        edge_x,
        at.y.clamp(panel.top() + 24.0, panel.bottom() - 24.0),
    );
    (panel, edge)
}

/// The leader line from a glyph at `at` to the callout's edge: out of the glyph level, then to the
/// panel, glowing in the part's colour, a ring at the glyph and a bar where it meets the panel.
pub fn leader(painter: &egui::Painter, at: Pos2, edge: Pos2, colour: egui::Color32, time: f64) {
    let dir = (edge - at).normalized();
    let start = at + dir * 30.0;
    let elbow = egui::pos2(start.x + (edge.x - start.x) * 0.45, start.y);
    glow_line(painter, vec![start, elbow, edge], false, colour, 1.4);
    let t = time as f32;
    crate::theme::dashed_ring(
        painter,
        at,
        30.0,
        16,
        0.5,
        t * 0.8,
        Stroke::new(1.2, fade(colour, 0.8)),
    );
    painter.circle_filled(start, 2.5, colour);
    painter.line_segment(
        [edge - vec2(0.0, 10.0), edge + vec2(0.0, 10.0)],
        Stroke::new(3.0, colour),
    );
}

/// One step deeper (`steps` > 0) or back, for `part`.
fn zoom(focus: &mut Option<Focus>, part: &'static str, steps: i32) {
    let now = match focus {
        Some(f) if f.part == part => i32::from(f.level),
        _ => 0,
    };
    let level = (now + steps).clamp(0, i32::from(DEEPEST));
    *focus = (level > 0).then_some(Focus {
        part,
        level: level as u8,
    });
}

/// One glyph at `at`, its label and number on the side away from the scope (`out`).
#[allow(clippy::too_many_arguments)]
fn glyph(
    painter: &egui::Painter,
    at: Pos2,
    out: egui::Vec2,
    part: &Part,
    colour: egui::Color32,
    focused: bool,
    hovered: bool,
    t: f32,
) {
    let share = if part.readings == 0 {
        0.0
    } else {
        part.present as f32 / part.readings as f32
    };
    let strength = 0.35 + 0.65 * share;
    let pulse = if focused {
        1.0 + 0.12 * (t * 5.0).sin()
    } else {
        1.0
    };
    let r = if focused || hovered { 22.0 } else { 18.0 } * pulse;
    radial(painter, at, r * 2.4, colour, 0.25 * strength);
    let outline = role_shape(part.role, at, r);
    painter.add(egui::Shape::convex_polygon(
        outline.clone(),
        fade(colour, 0.10 * strength),
        Stroke::NONE,
    ));
    glow_line(
        painter,
        outline,
        true,
        fade(colour, strength),
        if focused { 2.0 } else { 1.3 },
    );
    if focused {
        brackets(
            painter,
            Rect::from_center_size(at, vec2(r, r) * 2.9),
            7.0,
            Stroke::new(1.2, colour),
        );
    }
    // The label and the number, on the outer side: beside a glyph on the flanks, above one at the
    // top, below one at the bottom.
    let (title_at, key_at, align) = if out.x.abs() >= 0.35 {
        let x = at.x + out.x.signum() * (r + 12.0);
        let align = if out.x > 0.0 {
            Align2::LEFT_CENTER
        } else {
            Align2::RIGHT_CENTER
        };
        (egui::pos2(x, at.y - 8.0), egui::pos2(x, at.y + 8.0), align)
    } else if out.y < 0.0 {
        (
            at - vec2(0.0, r + 24.0),
            at - vec2(0.0, r + 10.0),
            Align2::CENTER_CENTER,
        )
    } else {
        (
            at + vec2(0.0, r + 10.0),
            at + vec2(0.0, r + 24.0),
            Align2::CENTER_CENTER,
        )
    };
    painter.text(
        title_at,
        align,
        part.part.to_uppercase(),
        mono(11.0),
        fade(colour, 0.6 + 0.4 * strength),
    );
    if let Some(key) = part.key.first() {
        let flicker = if key.validity == Validity::Sample {
            0.6 + 0.4 * (t * 23.0 + at.x).sin().abs()
        } else if key.validity == Validity::BelowFloor {
            0.45
        } else {
            1.0
        };
        painter.text(
            key_at,
            align,
            with_unit(key),
            mono(11.0),
            fade(TEXT, flicker),
        );
    }
    painter.text(
        at + vec2(r + 4.0, -r),
        Align2::LEFT_CENTER,
        format!("{}", part.readings),
        mono(9.0),
        DIM,
    );
}

/// The inspector for the focused part: its figures at the focused zoom (`figures`, drawn by
/// `plot`), `width` wide and as tall as they are up to `max_height`, past which they scroll. Drawn
/// where `ui` starts — a callout's area. Closing it or zooming changes `focus`.
#[allow(clippy::too_many_arguments)]
pub fn inspector(
    ui: &mut Ui,
    width: f32,
    max_height: f32,
    report: &Report,
    scene: &Scene,
    focus: &mut Option<Focus>,
    know: &Knowledge,
    curves: Option<&Curves>,
    playhead_ms: Option<f64>,
) {
    let Some(f) = focus.clone() else {
        return;
    };
    let Some(part) = scene.parts.iter().find(|p| p.part == f.part) else {
        *focus = None;
        return;
    };
    let section = &report.sections[part.section];
    let colour = role_colour(part.role);
    // An area's contents get last frame's size unless told otherwise, and a scroll area inside would
    // never grow past it: give them the whole allowance, and the panel fits its figures up to it.
    ui.set_max_size(vec2(width, max_height));
    // The panel's background waits for the content's size: placeholders now, shapes after.
    let painter = ui.painter().clone();
    let background = painter.add(egui::Shape::Noop);
    let glow = painter.add(egui::Shape::Noop);

    let family = report.family.family.name();
    let frame = egui::Frame::new().inner_margin(14.0).show(ui, |ui| {
        ui.set_width(width - 28.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(part.part.to_uppercase())
                    .font(mono(18.0))
                    .color(colour),
            );
            ui.label(
                RichText::new(format!("{} · ZOOM {}/{DEEPEST}", part.role.name(), f.level))
                    .font(mono(10.0))
                    .color(DIM),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("×")
                    .on_hover_text("Close (Escape, or scroll down past zoom 1)")
                    .clicked()
                {
                    *focus = None;
                }
                if f.level < DEEPEST && ui.small_button("+").on_hover_text("Zoom in").clicked() {
                    zoom(focus, part.part, 1);
                }
                if f.level > 1 && ui.small_button("−").on_hover_text("Zoom out").clicked() {
                    zoom(focus, part.part, -1);
                }
            });
        });
        ui.label(RichText::new(section.title).font(mono(11.0)).color(TEXT));
        ui.label(
            RichText::new(format!(
                "{} readings · {} with a value · {} tables",
                part.readings, part.present, part.tables
            ))
            .font(mono(10.0))
            .color(DIM),
        );
        ui.add_space(6.0);
        let shown = match f.level {
            1 => {
                let key: Vec<&str> = part.key.iter().map(|k| k.id).collect();
                figures::context(section, curves, &key)
            }
            2 => figures::every(section),
            _ => figures::tables(section),
        };
        let cx = plot::Ctx {
            section,
            know,
            family,
            colour,
            playhead_ms,
        };
        egui::ScrollArea::vertical()
            .max_height(max_height - 110.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for figure in &shown {
                    plot::draw(ui, figure, &cx);
                }
                if f.level == DEEPEST {
                    if shown.is_empty() {
                        ui.label(
                            RichText::new("This part holds no tables.")
                                .font(mono(10.0))
                                .color(DIM),
                        );
                    }
                    let mut sources: Vec<&str> =
                        section.readings.iter().map(|r| r.source).collect();
                    sources.sort_unstable();
                    sources.dedup();
                    ui.label(RichText::new("SOURCES").font(mono(10.0)).color(DIM));
                    for s in sources {
                        ui.label(RichText::new(format!("▸ {s}")).font(mono(9.0)).color(TEXT));
                    }
                } else {
                    ui.label(
                        RichText::new(
                            "Scroll up over the glyph for more; hover any mark for its reading.",
                        )
                        .font(mono(9.0))
                        .color(DIM),
                    );
                }
            });
    });
    let rect = frame.response.rect;
    painter.set(
        background,
        egui::Shape::rect_filled(rect, 2.0, fade(PANEL, 0.97)),
    );
    let mut light = egui::epaint::Mesh::default();
    let (c, r) = (rect.left_top(), 160.0_f32);
    light.colored_vertex(c, crate::theme::light(colour, 0.10));
    for k in 0..=32 {
        let a = std::f32::consts::FRAC_PI_2 * k as f32 / 32.0;
        light.colored_vertex(c + r * vec2(a.cos(), a.sin()), egui::Color32::TRANSPARENT);
    }
    for k in 1..=32u32 {
        light.add_triangle(0, k, k + 1);
    }
    painter.set(glow, egui::Shape::mesh(light));
    painter.rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0, fade(colour, 0.35)),
        egui::StrokeKind::Inside,
    );
    brackets(&painter, rect, 14.0, Stroke::new(1.5, colour));
}

/// The legend: each role's colour and shape, drawn in a row.
pub fn legend(painter: &egui::Painter, at: Pos2) {
    let mut x = at.x;
    for role in [
        Role::Time,
        Role::Ring,
        Role::Noise,
        Role::Character,
        Role::Chain,
    ] {
        let c = role_colour(role);
        let p = egui::pos2(x + 6.0, at.y);
        glow_line(painter, role_shape(role, p, 5.0), true, c, 1.0);
        let r = painter.text(
            p + vec2(10.0, 0.0),
            Align2::LEFT_CENTER,
            role.name(),
            mono(9.0),
            fade(c, 0.9),
        );
        x = r.right() + 14.0;
    }
}
