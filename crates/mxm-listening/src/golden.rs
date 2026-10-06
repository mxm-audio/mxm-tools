//! Golden cases: what the owner heard, and what an approved sound is known to keep, as data the
//! listener is scored against (the plan's §6). Sounds are logical names; a local file maps each name
//! to a file, so no recording and no path into a private folder is committed.

use crate::compare::{Comparison, Finding};

/// What a case asks of the listener.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The owner's words: an expected reading must land among the first [`TOP`] headline groups.
    Phrase,
    /// A recorded difference of an approved sound: it must be among the audible differences.
    Deviation,
    /// The owner approved the pair: every audible difference is a false alarm.
    Approved,
}

/// How far down the headline a phrase may land and still count (the plan's "top three").
pub const TOP: usize = 3;

/// One reading that means the case: its id, optionally its band's lower edge, and the candidate's
/// side (`+1` above the reference, `−1` below).
#[derive(Clone, Debug, PartialEq)]
pub struct Expect {
    pub id: String,
    pub band_lo: Option<f64>,
    pub sign: Option<f64>,
}

impl Expect {
    /// Whether `f` is this reading, in this band, on this side.
    #[must_use]
    pub fn matches(&self, f: &Finding) -> bool {
        f.id == self.id
            && self
                .band_lo
                .is_none_or(|lo| f.band_hz.is_some_and(|b| (b.0 - lo).abs() <= 0.01 * lo))
            && self
                .sign
                .is_none_or(|s| (f.candidate - f.reference) * s > 0.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Case {
    pub name: String,
    pub date: String,
    pub source: String,
    pub kind: Kind,
    /// (reference, candidate) logical names.
    pub pairs: Vec<(String, String)>,
    /// Any of these.
    pub expect: Vec<Expect>,
    pub phrase: String,
}

/// The committed cases.
#[must_use]
pub fn cases() -> Vec<Case> {
    parse(include_str!("../data/golden.tsv")).expect("the committed golden cases parse")
}

/// Cases from TSV text (the columns are described in `data/golden.tsv`).
///
/// # Errors
/// A line with the wrong number of columns, an unknown kind, or a malformed pair or reading.
pub fn parse(text: &str) -> Result<Vec<Case>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        let [name, date, source, kind, pairs, expect, phrase] = cols[..] else {
            return Err(format!("line {}: {} columns, want 7", n + 1, cols.len()));
        };
        let kind = match kind {
            "phrase" => Kind::Phrase,
            "deviation" => Kind::Deviation,
            "approved" => Kind::Approved,
            other => return Err(format!("line {}: unknown kind {other:?}", n + 1)),
        };
        let pairs = pairs
            .split(',')
            .map(|p| {
                p.split_once('>')
                    .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
                    .ok_or_else(|| {
                        format!("line {}: a pair needs reference>candidate: {p:?}", n + 1)
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let expect = if kind == Kind::Approved {
            Vec::new()
        } else {
            expect
                .split('|')
                .map(|e| {
                    parse_expect(e.trim())
                        .ok_or_else(|| format!("line {}: bad reading {e:?}", n + 1))
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        out.push(Case {
            name: name.to_string(),
            date: date.to_string(),
            source: source.to_string(),
            kind,
            pairs,
            expect,
            phrase: phrase.to_string(),
        });
    }
    Ok(out)
}

fn parse_expect(s: &str) -> Option<Expect> {
    let (rest, sign) = match s.rsplit_once(':') {
        Some((r, "+")) => (r, Some(1.0)),
        Some((r, "-")) => (r, Some(-1.0)),
        Some(_) => return None,
        None => (s, None),
    };
    let (id, band_lo) = match rest.split_once('@') {
        Some((id, lo)) => (id, Some(lo.parse::<f64>().ok()?)),
        None => (rest, None),
    };
    (!id.is_empty()).then(|| Expect {
        id: id.to_string(),
        band_lo,
        sign,
    })
}

/// One pair's result.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    /// The 1-based headline group an expected reading leads or joins, if any is audible.
    pub rank: Option<usize>,
    /// Whether the case holds on this pair.
    pub hit: bool,
    /// The audible headline groups, which on an approved pair are its false alarms.
    pub audible_groups: usize,
    /// The first [`TOP`] groups' phrases.
    pub top: Vec<String>,
}

/// How a case fares on one comparison.
#[must_use]
pub fn evaluate(case: &Case, c: &Comparison) -> Outcome {
    let groups = c.headline();
    let rank = groups
        .iter()
        .position(|g| g.iter().any(|f| case.expect.iter().any(|e| e.matches(f))))
        .map(|i| i + 1);
    let hit = match case.kind {
        Kind::Phrase => rank.is_some_and(|r| r <= TOP),
        Kind::Deviation => rank.is_some(),
        Kind::Approved => groups.is_empty(),
    };
    Outcome {
        rank,
        hit,
        audible_groups: groups.len(),
        top: groups
            .iter()
            .take(TOP)
            .map(|g| {
                g[0].phrase
                    .clone()
                    .unwrap_or_else(|| g[0].label.to_string())
            })
            .collect(),
    }
}
