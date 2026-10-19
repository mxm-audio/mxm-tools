//! Cases, the files they are put together into, and the ground truth written beside each file.
//!
//! A case is rendered alone, at the file's rate, from its own time zero, with its notes and its
//! f0 on a 1 ms grid. A file lays its cases out one after another and writes, in URMP's format:
//! `Notes_1_<instrument>_<nn>_<set>.txt` (onset s, pitch Hz, length s, tab-separated, no header),
//! `F0s_1_<instrument>_<nn>_<set>.txt` (time s, f0 Hz, every 10 ms, 0 where nothing pitched
//! sounds) and `<slug>.case.json`, every note with the case and parameters it was made with.

use crate::json::{Json, obj};
use crate::vst::Result;
use std::fmt::Write as _;
use std::path::Path;

/// The f0 grid's step: 1 ms.
pub const F0_RATE: f64 = 1000.0;

#[derive(Clone, Debug)]
pub struct Note {
    /// Where it leaves the previous pitch or begins to sound, s.
    pub onset: f64,
    /// Where the next begins or its release falls 40 dB, s.
    pub end: f64,
    /// Its intended centre (a vibrato's midline), Hz.
    pub hz: f64,
    /// What this note was made with, beyond its case's parameters.
    pub params: Json,
}

#[derive(Clone, Debug)]
pub struct Case {
    pub kind: String,
    pub control: bool,
    pub engine: &'static str,
    pub params: Json,
    pub audio: Vec<f64>,
    pub notes: Vec<Note>,
    /// f0 at each millisecond from the case's start, Hz; 0 where nothing pitched sounds.
    pub f0: Vec<f64>,
}

impl Case {
    pub fn new(kind: &str, engine: &'static str, control: bool) -> Self {
        Self {
            kind: kind.to_owned(),
            control,
            engine,
            params: Json::Obj(Vec::new()),
            audio: Vec::new(),
            notes: Vec::new(),
            f0: Vec::new(),
        }
    }

    pub fn duration(&self, rate: f64) -> f64 {
        self.audio.len() as f64 / rate
    }

    /// Mixes `audio` in at `at` seconds, growing the case to hold it.
    pub fn mix(&mut self, rate: f64, at: f64, audio: &[f64]) {
        let start = (at * rate).round() as usize;
        if self.audio.len() < start + audio.len() {
            self.audio.resize(start + audio.len(), 0.0);
        }
        for (out, v) in self.audio[start..].iter_mut().zip(audio) {
            *out += v;
        }
    }

    /// Sets the f0 from `from` to `to` seconds to `hz(t)`.
    pub fn set_f0(&mut self, from: f64, to: f64, hz: impl Fn(f64) -> f64) {
        let (a, b) = (
            (from * F0_RATE).round() as usize,
            (to * F0_RATE).round() as usize,
        );
        if self.f0.len() < b {
            self.f0.resize(b, 0.0);
        }
        for i in a..b {
            self.f0[i] = hz(i as f64 / F0_RATE);
        }
    }
}

/// One file: its cases in order and how they are spaced.
pub struct Take {
    pub set: String,
    pub instrument: String,
    pub nn: usize,
    /// Appended to the slug: `-44k` for a copy rendered at 44.1 kHz, `-rt15` for a reverb copy.
    pub suffix: String,
    pub rate: f64,
    pub seed: u64,
    pub cases: Vec<Case>,
    /// Silence before the first case and after the last, s.
    pub lead: f64,
    pub tail: f64,
    /// Space after each case, s (the last is unused).
    pub gaps: Vec<f64>,
    /// What the whole file was made with (hum, noise floor, reverb, gain).
    pub params: Json,
}

/// A take laid out: the samples and the truth in file time.
pub struct Laid {
    pub audio: Vec<f64>,
    pub notes: Vec<(Note, usize)>,
    pub f0: Vec<f64>,
    pub starts: Vec<f64>,
}

impl Take {
    pub fn slug(&self) -> String {
        format!(
            "synth-{}-{}-{:02}{}",
            self.set, self.instrument, self.nn, self.suffix
        )
    }

    /// The take with its cases laid end to end.
    pub fn lay_out(&self) -> Laid {
        let mut t = self.lead;
        let mut starts = Vec::new();
        for (i, case) in self.cases.iter().enumerate() {
            starts.push(t);
            t += case.duration(self.rate).max(case.f0.len() as f64 / F0_RATE);
            if i + 1 < self.cases.len() {
                t += self.gaps.get(i).copied().unwrap_or(0.75);
            }
        }
        let length = t + self.tail;
        let mut audio = vec![0.0; (length * self.rate).round() as usize];
        let mut f0 = vec![0.0; (length * F0_RATE).round() as usize + 1];
        let mut notes = Vec::new();
        for (i, (case, &start)) in self.cases.iter().zip(&starts).enumerate() {
            let at = (start * self.rate).round() as usize;
            for (out, v) in audio[at..].iter_mut().zip(&case.audio) {
                *out += v;
            }
            let at = (start * F0_RATE).round() as usize;
            for (j, &v) in case.f0.iter().enumerate() {
                if v > 0.0 && at + j < f0.len() {
                    f0[at + j] = v;
                }
            }
            for note in &case.notes {
                let mut note = note.clone();
                note.onset += start;
                note.end += start;
                notes.push((note, i));
            }
        }
        notes.sort_by(|a, b| a.0.onset.total_cmp(&b.0.onset));
        Laid {
            audio,
            notes,
            f0,
            starts,
        }
    }
}

/// Writes the audio and its truth under `root` (newDAWn's `examples/audio`): the WAV in
/// `synthetic/`, the annotations in `annotations/<slug>/`.
pub fn write(root: &Path, take: &Take, laid: &Laid, generator: &str) -> Result<()> {
    let slug = take.slug();
    let peak = laid.audio.iter().fold(0.0f64, |p, v| p.max(v.abs()));
    if peak > crate::dsp::db_to_gain(-1.0) {
        return Err(format!(
            "{slug}: peak {:.2} dBFS is above -1",
            crate::dsp::gain_to_db(peak)
        )
        .into());
    }
    let samples: Vec<f32> = laid.audio.iter().map(|&v| v as f32).collect();
    mxm_audio_file::write(
        root.join("synthetic").join(format!("{slug}.wav")),
        &samples,
        1,
        take.rate as u32,
        mxm_audio_file::Target::WavFloat32,
    )?;

    let folder = root.join("annotations").join(&slug);
    std::fs::create_dir_all(&folder)?;
    let tag = format!("{}_{:02}_{}", take.instrument, take.nn, take.set);

    let mut text = String::new();
    for (note, _) in &laid.notes {
        let _ = writeln!(
            text,
            "{:.6}\t{:.4}\t{:.6}",
            note.onset,
            note.hz,
            note.end - note.onset
        );
    }
    std::fs::write(folder.join(format!("Notes_1_{tag}.txt")), text)?;

    let duration = laid.audio.len() as f64 / take.rate;
    let mut text = String::new();
    let frames = (duration * 100.0).floor() as usize;
    for k in 0..=frames {
        let t = k as f64 / 100.0;
        let v = laid
            .f0
            .get((t * F0_RATE).round() as usize)
            .copied()
            .unwrap_or(0.0);
        let _ = writeln!(text, "{t:.2}\t{v:.4}");
    }
    std::fs::write(folder.join(format!("F0s_1_{tag}.txt")), text)?;

    let notes: Vec<Json> = laid
        .notes
        .iter()
        .enumerate()
        .map(|(n, (note, case))| {
            let c = &take.cases[*case];
            let mut entry = obj(vec![
                ("n", Json::from(n + 1)),
                ("onset", note.onset.into()),
                ("end", note.end.into()),
                ("hz", note.hz.into()),
                ("case", Json::from(case + 1)),
                ("kind", c.kind.as_str().into()),
                ("control", c.control.into()),
            ]);
            if let Json::Obj(pairs) = &note.params {
                for (k, v) in pairs {
                    entry.push(k, v.clone());
                }
            }
            entry
        })
        .collect();
    let cases: Vec<Json> = take
        .cases
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let numbers: Vec<Json> = laid
                .notes
                .iter()
                .enumerate()
                .filter(|(_, (_, case))| *case == i)
                .map(|(n, _)| Json::from(n + 1))
                .collect();
            obj(vec![
                ("case", Json::from(i + 1)),
                ("kind", c.kind.as_str().into()),
                ("control", c.control.into()),
                ("engine", c.engine.into()),
                ("start", laid.starts[i].into()),
                ("end", (laid.starts[i] + c.duration(take.rate)).into()),
                ("notes", Json::Arr(numbers)),
                ("params", c.params.clone()),
            ])
        })
        .collect();
    let sidecar = obj(vec![
        ("slug", slug.as_str().into()),
        ("set", take.set.as_str().into()),
        ("instrument", take.instrument.as_str().into()),
        ("nn", Json::from(take.nn)),
        ("sample_rate_hz", take.rate.into()),
        ("duration_s", duration.into()),
        ("peak_dbfs", crate::dsp::gain_to_db(peak).into()),
        ("generator", generator.into()),
        ("seed", Json::Str(format!("{:#018x}", take.seed))),
        ("file", take.params.clone()),
        ("cases", Json::Arr(cases)),
        ("notes", Json::Arr(notes)),
    ]);
    std::fs::write(folder.join(format!("{slug}.case.json")), sidecar.pretty())?;
    Ok(())
}
