//! The HUD's look: its palette, the roles' colours and shapes, and the drawing tools — glow, brackets,
//! dashed arcs, gradients, the sweep. Its own design, owing nothing to mxm-kit's `crates/ui` (the
//! owner's ruling, `AGENTS.md`).
//!
//! **Glow is additive light.** egui blends premultiplied colour; a colour with its alpha at zero adds
//! its light to what is under it instead of covering it, so each line is drawn a few times, wider and
//! fainter, as light, and once solid. A true bloom would need a shader pass (the plan's H4).
//!
//! **Colour never carries meaning alone** (the owner's working preference that meters and status
//! displays be readable without red/green discrimination, once in the monorepo root's *User
//! Preferences*: the owner is red-green colour-blind): the palette holds no red/green pair, and
//! every role has its own shape and place.

use egui::epaint::{Mesh, Vertex};
use egui::{Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

use crate::scene::Role;

pub const BG: Color32 = Color32::from_rgb(4, 7, 11);
pub const PANEL: Color32 = Color32::from_rgb(7, 13, 20);
pub const GRID: Color32 = Color32::from_rgb(26, 44, 58);
pub const DIM: Color32 = Color32::from_rgb(92, 118, 136);
pub const TEXT: Color32 = Color32::from_rgb(210, 228, 238);
pub const BRIGHT: Color32 = Color32::from_rgb(240, 250, 255);

pub const CYAN: Color32 = Color32::from_rgb(61, 232, 255);
pub const AMBER: Color32 = Color32::from_rgb(255, 178, 62);
pub const MAGENTA: Color32 = Color32::from_rgb(255, 79, 209);
pub const VIOLET: Color32 = Color32::from_rgb(156, 140, 255);
pub const STEEL: Color32 = Color32::from_rgb(143, 163, 176);

#[must_use]
pub fn role_colour(role: Role) -> Color32 {
    match role {
        Role::Time => AMBER,
        Role::Ring => CYAN,
        Role::Noise => MAGENTA,
        Role::Character => VIOLET,
        Role::Chain => STEEL,
    }
}

/// The role's shape around `c`, `r` from it: triangle, circle, diamond, hexagon or square.
#[must_use]
pub fn role_shape(role: Role, c: Pos2, r: f32) -> Vec<Pos2> {
    let poly = |n: usize, turn: f32| -> Vec<Pos2> {
        (0..n)
            .map(|k| {
                let a = turn + std::f32::consts::TAU * k as f32 / n as f32;
                c + r * vec2(a.cos(), a.sin())
            })
            .collect()
    };
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
    match role {
        Role::Time => poly(3, -FRAC_PI_2),
        Role::Ring => poly(40, 0.0),
        Role::Noise => poly(4, -FRAC_PI_2),
        Role::Character => poly(6, 0.0),
        Role::Chain => poly(4, FRAC_PI_4),
    }
}

/// The HUD's type: everything monospace.
#[must_use]
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

/// A colour as light: added to what is under it, `strength` 0–1.
#[must_use]
pub fn light(c: Color32, strength: f32) -> Color32 {
    let s = strength.clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied(
        (f32::from(c.r()) * s) as u8,
        (f32::from(c.g()) * s) as u8,
        (f32::from(c.b()) * s) as u8,
        0,
    )
}

/// A colour faded towards transparent, still covering what is under it.
#[must_use]
pub fn fade(c: Color32, alpha: f32) -> Color32 {
    c.gamma_multiply(alpha.clamp(0.0, 1.0))
}

/// A glowing line through `points`: three passes of light and a solid core.
pub fn glow_line(painter: &Painter, points: Vec<Pos2>, closed: bool, c: Color32, width: f32) {
    if points.len() < 2 {
        return;
    }
    for (w, s) in [
        (width * 7.0, 0.10),
        (width * 3.5, 0.22),
        (width * 1.8, 0.45),
    ] {
        let stroke = Stroke::new(w, light(c, s));
        painter.add(if closed {
            Shape::closed_line(points.clone(), stroke)
        } else {
            Shape::line(points.clone(), stroke)
        });
    }
    let core = Stroke::new(width, c);
    painter.add(if closed {
        Shape::closed_line(points, core)
    } else {
        Shape::line(points, core)
    });
}

/// A glowing circle. Its closed path must not repeat its first point: a zero-length segment gives a
/// thick stroke's miter no direction, and it spikes.
pub fn glow_circle(painter: &Painter, c: Pos2, r: f32, colour: Color32, width: f32) {
    let mut points = arc(c, r, 0.0, std::f32::consts::TAU, 96);
    points.pop();
    glow_line(painter, points, true, colour, width);
}

/// Points along an arc from `a0` to `a1` (radians, 0 at 3 o'clock, clockwise on screen).
#[must_use]
pub fn arc(c: Pos2, r: f32, a0: f32, a1: f32, n: usize) -> Vec<Pos2> {
    (0..=n)
        .map(|k| {
            let a = a0 + (a1 - a0) * k as f32 / n as f32;
            c + r * vec2(a.cos(), a.sin())
        })
        .collect()
}

/// A ring of dashes: `dashes` segments, each `fill` of its share, turned by `phase` radians.
pub fn dashed_ring(
    painter: &Painter,
    c: Pos2,
    r: f32,
    dashes: usize,
    fill: f32,
    phase: f32,
    stroke: Stroke,
) {
    let step = std::f32::consts::TAU / dashes as f32;
    for k in 0..dashes {
        let a0 = phase + step * k as f32;
        painter.add(Shape::line(arc(c, r, a0, a0 + step * fill, 6), stroke));
    }
}

/// Tick marks around a circle, every `every` radians, `len` long inward; every `major`th longer.
pub fn ticks(
    painter: &Painter,
    c: Pos2,
    r: f32,
    count: usize,
    major: usize,
    len: f32,
    colour: Color32,
) {
    for k in 0..count {
        let a = std::f32::consts::TAU * k as f32 / count as f32;
        let l = if major > 0 && k % major == 0 {
            len * 2.2
        } else {
            len
        };
        let d = vec2(a.cos(), a.sin());
        painter.line_segment([c + d * r, c + d * (r - l)], Stroke::new(1.0, colour));
    }
}

/// Corner brackets around `rect`, each arm `len` long.
pub fn brackets(painter: &Painter, rect: Rect, len: f32, stroke: Stroke) {
    let (l, t, r, b) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    for (corner, dx, dy) in [
        (pos2(l, t), len, len),
        (pos2(r, t), -len, len),
        (pos2(l, b), len, -len),
        (pos2(r, b), -len, -len),
    ] {
        painter.add(Shape::line(
            vec![corner + vec2(0.0, dy), corner, corner + vec2(dx, 0.0)],
            stroke,
        ));
    }
}

/// A dashed straight line.
pub fn dashed(painter: &Painter, a: Pos2, b: Pos2, dash: f32, gap: f32, stroke: Stroke) {
    painter.add(Shape::dashed_line(&[a, b], stroke, dash, gap));
}

/// A disc of light fading from `colour` at the centre to nothing at `r`.
pub fn radial(painter: &Painter, c: Pos2, r: f32, colour: Color32, strength: f32) {
    let mut mesh = Mesh::default();
    mesh.vertices.push(Vertex {
        pos: c,
        uv: Default::default(),
        color: light(colour, strength),
    });
    let n = 64;
    for k in 0..=n {
        let a = std::f32::consts::TAU * k as f32 / n as f32;
        mesh.vertices.push(Vertex {
            pos: c + r * vec2(a.cos(), a.sin()),
            uv: Default::default(),
            color: Color32::TRANSPARENT,
        });
    }
    for k in 1..=n as u32 {
        mesh.add_triangle(0, k, k + 1);
    }
    painter.add(Shape::mesh(mesh));
}

/// A radar sweep: a wedge of light trailing behind the angle `at`, `trail` radians long.
pub fn sweep(
    painter: &Painter,
    c: Pos2,
    r: f32,
    at: f32,
    trail: f32,
    colour: Color32,
    strength: f32,
) {
    let mut mesh = Mesh::default();
    mesh.vertices.push(Vertex {
        pos: c,
        uv: Default::default(),
        color: light(colour, strength * 0.6),
    });
    let n = 40;
    for k in 0..=n {
        let f = k as f32 / n as f32;
        let a = at - trail * (1.0 - f);
        mesh.vertices.push(Vertex {
            pos: c + r * vec2(a.cos(), a.sin()),
            uv: Default::default(),
            color: light(colour, strength * f * f),
        });
    }
    for k in 1..=n as u32 {
        mesh.add_triangle(0, k, k + 1);
    }
    painter.add(Shape::mesh(mesh));
    let tip = c + r * vec2(at.cos(), at.sin());
    glow_line(painter, vec![c, tip], false, colour, 1.2);
}

/// Text with a glow behind it.
pub fn glow_text(
    painter: &Painter,
    at: Pos2,
    anchor: egui::Align2,
    text: &str,
    font: FontId,
    colour: Color32,
) -> Rect {
    for (d, s) in [(1.5, 0.35), (-1.5, 0.35)] {
        painter.text(
            at + vec2(d, 0.0),
            anchor,
            text,
            font.clone(),
            light(colour, s),
        );
        painter.text(
            at + vec2(0.0, d),
            anchor,
            text,
            font.clone(),
            light(colour, s),
        );
    }
    painter.text(at, anchor, text, font, colour)
}

/// A level in dB as a colour on a ramp safe for the owner's eyes: dark, cyan, white.
#[must_use]
pub fn heat(db: f64, floor_db: f64) -> Color32 {
    let t = ((db - floor_db) / -floor_db).clamp(0.0, 1.0) as f32;
    let (r, g, b) = if t < 0.6 {
        let u = t / 0.6;
        (8.0 + 20.0 * u, 18.0 + 170.0 * u, 30.0 + 200.0 * u)
    } else {
        let u = (t - 0.6) / 0.4;
        (28.0 + 212.0 * u, 188.0 + 62.0 * u, 230.0 + 25.0 * u)
    };
    Color32::from_rgb(r as u8, g as u8, b as u8)
}
