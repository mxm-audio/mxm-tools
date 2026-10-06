//! What the listener reports: readings, grouped into sections by the part of the sound they describe,
//! gathered into a report.
//!
//! A reading always says where it was taken (window and band), how finely (resolution), what it is
//! worth (validity) and where its definition comes from. The rules are the plan's §2 and
//! `mxm-measure`'s result forms: **never NaN; absent rather than zero**.

/// A reading's unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Milliseconds,
    Seconds,
    /// A level relative to a stated reference.
    Decibels,
    /// A level relative to digital full scale.
    DecibelsFullScale,
    DecibelsPerSecond,
    /// A slope across frequency: dB for each doubling.
    DecibelsPerOctave,
    Hertz,
    Cents,
    Ratio,
    Percent,
    /// +1 or −1.
    Sign,
    /// A dimensionless quantity such as a Q.
    Plain,
    /// Loudness after the Sottek hearing model, sone_HMS (ECMA-418-2).
    Sone,
    /// Sharpness, acum.
    Acum,
    /// Tonality after the Sottek hearing model, tu_HMS.
    Tonality,
    /// Roughness, asper.
    Asper,
    /// Fluctuation strength after the Sottek hearing model, vacil_HMS (ECMA-418-2 Clause 9).
    Vacil,
}

impl Unit {
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            Unit::Milliseconds => "ms",
            Unit::Seconds => "s",
            Unit::Decibels => "dB",
            Unit::DecibelsFullScale => "dBFS",
            Unit::DecibelsPerSecond => "dB/s",
            Unit::DecibelsPerOctave => "dB/oct",
            Unit::Hertz => "Hz",
            Unit::Cents => "c",
            Unit::Ratio => "×",
            Unit::Percent => "%",
            Unit::Sign => "±",
            Unit::Plain => "",
            Unit::Sone => "sone",
            Unit::Acum => "acum",
            Unit::Tonality => "tu",
            Unit::Asper => "asper",
            Unit::Vacil => "vacil",
        }
    }
}

/// What a reading is worth.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Validity {
    Valid,
    /// Taken in a window more than [`FLOOR_DB`] below the sound's own peak: it may be measuring the
    /// noise floor rather than the sound (the guide's trap: a tambourine window put model and
    /// recording 2.4 kHz apart and neither was audible).
    BelowFloor,
    /// A single render's draw of a random quantity (a modulation or texture measure), not a trend.
    Sample,
    /// At the analysis's resolution limit: the value is the window's, and the sound's may be smaller.
    Unresolved,
    /// Could not be measured, and why. Never reported as zero.
    Absent(&'static str),
}

/// A window this far below the sound's own peak is marked [`Validity::BelowFloor`].
pub const FLOOR_DB: f64 = -45.0;

/// A band this far below the sound's peak holds nothing but rounding: its readings are absent.
pub const NUMERICAL_FLOOR_DB: f64 = -100.0;

/// How finely a reading resolves time and frequency. Every short-time spectrum trades one for the
/// other (the Gabor limit), and a Q means nothing without its smoothing span.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resolution {
    /// The analysis window, ms.
    pub window_ms: f64,
    /// The spectral bin spacing after zero-padding, Hz.
    pub bin_hz: Option<f64>,
    /// A smoothing span as a fraction of frequency (±), where one was applied.
    pub span: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    /// A stable identifier, `part.measure`, the same for every window and band of one measure.
    pub id: &'static str,
    pub label: &'static str,
    pub value: Option<f64>,
    pub unit: Unit,
    /// Milliseconds from the onset.
    pub window_ms: Option<(f64, f64)>,
    pub band_hz: Option<(f64, f64)>,
    pub resolution: Option<Resolution>,
    pub validity: Validity,
    /// The window's level against the sound's peak, dB, where the reading knows it: what a comparison
    /// weighs the reading's difference by.
    pub level_db: Option<f64>,
    /// Where the definition comes from.
    pub source: &'static str,
}

impl Reading {
    /// A reading of `value`. A NaN becomes absent; an infinity (a silent window in decibels) becomes
    /// absent as "silent", because a report is read by people and a JSON number cannot hold it.
    #[must_use]
    pub fn new(
        id: &'static str,
        label: &'static str,
        value: Option<f64>,
        unit: Unit,
        source: &'static str,
    ) -> Self {
        let (value, validity) = match value {
            None => (None, Validity::Absent("not measurable here")),
            Some(v) if v.is_nan() => (None, Validity::Absent("not a number")),
            Some(v) if v.is_infinite() => (None, Validity::Absent("silent")),
            Some(v) => (Some(v), Validity::Valid),
        };
        Self {
            id,
            label,
            value,
            unit,
            window_ms: None,
            band_hz: None,
            resolution: None,
            validity,
            level_db: None,
            source,
        }
    }

    /// Absent, with the reason.
    #[must_use]
    pub fn absent(
        id: &'static str,
        label: &'static str,
        unit: Unit,
        reason: &'static str,
        source: &'static str,
    ) -> Self {
        let mut r = Self::new(id, label, None, unit, source);
        r.validity = Validity::Absent(reason);
        r
    }

    #[must_use]
    pub fn window(mut self, from_ms: f64, to_ms: f64) -> Self {
        self.window_ms = Some((from_ms, to_ms));
        self
    }

    #[must_use]
    pub fn band(mut self, lo_hz: f64, hi_hz: f64) -> Self {
        self.band_hz = Some((lo_hz, hi_hz));
        self
    }

    #[must_use]
    pub fn resolution(mut self, resolution: Resolution) -> Self {
        self.resolution = Some(resolution);
        self
    }

    /// Marks a valid reading below the floor when `level_db` (the window's level against the sound's
    /// peak) is under [`FLOOR_DB`]. An absent reading stays absent.
    #[must_use]
    pub fn floor(mut self, level_db: Option<f64>) -> Self {
        self.level_db = level_db.filter(|l| l.is_finite());
        if self.validity == Validity::Valid && level_db.is_some_and(|l| l.is_nan() || l < FLOOR_DB)
        {
            self.validity = Validity::BelowFloor;
        }
        self
    }

    /// Marks a valid reading as a single render's sample.
    #[must_use]
    pub fn sample(mut self) -> Self {
        if self.validity == Validity::Valid {
            self.validity = Validity::Sample;
        }
        self
    }
}

/// The readings for one part of the sound, and any tables (a list of modes, say).
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    /// `attack`, `level`, `decay`, `tone`, …
    pub part: &'static str,
    pub title: &'static str,
    pub readings: Vec<Reading>,
    pub tables: Vec<Table>,
}

impl Section {
    #[must_use]
    pub fn new(part: &'static str, title: &'static str, readings: Vec<Reading>) -> Self {
        Self {
            part,
            title,
            readings,
            tables: Vec::new(),
        }
    }
}

/// A table of numbers that belong together row by row: each row one mode, one partial, one line.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub id: &'static str,
    pub title: String,
    /// Column labels and units.
    pub columns: Vec<(&'static str, Unit)>,
    pub rows: Vec<Vec<Option<f64>>>,
    /// Milliseconds from the onset, for the whole table.
    pub window_ms: Option<(f64, f64)>,
    pub source: &'static str,
}

/// Everything the listener heard in one sound.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    /// A logical name, never a path: a report may be kept, and a path into a private folder must not
    /// travel with it.
    pub name: String,
    pub rate: u32,
    pub duration_s: f64,
    pub family: crate::family::Detected,
    /// Seconds from the start of the file to the onset every window is measured from.
    pub onset_s: Option<f64>,
    pub sections: Vec<Section>,
    /// Things worth knowing that are not measurements (a recording-chain wart, a family guess).
    pub notes: Vec<String>,
}

impl Report {
    /// The first reading with this id (and, where given, this window start).
    #[must_use]
    pub fn find(&self, id: &str, window_from_ms: Option<f64>) -> Option<&Reading> {
        self.sections.iter().flat_map(|s| &s.readings).find(|r| {
            r.id == id
                && window_from_ms
                    .is_none_or(|w| r.window_ms.is_some_and(|(a, _)| (a - w).abs() < 1e-9))
        })
    }
}
