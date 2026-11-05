//! The ring's lines against lines known by construction: a sine over noise reads at its frequency and
//! its share of the span's power; a candidate lacking the reference's lines is heard lacking them,
//! though it holds as much in every octave at other pitches; the same lines over other noise are not.

use mxm_listening::audibility::{Thresholds, Vocabulary};
use mxm_listening::compare::compare;
use mxm_listening::describe::{Options, describe_with};
use mxm_listening::parts::{Context, tone};
use mxm_listening::{Family, Report, Sound};

const TAU: f64 = std::f64::consts::TAU;
const RATE: u32 = 48_000;

fn noise(n: usize, seed: u64) -> Vec<f64> {
    let mut s = mxm_rng::Xorshift64Star::from_state(seed);
    (0..n).map(|_| s.bipolar_f64()).collect()
}

/// A struck ring, 3.2 s: a click, white noise at `noise_rms`, and steady sines (Hz, amplitude).
fn ring(lines: &[(f64, f64)], noise_rms: f64, seed: u64) -> Vec<f32> {
    let rate = f64::from(RATE);
    let n = (3.2 * rate) as usize;
    // Uniform noise on ±1 has an RMS of 1/√3.
    let hiss = noise(n, seed);
    let mut x: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f64 / rate;
            let tone: f64 = lines
                .iter()
                .enumerate()
                .map(|(k, &(hz, a))| a * (TAU * hz * t + k as f64).sin())
                .sum();
            (tone + noise_rms * 3f64.sqrt() * hiss[i]) as f32
        })
        .collect();
    // The strike: one loud sample sets the onset at the start.
    x[0] = 1.0;
    x
}

fn table(x: &[f32], id: &str) -> Vec<Vec<f64>> {
    let c = Context::new(x, RATE).expect("a sound");
    tone::read(&c)
        .tables
        .into_iter()
        .filter(|t| t.id == id)
        .flat_map(|t| t.rows)
        .map(|r| r.into_iter().map(|v| v.expect("present")).collect())
        .collect()
}

#[test]
fn a_sine_over_noise_reads_at_its_frequency_and_share() {
    // Two sines of 0.1 and 0.05 over noise of RMS 0.01: powers 0.005, 0.00125 and 0.0001.
    let x = ring(&[(2181.3, 0.1), (3366.0, 0.05)], 0.01, 7);
    let total = 0.005 + 0.00125 + 0.0001;
    let lines = table(&x, "tone.ring_lines");
    for (hz, power) in [(2181.3, 0.005), (3366.0, 0.00125)] {
        let want = 10.0 * f64::log10(power / total);
        for span in tone::RING_MS {
            let line = lines
                .iter()
                .filter(|r| r[0] == span.0 && r[1] == span.1)
                .find(|r| (r[2] - hz).abs() < 0.2)
                .unwrap_or_else(|| panic!("no line at {hz} Hz in {span:?}: {lines:?}"));
            // Within half a dB: the span's power is read through its window, and the noise's own
            // power in it varies by a few hundredths.
            assert!(
                (line[3] - want).abs() < 0.5,
                "{hz} Hz in {span:?}: {} dB, want {want:.2}",
                line[3]
            );
        }
    }
    // Nothing else stands over white noise's floor as a line.
    assert!(
        lines
            .iter()
            .all(|r| (r[2] - 2181.3).abs() < 0.2 || (r[2] - 3366.0).abs() < 0.2),
        "{lines:?}"
    );
}

fn report_as(x: Vec<f32>, name: &str, family: Family) -> Report {
    describe_with(
        &Sound::new(name, RATE, x),
        &Options {
            family: Some(family),
            without_perception: true,
            ..Options::default()
        },
    )
}

fn report(x: Vec<f32>, name: &str) -> Report {
    report_as(x, name, Family::Percussive)
}

/// Each `id` finding (audible or not): the middle of its band, and candidate minus reference.
fn findings_as(
    reference: Vec<f32>,
    candidate: Vec<f32>,
    family: Family,
    id: &str,
) -> (Vec<(f64, f64, bool)>, Vec<String>) {
    let c = compare(
        &report_as(reference, "reference", family),
        &report_as(candidate, "candidate", family),
        &Thresholds::literature(),
        &Vocabulary::owner(),
    );
    let found = c
        .findings
        .iter()
        .filter(|f| f.id == id)
        .map(|f| {
            (
                f.band_hz.map_or(0.0, |b| 0.5 * (b.0 + b.1)),
                f.candidate - f.reference,
                f.units >= 1.0,
            )
        })
        .collect();
    (found, c.notes)
}

fn line_findings(reference: Vec<f32>, candidate: Vec<f32>) -> Vec<(f64, f64)> {
    let c = compare(
        &report(reference, "reference"),
        &report(candidate, "candidate"),
        &Thresholds::literature(),
        &Vocabulary::owner(),
    );
    c.audible()
        .filter(|f| f.id == "tone.line_level")
        .map(|f| {
            (
                f.band_hz.map_or(0.0, |b| 0.5 * (b.0 + b.1)),
                f.candidate - f.reference,
            )
        })
        .collect()
}

/// A note's partials, 440 Hz and its harmonics to the fifth, each moved up 4 Hz: past the narrow
/// match of every one but the fifth (±1.5 Hz, or ±0.2 % = 4.4 Hz at 2200 Hz), as a table's
/// rounding moves them. Read as a note, every line is there, its pitch moved by the cents the
/// 4 Hz make; read as a drum, as before, the lower four are missing.
#[test]
fn a_note_s_line_moved_a_little_reads_as_moved_not_missing() {
    let partials: Vec<(f64, f64)> = (1..=5)
        .map(|n| (440.0 * n as f64, 0.1 / n as f64))
        .collect();
    let moved: Vec<(f64, f64)> = partials.iter().map(|&(hz, a)| (hz + 4.0, a)).collect();
    let reference = ring(&partials, 0.001, 7);
    let candidate = ring(&moved, 0.001, 7);

    let (levels, notes) = findings_as(
        reference.clone(),
        candidate.clone(),
        Family::Note,
        "tone.line_level",
    );
    assert!(
        levels.iter().all(|&(_, d, audible)| !audible || d > -10.0),
        "a moved line read as missing in a note: {levels:?}"
    );
    assert!(
        !notes
            .iter()
            .any(|n| n.starts_with("lines the reference rings")),
        "{notes:?}"
    );
    let (tunings, _) = findings_as(
        reference.clone(),
        candidate.clone(),
        Family::Note,
        "tone.line_tuning",
    );
    for &(hz, _) in &partials {
        let want = 1200.0 * ((hz + 4.0) / hz).log2();
        // Within a cent: the ring's lines are read to about 0.2 Hz (the test above), under a cent
        // at 440 Hz and less above it.
        assert!(
            tunings
                .iter()
                .any(|&(at, d, _)| (at - hz).abs() < 0.03 * hz && (d - want).abs() < 1.0),
            "{hz} Hz: want +{want:.2} cents in {tunings:?}"
        );
    }

    let (drum, _) = findings_as(reference, candidate, Family::Percussive, "tone.line_level");
    for &(hz, _) in &partials[..4] {
        assert!(
            drum.iter()
                .any(|&(at, d, audible)| (at - hz).abs() < 5.0 && audible && d < -10.0),
            "{hz} Hz no longer missing for a drum: {drum:?}"
        );
    }
}

#[test]
fn lines_at_other_pitches_do_not_stand_in_for_the_reference_s() {
    // The reference rings 2181 and 3366 Hz; the candidate as much in the same octaves at 2400 and
    // 3000 Hz, over the same noise.
    let reference = ring(&[(2181.3, 0.1), (3366.0, 0.05)], 0.01, 7);
    let candidate = ring(&[(2400.0, 0.1), (3000.0, 0.05)], 0.01, 7);
    let found = line_findings(reference, candidate);
    for hz in [2181.3, 3366.0] {
        assert!(
            found
                .iter()
                .any(|&(at, d)| (at - hz).abs() < 5.0 && d < -10.0),
            "{hz} Hz not heard missing: {found:?}"
        );
    }
}

#[test]
fn the_same_lines_over_other_noise_are_not_heard() {
    let reference = ring(&[(2181.3, 0.1), (3366.0, 0.05)], 0.01, 7);
    let candidate = ring(&[(2181.3, 0.1), (3366.0, 0.05)], 0.01, 99);
    let found = line_findings(reference, candidate);
    assert!(found.is_empty(), "{found:?}");
}
