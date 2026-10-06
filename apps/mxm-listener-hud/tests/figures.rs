//! The inspector's figures, decided without drawing, on a synthetic hit: every reading of a part
//! appears once at zoom 2 — graphs lose nothing a list held — each mark sits at its reading's own value,
//! window or band, and the tables become the plots their columns call for. And the waveform's columns
//! are the samples' own lowest and highest.

use mxm_listener_hud::figures::{
    Axis, Figure, Mark, PlotKind, context, every, plot_kind, scale, tables,
};
use mxm_listener_hud::timeline::wave_columns;
use mxm_listening::Sound;
use mxm_listening::curves::curves;
use mxm_listening::describe::{Options, describe_with};
use mxm_listening::reading::{Report, Section, Table, Unit};

fn hit() -> (Sound, Report) {
    let rate = 48_000.0;
    let mut x = vec![0.0f32; 480];
    x.extend((0..48_000).map(|i| {
        let t = f64::from(i) / rate;
        (0.8 * (-t / 0.2).exp() * (std::f64::consts::TAU * 180.0 * t).sin()
            + 0.2 * (-t / 0.1).exp() * (std::f64::consts::TAU * 520.0 * t).sin()) as f32
    }));
    let sound = Sound::new("hit", rate as u32, x);
    let report = describe_with(&sound, &Options::default());
    (sound, report)
}

fn section<'a>(report: &'a Report, part: &str) -> &'a Section {
    report
        .sections
        .iter()
        .find(|s| s.part == part)
        .unwrap_or_else(|| panic!("no {part}"))
}

/// The readings a figure shows, by index.
fn shown(figure: &Figure) -> Vec<usize> {
    match figure {
        Figure::Series { steps, .. } => steps.iter().map(|s| s.0).collect(),
        Figure::Heat { cells, .. } => cells.iter().flatten().flatten().copied().collect(),
        Figure::Gauges { readings } | Figure::Durations { readings } => readings.clone(),
        _ => Vec::new(),
    }
}

#[test]
fn zoom_two_shows_every_reading_once() {
    let (_, report) = hit();
    for s in &report.sections {
        let mut seen: Vec<usize> = every(s).iter().flat_map(shown).collect();
        seen.sort_unstable();
        let all: Vec<usize> = (0..s.readings.len()).collect();
        assert_eq!(seen, all, "{}: a reading missing or shown twice", s.part);
    }
    // The shapes: a reading over windows and bands is a heat map; over windows alone, a series.
    let tone = every(section(&report, "tone"));
    assert!(tone.iter().any(|f| matches!(
        f,
        Figure::Heat {
            id: "tone.band_level",
            ..
        }
    )));
    assert!(tone.iter().any(|f| matches!(
        f,
        Figure::Series {
            id: "tone.centroid",
            axis: Axis::Ms,
            ..
        }
    )));
}

#[test]
fn zoom_one_puts_the_readings_on_the_envelope_or_the_spectrum() {
    let (sound, report) = hit();
    let drawn = curves(&sound, report.onset_s);

    // The decay: the envelope, each windowed level at its own window and value.
    let decay = section(&report, "decay");
    let figures = context(decay, Some(&drawn), &["decay.t40"]);
    let Figure::Context {
        axis: Axis::Ms,
        curve,
        marks,
        ..
    } = &figures[0]
    else {
        panic!("{:?}", figures[0])
    };
    assert_eq!(curve, &drawn.envelope.as_ref().unwrap().points);
    for m in marks {
        let Mark::Segment {
            reading,
            from_ms,
            to_ms,
            db,
        } = m
        else {
            panic!("{m:?}")
        };
        let r = &decay.readings[*reading];
        assert_eq!(r.window_ms, Some((*from_ms, *to_ms)));
        assert_eq!(r.value, Some(*db));
    }
    assert!(
        figures
            .iter()
            .any(|f| matches!(f, Figure::Durations { .. }))
    );
    assert!(
        figures
            .iter()
            .any(|f| matches!(f, Figure::Gauges { readings } if readings.len() == 1))
    );

    // The pitch: the spectrum, each frequency where it reads, the modes as stems.
    let pitch = section(&report, "pitch");
    let figures = context(pitch, Some(&drawn), &[]);
    let Figure::Context {
        axis: Axis::Hz,
        marks,
        ..
    } = &figures[0]
    else {
        panic!("{:?}", figures[0])
    };
    let rest = pitch
        .readings
        .iter()
        .position(|r| r.id == "pitch.rest" && r.value.is_some())
        .unwrap();
    assert!(marks.iter().any(
        |m| matches!(m, Mark::At { reading, hz } if *reading == rest && Some(*hz) == pitch.readings[rest].value)
    ));
    assert!(marks.iter().any(|m| matches!(m, Mark::Stem { .. })));
}

#[test]
fn tables_become_the_plots_their_columns_call_for() {
    let (_, report) = hit();
    let pitch = tables(section(&report, "pitch"));
    assert!(!pitch.is_empty());
    assert!(pitch.iter().all(|f| matches!(
        f,
        Figure::Plot {
            kind: PlotKind::Stems { hz: 0, .. },
            ..
        }
    )));
    let perception = tables(section(&report, "perception"));
    assert!(matches!(
        &perception[0],
        Figure::Plot { kind: PlotKind::Lanes { x: 0, ys }, .. } if ys.len() >= 5
    ));
    // A table whose first column is a plain number fits no plot: its numbers.
    let settings = Table {
        id: "respond.settings",
        title: "Readings against a setting".into(),
        columns: vec![("The setting", Unit::Plain), ("Gain", Unit::Decibels)],
        rows: vec![vec![Some(1.0), Some(-3.0)]],
        window_ms: None,
        source: "test",
    };
    assert_eq!(plot_kind(&settings), None);
    // A value with no natural scale is shown as a number.
    assert_eq!(scale(Unit::Plain, 3.0), None);
    assert_eq!(
        scale(Unit::DecibelsFullScale, -6.0),
        Some((-96.0, 0.0, false))
    );
}

#[test]
fn the_waveform_columns_are_the_samples_lowest_and_highest() {
    // A ramp from −1 to 1 after 100 samples of silence, drawn from the onset at 100 in 4 columns.
    let mut x = vec![0.0f32; 100];
    x.extend((0..400).map(|i| -1.0 + i as f32 / 200.0));
    let columns = wave_columns(&x, 1000, 0.1, 400.0, 4);
    assert_eq!(columns.len(), 4);
    for (c, (lo, hi)) in columns.iter().enumerate() {
        assert_eq!(*lo, x[100 + c * 100]);
        assert_eq!(*hi, x[100 + c * 100 + 99]);
    }
    assert!(wave_columns(&x, 1000, 0.1, 0.0, 4).is_empty());
}

/// The callout sits beside its glyph on the side towards the scope's centre, inside the bounds, and
/// the leader meets the panel's near edge — whichever side of the orbit the glyph is on.
#[test]
fn a_callout_sits_beside_its_glyph_towards_the_centre() {
    use egui::{Rect, pos2, vec2};
    use mxm_listener_hud::glyphs::callout_place;

    let bounds = Rect::from_min_max(pos2(0.0, 0.0), pos2(1200.0, 600.0));
    let centre = pos2(600.0, 300.0);
    let size = vec2(400.0, 300.0);
    // A glyph on the left: the panel to its right; on the right: to its left.
    let (panel, edge) = callout_place(pos2(150.0, 300.0), centre, size, bounds);
    assert!(panel.left() > 150.0 && bounds.contains_rect(panel));
    assert_eq!(edge.x, panel.left());
    let (panel, edge) = callout_place(pos2(1050.0, 300.0), centre, size, bounds);
    assert!(panel.right() < 1050.0 && bounds.contains_rect(panel));
    assert_eq!(edge.x, panel.right());
    // Near the top: kept inside, the leader meeting the panel level with the glyph where it can.
    let (panel, edge) = callout_place(pos2(600.0, 40.0), centre, size, bounds);
    assert!(bounds.contains_rect(panel));
    assert!(edge.y >= panel.top() && edge.y <= panel.bottom());
}
