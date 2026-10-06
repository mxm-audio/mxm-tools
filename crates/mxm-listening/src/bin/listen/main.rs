//! `listen`: the command-line listener.
//!
//! ```text
//! listen describe <file> [--name NAME] [--family percussive|note|sustained|voice|impulse] [--expect NOTE] [--off S] [--room HZ,HZ,…] [--json OUT] [--md OUT] [--quiet]
//! listen set <manifest.tsv> [--family percussive|note] [--json OUT] [--md OUT] [--quiet]
//! listen compare <reference> <candidate> [--family percussive|note] [--expect NOTE] [--room HZ,…] [--all] [--json OUT] [--md OUT] [--quiet]
//! listen compare --set <manifest.tsv> --group <name> <candidate> [--family percussive|note] [--all] [--json OUT] [--md OUT]
//! listen selftest <file> [--family percussive|note|sustained|voice] [--room HZ,…] [--write REBUILT.wav] [--write-parts PREFIX] [--json OUT] [--md OUT]
//! listen golden --files <map.tsv> [--cases <golden.tsv>] [--family percussive|note] [--md OUT] [--quiet]
//! listen site <site-dir> [--family percussive|note] [--quiet]
//! listen session <file> [--name NAME] [--family percussive|note|sustained] [--operators pitch,decay,…] [--minutes 6] [--port 8781] [--device TEXT] [--dir .listening]
//! listen calibrate [--dir .listening]
//! listen split <run>… [--held] --out DIR
//! listen keymap <note>… [--expect-from-name] [--room HZ,…] [--title T] [--md OUT]
//! listen stimulus <kind> --out PREFIX [--rate HZ] [--level DBFS] [--lead S] [--seconds S] [--tail S] [--channel both|left|right] [--hz F] [--hz2 F] [--second-db DB] [--from F] [--to F] [--cycles N] [--every S] [--levels DB,DB,…] [--seed N] [--key K] [--velocity V] [--set NAME=VALUE]…
//! listen respond <sidecar> <response> [<sidecar> <response>]… [--name NAME] [--json OUT] [--md OUT] [--quiet]
//! listen respond <sidecar> <response> --against <reference response> [--all] [--json OUT] [--md OUT] [--quiet]
//! listen explain "<phrase>" [<reference> <candidate>] [--family percussive|note|…] [--expect NOTE]
//! ```
//!
//! `explain` says what the owner's phrase has meant (the plan's §4): the vocabulary's rows and the
//! golden cases whose words hold it, each with the reading, where and which way; given two sounds, it
//! compares them and lists those readings' differences, audible or not, so a note is checked against
//! the measurements before anything is changed.
//!
//! `stimulus` writes what an instrument or an effect is to be played — `impulse`, `sweep`, `bursts`,
//! `steps`, `sine`, `dual-sine`, `noise`, `silence`, or a `note` — as `PREFIX.wav` (32-bit float; none
//! for a note, which is played rather than heard) and its sidecar `PREFIX.stimulus.tsv`, which records
//! exactly what it is. `--set` records what the device was set to. `respond` reads each response
//! against its sidecar; given a response driven left and one driven right it adds the true-stereo
//! matrix, and given several that differ in a setting, their readings against it. `--against` compares
//! one response with a reference's response to the same stimulus — a model effect against the hardware —
//! and lists the audible differences as `compare` does.
//!
//! `keymap` describes a set of pitched notes spanning an instrument's compass and fits each note
//! reading against octaves from middle C: its value there and its change for each octave up.
//!
//! `split` cuts a run of struck notes (a chromatic scale, one note at a time, named with its range as
//! Iowa's mallet files are: `Marimba.yarn.mf.C4B4`) into one file per note, `<run>.<note>.wav`, and says
//! how far each cut's spectral peak sits from its note's name.
//!
//! `session` serves a calibration session on `http://127.0.0.1:PORT/`: odd-one-out trials of the
//! sound against itself changed by each operator (`level`, `pitch`, `decay`, `band-LO-HI`, `attack`,
//! `noise`, `wobble`, `click`), interleaved staircases, check trials; its log and outcomes go to
//! `DIR/sessions/`. `calibrate` turns every session's outcomes into `DIR/owner-thresholds.tsv`, which
//! `compare`, `selftest`, `golden` and `site` then lay over the literature's. `DIR` is `.listening` in
//! the current directory, ignored by git: the owner's thresholds stay local.
//!
//! `site` writes, beside an A/B comparison site (the drum tools' `ab-mapping.json` and its
//! `audio/NNN-original.wav` and `NNN-model.wav`), a report per row in `listen/NNN.md` and `.json`,
//! and `listen/index.md` with each row's first differences: read before the owner listens.
//!
//! `golden` scores the listener on the owner's golden cases (`data/golden.tsv`, or `--cases`): each
//! case names its sounds logically, and the map, a local file never committed, has one
//! `name<TAB>path` per line, paths relative to it. A pair whose sounds are not mapped is listed, not
//! scored.
//!
//! `selftest` rebuilds a sound from its own readings and compares the rebuild with it: whatever it
//! finds is something the readings do not hold (the owner's self-test, the plan's revision 9).
//!
//! `compare` reads the candidate against the reference — a model against its recording — and lists the
//! audible differences, most audible first; `--set` compares against every take of one group of a set,
//! so a difference must hold across the takes; `--all` also lists the ones within threshold.
//!
//! A set's manifest has one file per line, `group<TAB>path[<TAB>name]`, paths relative to the
//! manifest; `#` starts a comment. Groups keep the order they first appear in (soft to hard, say).
//!
//! Prints the Markdown report unless `--quiet`; `--json` and `--md` also write files. The report names
//! the sound by `--name`, or by the file's stem — never by its path, which may point into a private
//! folder.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mxm_listening::report::{json, markdown};
use mxm_listening::{Family, Sound};

mod serve;

fn usage() -> ExitCode {
    eprintln!(
        "usage: listen describe <file> [--name NAME] [--family percussive|note|sustained|voice|impulse] [--expect NOTE] [--off S] [--room HZ,…] [--json OUT] [--md OUT] [--quiet]\n       listen set <manifest.tsv> [--family percussive|note] [--json OUT] [--md OUT] [--quiet]\n       listen compare <reference> <candidate> [--family percussive|note] [--expect NOTE] [--room HZ,…] [--all] [--json OUT] [--md OUT] [--quiet]\n       listen compare --set <manifest.tsv> --group <name> <candidate> [--family percussive|note] [--all] [--json OUT] [--md OUT]\n       listen selftest <file> [--family percussive|note|sustained|voice] [--room HZ,…] [--write REBUILT.wav] [--write-parts PREFIX] [--json OUT] [--md OUT]\n       listen golden --files <map.tsv> [--cases <golden.tsv>] [--family percussive|note] [--md OUT] [--quiet]\n       listen site <site-dir> [--family percussive|note] [--quiet]\n       listen session <file> [--name NAME] [--family percussive|note|sustained] [--operators pitch,decay,…] [--minutes 6] [--port 8781] [--device TEXT] [--dir .listening]\n       listen calibrate [--dir .listening]\n       listen split <run>… [--held] --out DIR\n       listen keymap <note>… [--expect-from-name] [--room HZ,…] [--title T] [--md OUT]\n       listen stimulus <kind> --out PREFIX [--rate HZ] [--level DBFS] [--lead S] [--seconds S] [--tail S] [--channel both|left|right] [--hz F] [--hz2 F] [--second-db DB] [--from F] [--to F] [--cycles N] [--every S] [--levels DB,…] [--seed N] [--key K] [--velocity V] [--set NAME=VALUE]…\n       listen respond <sidecar> <response> [<sidecar> <response>]… [--name NAME] [--json OUT] [--md OUT] [--quiet]\n       listen respond <sidecar> <response> --against <reference> [--all] [--json OUT] [--md OUT] [--quiet]\n       listen explain \"<phrase>\" [<reference> <candidate>] [--family percussive|note|…] [--expect NOTE]"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first() else {
        return usage();
    };
    match command.as_str() {
        "describe" => run_describe(&args[1..]),
        "set" => run_set(&args[1..]),
        "compare" => run_compare(&args[1..]),
        "selftest" => run_selftest(&args[1..]),
        "golden" => run_golden(&args[1..]),
        "site" => run_site(&args[1..]),
        "session" => run_session(&args[1..]),
        "calibrate" => run_calibrate(&args[1..]),
        "split" => run_split(&args[1..]),
        "keymap" => run_keymap(&args[1..]),
        "stimulus" => run_stimulus(&args[1..]),
        "respond" => run_respond(&args[1..]),
        "explain" => run_explain(&args[1..]),
        _ => usage(),
    }
}

fn run_describe(args: &[String]) -> ExitCode {
    let mut file: Option<PathBuf> = None;
    let mut off: Option<f64> = None;
    let mut name: Option<String> = None;
    let mut family: Option<Family> = None;
    let mut room: Vec<f64> = Vec::new();
    let mut expect: Option<f64> = None;
    let mut json_out: Option<PathBuf> = None;
    let mut md_out: Option<PathBuf> = None;
    let mut quiet = false;
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1);
        match args[i].as_str() {
            "--off" => {
                off = value
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite());
                if off.is_none() {
                    eprintln!("--off takes the note-off time in seconds from the file's start");
                    return ExitCode::from(2);
                }
                i += 1;
            }
            "--expect" => {
                expect = expected(value);
                if expect.is_none() {
                    eprintln!("--expect takes a note name (A4, Db6) or a frequency in hertz");
                    return ExitCode::from(2);
                }
                i += 1;
            }
            "--room" => {
                let parsed: Option<Vec<f64>> = value.map(|v| {
                    v.split(',')
                        .filter_map(|x| x.trim().parse::<f64>().ok())
                        .collect()
                });
                match parsed {
                    Some(r) if !r.is_empty() => room = r,
                    _ => {
                        eprintln!("--room takes frequencies in hertz, comma-separated");
                        return ExitCode::from(2);
                    }
                }
                i += 1;
            }
            "--name" => {
                name = value.cloned();
                i += 1;
            }
            "--family" => {
                family = value.and_then(|v| Family::parse(v));
                if family.is_none() {
                    eprintln!("unknown family: `percussive` or `note`");
                    return ExitCode::from(2);
                }
                i += 1;
            }
            "--json" => {
                json_out = value.map(PathBuf::from);
                i += 1;
            }
            "--md" => {
                md_out = value.map(PathBuf::from);
                i += 1;
            }
            "--quiet" => quiet = true,
            other if file.is_none() && !other.starts_with("--") => {
                file = Some(PathBuf::from(other))
            }
            _ => return usage(),
        }
        i += 1;
    }
    let Some(file) = file else {
        return usage();
    };
    let name = name.unwrap_or_else(|| stem(&file));
    let sound = match Sound::load(&file, name) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read the file: {e}");
            return ExitCode::from(1);
        }
    };
    let report = mxm_listening::describe::describe_with(
        &sound,
        &mxm_listening::describe::Options {
            family,
            room_lines: room,
            without_perception: false,
            expect_hz: expect,
            note_off_s: off,
        },
    );
    let md = markdown::to_markdown(&report);
    if let Some(path) = json_out {
        if let Err(e) = std::fs::write(&path, json::to_json(&report)) {
            eprintln!("cannot write {}: {e}", path.display());
            return ExitCode::from(1);
        }
    }
    if let Some(path) = md_out {
        if let Err(e) = std::fs::write(&path, &md) {
            eprintln!("cannot write {}: {e}", path.display());
            return ExitCode::from(1);
        }
    }
    if !quiet {
        print!("{md}");
    }
    ExitCode::SUCCESS
}

fn run_set(args: &[String]) -> ExitCode {
    let mut manifest: Option<PathBuf> = None;
    let mut family: Option<Family> = None;
    let mut json_out: Option<PathBuf> = None;
    let mut md_out: Option<PathBuf> = None;
    let mut quiet = false;
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1);
        match args[i].as_str() {
            "--family" => {
                family = value.and_then(|v| Family::parse(v));
                if family.is_none() {
                    eprintln!("unknown family: `percussive` or `note`");
                    return ExitCode::from(2);
                }
                i += 1;
            }
            "--json" => {
                json_out = value.map(PathBuf::from);
                i += 1;
            }
            "--md" => {
                md_out = value.map(PathBuf::from);
                i += 1;
            }
            "--quiet" => quiet = true,
            other if manifest.is_none() && !other.starts_with("--") => {
                manifest = Some(PathBuf::from(other));
            }
            _ => return usage(),
        }
        i += 1;
    }
    let Some(manifest) = manifest else {
        return usage();
    };
    let text = match std::fs::read_to_string(&manifest) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot read the manifest: {e}");
            return ExitCode::from(1);
        }
    };
    let base = manifest.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut groups: Vec<(String, Vec<Sound>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 2 {
            eprintln!("a manifest line needs a group and a path: {line}");
            return ExitCode::from(2);
        }
        let path = base.join(fields[1]);
        let name = fields
            .get(2)
            .map_or_else(|| stem(&path), |n| (*n).to_string());
        let sound = match Sound::load(&path, name) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("cannot read {}: {e}", fields[1]);
                return ExitCode::from(1);
            }
        };
        match groups.iter_mut().find(|g| g.0 == fields[0]) {
            Some(g) => g.1.push(sound),
            None => groups.push((fields[0].to_string(), vec![sound])),
        }
    }
    let report = mxm_listening::set::analyse(&groups, family);
    let md = markdown::set_to_markdown(&report);
    if let Some(path) = json_out {
        if let Err(e) = std::fs::write(&path, json::set_to_json(&report)) {
            eprintln!("cannot write {}: {e}", path.display());
            return ExitCode::from(1);
        }
    }
    if let Some(path) = md_out {
        if let Err(e) = std::fs::write(&path, &md) {
            eprintln!("cannot write {}: {e}", path.display());
            return ExitCode::from(1);
        }
    }
    if !quiet {
        print!("{md}");
    }
    ExitCode::SUCCESS
}

/// Reads a set manifest into groups of sounds.
fn read_manifest(manifest: &Path) -> Result<Vec<(String, Vec<Sound>)>, String> {
    let text =
        std::fs::read_to_string(manifest).map_err(|e| format!("cannot read the manifest: {e}"))?;
    let base = manifest.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut groups: Vec<(String, Vec<Sound>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 2 {
            return Err(format!("a manifest line needs a group and a path: {line}"));
        }
        let path = base.join(fields[1]);
        let name = fields
            .get(2)
            .map_or_else(|| stem(&path), |n| (*n).to_string());
        let sound =
            Sound::load(&path, name).map_err(|e| format!("cannot read {}: {e}", fields[1]))?;
        match groups.iter_mut().find(|g| g.0 == fields[0]) {
            Some(g) => g.1.push(sound),
            None => groups.push((fields[0].to_string(), vec![sound])),
        }
    }
    Ok(groups)
}

fn run_compare(args: &[String]) -> ExitCode {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut family: Option<Family> = None;
    let mut room: Vec<f64> = Vec::new();
    let mut expect: Option<f64> = None;
    let mut set: Option<PathBuf> = None;
    let mut group: Option<String> = None;
    let mut all = false;
    let mut json_out: Option<PathBuf> = None;
    let mut md_out: Option<PathBuf> = None;
    let mut quiet = false;
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1);
        match args[i].as_str() {
            "--family" => {
                family = value.and_then(|v| Family::parse(v));
                if family.is_none() {
                    eprintln!("unknown family: `percussive` or `note`");
                    return ExitCode::from(2);
                }
                i += 1;
            }
            "--expect" => {
                expect = expected(value);
                if expect.is_none() {
                    eprintln!("--expect takes a note name (A4, Db6) or a frequency in hertz");
                    return ExitCode::from(2);
                }
                i += 1;
            }
            "--room" => {
                room = value.map_or_else(Vec::new, |v| {
                    v.split(',')
                        .filter_map(|x| x.trim().parse::<f64>().ok())
                        .collect()
                });
                i += 1;
            }
            "--set" => {
                set = value.map(PathBuf::from);
                i += 1;
            }
            "--group" => {
                group = value.cloned();
                i += 1;
            }
            "--all" => all = true,
            "--json" => {
                json_out = value.map(PathBuf::from);
                i += 1;
            }
            "--md" => {
                md_out = value.map(PathBuf::from);
                i += 1;
            }
            "--quiet" => quiet = true,
            other if !other.starts_with("--") => files.push(PathBuf::from(other)),
            _ => return usage(),
        }
        i += 1;
    }
    let (thresholds, owner_note) = owner_thresholds();
    let vocabulary = mxm_listening::audibility::Vocabulary::owner();
    let comparison = if let Some(manifest) = set {
        let Some(group) = group else {
            eprintln!("--set needs --group");
            return ExitCode::from(2);
        };
        let [candidate] = files.as_slice() else {
            return usage();
        };
        let groups = match read_manifest(&manifest) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(1);
            }
        };
        let Some((room_lines, references)) =
            mxm_listening::set::analyse_group(&groups, &group, family)
        else {
            eprintln!("no group `{group}` in the manifest");
            return ExitCode::from(1);
        };
        let options = mxm_listening::describe::Options {
            family,
            room_lines,
            without_perception: false,
            expect_hz: expect,
            note_off_s: None,
        };
        let candidate = match Sound::load(candidate, stem(candidate)) {
            Ok(s) => mxm_listening::describe::describe_with(&s, &options),
            Err(e) => {
                eprintln!("cannot read the candidate: {e}");
                return ExitCode::from(1);
            }
        };
        let refs: Vec<&mxm_listening::Report> = references.iter().collect();
        mxm_listening::compare::compare_to_set(&refs, &candidate, &thresholds, &vocabulary, &group)
    } else {
        let [reference, candidate] = files.as_slice() else {
            return usage();
        };
        let options = mxm_listening::describe::Options {
            family,
            room_lines: room,
            without_perception: false,
            expect_hz: expect,
            note_off_s: None,
        };
        let load = |p: &PathBuf| {
            Sound::load(p, stem(p)).map(|s| mxm_listening::describe::describe_with(&s, &options))
        };
        match (load(reference), load(candidate)) {
            (Ok(r), Ok(c)) => mxm_listening::compare::compare(&r, &c, &thresholds, &vocabulary),
            (Err(e), _) | (_, Err(e)) => {
                eprintln!("cannot read a file: {e}");
                return ExitCode::from(1);
            }
        }
    };
    let mut comparison = comparison;
    comparison.notes.extend(owner_note);
    if let (Some(reference), Some(candidate)) = (files.first(), files.last()) {
        if files.len() == 2 {
            if let (Ok(r), Ok(c)) = (
                Sound::load(reference, stem(reference)),
                Sound::load(candidate, stem(candidate)),
            ) {
                comparison.map(&r, &c);
            }
        }
    }
    let mut md = markdown::comparison_to_markdown(&comparison);
    if all {
        md.push_str("\nWithin threshold:\n\n");
        for f in comparison.findings.iter().filter(|f| f.units < 1.0) {
            md.push_str(&format!(
                "- {}{}: {:.3} → {:.3} {} ({:.2}×)\n",
                f.label,
                f.window_ms
                    .map_or(String::new(), |(a, b)| format!(" {a:.0}–{b:.0} ms")),
                f.reference,
                f.candidate,
                f.unit.symbol(),
                f.units
            ));
        }
    }
    if let Some(path) = json_out {
        if let Err(e) = std::fs::write(&path, json::comparison_to_json(&comparison)) {
            eprintln!("cannot write {}: {e}", path.display());
            return ExitCode::from(1);
        }
    }
    if let Some(path) = md_out {
        if let Err(e) = std::fs::write(&path, &md) {
            eprintln!("cannot write {}: {e}", path.display());
            return ExitCode::from(1);
        }
    }
    if !quiet {
        print!("{md}");
    }
    ExitCode::SUCCESS
}

fn run_selftest(args: &[String]) -> ExitCode {
    let mut file: Option<PathBuf> = None;
    let mut family: Option<Family> = None;
    let mut room: Vec<f64> = Vec::new();
    let mut write: Option<PathBuf> = None;
    let mut parts: Option<String> = None;
    let mut json_out: Option<PathBuf> = None;
    let mut md_out: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1);
        match args[i].as_str() {
            "--family" => {
                family = value.and_then(|v| Family::parse(v));
                i += 1;
            }
            "--write-parts" => {
                parts = value.cloned();
                i += 1;
            }
            "--room" => {
                room = value.map_or_else(Vec::new, |v| {
                    v.split(',')
                        .filter_map(|x| x.trim().parse::<f64>().ok())
                        .collect()
                });
                i += 1;
            }
            "--write" => {
                write = value.map(PathBuf::from);
                i += 1;
            }
            "--json" => {
                json_out = value.map(PathBuf::from);
                i += 1;
            }
            "--md" => {
                md_out = value.map(PathBuf::from);
                i += 1;
            }
            other if file.is_none() && !other.starts_with("--") => {
                file = Some(PathBuf::from(other))
            }
            _ => return usage(),
        }
        i += 1;
    }
    let Some(file) = file else {
        return usage();
    };
    let original = match Sound::load(&file, stem(&file)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read the file: {e}");
            return ExitCode::from(1);
        }
    };
    let Some(whole) = mxm_listening::resynth::rebuild_with(&original, &room, family) else {
        eprintln!("the listener cannot measure this sound");
        return ExitCode::from(1);
    };
    if let Some(prefix) = &parts {
        for (name, part) in [("modes", &whole.modal), ("noise", &whole.noise)] {
            let samples: Vec<f32> = part.iter().map(|&v| v as f32).collect();
            let path = format!("{prefix}-{name}.wav");
            if let Err(e) = mxm_audio_file::write(
                &path,
                &samples,
                1,
                whole.sound.rate,
                mxm_audio_file::Target::WavFloat32,
            ) {
                eprintln!("cannot write {path}: {e:?}");
                return ExitCode::from(1);
            }
        }
    }
    let rebuilt = whole.sound;
    if let Some(path) = &write {
        if let Err(e) = mxm_audio_file::write(
            path,
            &rebuilt.samples,
            1,
            rebuilt.rate,
            mxm_audio_file::Target::WavFloat32,
        ) {
            eprintln!("cannot write {}: {e:?}", path.display());
            return ExitCode::from(1);
        }
    }
    let options = mxm_listening::describe::Options {
        family,
        room_lines: room,
        without_perception: false,
        expect_hz: None,
        note_off_s: None,
    };
    let a = mxm_listening::describe::describe_with(&original, &options);
    let b = mxm_listening::describe::describe_with(&rebuilt, &options);
    let (thresholds, owner_note) = owner_thresholds();
    let mut comparison = mxm_listening::compare::compare(
        &a,
        &b,
        &thresholds,
        &mxm_listening::audibility::Vocabulary::owner(),
    );
    comparison.notes.extend(owner_note);
    comparison.map(&original, &rebuilt);
    let mut md =
        String::from("Self-test: the sound rebuilt from its own readings, against the sound.\n\n");
    md.push_str(&markdown::comparison_to_markdown(&comparison));
    if let Some(path) = json_out {
        if let Err(e) = std::fs::write(&path, json::comparison_to_json(&comparison)) {
            eprintln!("cannot write {}: {e}", path.display());
            return ExitCode::from(1);
        }
    }
    if let Some(path) = md_out {
        if let Err(e) = std::fs::write(&path, &md) {
            eprintln!("cannot write {}: {e}", path.display());
            return ExitCode::from(1);
        }
    }
    print!("{md}");
    ExitCode::SUCCESS
}

fn run_golden(args: &[String]) -> ExitCode {
    use mxm_listening::golden::{self, Kind, TOP};
    let mut files: Option<PathBuf> = None;
    let mut cases_file: Option<PathBuf> = None;
    let mut family = Some(Family::Percussive);
    let mut md_out: Option<PathBuf> = None;
    let mut quiet = false;
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1);
        match args[i].as_str() {
            "--files" => {
                files = value.map(PathBuf::from);
                i += 1;
            }
            "--cases" => {
                cases_file = value.map(PathBuf::from);
                i += 1;
            }
            "--family" => {
                family = value.and_then(|v| Family::parse(v));
                i += 1;
            }
            "--md" => {
                md_out = value.map(PathBuf::from);
                i += 1;
            }
            "--quiet" => quiet = true,
            _ => return usage(),
        }
        i += 1;
    }
    let Some(files) = files else {
        return usage();
    };
    let cases = match &cases_file {
        None => golden::cases(),
        Some(p) => match std::fs::read_to_string(p)
            .map_err(|e| e.to_string())
            .and_then(|t| golden::parse(&t))
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("cannot read the cases: {e}");
                return ExitCode::from(1);
            }
        },
    };
    // The map: logical name to file.
    let map: Vec<(String, PathBuf)> = match std::fs::read_to_string(&files) {
        Ok(text) => {
            let base = files.parent().map(Path::to_path_buf).unwrap_or_default();
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .filter_map(|l| l.split_once('\t'))
                .map(|(n, p)| (n.trim().to_string(), base.join(p.trim())))
                .collect()
        }
        Err(e) => {
            eprintln!("cannot read the map: {e}");
            return ExitCode::from(1);
        }
    };
    let options = mxm_listening::describe::Options {
        family,
        ..mxm_listening::describe::Options::default()
    };
    let (thresholds, _owner_note) = owner_thresholds();
    let vocabulary = mxm_listening::audibility::Vocabulary::owner();
    // Each sound described once, however many cases name it.
    let mut reports: Vec<(String, mxm_listening::Report)> = Vec::new();
    let mut report_of = |name: &str| -> Option<mxm_listening::Report> {
        if let Some((_, r)) = reports.iter().find(|(n, _)| n == name) {
            return Some(r.clone());
        }
        let path = &map.iter().find(|(n, _)| n == name)?.1;
        match Sound::load(path, name) {
            Ok(s) => {
                let r = mxm_listening::describe::describe_with(&s, &options);
                reports.push((name.to_string(), r.clone()));
                Some(r)
            }
            Err(e) => {
                eprintln!("cannot read {name}: {e}");
                None
            }
        }
    };
    let mut rows = String::new();
    let (mut phrases, mut phrase_hits) = (0, 0);
    let (mut deviations, mut deviation_hits) = (0, 0);
    let (mut approved, mut approved_clean) = (0, 0);
    let mut unmapped = 0;
    for case in &cases {
        for (a, b) in &case.pairs {
            let (Some(ra), Some(rb)) = (report_of(a), report_of(b)) else {
                unmapped += 1;
                rows.push_str(&format!(
                    "| {} | {:?} | {a} → {b} | not mapped | | |\n",
                    case.name, case.kind
                ));
                continue;
            };
            let c = mxm_listening::compare::compare(&ra, &rb, &thresholds, &vocabulary);
            let o = golden::evaluate(case, &c);
            let hit = usize::from(o.hit);
            match case.kind {
                Kind::Phrase => (phrases, phrase_hits) = (phrases + 1, phrase_hits + hit),
                Kind::Deviation => {
                    (deviations, deviation_hits) = (deviations + 1, deviation_hits + hit)
                }
                Kind::Approved => (approved, approved_clean) = (approved + 1, approved_clean + hit),
            }
            let result = match case.kind {
                Kind::Approved => format!("{} audible", o.audible_groups),
                _ if o.hit => "found".to_string(),
                _ => "missed".to_string(),
            };
            rows.push_str(&format!(
                "| {} | {:?} | {a} → {b} | {result} | {} | {} |\n",
                case.name,
                case.kind,
                o.rank.map_or_else(
                    || "—".to_string(),
                    |r| format!("{r} of {}", o.audible_groups)
                ),
                o.top.join("; ")
            ));
        }
    }
    let mut md = String::from("# Golden cases\n\n");
    md.push_str(&format!(
        "- The owner's phrases among the first {TOP} differences: {phrase_hits} of {phrases} pairs.\n\
         - Known deviations among the audible differences: {deviation_hits} of {deviations}.\n\
         - Approved pairs with no audible difference: {approved_clean} of {approved}.\n\
         - Pairs not mapped to files: {unmapped}.\n\n"
    ));
    md.push_str(
        "| Case | Kind | Pair | Result | Rank | The first three |\n|---|---|---|---|---|---|\n",
    );
    md.push_str(&rows);
    md.push_str("\n## The cases\n\n");
    for case in &cases {
        md.push_str(&format!(
            "- **{}** ({}, {}): \"{}\"\n",
            case.name, case.date, case.source, case.phrase
        ));
    }
    if let Some(path) = md_out {
        if let Err(e) = std::fs::write(&path, &md) {
            eprintln!("cannot write {}: {e}", path.display());
            return ExitCode::from(1);
        }
    }
    if !quiet {
        print!("{md}");
    }
    ExitCode::SUCCESS
}

/// The rows of an A/B site's `ab-mapping.json`: each object's `id` and `label`. Read by scanning, not
/// by a JSON parser: the file is the drum tools' own flat array, and the listener takes no dependency.
fn site_rows(json: &str) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    for object in json.split('{').skip(1) {
        let object = object.split('}').next().unwrap_or("");
        let field = |name: &str| -> Option<&str> {
            let at = object.find(&format!("\"{name}\""))?;
            let rest = object[at + name.len() + 2..]
                .trim_start()
                .strip_prefix(':')?;
            Some(rest.trim_start())
        };
        let Some(id) = field("id").and_then(|v| {
            v.split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|d| d.parse::<u32>().ok())
        }) else {
            continue;
        };
        let label = field("label")
            .and_then(|v| v.strip_prefix('"'))
            .map(|v| {
                let mut s = String::new();
                let mut chars = v.chars();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => s.push(' '),
                            Some('u') => {
                                let hex: String = chars.by_ref().take(4).collect();
                                if let Some(ch) =
                                    u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
                                {
                                    s.push(ch);
                                }
                            }
                            Some(other) => s.push(other),
                            None => break,
                        },
                        _ => s.push(c),
                    }
                }
                s
            })
            .unwrap_or_else(|| id.to_string());
        out.push((id, label));
    }
    out
}

fn run_site(args: &[String]) -> ExitCode {
    let mut site: Option<PathBuf> = None;
    let mut family = Some(Family::Percussive);
    let mut quiet = false;
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1);
        match args[i].as_str() {
            "--family" => {
                family = value.and_then(|v| Family::parse(v));
                i += 1;
            }
            "--quiet" => quiet = true,
            other if site.is_none() && !other.starts_with("--") => {
                site = Some(PathBuf::from(other))
            }
            _ => return usage(),
        }
        i += 1;
    }
    let Some(site) = site else {
        return usage();
    };
    let rows = match std::fs::read_to_string(site.join("ab-mapping.json")) {
        Ok(text) => site_rows(&text),
        Err(e) => {
            eprintln!("cannot read the site's ab-mapping.json: {e}");
            return ExitCode::from(1);
        }
    };
    let out_dir = site.join("listen");
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        eprintln!("cannot make {}: {e}", out_dir.display());
        return ExitCode::from(1);
    }
    let options = mxm_listening::describe::Options {
        family,
        ..mxm_listening::describe::Options::default()
    };
    let (thresholds, _owner_note) = owner_thresholds();
    let vocabulary = mxm_listening::audibility::Vocabulary::owner();
    let mut index = String::from(
        "# What the listener hears on this page\n\nEach row's model against its original, most audible first; the full report is beside it in `listen/`.\n",
    );
    for (id, label) in rows {
        let original = site.join("audio").join(format!("{id:03}-original.wav"));
        let model = site.join("audio").join(format!("{id:03}-model.wav"));
        let (Ok(a), Ok(b)) = (
            Sound::load(&original, format!("{id} {label}: original")),
            Sound::load(&model, format!("{id} {label}: model")),
        ) else {
            let _ = std::fmt::Write::write_fmt(
                &mut index,
                format_args!("\n## {id}: {label}\n\nNo original and model audio for this row.\n"),
            );
            continue;
        };
        let ra = mxm_listening::describe::describe_with(&a, &options);
        let rb = mxm_listening::describe::describe_with(&b, &options);
        let mut c = mxm_listening::compare::compare(&ra, &rb, &thresholds, &vocabulary);
        c.map(&a, &b);
        let md = markdown::comparison_to_markdown(&c);
        let written = std::fs::write(out_dir.join(format!("{id:03}.md")), &md).and_then(|()| {
            std::fs::write(
                out_dir.join(format!("{id:03}.json")),
                json::comparison_to_json(&c),
            )
        });
        if let Err(e) = written {
            eprintln!("cannot write row {id}'s report: {e}");
            return ExitCode::from(1);
        }
        let _ = std::fmt::Write::write_fmt(&mut index, format_args!("\n## {id}: {label}\n\n"));
        let groups = c.headline();
        if groups.is_empty() {
            index.push_str("No audible difference.\n");
        }
        for (k, g) in groups.iter().take(SITE_HEADLINE).enumerate() {
            let _ = std::fmt::Write::write_fmt(
                &mut index,
                format_args!("{}. {}\n", k + 1, markdown::finding_line(g[0])),
            );
        }
        let unexplained = c.unexplained().count();
        if unexplained > 0 {
            let _ = std::fmt::Write::write_fmt(
                &mut index,
                format_args!(
                    "\n{unexplained} region(s) differ where no reading does: see the report.\n"
                ),
            );
        }
    }
    if let Err(e) = std::fs::write(out_dir.join("index.md"), &index) {
        eprintln!("cannot write the index: {e}");
        return ExitCode::from(1);
    }
    if !quiet {
        print!("{index}");
    }
    ExitCode::SUCCESS
}

/// How many differences the site's index shows per row.
const SITE_HEADLINE: usize = 5;

/// The literature's thresholds, with the owner's from `.listening/owner-thresholds.tsv` laid over
/// them when that local table exists (the plan's §5 precedence), and a note saying so.
fn owner_thresholds() -> (mxm_listening::audibility::Thresholds, Option<String>) {
    mxm_listening::audibility::Thresholds::with_owner(&serve::state_dir(None))
}

/// What a session measures when the owner does not say: three staircases, which a few minutes can
/// finish.
const SESSION_OPERATORS: &str = "pitch,decay,band-3200-6400";

fn run_session(args: &[String]) -> ExitCode {
    use mxm_listening::perturb::{Operator, Subject};
    let mut file: Option<PathBuf> = None;
    let mut name: Option<String> = None;
    let mut family = Some(Family::Percussive);
    let mut operators = SESSION_OPERATORS.to_string();
    let mut minutes = 6.0;
    let mut port: u16 = 8781;
    let mut device = "headphones".to_string();
    let mut dir: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1);
        match args[i].as_str() {
            "--name" => {
                name = value.cloned();
                i += 1;
            }
            "--family" => {
                family = value.and_then(|v| Family::parse(v));
                i += 1;
            }
            "--operators" => {
                operators = value.cloned().unwrap_or_default();
                i += 1;
            }
            "--minutes" => {
                minutes = value.and_then(|v| v.parse().ok()).unwrap_or(minutes);
                i += 1;
            }
            "--port" => {
                port = value.and_then(|v| v.parse().ok()).unwrap_or(port);
                i += 1;
            }
            "--device" => {
                device = value.cloned().unwrap_or(device);
                i += 1;
            }
            "--dir" => {
                dir = value.map(PathBuf::from);
                i += 1;
            }
            other if file.is_none() && !other.starts_with("--") => {
                file = Some(PathBuf::from(other))
            }
            _ => return usage(),
        }
        i += 1;
    }
    let Some(file) = file else {
        return usage();
    };
    let ops: Vec<Operator> = operators
        .split(',')
        .filter_map(|s| Operator::parse(s.trim()))
        .collect();
    if ops.is_empty() {
        eprintln!(
            "no operator known in {operators:?}: level, pitch, decay, band-LO-HI, attack, noise, wobble, click; for a note tilt, decay-law, inharmonicity, vibrato"
        );
        return ExitCode::from(2);
    }
    let sound = match Sound::load(&file, name.unwrap_or_else(|| stem(&file))) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read the file: {e}");
            return ExitCode::from(1);
        }
    };
    println!("Measuring the sound…");
    let subject = Subject::new(sound, family);
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64);
    let session = mxm_listening::session::Session::new(subject, &ops, &device, seed);
    if session.stairs.is_empty() {
        eprintln!("none of these operators applies to this sound");
        return ExitCode::from(1);
    }
    let mut served = serve::Served::new(session, minutes, serve::state_dir(dir.as_deref()));
    served.file = std::fs::canonicalize(&file).ok().map(|p| {
        p.to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('\\', "/")
    });
    match served.run(port) {
        Ok(paths) => {
            for p in paths {
                println!("wrote {}", p.display());
            }
            println!(
                "Run `listen calibrate` to turn every session so far into the owner's thresholds."
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn run_calibrate(args: &[String]) -> ExitCode {
    use mxm_listening::perturb::Subject;
    use mxm_listening::session::{self, Answer};
    let mut dir: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dir" => {
                dir = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            _ => return usage(),
        }
        i += 1;
    }
    let dir = serve::state_dir(dir.as_deref());
    let mut logs: Vec<PathBuf> = std::fs::read_dir(dir.join("sessions"))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    let name = p.to_string_lossy();
                    name.ends_with(".tsv") && !name.ends_with("-outcomes.tsv")
                })
                .collect()
        })
        .unwrap_or_default();
    logs.sort();
    if logs.is_empty() {
        eprintln!("no session logs under {}", dir.join("sessions").display());
        return ExitCode::from(1);
    }
    let literature = mxm_listening::audibility::Thresholds::literature();
    let mut answers: Vec<Answer> = Vec::new();
    for path in &logs {
        let Ok(text) = std::fs::read_to_string(path) else {
            eprintln!("cannot read {}", path.display());
            continue;
        };
        let log = session::parse_log(&text);
        let Some(file) = &log.file else {
            eprintln!(
                "{}: the log names no sound file, so its answers cannot be measured",
                log.date
            );
            continue;
        };
        let sound = match Sound::load(Path::new(file), log.sound.clone()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: cannot read its sound: {e}", log.date);
                continue;
            }
        };
        let family = if log.family.starts_with("percussive") {
            Some(Family::Percussive)
        } else {
            None
        };
        println!(
            "Measuring the {} answers of {}…",
            log.trials.len(),
            log.date
        );
        let subject = Subject::new(sound, family);
        let reference = mxm_listening::describe::describe_with(
            &subject.sound,
            &mxm_listening::describe::Options {
                family: subject.family,
                without_perception: true,
                ..mxm_listening::describe::Options::default()
            },
        );
        // Each distinct change measured once.
        let mut measured: Vec<(String, f64, Option<session::Measured>)> = Vec::new();
        for &(op, size, correct) in &log.trials {
            let key = (op.name(), size);
            let m = match measured.iter().find(|(n, s, _)| *n == key.0 && *s == key.1) {
                Some((_, _, m)) => m.clone(),
                None => {
                    let m = session::measure(&subject, &reference, op, size, &literature);
                    measured.push((key.0, key.1, m.clone()));
                    m
                }
            };
            if let Some(m) = m {
                answers.push(Answer {
                    reading: m.reading.to_string(),
                    kind: m.kind,
                    context: log.family.clone(),
                    delta: m.delta,
                    correct,
                    session: log.date.clone(),
                });
            }
        }
    }
    let (table, lines) = session::table(&answers);
    let out = dir.join("owner-thresholds.tsv");
    if let Err(e) = std::fs::write(&out, &table) {
        eprintln!("cannot write {}: {e}", out.display());
        return ExitCode::from(1);
    }
    for l in lines {
        println!("{l}");
    }
    println!("wrote {} from {} session(s)", out.display(), logs.len());
    ExitCode::SUCCESS
}

fn run_keymap(args: &[String]) -> ExitCode {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut md_out: Option<PathBuf> = None;
    let mut title = String::from("Key map");
    let mut room: Vec<f64> = Vec::new();
    let mut from_name = false;
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1);
        match args[i].as_str() {
            "--md" => {
                md_out = value.map(PathBuf::from);
                i += 1;
            }
            "--title" => {
                if let Some(t) = value {
                    title.clone_from(t);
                }
                i += 1;
            }
            "--expect-from-name" => from_name = true,
            "--room" => {
                room = value
                    .map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect())
                    .unwrap_or_default();
                i += 1;
            }
            other if !other.starts_with("--") => files.push(PathBuf::from(other)),
            _ => return usage(),
        }
        i += 1;
    }
    if files.len() < 3 {
        eprintln!("a key map needs at least three notes");
        return usage();
    }
    let mut sounds = Vec::new();
    for file in &files {
        match Sound::load(file, stem(file)) {
            Ok(s) => sounds.push(s),
            Err(e) => {
                eprintln!("cannot read {}: {e}", file.display());
                return ExitCode::from(1);
            }
        }
    }
    // Notes are described without the perceptual models, which the key map does not read, on as many
    // threads as the machine offers.
    // Each note's expected pitch, where asked: the last dot-separated field of its name, as `split`
    // names its cuts (`Marimba.yarn.mf.C4B4.A4`).
    let describe_all = |room: &[f64]| -> Vec<mxm_listening::reading::Report> {
        let options: Vec<mxm_listening::describe::Options> = sounds
            .iter()
            .map(|s| mxm_listening::describe::Options {
                family: Some(Family::Note),
                room_lines: room.to_vec(),
                without_perception: true,
                expect_hz: if from_name {
                    s.name
                        .rsplit('.')
                        .next()
                        .and_then(mxm_listening::split::note_hz)
                } else {
                    None
                },
                note_off_s: None,
            })
            .collect();
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..threads)
                .map(|t| {
                    let (sounds, options) = (&sounds, &options);
                    scope.spawn(move || {
                        sounds
                            .iter()
                            .enumerate()
                            .filter(|(k, _)| k % threads == t)
                            .map(|(k, s)| {
                                (k, mxm_listening::describe::describe_with(s, &options[k]))
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            let mut all: Vec<_> = handles
                .into_iter()
                .flat_map(|h| h.join().unwrap_or_default())
                .collect();
            all.sort_by_key(|(k, _)| *k);
            all.into_iter().map(|(_, r)| r).collect()
        })
    };
    // Lines under half the notes are the body or the room: found on a first pass, kept out of every
    // note's partials on a second, unless the caller named the room.
    let first = describe_all(&room);
    let recurring = if room.is_empty() {
        let refs: Vec<&mxm_listening::reading::Report> = first.iter().collect();
        mxm_listening::keymap::recurring_lines(&refs)
    } else {
        Vec::new()
    };
    let reports = if recurring.is_empty() {
        first
    } else {
        describe_all(&recurring)
    };
    let refs: Vec<&mxm_listening::reading::Report> = reports.iter().collect();
    let trends = mxm_listening::keymap::trends(&refs);
    let body = mxm_listening::keymap::body(&refs);
    let md = mxm_listening::keymap::markdown(&title, reports.len(), &trends, &body, &recurring);
    print!("{md}");
    if let Some(path) = md_out {
        if let Err(e) = std::fs::write(&path, &md) {
            eprintln!("cannot write {}: {e}", path.display());
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}

fn run_split(args: &[String]) -> ExitCode {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut out: Option<PathBuf> = None;
    let mut held = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                out = args.get(i + 1).map(PathBuf::from);
                i += 1;
            }
            "--held" => held = true,
            other if !other.starts_with("--") => files.push(PathBuf::from(other)),
            _ => return usage(),
        }
        i += 1;
    }
    let Some(out) = out else {
        return usage();
    };
    if let Err(e) = std::fs::create_dir_all(&out) {
        eprintln!("cannot make {}: {e}", out.display());
        return ExitCode::from(1);
    }
    let mut failed = false;
    for file in files {
        let run = match Sound::load_whole(&file, stem(&file)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("cannot read {}: {e}", file.display());
                failed = true;
                continue;
            }
        };
        // Held notes are cut both ways and the cut that names more of its notes kept: a bow's start is
        // soft, and on Iowa's cello the struck cut names more, on its violin the held one.
        let named = |notes: &[Sound]| {
            notes
                .iter()
                .filter(|n| {
                    let name = n.name.rsplit(' ').next().unwrap_or_default();
                    mxm_listening::split::note_hz(name)
                        .and_then(|hz| mxm_listening::split::check_cents(n, hz))
                        .is_some_and(|c| c.abs() <= mxm_listening::split::NAME_TOLERANCE_CENTS)
                })
                .count()
        };
        let cut = match (held, mxm_listening::split::notes(&run)) {
            (true, struck) => {
                let by_pitch = mxm_listening::split::notes_held(&run);
                match (struck, by_pitch) {
                    (Ok(a), Ok(b)) => Ok(if named(&b) > named(&a) { b } else { a }),
                    (a, b) => a.or(b),
                }
            }
            (false, struck) => struck,
        };
        let notes = match cut {
            Ok(n) => n,
            Err(e) => {
                eprintln!("{e}");
                failed = true;
                continue;
            }
        };
        let mut worst = 0.0f64;
        let mut misses: Vec<String> = Vec::new();
        for note in &notes {
            let name = note.name.rsplit(' ').next().unwrap_or_default().to_string();
            let cents = mxm_listening::split::note_hz(&name)
                .and_then(|hz| mxm_listening::split::check_cents(note, hz));
            worst = worst.max(cents.map_or(f64::INFINITY, f64::abs));
            if let Some(c) = cents.filter(|c| c.abs() > mxm_listening::split::NAME_TOLERANCE_CENTS)
            {
                misses.push(format!("{name} {c:+.0} c"));
            }
            let path = out.join(format!("{}.{name}.wav", run.name));
            if let Err(e) = mxm_audio_file::write(
                &path,
                &note.samples,
                1,
                note.rate,
                mxm_audio_file::Target::WavFloat32,
            ) {
                eprintln!("cannot write {}: {e:?}", path.display());
                failed = true;
            }
        }
        println!(
            "{}: {} notes, the furthest from its name {}",
            run.name,
            notes.len(),
            if worst.is_finite() {
                format!("{worst:.0} c")
            } else {
                "unchecked".to_string()
            }
        );
        if !misses.is_empty() {
            println!("  named wrong or cut wrong: {}", misses.join(", "));
            failed = true;
        }
    }
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// A note a caller expects, as a name (`A4`, `Db6`) or in Hz.
fn run_stimulus(args: &[String]) -> ExitCode {
    use mxm_listening::stimulus::{Channel, Kind, Stimulus};
    let Some(kind_name) = args.first() else {
        return usage();
    };
    let Some(kind) = Kind::default_of(kind_name) else {
        eprintln!(
            "unknown stimulus `{kind_name}`: impulse, sweep, bursts, steps, sine, dual-sine, noise, silence or note"
        );
        return ExitCode::from(2);
    };
    let mut s = Stimulus::new(kind);
    let mut out: Option<PathBuf> = None;
    let mut i = 1;
    while i < args.len() {
        let flag = args[i].as_str();
        let Some(value) = args.get(i + 1) else {
            eprintln!("{flag} takes a value");
            return ExitCode::from(2);
        };
        let num = value.parse::<f64>().ok().filter(|v| v.is_finite());
        let need = |v: Option<f64>| -> Result<f64, ExitCode> {
            v.ok_or_else(|| {
                eprintln!("{flag} takes a number, not {value}");
                ExitCode::from(2)
            })
        };
        let result: Result<(), ExitCode> = (|| {
            match flag {
                "--out" => out = Some(PathBuf::from(value)),
                "--rate" => s.rate = need(num)? as u32,
                "--level" => s.level_dbfs = need(num)?,
                "--lead" => s.lead_s = need(num)?,
                "--seconds" => s.seconds = need(num)?,
                "--tail" => s.tail_s = need(num)?,
                "--channel" => {
                    s.channel = Channel::parse(value).ok_or_else(|| {
                        eprintln!("--channel takes both, left or right");
                        ExitCode::from(2)
                    })?;
                }
                "--set" => {
                    let (k, v) = value.split_once('=').ok_or_else(|| {
                        eprintln!("--set takes NAME=VALUE");
                        ExitCode::from(2)
                    })?;
                    s.settings.push((k.to_string(), v.to_string()));
                }
                _ => {
                    let v = need(num);
                    match (&mut s.kind, flag) {
                        (Kind::Sweep { from_hz, .. }, "--from") => *from_hz = v?,
                        (Kind::Sweep { to_hz, .. }, "--to") => *to_hz = v?,
                        (
                            Kind::Bursts { hz, .. }
                            | Kind::Steps { hz, .. }
                            | Kind::Sine { hz }
                            | Kind::DualSine { hz, .. },
                            "--hz",
                        ) => *hz = v?,
                        (Kind::DualSine { hz2, .. }, "--hz2") => *hz2 = v?,
                        (Kind::DualSine { second_db, .. }, "--second-db") => *second_db = v?,
                        (Kind::Bursts { cycles, .. }, "--cycles") => *cycles = v?,
                        (Kind::Bursts { every_s, .. }, "--every") => *every_s = v?,
                        (Kind::Steps { levels_db, .. }, "--levels") => {
                            let parsed: Option<Vec<f64>> =
                                value.split(',').map(|x| x.trim().parse().ok()).collect();
                            *levels_db = parsed.ok_or_else(|| {
                                eprintln!("--levels takes dB values, comma-separated");
                                ExitCode::from(2)
                            })?;
                        }
                        (Kind::Noise { seed }, "--seed") => *seed = v? as u64,
                        (Kind::Note { key, .. }, "--key") => *key = v? as u8,
                        (Kind::Note { velocity, .. }, "--velocity") => *velocity = v? as u8,
                        _ => {
                            eprintln!("{flag} does not apply to a {kind_name} stimulus");
                            return Err(ExitCode::from(2));
                        }
                    }
                }
            }
            Ok(())
        })();
        if let Err(code) = result {
            return code;
        }
        i += 2;
    }
    let Some(out) = out else {
        eprintln!("stimulus needs --out PREFIX");
        return ExitCode::from(2);
    };
    if let Err(e) = s.check() {
        eprintln!("{e}");
        return ExitCode::from(2);
    }
    let with = |ext: &str| {
        let mut p = out.clone().into_os_string();
        p.push(ext);
        PathBuf::from(p)
    };
    if !matches!(s.kind, Kind::Note { .. }) {
        let channels = s.channels();
        let frames = channels[0].len();
        let interleaved: Vec<f32> = (0..frames)
            .flat_map(|n| channels.iter().map(move |c| c[n]))
            .collect();
        let wav = with(".wav");
        if let Err(e) = mxm_audio_file::write(
            &wav,
            &interleaved,
            channels.len() as u16,
            s.rate,
            mxm_audio_file::Target::WavFloat32,
        ) {
            eprintln!("cannot write {}: {e:?}", wav.display());
            return ExitCode::from(1);
        }
        println!("wrote {}", wav.display());
    }
    let sidecar = with(".stimulus.tsv");
    if let Err(e) = std::fs::write(&sidecar, s.sidecar()) {
        eprintln!("cannot write {}: {e}", sidecar.display());
        return ExitCode::from(1);
    }
    println!("wrote {}", sidecar.display());
    ExitCode::SUCCESS
}

fn run_respond(args: &[String]) -> ExitCode {
    use mxm_listening::stimulus::{Channel, Stimulus};
    let mut files: Vec<PathBuf> = Vec::new();
    let mut name: Option<String> = None;
    let mut json_out: Option<PathBuf> = None;
    let mut md_out: Option<PathBuf> = None;
    let mut quiet = false;
    let mut against: Option<PathBuf> = None;
    let mut all = false;
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1);
        match args[i].as_str() {
            "--name" => {
                name = value.cloned();
                i += 1;
            }
            "--against" => {
                against = value.map(PathBuf::from);
                i += 1;
            }
            "--all" => all = true,
            "--json" => {
                json_out = value.map(PathBuf::from);
                i += 1;
            }
            "--md" => {
                md_out = value.map(PathBuf::from);
                i += 1;
            }
            "--quiet" => quiet = true,
            other => files.push(PathBuf::from(other)),
        }
        i += 1;
    }
    if files.is_empty() || !files.len().is_multiple_of(2) || (against.is_some() && files.len() != 2)
    {
        eprintln!(
            "respond takes pairs: a stimulus sidecar, then its response (one pair with --against)"
        );
        return ExitCode::from(2);
    }
    let mut stimuli = Vec::new();
    let mut responses = Vec::new();
    let mut reports = Vec::new();
    for pair in files.chunks(2) {
        let text = match std::fs::read_to_string(&pair[0]) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("cannot read {}: {e}", pair[0].display());
                return ExitCode::from(1);
            }
        };
        let s = match Stimulus::parse(&text) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: {e}", pair[0].display());
                return ExitCode::from(1);
            }
        };
        let (channels, rate) = match mxm_listening::prep::decode_channels(&pair[1]) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("cannot read the response: {e}");
                return ExitCode::from(1);
            }
        };
        let label = match (&name, files.len()) {
            (Some(n), 2) => n.clone(),
            (Some(n), _) => format!("{n} {}", reports.len() + 1),
            (None, _) => stem(&pair[1]),
        };
        match mxm_listening::respond::respond(&s, &channels, rate, &label) {
            Ok(r) => reports.push(r),
            Err(e) => {
                eprintln!("{}: {e}", pair[1].display());
                return ExitCode::from(1);
            }
        }
        stimuli.push(s);
        responses.push(channels);
    }
    if let Some(reference) = against {
        let read = mxm_listening::prep::decode_channels(&reference).and_then(|(channels, rate)| {
            mxm_listening::respond::respond(&stimuli[0], &channels, rate, &stem(&reference))
        });
        let reference = match read {
            Ok(r) => r,
            Err(e) => {
                eprintln!("the reference: {e}");
                return ExitCode::from(1);
            }
        };
        let (thresholds, owner_note) = owner_thresholds();
        let mut comparison = mxm_listening::compare::compare(
            &reference,
            &reports[0],
            &thresholds,
            &mxm_listening::audibility::Vocabulary::owner(),
        );
        comparison.notes.extend(owner_note);
        let mut md = markdown::comparison_to_markdown(&comparison);
        if all {
            md.push_str("\nWithin threshold:\n\n");
            for f in comparison.findings.iter().filter(|f| f.units < 1.0) {
                md.push_str(&format!("- {}\n", markdown::finding_line(f)));
            }
        }
        if let Some(path) = json_out {
            if let Err(e) = std::fs::write(&path, json::comparison_to_json(&comparison)) {
                eprintln!("cannot write {}: {e}", path.display());
                return ExitCode::from(1);
            }
        }
        if let Some(path) = md_out {
            if let Err(e) = std::fs::write(&path, &md) {
                eprintln!("cannot write {}: {e}", path.display());
                return ExitCode::from(1);
            }
        }
        if !quiet {
            print!("{md}");
        }
        return ExitCode::SUCCESS;
    }
    let mut across = Vec::new();
    let find = |side: Channel| stimuli.iter().position(|s| s.channel == side);
    if let (Some(l), Some(r)) = (find(Channel::Left), find(Channel::Right)) {
        if let Some(m) = mxm_listening::respond::matrix(
            (&stimuli[l], &responses[l]),
            (&stimuli[r], &responses[r]),
        ) {
            across.push(m);
        }
    }
    let pairs: Vec<_> = stimuli.iter().zip(&reports).collect();
    if let Some(t) = mxm_listening::respond::against_settings(&pairs) {
        across.push(t);
    }
    if !across.is_empty() {
        reports.push(mxm_listening::Report {
            name: "across the responses".into(),
            rate: stimuli[0].rate,
            duration_s: 0.0,
            family: mxm_listening::family::Detected {
                family: Family::Effect,
                how: "read across every response given".into(),
            },
            onset_s: None,
            sections: across,
            notes: Vec::new(),
        });
    }
    let md: String = reports
        .iter()
        .map(markdown::to_markdown)
        .collect::<Vec<_>>()
        .join("\n");
    if let Some(path) = json_out {
        let body: Vec<String> = reports.iter().map(json::to_json).collect();
        if let Err(e) = std::fs::write(&path, format!("[\n{}\n]\n", body.join(",\n"))) {
            eprintln!("cannot write {}: {e}", path.display());
            return ExitCode::from(1);
        }
    }
    if let Some(path) = md_out {
        if let Err(e) = std::fs::write(&path, &md) {
            eprintln!("cannot write {}: {e}", path.display());
            return ExitCode::from(1);
        }
    }
    if !quiet {
        print!("{md}");
    }
    ExitCode::SUCCESS
}

fn run_explain(args: &[String]) -> ExitCode {
    let Some(query) = args.first() else {
        return usage();
    };
    let mut files: Vec<PathBuf> = Vec::new();
    let mut family: Option<Family> = None;
    let mut expect: Option<f64> = None;
    let mut i = 1;
    while i < args.len() {
        let value = args.get(i + 1);
        match args[i].as_str() {
            "--family" => {
                family = value.and_then(|v| Family::parse(v));
                i += 1;
            }
            "--expect" => {
                expect = expected(value);
                i += 1;
            }
            other => files.push(PathBuf::from(other)),
        }
        i += 1;
    }
    if !files.is_empty() && files.len() != 2 {
        eprintln!("explain takes a phrase, and optionally a reference and a candidate");
        return ExitCode::from(2);
    }
    let meanings = mxm_listening::audibility::Vocabulary::owner().explain(query);
    let words = mxm_listening::audibility::query_words(query);
    let cases: Vec<mxm_listening::golden::Case> = mxm_listening::golden::cases()
        .into_iter()
        .filter(|c| {
            let text = format!("{} {}", c.name, c.phrase).to_lowercase();
            !words.is_empty() && words.iter().all(|w| text.contains(w.as_str()))
        })
        .collect();
    let mut md = format!("# What \"{query}\" has meant\n\n");
    if meanings.is_empty() && cases.is_empty() {
        md.push_str("Nothing yet: no phrase in the vocabulary or the golden cases holds these words. When the owner uses it, add it to `data/vocabulary.tsv`.\n");
    }
    let where_ = |window: Option<f64>, band: Option<f64>| {
        let mut w = Vec::new();
        if let Some(ms) = window {
            w.push(format!("from {ms:.0} ms"));
        }
        if let Some(hz) = band {
            w.push(format!("band from {hz:.0} Hz"));
        }
        w.join(", ")
    };
    for m in &meanings {
        let dir = match m.direction {
            1 => "higher in the candidate",
            -1 => "lower in the candidate",
            _ => "either way",
        };
        let place = where_(m.window, m.band);
        md.push_str(&format!(
            "- `{}`{}, {dir}: \"{}\"\n",
            m.id,
            if place.is_empty() {
                String::new()
            } else {
                format!(" ({place})")
            },
            m.phrase
        ));
    }
    if !cases.is_empty() {
        md.push_str("\nGolden cases:\n\n");
        for c in &cases {
            let expect: Vec<String> = c
                .expect
                .iter()
                .map(|e| match e.band_lo {
                    Some(hz) => format!("{} from {hz:.0} Hz", e.id),
                    None => e.id.clone(),
                })
                .collect();
            md.push_str(&format!(
                "- {} ({}, {}): \"{}\" — {}\n",
                c.name,
                c.date,
                c.source,
                c.phrase,
                expect.join(" or ")
            ));
        }
    }
    if let [reference, candidate] = files.as_slice() {
        let options = mxm_listening::describe::Options {
            family,
            room_lines: Vec::new(),
            without_perception: false,
            expect_hz: expect,
            note_off_s: None,
        };
        let load = |p: &PathBuf| {
            Sound::load(p, stem(p)).map(|s| mxm_listening::describe::describe_with(&s, &options))
        };
        let (r, c) = match (load(reference), load(candidate)) {
            (Ok(r), Ok(c)) => (r, c),
            (Err(e), _) | (_, Err(e)) => {
                eprintln!("cannot read a file: {e}");
                return ExitCode::from(1);
            }
        };
        let (thresholds, _) = owner_thresholds();
        let comparison = mxm_listening::compare::compare(
            &r,
            &c,
            &thresholds,
            &mxm_listening::audibility::Vocabulary::owner(),
        );
        let mut ids: Vec<&str> = meanings.iter().map(|m| m.id.as_str()).collect();
        ids.extend(
            cases
                .iter()
                .flat_map(|c| c.expect.iter().map(|e| e.id.as_str())),
        );
        ids.sort_unstable();
        ids.dedup();
        md.push_str(&format!("\n## In {} against {}\n\n", c.name, r.name));
        let mut any = false;
        for f in comparison.findings.iter().filter(|f| ids.contains(&f.id)) {
            any = true;
            md.push_str(&format!(
                "- {}{}\n",
                markdown::finding_line(f),
                if f.units >= 1.0 {
                    ""
                } else {
                    " — within its threshold"
                }
            ));
        }
        if !any {
            md.push_str("None of these readings was compared: absent in one of the sounds.\n");
        }
    }
    print!("{md}");
    ExitCode::SUCCESS
}

fn expected(value: Option<&String>) -> Option<f64> {
    let v = value?;
    mxm_listening::split::note_hz(v).or_else(|| v.parse::<f64>().ok().filter(|hz| *hz > 0.0))
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| "sound".into(), |s| s.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::site_rows;

    #[test]
    fn site_rows_read_ids_and_labels() {
        let json = r#"[
 {"id": 61, "label": "14s soft (vl2)", "reference": "x", "note": "a \"quoted\" note"},
 {"id": 62, "label": "café \"b\"", "reference": null},
 {"label": "no id"}
]"#;
        assert_eq!(
            site_rows(json),
            vec![
                (61, "14s soft (vl2)".to_string()),
                (62, "café \"b\"".to_string())
            ]
        );
    }
}
