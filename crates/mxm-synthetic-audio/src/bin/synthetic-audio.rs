//! `synthetic-audio`: renders the synthetic test sets and their ground truth.

use mxm_synthetic_audio::json::Json;
use mxm_synthetic_audio::sets::{self, Context};
use mxm_synthetic_audio::strings::Strings;
use mxm_synthetic_audio::truth::{self, Take};
use mxm_synthetic_audio::vst::{Instrument, Result};
use std::path::{Path, PathBuf};

/// Every set, in the order they are rendered, with its instruments.
const SETS: &[(&str, &[&str])] = &[
    ("legato", &sets::legato::INSTRUMENTS),
    ("wide-vibrato", &sets::vibrato::INSTRUMENTS),
    ("hum", &sets::hum::INSTRUMENTS),
    ("weak-fundamental", &sets::weak::INSTRUMENTS),
    ("dynamics", &sets::dynamics::INSTRUMENTS),
    ("re-entry", &sets::reentry::INSTRUMENTS),
    ("soft-attack", &sets::soft::INSTRUMENTS),
    ("repeated", &sets::repeated::INSTRUMENTS),
];

/// Set 9: files of sets 1, 4 and 5 again in a room, three per reverberation time, the
/// instruments and engines rotated: `(set, instrument, nn, RT60 s)`.
const REVERB: &[(&str, &str, usize, f64)] = &[
    ("legato", "cello", 1, 0.5),
    ("weak-fundamental", "bassoon", 1, 0.5),
    ("dynamics", "clarinet", 1, 0.5),
    ("legato", "violin", 3, 1.5),
    ("weak-fundamental", "viola", 1, 1.5),
    ("dynamics", "violin", 1, 1.5),
    ("legato", "voice", 2, 3.0),
    ("weak-fundamental", "double-bass", 1, 3.0),
    ("dynamics", "voice", 1, 3.0),
];

fn build(set: &str, ctx: &mut Context, instrument: &str) -> Result<Vec<Take>> {
    match set {
        "legato" => sets::legato::files(ctx, instrument),
        "wide-vibrato" => sets::vibrato::files(ctx, instrument),
        "hum" => sets::hum::files(ctx, instrument),
        "weak-fundamental" => sets::weak::files(ctx, instrument),
        "dynamics" => sets::dynamics::files(ctx, instrument),
        "re-entry" => sets::reentry::files(ctx, instrument),
        "soft-attack" => sets::soft::files(ctx, instrument),
        "repeated" => sets::repeated::files(ctx, instrument),
        other => Err(format!("no set {other:?}").into()),
    }
}

fn param(take: &Take, key: &str) -> Option<Json> {
    match &take.params {
        Json::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()),
        _ => None,
    }
}

/// Lays a take out, finishes it as its set asks (hum, a room, its level, a noise floor) and
/// writes it; returns its listing line.
fn finish(root: &Path, mut take: Take, room: Option<f64>) -> Result<String> {
    let mut laid = take.lay_out();
    if take.set == "hum" {
        sets::hum::add_hum(&mut take, &mut laid.audio);
    }
    if let Some(rt60) = room {
        let (wet, params) =
            mxm_synthetic_audio::reverb::apply(&laid.audio, rt60, take.rate, take.seed);
        laid.audio = wet;
        take.params.push("room", params);
    }
    if take.set == "dynamics" {
        sets::dynamics::finish_levels(&mut take, &mut laid);
    } else {
        let gain = sets::normalise(&mut laid.audio, -3.0);
        take.params.push("normalised_gain_db", gain);
        if let Some(Json::Num(floor)) = param(&take, "noise_floor_dbfs") {
            let mut rng = mxm_synthetic_audio::dsp::Rng::stream(take.seed, "noise floor");
            let level = mxm_synthetic_audio::dsp::db_to_gain(floor);
            for v in laid.audio.iter_mut() {
                *v += level * rng.normal();
            }
        }
    }
    truth::write(root, &take, &laid, mxm_synthetic_audio::GENERATOR)?;
    let seconds = laid.audio.len() as f64 / take.rate;
    if !(20.0..=90.0).contains(&seconds) {
        eprintln!(
            "warning: {} is {seconds:.1} s, outside 20 to 90",
            take.slug()
        );
    }
    Ok(format!(
        "{}	{}	{}	{:.1} s	{} Hz	{} notes",
        take.slug(),
        take.set,
        take.instrument,
        seconds,
        take.rate,
        laid.notes.len()
    ))
}

/// Whether a file also gets a copy rendered at 44.1 kHz: a fixed quarter or so of each set, by its
/// instrument's place and its number, so it doesn't depend on what one run renders.
fn copied(instrument_index: usize, nn: usize) -> bool {
    (instrument_index + nn - 1).is_multiple_of(4)
}

/// Renders the sets named (all when none) into `root`, newDAWn's `examples/audio`.
fn render(root: &Path, only_sets: &[String], only_instruments: &[String]) -> Result<()> {
    let mut strings = Strings::load()?;
    let mut listing = Vec::new();
    for &(set, instruments) in SETS {
        if !only_sets.is_empty() && !only_sets.iter().any(|s| s == set) {
            continue;
        }
        for (i, &instrument) in instruments.iter().enumerate() {
            if !only_instruments.is_empty() && !only_instruments.iter().any(|s| s == instrument) {
                continue;
            }
            let takes = build(
                set,
                &mut Context {
                    rate: 48_000.0,
                    strings: &mut strings,
                },
                instrument,
            )?;
            let copies: Vec<usize> = takes
                .iter()
                .map(|t| t.nn)
                .filter(|&nn| copied(i, nn))
                .collect();
            for take in takes {
                let line = finish(root, take, None)?;
                println!("{line}");
                listing.push(line);
            }
            if !copies.is_empty() {
                let takes = build(
                    set,
                    &mut Context {
                        rate: 44_100.0,
                        strings: &mut strings,
                    },
                    instrument,
                )?;
                for mut take in takes.into_iter().filter(|t| copies.contains(&t.nn)) {
                    take.suffix = "-44k".into();
                    take.params.push(
                        "copy_of",
                        format!("synth-{}-{}-{:02}", take.set, take.instrument, take.nn),
                    );
                    take.params.push("rendered_at_44100", true);
                    let line = finish(root, take, None)?;
                    println!("{line}");
                    listing.push(line);
                }
            }
        }
    }
    if only_sets.is_empty() || only_sets.iter().any(|s| s == "reverb") {
        for (i, &(set, instrument, nn, rt60)) in REVERB.iter().enumerate() {
            if !only_instruments.is_empty() && !only_instruments.iter().any(|s| s == instrument) {
                continue;
            }
            let rates: &[f64] = if i % 4 == 0 {
                &[48_000.0, 44_100.0]
            } else {
                &[48_000.0]
            };
            for &rate in rates {
                let takes = build(
                    set,
                    &mut Context {
                        rate,
                        strings: &mut strings,
                    },
                    instrument,
                )?;
                let mut take = takes
                    .into_iter()
                    .find(|t| t.nn == nn)
                    .ok_or("no such file")?;
                take.suffix = format!(
                    "-rt{:02}{}",
                    (rt60 * 10.0).round() as u32,
                    if rate == 44_100.0 { "-44k" } else { "" }
                );
                take.params
                    .push("dry_copy_of", format!("synth-{set}-{instrument}-{nn:02}"));
                let line = finish(root, take, Some(rt60))?;
                println!("{line}");
                listing.push(line);
            }
        }
    }
    let path = root.join("synthetic").join("listing.tsv");
    let mut text = String::from("slug\tset\tinstrument\tlength\trate\tnotes\n");
    if let Ok(old) = std::fs::read_to_string(&path) {
        let new_slugs: Vec<&str> = listing
            .iter()
            .filter_map(|l| l.split('\t').next())
            .collect();
        for line in old.lines().skip(1) {
            if !new_slugs.contains(&line.split('\t').next().unwrap_or("")) {
                text.push_str(line);
                text.push('\n');
            }
        }
    }
    for line in &listing {
        text.push_str(line);
        text.push('\n');
    }
    std::fs::write(path, text)?;
    Ok(())
}

fn main() -> Result<()> {
    mxm_vst3_host::serve_probe();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("render") => {
            // render <examples/audio> [--sets a,b] [--instruments a,b]
            let root = PathBuf::from(args.get(1).ok_or("render <examples/audio>")?);
            let mut sets = Vec::new();
            let mut instruments = Vec::new();
            let mut rest = args[2..].iter();
            while let Some(arg) = rest.next() {
                let list = rest.next().ok_or("a list after the flag")?;
                let names = list.split(',').map(str::to_owned).collect();
                match arg.as_str() {
                    "--sets" => sets = names,
                    "--instruments" => instruments = names,
                    other => return Err(format!("unknown flag {other}").into()),
                }
            }
            render(&root, &sets, &instruments)?;
        }
        Some("sweep") => {
            // sweep <instrument> <hz> <key> <hold> <from cents> <to cents> <step>
            let mut strings = Strings::load()?;
            let note = mxm_synthetic_audio::strings::StringNote {
                hz: args[2].parse()?,
                velocity: 0.8,
                hold: args[4].parse()?,
                tail: 0.2,
                automation: Vec::new(),
                fade: 0.2,
            };
            let (from, to, step): (f64, f64, f64) =
                (args[5].parse()?, args[6].parse()?, args[7].parse()?);
            let mut bend = from;
            while bend <= to + 1e-9 {
                let hz = strings.pitch_at(&args[1], &note, args[3].parse()?, bend, 48_000.0)?;
                println!("{bend:+8.3}	{hz:.4}	{:+.3}", 1200.0 * (hz / note.hz).log2());
                let _ = &mut strings;
                bend += step;
            }
        }
        Some("params") => {
            let instrument = Instrument::load(&PathBuf::from(&args[1]))?;
            let mut instance = instrument.instance(None)?;
            for param in instance.params()? {
                let value = instance.value(param.id)?;
                let text = if param.hidden {
                    String::new()
                } else {
                    instance.text(param.id, value).unwrap_or_default()
                };
                println!(
                    "{:>6}  {:<36} {:>9.4} {:>9.4} {:>9.4} {}{} {}",
                    param.id,
                    param.name,
                    param.min,
                    param.max,
                    value,
                    if param.stepped { "S" } else { "-" },
                    if param.hidden { "H" } else { "-" },
                    text
                );
            }
        }
        Some("text") => {
            // text <bundle> <id> <value or text>...
            let instrument = Instrument::load(&PathBuf::from(&args[1]))?;
            let mut instance = instrument.instance(None)?;
            let id: u32 = args[2].parse()?;
            for arg in &args[3..] {
                match arg.parse::<f64>() {
                    Ok(value) => println!("{value} -> {:?}", instance.text(id, value)?),
                    Err(_) => println!("{arg:?} -> {:?}", instance.parse(id, arg)?),
                }
            }
        }
        Some("state") => {
            let instrument = Instrument::load(&PathBuf::from(&args[1]))?;
            let mut instance = instrument.instance(None)?;
            std::fs::write(&args[2], instance.save_state()?)?;
        }
        Some("preset") => {
            // preset <bundle> <pack> <name> <param id>...
            let instrument = Instrument::load(&PathBuf::from(&args[1]))?;
            let default = instrument.instance(None)?.save_state()?;
            let pack = std::fs::read_to_string(&args[2])?;
            let preset = mxm_synthetic_audio::preset::find(&pack, &args[3])?;
            let state = mxm_synthetic_audio::preset::state_with(&default, &preset)?;
            let mut instance = instrument.instance(Some(&state))?;
            let names: std::collections::HashMap<u32, String> = instance
                .params()?
                .into_iter()
                .map(|param| (param.id, param.name))
                .collect();
            for id in &args[4..] {
                let id: u32 = id.parse()?;
                let value = instance.value(id)?;
                println!(
                    "{id:>6} {:<36} {value:.4} {}",
                    names[&id],
                    instance.text(id, value)?
                );
            }
            let saved = instance.save_state()?;
            let text = String::from_utf8_lossy(&saved);
            let at = text.find("programName").unwrap_or(0);
            println!("{}", &text[at..(at + 40).min(text.len())]);
        }
        Some("try") => {
            // try <bundle> <pack> <preset> <out.wav> <rate> <seconds> <time>:<on|off|p<id>>=<value>...
            use mxm_synthetic_audio::vst::Event;
            let instrument = Instrument::load(&PathBuf::from(&args[1]))?;
            let default = instrument.instance(None)?.save_state()?;
            let pack = std::fs::read_to_string(&args[2])?;
            let mut preset = mxm_synthetic_audio::preset::find(&pack, &args[3])?;
            let rate: f64 = args[5].parse()?;
            let frames = (args[6].parse::<f64>()? * rate) as usize;
            let mut events = Vec::new();
            for spec in &args[7..] {
                if let Some(setting) = spec.strip_prefix("set:") {
                    let (name, value) = setting.split_once('=').ok_or("set:G/N=v")?;
                    let (group, name) = name.split_once('/').ok_or("set:G/N=v")?;
                    preset.set(group, name, value.parse()?)?;
                    continue;
                }
                let (time, rest) = spec.split_once(':').ok_or("time:what=value")?;
                let (what, value) = rest.split_once('=').ok_or("time:what=value")?;
                let frame = (time.parse::<f64>()? * rate).round() as u64;
                events.push(match what {
                    "on" => Event::NoteOn {
                        frame,
                        key: value.parse()?,
                        velocity: 0.8,
                    },
                    "off" => Event::NoteOff {
                        frame,
                        key: value.parse()?,
                    },
                    _ => Event::Param {
                        frame,
                        id: what.trim_start_matches('p').parse()?,
                        value: value.parse()?,
                    },
                });
            }
            let state = mxm_synthetic_audio::preset::state_with(&default, &preset)?;
            let mut instance = instrument.instance(Some(&state))?;
            let audio = instance.render(rate, frames, &events)?;
            mxm_audio_file::write(
                &args[4],
                &audio,
                1,
                rate as u32,
                mxm_audio_file::Target::WavFloat32,
            )?;
        }
        _ => eprintln!("usage: synthetic-audio render|params|text|state|preset|try ..."),
    }
    Ok(())
}
