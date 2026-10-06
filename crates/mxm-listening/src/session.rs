//! A calibration session: interleaved staircases over one sound's perturbations, odd-one-out of
//! three, with an obvious check trial now and then; and, at its end, the owner's threshold for each
//! perturbed reading in that reading's own terms (the plan's §5). Independent of how the trials are
//! played: `listen session` serves them to a local page, a test answers them with a simulated listener.
//!
//! The owner's thresholds stay local (the plan's revision 8): sessions and the table they make are
//! written under an ignored folder, never committed.

use crate::audibility::{Kind, Thresholds};
use crate::describe::{Options, describe_with};
use crate::perturb::{Operator, Subject};
use crate::reading::Report;
use crate::sound::Sound;
use crate::staircase::{Rng, Staircase};

/// Every this many trials, one is an obvious check: a staircase's largest change. A listener who
/// misses checks was not listening, and the session says so.
pub const CHECK_EVERY: usize = 8;

/// One trial: three sounds, one of them different.
#[derive(Clone, Debug)]
pub struct Trial {
    pub id: u64,
    /// The staircase it serves; `None` for a check trial.
    pub stair: Option<usize>,
    pub operator: Operator,
    pub size: f64,
    /// Which of the three (0, 1, 2) is the changed one.
    pub odd: usize,
    pub sounds: [Sound; 3],
}

/// One answered trial.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub trial: u64,
    pub operator: String,
    pub check: bool,
    pub size: f64,
    pub odd: usize,
    pub choice: usize,
    pub correct: bool,
}

/// One staircase's result: the owner's threshold for the reading its operator moves.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub operator: Operator,
    pub reading: &'static str,
    pub band_lo: Option<f64>,
    /// The operator's size answered 70.7 % correct.
    pub size: f64,
    pub reversals: usize,
    pub trials: usize,
    /// The reading on the sound and on the sound changed by `size`.
    pub reference: f64,
    pub candidate: f64,
    /// The threshold in the reading's terms (the literature's kind for that reading), and the
    /// literature's value beside it.
    pub kind: Kind,
    pub value: f64,
    pub literature: f64,
}

pub struct Session {
    pub subject: Subject,
    pub stairs: Vec<(Operator, Staircase)>,
    pub device: String,
    pub log: Vec<Record>,
    rng: Rng,
    next_id: u64,
    pending: Option<(u64, Option<usize>, Operator, f64, usize)>,
    report: Option<Report>,
}

impl Session {
    /// A session over `subject` with one staircase per operator that the sound supports.
    #[must_use]
    pub fn new(subject: Subject, operators: &[Operator], device: &str, seed: u64) -> Self {
        let stairs = operators
            .iter()
            .filter(|op| {
                let (start, _, _) = op.range();
                subject.perturbed(**op, start).is_some()
            })
            .map(|op| {
                let (start, min, max) = op.range();
                (*op, Staircase::new(start, min, max))
            })
            .collect();
        Self {
            subject,
            stairs,
            device: device.to_string(),
            log: Vec::new(),
            rng: Rng::new(seed),
            next_id: 1,
            pending: None,
            report: None,
        }
    }

    /// Whether every staircase has finished.
    #[must_use]
    pub fn done(&self) -> bool {
        self.stairs.iter().all(|(_, s)| s.done())
    }

    /// Trials answered so far, and a rough count of those still to come.
    #[must_use]
    pub fn progress(&self) -> (usize, usize) {
        let left: usize = self
            .stairs
            .iter()
            .map(|(_, s)| {
                if s.done() {
                    0
                } else {
                    (crate::staircase::REVERSALS - s.reversals.len()) * 3
                }
            })
            .sum();
        (self.log.len(), left)
    }

    /// The next trial, or `None` when the session is done. The trial stays pending until answered.
    pub fn next_trial(&mut self) -> Option<Trial> {
        if self.done() {
            return None;
        }
        // The open staircases with the fewest trials take turns, in random order among themselves:
        // a random pick gave one staircase of three only three trials in a six-minute session.
        let open: Vec<usize> = (0..self.stairs.len())
            .filter(|&i| !self.stairs[i].1.done())
            .collect();
        let fewest = open
            .iter()
            .map(|&i| self.stairs[i].1.trials)
            .min()
            .unwrap_or(0);
        let turn: Vec<usize> = open
            .into_iter()
            .filter(|&i| self.stairs[i].1.trials == fewest)
            .collect();
        let pick = turn[self.rng.below(turn.len())];
        let check = self.log.len() % CHECK_EVERY == CHECK_EVERY - 1;
        let (op, size, stair) = if check {
            let (op, _) = self.stairs[pick];
            (op, op.range().2, None)
        } else {
            let (op, s) = &self.stairs[pick];
            (*op, s.size, Some(pick))
        };
        let odd = self.rng.below(3);
        let candidate = self.subject.perturbed(op, size)?;
        let reference = self.subject.sound.clone();
        let sounds: [Sound; 3] = std::array::from_fn(|i| {
            if i == odd {
                candidate.clone()
            } else {
                reference.clone()
            }
        });
        let id = self.next_id;
        self.next_id += 1;
        self.pending = Some((id, stair, op, size, odd));
        Some(Trial {
            id,
            stair,
            operator: op,
            size,
            odd,
            sounds,
        })
    }

    /// Records the answer to the pending trial; `None` when `trial` is not the pending one.
    pub fn answer(&mut self, trial: u64, choice: usize) -> Option<bool> {
        let (id, stair, op, size, odd) = self.pending?;
        if id != trial {
            return None;
        }
        self.pending = None;
        let correct = choice == odd;
        if let Some(i) = stair {
            self.stairs[i].1.record(correct);
        }
        self.log.push(Record {
            trial: id,
            operator: op.name(),
            check: stair.is_none(),
            size,
            odd,
            choice,
            correct,
        });
        Some(correct)
    }

    /// Check trials answered, and how many correctly.
    #[must_use]
    pub fn checks(&self) -> (usize, usize) {
        let checks: Vec<&Record> = self.log.iter().filter(|r| r.check).collect();
        (checks.len(), checks.iter().filter(|r| r.correct).count())
    }

    /// Each finished staircase's threshold, measured in its reading's terms on the sound itself.
    pub fn outcomes(&mut self, literature: &Thresholds) -> Vec<Outcome> {
        let options = Options {
            family: self.subject.family,
            without_perception: true,
            ..Options::default()
        };
        if self.report.is_none() {
            self.report = Some(describe_with(&self.subject.sound, &options));
        }
        let reference = self.report.clone().expect("described above");
        let mut out = Vec::new();
        for (op, stair) in &self.stairs {
            let Some(size) = stair.estimate() else {
                continue;
            };
            let Some(m) = measure(&self.subject, &reference, *op, size, literature) else {
                continue;
            };
            out.push(Outcome {
                operator: *op,
                reading: m.reading,
                band_lo: m.band_lo,
                size,
                reversals: stair.reversals.len(),
                trials: stair.trials,
                reference: m.reference,
                candidate: m.candidate,
                kind: m.kind,
                value: m.delta,
                literature: m.literature,
            });
        }
        out
    }
}

/// What a change of one operator at one size does to its reading.
#[derive(Clone, Debug, PartialEq)]
pub struct Measured {
    pub reading: &'static str,
    pub band_lo: Option<f64>,
    pub kind: Kind,
    pub reference: f64,
    pub candidate: f64,
    /// The change in the reading's own terms: in its unit for an absolute threshold, as a fraction
    /// for a relative one.
    pub delta: f64,
    pub literature: f64,
}

/// The subject changed by `op` at `size`, measured on the reading the operator names, against the
/// subject's own report (described without the perceptual models).
#[must_use]
pub fn measure(
    subject: &Subject,
    reference: &Report,
    op: Operator,
    size: f64,
    literature: &Thresholds,
) -> Option<Measured> {
    let options = Options {
        family: subject.family,
        without_perception: true,
        ..Options::default()
    };
    let candidate = subject.perturbed(op, size)?;
    let changed = describe_with(&candidate, &options);
    let (id, band_lo) = op.reading();
    let (r, unit) = value(reference, id, band_lo)?;
    let (c, _) = value(&changed, id, band_lo)?;
    let lit = literature.for_reading(id, unit)?;
    let delta = match lit.kind {
        Kind::Absolute => (c - r).abs(),
        Kind::Relative => {
            if r > 0.0 && c > 0.0 {
                (c / r).ln().abs().exp_m1()
            } else {
                (c - r).abs() / r.abs().max(1e-12)
            }
        }
    };
    Some(Measured {
        reading: id,
        band_lo,
        kind: lit.kind,
        reference: r,
        candidate: c,
        delta,
        literature: lit.value,
    })
}

/// A reading's value and unit in a report: the first with this id (and band, when given) that has a
/// value, in time order.
fn value(report: &Report, id: &str, band_lo: Option<f64>) -> Option<(f64, crate::reading::Unit)> {
    report
        .sections
        .iter()
        .flat_map(|s| &s.readings)
        .filter(|r| r.id == id)
        .filter(|r| {
            band_lo.is_none_or(|lo| r.band_hz.is_some_and(|b| (b.0 - lo).abs() <= 0.01 * lo))
        })
        .find_map(|r| r.value.map(|v| (v, r.unit)))
}

/// The session log as TSV: a `#` header, then one row per answered trial. `file` is the sound's
/// file, which `calibrate` reads again to measure every answer: the log is local, never committed.
#[must_use]
pub fn log_tsv(s: &Session, date: &str, file: Option<&str>) -> String {
    let mut out = format!(
        "# listen session\n# date\t{date}\n# sound\t{}\n# family\t{}\n# device\t{}\n",
        s.subject.sound.name,
        s.subject.family.map_or("unknown", |f| f.name()),
        s.device
    );
    if let Some(f) = file {
        out.push_str(&format!("# file\t{f}\n"));
    }
    out.push_str("trial\toperator\tcheck\tsize\todd\tchoice\tcorrect\n");
    for r in &s.log {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            r.trial, r.operator, r.check, r.size, r.odd, r.choice, r.correct
        ));
    }
    out
}

/// The session's outcomes as TSV, which `calibrate` reads.
#[must_use]
pub fn outcomes_tsv(s: &Session, outcomes: &[Outcome], date: &str) -> String {
    let (checks, right) = s.checks();
    let mut out = format!(
        "# listen session outcomes\n# date\t{date}\n# sound\t{}\n# family\t{}\n# device\t{}\n# checks\t{right} of {checks}\nreading\tband_lo\tkind\tvalue\tliterature\toperator\tsize\treversals\ttrials\treference\tcandidate\n",
        s.subject.sound.name,
        s.subject.family.map_or("unknown", |f| f.name()),
        s.device
    );
    for o in outcomes {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            o.reading,
            o.band_lo.map_or(String::new(), |b| b.to_string()),
            match o.kind {
                Kind::Absolute => "abs",
                Kind::Relative => "rel",
            },
            o.value,
            o.literature,
            o.operator.name(),
            o.size,
            o.reversals,
            o.trials,
            o.reference,
            o.candidate
        ));
    }
    out
}

/// A session log read back: its header fields and its answered staircase trials (check trials left
/// out: they measure attention, not the ear).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Log {
    pub date: String,
    pub sound: String,
    pub family: String,
    pub file: Option<String>,
    /// (operator, size, correct).
    pub trials: Vec<(Operator, f64, bool)>,
}

/// Parses a session log written by [`log_tsv`].
#[must_use]
pub fn parse_log(text: &str) -> Log {
    let mut log = Log::default();
    for line in text.lines() {
        if let Some(h) = line.strip_prefix("# ") {
            if let Some((k, v)) = h.split_once('\t') {
                match k {
                    "date" => log.date = v.to_string(),
                    "sound" => log.sound = v.to_string(),
                    "family" => log.family = v.to_string(),
                    "file" => log.file = Some(v.to_string()),
                    _ => {}
                }
            }
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        let [_, op, check, size, _, _, correct] = f[..] else {
            continue;
        };
        let (Some(op), Ok(size)) = (Operator::parse(op), size.parse::<f64>()) else {
            continue;
        };
        if check != "true" {
            log.trials.push((op, size, correct == "true"));
        }
    }
    log
}

/// One answered trial, its change measured in its reading's own terms.
#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    pub reading: String,
    pub kind: Kind,
    /// The sound's family: the context the owner's threshold applies in.
    pub context: String,
    pub delta: f64,
    pub correct: bool,
    /// The session's date.
    pub session: String,
}

/// A threshold fitted to answers: the change answered correctly 70.7 % of the time, and the range
/// the data allow (the 95 % likelihood interval; `None` where it runs off the sizes tried).
#[derive(Clone, Debug, PartialEq)]
pub struct Fit {
    pub threshold: f64,
    pub low: Option<f64>,
    pub high: Option<f64>,
    pub trials: usize,
    pub correct: usize,
}

/// The slope and lapse rate the fit assumes: the simulated listener's, which the staircase is proved
/// against (`staircase::Simulated`).
pub const FIT_SLOPE: f64 = 3.5;
pub const FIT_LAPSE: f64 = 0.02;
/// Answers a fit needs before its threshold is used (**chosen**).
pub const FIT_MIN_TRIALS: usize = 8;

/// The maximum-likelihood threshold of odd-one-out answers `(change, correct)` under a Weibull
/// psychometric function (chance one in three), with its 95 % likelihood interval. Every answer
/// counts, however the staircases ended; `None` without positive changes.
#[must_use]
pub fn fit(answers: &[(f64, bool)]) -> Option<Fit> {
    let sizes: Vec<f64> = answers
        .iter()
        .map(|a| a.0)
        .filter(|d| *d > 0.0 && d.is_finite())
        .collect();
    if sizes.is_empty() {
        return None;
    }
    let lo = sizes.iter().copied().fold(f64::INFINITY, f64::min) / 10.0;
    let hi = sizes.iter().copied().fold(0.0f64, f64::max) * 10.0;
    const STEPS: usize = 800;
    let grid: Vec<f64> = (0..=STEPS)
        .map(|k| (lo.ln() + (hi / lo).ln() * k as f64 / STEPS as f64).exp())
        .collect();
    let likelihood = |t: f64| -> f64 {
        let listener = crate::staircase::Simulated::new(t, FIT_SLOPE, FIT_LAPSE, 1);
        answers
            .iter()
            .filter(|a| a.0 > 0.0)
            .map(|&(d, ok)| {
                let p = listener.p_correct(d).clamp(1e-9, 1.0 - 1e-9);
                if ok { p.ln() } else { (1.0 - p).ln() }
            })
            .sum()
    };
    let ll: Vec<f64> = grid.iter().map(|&t| likelihood(t)).collect();
    let (best, top) = ll
        .iter()
        .enumerate()
        .fold((0, f64::NEG_INFINITY), |(bi, bv), (i, &v)| {
            if v > bv { (i, v) } else { (bi, bv) }
        });
    let inside: Vec<usize> = (0..=STEPS).filter(|&i| ll[i] >= top - 1.92).collect();
    let (first, last) = (inside[0], inside[inside.len() - 1]);
    Some(Fit {
        threshold: grid[best],
        low: (first > 0).then(|| grid[first]),
        high: (last < STEPS).then(|| grid[last]),
        trials: answers.len(),
        correct: answers.iter().filter(|a| a.1).count(),
    })
}

/// The owner's thresholds from every answer so far: a fit per reading and context, as a thresholds
/// table (`id`, kind, value, source, context) for [`Thresholds::overlaid`], and a line per reading
/// saying what the answers show. A reading enters the table once it has [`FIT_MIN_TRIALS`] answers
/// and a range bounded on both sides.
#[must_use]
pub fn table(answers: &[Answer]) -> (String, Vec<String>) {
    let mut keys: Vec<(String, Kind, String)> = Vec::new();
    for a in answers {
        let key = (a.reading.clone(), a.kind, a.context.clone());
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    let mut out = String::from(
        "# The owner's thresholds, fitted to every calibration answer (local only, never committed)\n# id\tkind\tvalue\tsource\tcontext\n",
    );
    let mut lines = Vec::new();
    let unit = |kind: Kind, v: f64| match kind {
        Kind::Absolute => format!("{v:.3}"),
        Kind::Relative => format!("{:.1} %", 100.0 * v),
    };
    for (reading, kind, context) in keys {
        let group: Vec<&Answer> = answers
            .iter()
            .filter(|a| a.reading == reading && a.kind == kind && a.context == context)
            .collect();
        let pairs: Vec<(f64, bool)> = group.iter().map(|a| (a.delta, a.correct)).collect();
        let Some(f) = fit(&pairs) else {
            continue;
        };
        let mut sessions: Vec<&str> = group.iter().map(|a| a.session.as_str()).collect();
        sessions.sort_unstable();
        sessions.dedup();
        let range = match (f.low, f.high) {
            (Some(l), Some(h)) => format!("{} to {}", unit(kind, l), unit(kind, h)),
            (None, Some(h)) => format!("under {}", unit(kind, h)),
            (Some(l), None) => format!("over {}", unit(kind, l)),
            (None, None) => "anywhere".to_string(),
        };
        let usable = f.trials >= FIT_MIN_TRIALS && f.low.is_some() && f.high.is_some();
        lines.push(format!(
            "{reading} ({context}): {} (95 %: {range}) from {} answers, {} right, in {} session(s){}",
            unit(kind, f.threshold),
            f.trials,
            f.correct,
            sessions.len(),
            if usable { "" } else { " — more answers needed before it is used" }
        ));
        if usable {
            out.push_str(&format!(
                "{reading}\t{}\t{}\towner: {} answers in {} session(s) ({}), 95 % {range}\t{context}\n",
                match kind {
                    Kind::Absolute => "abs",
                    Kind::Relative => "rel",
                },
                f.threshold,
                f.trials,
                sessions.len(),
                sessions.join(", ")
            ));
        }
    }
    (out, lines)
}

/// The date and time now, UTC, as `YYYY-MM-DDTHH-MM-SSZ` (safe in a file name).
#[must_use]
pub fn now_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Civil date from days since 1970-01-01 (Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}-{:02}-{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}
