//! Sets: round robins and velocity layers. Each group's readings are summarised by their mean and
//! spread across its files — the round-robin spread is the variation below which a difference between
//! two sounds means nothing (the plan's §3) — together with the velocity map the layers' loudness gives,
//! and the late lines that recur across a set: the room or the kit ringing along, not the drum.
//!
//! Sources: `docs/drum-model-fitting.md` (in mxm-drum-machine) §7 ("soft to hard over every round
//! robin … one strike per layer is noise"; "map velocity from the recordings' loudness"; "tell the
//! drum from the room").

use crate::reading::{Report, Unit, Validity};
use crate::repr::spectrum;
use crate::sound::Sound;

/// One reading summarised across a group's files.
#[derive(Clone, Debug, PartialEq)]
pub struct Stat {
    pub id: &'static str,
    pub label: &'static str,
    pub unit: Unit,
    pub window_ms: Option<(f64, f64)>,
    pub band_hz: Option<(f64, f64)>,
    pub mean: f64,
    /// The standard deviation across the files that had a value.
    pub spread: f64,
    pub min: f64,
    pub max: f64,
    /// Files with a value, and files in the group.
    pub n: usize,
    pub of: usize,
}

/// One group — a velocity layer, or any set of takes of one sound.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub name: String,
    pub files: Vec<String>,
    pub stats: Vec<Stat>,
}

impl Group {
    /// The statistic for a reading, matched by id and window start.
    #[must_use]
    pub fn stat(&self, id: &str, window_from_ms: Option<f64>) -> Option<&Stat> {
        self.stats.iter().find(|s| {
            s.id == id
                && window_from_ms
                    .is_none_or(|w| s.window_ms.is_some_and(|(a, _)| (a - w).abs() < 1e-9))
        })
    }
}

/// A set's summary.
#[derive(Clone, Debug, PartialEq)]
pub struct SetReport {
    pub groups: Vec<Group>,
    /// Each group's mean body loudness against the middle group's, dB, in the order given.
    pub velocity_map: Vec<(String, f64)>,
    /// Late lines found in at least half the set's files, Hz: the room's, not the drum's.
    pub room_lines: Vec<f64>,
    /// Every file's report, by group, described with the room lines known.
    pub reports: Vec<(String, Vec<Report>)>,
}

/// A value that counts: present, and neither absent nor below the floor.
fn usable(v: &Validity) -> bool {
    matches!(v, Validity::Valid | Validity::Sample | Validity::Unresolved)
}

fn same_place(a: Option<(f64, f64)>, b: Option<(f64, f64)>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => (x.0 - y.0).abs() < 1e-9 && (x.1 - y.1).abs() < 1e-9,
        _ => false,
    }
}

/// A reading's identity across files: id, label, unit, window and band.
type Key = (
    &'static str,
    &'static str,
    Unit,
    Option<(f64, f64)>,
    Option<(f64, f64)>,
);

/// Summarises each group's reports.
#[must_use]
pub fn summarise(groups: &[(String, Vec<Report>)]) -> Vec<Group> {
    groups
        .iter()
        .map(|(name, reports)| {
            let mut stats: Vec<Stat> = Vec::new();
            let mut keys: Vec<Key> = Vec::new();
            for r in reports {
                for s in &r.sections {
                    for x in &s.readings {
                        if !keys.iter().any(|k| {
                            k.0 == x.id
                                && same_place(k.3, x.window_ms)
                                && same_place(k.4, x.band_hz)
                        }) {
                            keys.push((x.id, x.label, x.unit, x.window_ms, x.band_hz));
                        }
                    }
                }
            }
            for (id, label, unit, window, band) in keys {
                let values: Vec<f64> = reports
                    .iter()
                    .filter_map(|r| {
                        r.sections
                            .iter()
                            .flat_map(|s| &s.readings)
                            .find(|x| {
                                x.id == id
                                    && same_place(x.window_ms, window)
                                    && same_place(x.band_hz, band)
                            })
                            .filter(|x| usable(&x.validity))
                            .and_then(|x| x.value)
                    })
                    .collect();
                if values.is_empty() {
                    continue;
                }
                let n = values.len() as f64;
                let mean = values.iter().sum::<f64>() / n;
                let spread = (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n).sqrt();
                stats.push(Stat {
                    id,
                    label,
                    unit,
                    window_ms: window,
                    band_hz: band,
                    mean,
                    spread,
                    min: values.iter().copied().fold(f64::INFINITY, f64::min),
                    max: values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                    n: values.len(),
                    of: reports.len(),
                });
            }
            Group {
                name: name.clone(),
                files: reports.iter().map(|r| r.name.clone()).collect(),
                stats,
            }
        })
        .collect()
}

/// Each group's mean body loudness against the middle group's.
#[must_use]
pub fn velocity_map(groups: &[Group]) -> Vec<(String, f64)> {
    let loud: Vec<Option<f64>> = groups
        .iter()
        .map(|g| g.stat("level.body_loudness", None).map(|s| s.mean))
        .collect();
    let Some(reference) = loud.get(groups.len() / 2).copied().flatten() else {
        return Vec::new();
    };
    groups
        .iter()
        .zip(loud)
        .filter_map(|(g, l)| l.map(|l| (g.name.clone(), l - reference)))
        .collect()
}

/// A late window, and the band its lines are looked for in, s and Hz.
pub const LATE_WINDOW_S: (f64, f64) = (0.8, 1.5);
pub const LATE_BAND_HZ: (f64, f64) = (100.0, 400.0);
/// Lines within this of each other are one line, Hz.
pub const SAME_LINE_HZ: f64 = 2.0;

/// The six strongest spectral peaks of a sound's late window in [`LATE_BAND_HZ`]; empty for a file too
/// short to have one.
#[must_use]
pub fn late_lines(sound: &Sound) -> Vec<f64> {
    let rate = f64::from(sound.rate);
    let Some(onset) = crate::prep::onset(&sound.samples) else {
        return Vec::new();
    };
    let (a, b) = (
        onset + (LATE_WINDOW_S.0 * rate) as usize,
        onset + (LATE_WINDOW_S.1 * rate) as usize,
    );
    if b > sound.samples.len() {
        return Vec::new();
    }
    let seg: Vec<f64> = sound.samples[a..b].iter().map(|&v| f64::from(v)).collect();
    let Some(s) = spectrum::power_spectrum(&seg, rate, 1 << 17) else {
        return Vec::new();
    };
    let bins = s.bins(LATE_BAND_HZ.0, LATE_BAND_HZ.1);
    let mut peaks: Vec<(f64, f64)> = bins
        .clone()
        .filter(|&k| {
            k > 0
                && k + 1 < s.power.len()
                && s.power[k] > s.power[k - 1]
                && s.power[k] >= s.power[k + 1]
        })
        .map(|k| (s.hz(k), s.power[k]))
        .collect();
    peaks.sort_by(|x, y| y.1.total_cmp(&x.1));
    let mut chosen: Vec<f64> = Vec::new();
    for (hz, _) in peaks {
        if chosen.iter().all(|c| (c - hz).abs() > SAME_LINE_HZ) {
            chosen.push(hz);
        }
        if chosen.len() == 6 {
            break;
        }
    }
    chosen
}

/// Lines found in at least half of the files' late windows.
#[must_use]
pub fn room_lines(per_file: &[Vec<f64>]) -> Vec<f64> {
    let with_late = per_file.iter().filter(|l| !l.is_empty()).count();
    if with_late < 3 {
        return Vec::new();
    }
    let mut found: Vec<f64> = Vec::new();
    for lines in per_file {
        for &hz in lines {
            if found.iter().any(|f| (f - hz).abs() <= SAME_LINE_HZ) {
                continue;
            }
            let count = per_file
                .iter()
                .filter(|other| other.iter().any(|o| (o - hz).abs() <= SAME_LINE_HZ))
                .count();
            if 2 * count >= with_late {
                found.push(hz);
            }
        }
    }
    found.sort_by(f64::total_cmp);
    found
}

/// Keeps the candidate room lines that are not one of the drum's own main modes: a line within
/// [`crate::parts::pitch::REST_WITHIN_DB`] of the strongest mode of the ring window in at least half
/// the files is the drum ringing on, which every round robin of one drum shares — the room's lines are
/// the ones that recur **without** being the drum (the guide: "across a set's drums of every size").
#[must_use]
pub fn not_the_drum(candidates: &[f64], reports: &[&Report]) -> Vec<f64> {
    let main_modes: Vec<Vec<f64>> = reports
        .iter()
        .map(|r| {
            r.sections
                .iter()
                .flat_map(|s| &s.tables)
                .filter(|t| t.id == "pitch.modes")
                .filter(|t| t.window_ms.is_some_and(|w| w.0 >= 160.0))
                .take(1)
                .flat_map(|t| {
                    let strongest = t
                        .rows
                        .iter()
                        .filter_map(|row| row[2])
                        .fold(f64::NEG_INFINITY, f64::max);
                    t.rows
                        .iter()
                        .filter(move |row| {
                            row[2].is_some_and(|l| {
                                l >= strongest - crate::parts::pitch::REST_WITHIN_DB
                            })
                        })
                        .filter_map(|row| row[0])
                        .collect::<Vec<f64>>()
                })
                .collect()
        })
        .collect();
    candidates
        .iter()
        .copied()
        .filter(|&hz| {
            let tol = SAME_LINE_HZ.max(crate::parts::pitch::ROOM_FRACTION * hz);
            let drum = main_modes
                .iter()
                .filter(|modes| modes.iter().any(|m| (m - hz).abs() <= tol))
                .count();
            2 * drum < reports.len()
        })
        .collect()
}

/// One group of a set, described for a comparison: the set's recurring late lines from every file
/// (cheap), kept as room lines unless they are this group's own main modes, and the group's reports
/// with them known. Only the group is described.
#[must_use]
pub fn analyse_group(
    groups: &[(String, Vec<Sound>)],
    group: &str,
    declared: Option<crate::Family>,
) -> Option<(Vec<f64>, Vec<Report>)> {
    let takes = &groups.iter().find(|g| g.0 == group)?.1;
    let per_file: Vec<Vec<f64>> = groups
        .iter()
        .flat_map(|(_, s)| s.iter().map(late_lines))
        .collect();
    let describe_takes = |room: &[f64]| -> Vec<Report> {
        let options = crate::describe::Options {
            family: declared,
            room_lines: room.to_vec(),
            without_perception: false,
            expect_hz: None,
            note_off_s: None,
        };
        takes
            .iter()
            .map(|s| crate::describe::describe_with(s, &options))
            .collect()
    };
    let first = describe_takes(&[]);
    let refs: Vec<&Report> = first.iter().collect();
    let room = not_the_drum(&room_lines(&per_file), &refs);
    let reports = if room.is_empty() {
        first
    } else {
        describe_takes(&room)
    };
    Some((room, reports))
}

/// Finds the set's room lines, then describes every sound with them kept out of its pitch, and
/// summarises the set. Every sound is described once; where room lines are found, the set is described
/// again with them known.
#[must_use]
pub fn analyse(groups: &[(String, Vec<Sound>)], declared: Option<crate::Family>) -> SetReport {
    let per_file: Vec<Vec<f64>> = groups
        .iter()
        .flat_map(|(_, s)| s.iter().map(late_lines))
        .collect();
    let describe_all = |room: &[f64]| -> Vec<(String, Vec<Report>)> {
        let options = crate::describe::Options {
            family: declared,
            room_lines: room.to_vec(),
            without_perception: false,
            expect_hz: None,
            note_off_s: None,
        };
        groups
            .iter()
            .map(|(name, sounds)| {
                (
                    name.clone(),
                    sounds
                        .iter()
                        .map(|s| crate::describe::describe_with(s, &options))
                        .collect(),
                )
            })
            .collect()
    };
    let first = describe_all(&[]);
    let all: Vec<&Report> = first.iter().flat_map(|(_, r)| r).collect();
    let room = not_the_drum(&room_lines(&per_file), &all);
    let reports = if room.is_empty() {
        first
    } else {
        describe_all(&room)
    };
    let summary = summarise(&reports);
    SetReport {
        velocity_map: velocity_map(&summary),
        room_lines: room,
        groups: summary,
        reports,
    }
}
