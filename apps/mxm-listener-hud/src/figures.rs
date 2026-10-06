//! What the inspector draws for a part, decided without drawing (the owner: *"when I zoom in on the
//! specific geometries I get a list of numbers … Cant they be graphs?"*).
//!
//! Every reading is placed where it belongs: on the sound's envelope or spectrum (zoom 1), over its
//! windows, across its bands, over both as a heat map, or — a reading with one value — on its unit's
//! scale (zoom 2); and the part's tables become plots (zoom 3). Each mark carries the index of the
//! reading or the table row it is, so its explanation and its number are one hover away. Nothing is
//! computed about the sound: a mark's place is its reading's value, window or band.

use mxm_listening::curves::Curves;
use mxm_listening::reading::{Section, Table, Unit};

/// A figure's horizontal axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Milliseconds from the onset, linear.
    Ms,
    /// Frequency, logarithmic.
    Hz,
    /// A table's first column in its own unit, linear.
    Linear(Unit),
}

/// A reading drawn on a context figure.
#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    /// A level over a window: a segment at `db` from `from_ms` to `to_ms`.
    Segment {
        reading: usize,
        from_ms: f64,
        to_ms: f64,
        db: f64,
    },
    /// A frequency: a line at `hz`.
    At { reading: usize, hz: f64 },
    /// A table row: a stem at `hz`, `db` high on the stems' own scale.
    Stem {
        table: usize,
        row: usize,
        hz: f64,
        db: f64,
    },
}

/// How a table is plotted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlotKind {
    /// Stems over frequency: column `hz`, height column `db`.
    Stems { hz: usize, db: usize },
    /// One lane per column `ys`, each over column `x`.
    Lanes { x: usize, ys: Vec<usize> },
}

/// One figure of the inspector.
#[derive(Clone, Debug, PartialEq)]
pub enum Figure {
    /// The part in context: the envelope (`Axis::Ms`) or the spectrum (`Axis::Hz`), in dB against its
    /// loudest, with the part's readings marked on it.
    Context {
        title: &'static str,
        axis: Axis,
        curve: Vec<(f64, Option<f64>)>,
        marks: Vec<Mark>,
    },
    /// Readings that are times, each a bar from zero on one time scale.
    Durations { readings: Vec<usize> },
    /// One reading over its windows (`Axis::Ms`, a step each) or across its bands (`Axis::Hz`, a bar
    /// each): `(reading, from, to)`, the window or the band.
    Series {
        id: &'static str,
        label: &'static str,
        unit: Unit,
        axis: Axis,
        steps: Vec<(usize, f64, f64)>,
    },
    /// One reading over windows and bands at once.
    Heat {
        id: &'static str,
        label: &'static str,
        unit: Unit,
        windows: Vec<(f64, f64)>,
        bands: Vec<(f64, f64)>,
        /// `[band][window]`: the reading there.
        cells: Vec<Vec<Option<usize>>>,
    },
    /// Readings with one value each, each on its unit's scale.
    Gauges { readings: Vec<usize> },
    /// A table as a plot.
    Plot { table: usize, kind: PlotKind },
    /// A table no plot fits: its numbers.
    Numbers { table: usize },
}

/// Zoom 1: the part in context, then its key readings (`key`, ids) on their scales.
#[must_use]
pub fn context(section: &Section, curves: Option<&Curves>, key: &[&str]) -> Vec<Figure> {
    let mut out = Vec::new();
    let readings = &section.readings;
    let has = |unit: Unit| readings.iter().any(|r| r.unit == unit && r.value.is_some());

    // The hearing model's part is its own curves over time.
    if let Some(t) = section
        .tables
        .iter()
        .position(|t| t.id == "perception.curve")
    {
        out.push(Figure::Plot {
            table: t,
            kind: plot_kind(&section.tables[t]).unwrap_or(PlotKind::Lanes {
                x: 0,
                ys: Vec::new(),
            }),
        });
    } else if has(Unit::Hertz) {
        let mut marks: Vec<Mark> = readings
            .iter()
            .enumerate()
            .filter(|(_, r)| r.unit == Unit::Hertz)
            .filter_map(|(i, r)| r.value.map(|hz| Mark::At { reading: i, hz }))
            .collect();
        for (t, table) in section.tables.iter().enumerate() {
            if let Some(PlotKind::Stems { hz, db }) = plot_kind(table) {
                for (row, cells) in table.rows.iter().enumerate() {
                    if let (Some(Some(f)), Some(Some(l))) = (cells.get(hz), cells.get(db)) {
                        marks.push(Mark::Stem {
                            table: t,
                            row,
                            hz: *f,
                            db: *l,
                        });
                    }
                }
            }
        }
        out.push(Figure::Context {
            title: "SPECTRUM",
            axis: Axis::Hz,
            curve: curves
                .and_then(|c| c.spectrum.as_ref())
                .map(|c| c.points.clone())
                .unwrap_or_default(),
            marks,
        });
    } else {
        let marks = readings
            .iter()
            .enumerate()
            .filter(|(_, r)| matches!(r.unit, Unit::Decibels | Unit::DecibelsFullScale))
            .filter_map(|(i, r)| {
                let (from_ms, to_ms) = r.window_ms?;
                r.value.map(|db| Mark::Segment {
                    reading: i,
                    from_ms,
                    to_ms,
                    db,
                })
            })
            .collect();
        out.push(Figure::Context {
            title: "ENVELOPE",
            axis: Axis::Ms,
            curve: curves
                .and_then(|c| c.envelope.as_ref())
                .map(|c| c.points.clone())
                .unwrap_or_default(),
            marks,
        });
    }

    let durations: Vec<usize> = readings
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            matches!(r.unit, Unit::Milliseconds | Unit::Seconds)
                && r.window_ms.is_none()
                && r.value.is_some_and(|v| v > 0.0)
        })
        .map(|(i, _)| i)
        .collect();
    if !durations.is_empty() {
        out.push(Figure::Durations {
            readings: durations,
        });
    }
    let gauges: Vec<usize> = key
        .iter()
        .filter_map(|id| {
            let all = || readings.iter().enumerate().filter(|(_, r)| r.id == *id);
            all()
                .find(|(_, r)| r.value.is_some())
                .or_else(|| all().next())
                .map(|(i, _)| i)
        })
        .collect();
    if !gauges.is_empty() {
        out.push(Figure::Gauges { readings: gauges });
    }
    out
}

/// Where a reading lies along a series: its window or its band.
type Span = fn(&mxm_listening::Reading) -> Option<(f64, f64)>;

/// Zoom 2: every reading, grouped by id in the order the part reports them — over its windows,
/// across its bands, over both, or on its unit's scale.
#[must_use]
pub fn every(section: &Section) -> Vec<Figure> {
    let readings = &section.readings;
    let mut ids: Vec<&'static str> = Vec::new();
    for r in readings {
        if !ids.contains(&r.id) {
            ids.push(r.id);
        }
    }
    let mut out = Vec::new();
    let mut gauges: Vec<usize> = Vec::new();
    let flush = |gauges: &mut Vec<usize>, out: &mut Vec<Figure>| {
        if !gauges.is_empty() {
            out.push(Figure::Gauges {
                readings: std::mem::take(gauges),
            });
        }
    };
    for id in ids {
        let group: Vec<usize> = (0..readings.len())
            .filter(|&i| readings[i].id == id)
            .collect();
        let first = &readings[group[0]];
        let mut windows: Vec<(f64, f64)> = Vec::new();
        let mut bands: Vec<(f64, f64)> = Vec::new();
        for &i in &group {
            if let Some(w) = readings[i].window_ms
                && !windows.contains(&w)
            {
                windows.push(w);
            }
            if let Some(b) = readings[i].band_hz
                && !bands.contains(&b)
            {
                bands.push(b);
            }
        }
        if windows.len() > 1 && bands.len() > 1 {
            flush(&mut gauges, &mut out);
            let cells = bands
                .iter()
                .map(|b| {
                    windows
                        .iter()
                        .map(|w| {
                            group.iter().copied().find(|&i| {
                                readings[i].window_ms == Some(*w) && readings[i].band_hz == Some(*b)
                            })
                        })
                        .collect()
                })
                .collect();
            out.push(Figure::Heat {
                id,
                label: first.label,
                unit: first.unit,
                windows,
                bands,
                cells,
            });
        } else if windows.len() > 1 || bands.len() > 1 {
            flush(&mut gauges, &mut out);
            let (axis, span): (Axis, Span) = if windows.len() > 1 {
                (Axis::Ms, |r| r.window_ms)
            } else {
                (Axis::Hz, |r| r.band_hz)
            };
            let steps = group
                .iter()
                .filter_map(|&i| span(&readings[i]).map(|(a, b)| (i, a, b)))
                .collect();
            out.push(Figure::Series {
                id,
                label: first.label,
                unit: first.unit,
                axis,
                steps,
            });
        } else {
            gauges.extend(group);
        }
    }
    flush(&mut gauges, &mut out);
    out
}

/// Zoom 3: the part's tables as plots, or their numbers where no plot fits.
#[must_use]
pub fn tables(section: &Section) -> Vec<Figure> {
    section
        .tables
        .iter()
        .enumerate()
        .map(|(t, table)| match plot_kind(table) {
            Some(kind) => Figure::Plot { table: t, kind },
            None => Figure::Numbers { table: t },
        })
        .collect()
}

/// How a table plots: stems over its frequency column and a level beside it, or lanes over a first
/// column of time or level; `None` when neither fits.
#[must_use]
pub fn plot_kind(table: &Table) -> Option<PlotKind> {
    let unit = |c: usize| table.columns[c].1;
    let hz = (0..table.columns.len()).find(|&c| unit(c) == Unit::Hertz);
    let db = (0..table.columns.len()).find(|&c| unit(c) == Unit::Decibels);
    if let (Some(hz), Some(db)) = (hz, db)
        && hz < db
        && table.columns[0].1 != Unit::Milliseconds
    {
        return Some(PlotKind::Stems { hz, db });
    }
    let first = table.columns.first()?.1;
    if matches!(
        first,
        Unit::Milliseconds | Unit::Seconds | Unit::DecibelsFullScale | Unit::Hertz
    ) && table.columns.len() > 1
    {
        return Some(PlotKind::Lanes {
            x: 0,
            ys: (1..table.columns.len()).collect(),
        });
    }
    None
}

/// The scale a single value is drawn on, by its unit: `(low, high, logarithmic)`, or `None` for a
/// unit with no natural scale (a plain number is shown as a number).
#[must_use]
pub fn scale(unit: Unit, value: f64) -> Option<(f64, f64, bool)> {
    Some(match unit {
        Unit::DecibelsFullScale => (-96.0, 0.0, false),
        Unit::Decibels => (-80.0, 40.0, false),
        Unit::DecibelsPerSecond => (-300.0, 50.0, false),
        Unit::DecibelsPerOctave => (-24.0, 12.0, false),
        Unit::Milliseconds => (0.1, 10_000.0, true),
        Unit::Seconds => (0.01, 60.0, true),
        Unit::Hertz => (20.0, 20_000.0, true),
        Unit::Cents => (-100.0, 100.0, false),
        Unit::Percent => (0.0, 100.0, false),
        Unit::Sign => (-1.0, 1.0, false),
        Unit::Ratio => (0.01, 100.0, true),
        Unit::Tonality => (0.0, 1.0, false),
        Unit::Asper => (0.0, 1.0, false),
        Unit::Acum => (0.0, 4.0, false),
        Unit::Vacil => (0.0, 2.0, false),
        Unit::Sone => (0.0, (value * 1.5).max(4.0), false),
        Unit::Plain => return None,
    })
}
