//! Scores `mxm-creative-sampler`'s pitch detector against tones of exactly known pitch.
//!
//! **The collection renders its own ground truth.** A sample pack's key tag names a pitch class,
//! with no octave and no guarantee beyond what its maker typed; here the note is chosen before the
//! audio exists, so an error is the detector's and nothing else's. The waveform is
//! `mxm-mono-01-dsp`'s oscillator rather than a textbook sine, because an antialiased saw with a
//! sub is the kind of thing a sampler is actually given and a sine is the one case every detector
//! gets right.
//!
//! The awkward cases are here on purpose. An 808 glides its pitch down over the first few hundred
//! milliseconds, and a bass phrase does not start on its key — those are what
//! `plugins/mxm-creative-sampler/examples/measure_root.rs` found the detector failing on against a
//! real pack, and a synthetic case that reproduces a real failure is what lets a fix be checked
//! without the pack.

use mxm_creative_sampler_dsp::{Sample, detect_root};
use mxm_mono_01_dsp::oscillator::{MixLevels, Oscillator, SubShape};

const RATE: f32 = 48_000.0;

const NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

fn note_name(note: f32) -> String {
    let n = note.round().clamp(0.0, 127.0) as i32;
    format!("{}{}", NAMES[(n % 12) as usize], n / 12 - 1)
}

fn hz(note: f32) -> f32 {
    440.0 * ((note - 69.0) / 12.0).exp2()
}

/// A tone whose pitch is `note` for its whole length.
fn steady(note: f32, seconds: f32, levels: MixLevels) -> Sample {
    render(seconds, levels, |_| hz(note))
}

/// An 808's pitch envelope: starts `drop` semitones sharp and settles onto `note`.
///
/// The shape is a falling exponential, which is what a drum synthesiser's pitch envelope is and
/// what makes the first few hundred milliseconds unrepresentative of the note the sample is called.
fn glide(note: f32, drop: f32, tau_s: f32, seconds: f32, levels: MixLevels) -> Sample {
    render(seconds, levels, move |t| {
        hz(note + drop * (-t / tau_s).exp())
    })
}

/// Two notes in sequence, the first not the one the file would be named for.
fn phrase(first: f32, then: f32, seconds: f32, levels: MixLevels) -> Sample {
    render(seconds, levels, move |t| {
        hz(if t < seconds * 0.25 { first } else { then })
    })
}

fn render(seconds: f32, levels: MixLevels, mut pitch: impl FnMut(f32) -> f32) -> Sample {
    let frames = (seconds * RATE) as usize;
    let mut osc = Oscillator::new();
    // A short attack, so the detector meets an onset rather than a step — a step is a click, and a
    // click is broadband, which is exactly the thing that makes a first window unreliable.
    let attack = (0.005 * RATE) as usize;
    Sample::new(
        (0..frames)
            .map(|n| {
                let t = n as f32 / RATE;
                let value = osc.process(pitch(t), 0.5, SubShape::Oct1Square, &levels, RATE);
                let gain = (n as f32 / attack as f32).min(1.0);
                let value = value * gain * 0.8;
                [value, value]
            })
            .collect(),
        RATE,
    )
    .expect("a rendered tone")
}

fn main() {
    let saw = MixLevels {
        saw: 1.0,
        pulse: 0.0,
        sub: 0.0,
        noise: 0.0,
    };
    let bass = MixLevels {
        saw: 0.7,
        pulse: 0.3,
        sub: 0.3,
        noise: 0.0,
    };

    let mut cases: Vec<(String, f32, Sample)> = Vec::new();

    // The playable range, on a plain saw. A sampler is given C1 to C7 far more often than either
    // extreme, and every one of these should be exact.
    for note in [
        24.0f32, 31.0, 36.0, 43.0, 48.0, 55.0, 60.0, 67.0, 72.0, 84.0, 96.0,
    ] {
        cases.push((
            format!("saw {}", note_name(note)),
            note,
            steady(note, 1.0, saw),
        ));
    }

    // A bass voice: saw, pulse and a sub an octave down. The sub is the trap — it is a real
    // component of the sound an octave below the note, and a detector that follows energy rather
    // than period reports it.
    for note in [28.0f32, 33.0, 40.0] {
        cases.push((
            format!("bass+sub {}", note_name(note)),
            note,
            steady(note, 1.0, bass),
        ));
    }

    // 808s: the pitch envelope that made the real pack's F read as G.
    for (drop, tau) in [(7.0f32, 0.08f32), (12.0, 0.15), (4.0, 0.25)] {
        let note = 33.0;
        cases.push((
            format!("808 {} +{drop:.0}st over {tau:.2}s", note_name(note)),
            note,
            glide(note, drop, tau, 1.2, bass),
        ));
    }

    // A phrase whose first quarter is not the key.
    // **A phrase is rooted on the note it starts on**, which is the owner's rule: a loop's Root is
    // what its first note is, not whichever note it spends longest on.
    cases.push((
        "phrase G1 then C2".to_owned(),
        31.0,
        phrase(31.0, 36.0, 1.2, bass),
    ));

    println!(
        "{:<32} {:>8} {:>10} {:>9}  verdict",
        "case", "wanted", "detected", "error"
    );
    let (mut exact, mut close, mut wrong, mut declined) = (0, 0, 0, 0);
    for (name, want, sample) in &cases {
        match detect_root(sample, 0.0) {
            None => {
                declined += 1;
                println!(
                    "{name:<32} {:>8} {:>10} {:>9}  DECLINED",
                    note_name(*want),
                    "-",
                    "-"
                );
            }
            Some(found) => {
                let cents = (found - want) * 100.0;
                let verdict = if cents.abs() < 50.0 {
                    exact += 1;
                    "ok"
                } else if cents.abs() < 150.0 {
                    close += 1;
                    "near"
                } else {
                    wrong += 1;
                    "WRONG"
                };
                println!(
                    "{name:<32} {:>8} {:>10} {cents:>+8.0}c  {verdict}",
                    note_name(*want),
                    note_name(found)
                );
            }
        }
    }
    println!(
        "\n{exact} exact, {close} within a semitone, {wrong} wrong, {declined} declined, of {} cases",
        cases.len()
    );
}
