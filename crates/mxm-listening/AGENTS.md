# AGENTS.md — crates/mxm-listening

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

The listener. It takes a sound and says, part by part, what the sound does; it takes two and says
what differs, ranked by how audible each difference is, in the owner's words. **It ships nothing.**

It exists because the agents that tune the collection cannot listen. **Status: finished**, slice by
slice, in the owner's order: percussion first, other physically modelled sounds second, then synth
voices and effects. Why, the plan and what each slice built:
[NOTES.md § Why the listener exists](NOTES.md#why-the-listener-exists-and-how-it-was-built).

# Ownership

Each path's full scope: [NOTES.md § Ownership, in full](NOTES.md#ownership-in-full).

| Path | Scope |
|---|---|
| `src/prep.rs` | Decode, onset, trim, K-weighting and body loudness — the drum A/B page's own definitions, which the page calls; whole-file decode; `Kept` |
| `src/split.rs` | Cutting a run of notes into single notes, each checked against its name |
| `src/numeric/` | `Complex`, one-sided Jacobi SVD, Hessenberg–Francis QR, Householder least squares |
| `src/repr/` | Band-passes, envelopes, spectra and their statistics; `fir`, `modes`, `resample`, `demod`, `lines` |
| `src/stimulus.rs`, `src/respond.rs` | Stimuli and their sidecars; a response read against its stimulus |
| `src/perception/` | ECMA-418-2: `hearing`, `sottek`, `roughness`, `fluctuation`, sharpness in `mod` |
| `src/family.rs`, `src/objects.rs` | The sound's family; the ideal objects a note is read against |
| `src/parts/` | One analyser per part of the sound, `attack` to `words` |
| `src/set.rs`, `src/keymap.rs` | Sets of round robins and their room lines; key tracking |
| `src/describe.rs` | `describe`, at once or in two stages |
| `src/name.rs`, `src/glossary.rs`, `data/glossary.tsv`, `src/curves.rs` | A name's claims; what every id means; what a window draws |
| `src/audibility.rs`, `data/thresholds.tsv`, `data/vocabulary.tsv` | Thresholds with sources, the owner's laid over; the owner's phrases |
| `src/compare.rs`, `src/unexplained.rs`, `src/golden.rs`, `data/golden.tsv` | Findings and ranking; the auditory map of unexplained differences; the golden cases |
| `src/resynth.rs` | The self-test's rebuild; `tracks`, public for `mxm-model-drums-dsp` |
| `src/perturb.rs`, `src/staircase.rs`, `src/session.rs` | Perturbations, staircases, calibration sessions and `calibrate` |
| `src/bin/listen/`, `data/session.html` | The `listen` command (`main.rs`) and the session page's server (`serve.rs`) |
| `src/reading.rs`, `src/report/` | `Reading`, `Section`, `Table`, `Report`; reports as hand-written JSON and Markdown |
| `tests/` | Closed forms and synthetic ground truth, one suite per area |

# Local Contracts

Each rule's reasons and measurements: [NOTES.md § Local contracts, in full](NOTES.md#local-contracts-in-full).

**What lives here** ([detail](NOTES.md#what-lives-here-and-what-it-depends-on))
- **Interpretation lives here; rulers live in `mxm-measure`** (named computations with units, never
  a threshold); `dsp-lab` holds harnesses. A computation a second crate needs whose answer does not
  depend on what is acceptable moves to `mxm-measure` under its extraction gate; filter banks and
  estimators stay here.
- **Never in a shipped graph.** Every other crate takes this one as a dev-dependency, except the
  unshipped `apps/mxm-listener-hud`; shipping that is the owner's deferred decision, and the leak
  check in mxm-kit's `crates/mxm-measure/AGENTS.md` (which covers both names) forces it first. `mxm-measure` and
  `mxm-classic-verb-fit` (its `analyse` only: an impulse response is never fitted here) are normal
  dependencies.
- **No external crate** (the owner's choice): hand-rolled numerics, hand-written JSON, plain TSV.
- **The A/B page's definitions are part of the contract.** `prep.rs` is the page's decode, trim,
  K-weighting and body gating, and the page calls it. Changing it, or any reading's definition, is
  a **correction**, measured on every page row and approved, never a tidy-up.

**Readings** ([onsets](NOTES.md#onsets-readings-and-the-decay-envelope), [modes](NOTES.md#modes-the-rest-pitch-and-the-drum-readings))
- One onset per family: a hit's is the first sample at `prep::ONSET_FRACTION` of the peak, and every
  window is measured from it. Each sound is read from its own onset and level; no waveform alignment.
- **Never NaN; absent rather than zero.** A value is finite, or absent with the reason. A non-finite
  buffer is not measured; silence is reported, not measured; under −100 dB a band is numerically silent.
- Every reading states its window (ms from the onset), band, resolution, validity (`BelowFloor`,
  `Sample`) and the source of its definition.
- The decay envelope is one period long; fall times count a lasting fall. Texture and modulation
  readings of one render are `Validity::Sample`, compared across round robins, never trusted alone.
- Modes follow `research:listening/modal-estimation.md`: anticausal FIR filters only, at least 120
  samples per fit, a mode believed only when it recurs in another window; each filter stage spans at
  most `FILTER_SHARE` of its window, and each window's modes are refitted together (`modes::refit`).
- The rest pitch is the lowest believed mode within 12 dB of the strongest, **never a room line**
  (`--room`, or `set`). The glide is a line track. A line narrower than its window resolves is
  `Unresolved`. Definitions kept for continuity state their reach.

**The perceptual models** ([detail](NOTES.md#the-perceptual-models))
- From ECMA-418-2's text (4th edition); no implementation opened. Deviations from its calibration
  points are recorded on every reading, never calibrated away. Moore–Glasberg (ISO 532-3) is not built.
- Each sound is heard as the A/B page plays it (body loudness −20 dB K-weighted, full-scale sine =
  94 dB SPL); a gain alone changes no perceptual reading (tested).
- For a hit, the time-dependent values, never ECMA's single values; fluctuation strength reads a held
  note only. `Options::without_perception` leaves the models out.

**Comparison and the self-test** ([detail](NOTES.md#comparison-the-unexplained-map-and-the-self-test))
- A difference is measured in audibility thresholds (`data/thresholds.tsv`, each with its source). A
  finding an EQ or a fader could make is **ranked lower, never hidden**. Against a set, a difference
  is established only above its threshold and the takes' spread, against two thirds of them.
- What no reading explains is reported, through the auditory map.
- The self-test rebuilds a sound from its readings; read its findings knowing its limits. A note, a
  held note or a voice is rebuilt from a note's analysis (`selftest --family note`).

**Golden cases, calibration, sessions** ([detail](NOTES.md#golden-cases-calibration-and-the-session))
- Golden cases are data, never recordings: logical names, and a local map, never committed, names
  the files. A miss is reported, not tuned away.
- Calibration (odd-one-out, staircases, `calibrate`'s maximum-likelihood fit over every answer) is
  proved on a simulated listener before the owner sits. Sessions on notes are the owner's to sit.
- **The owner's thresholds stay local** in git-ignored `.listening/`. Precedence: the owner's in the
  sound's family, then the owner's in any, then the literature's.
- **The session server binds 127.0.0.1 only**, one request at a time, serving only the page, the
  pending trial's sounds and the answers; never published. A session's length is the owner's to set.
- `site` writes `listen/` beside the page, never into the repository. **A report names a sound,
  never a path.**

**Decoding, runs and notes** ([runs](NOTES.md#decoding-a-hit-and-cutting-a-run), [notes](NOTES.md#pitched-notes), [held](NOTES.md#sustained-notes-guitars-and-electric-pianos), [laws](NOTES.md#laws-objects-note-comparison-and-key-tracking))
- A hit decodes to 30 s at 48 kHz (`prep::decode_mono`, the page's limit; `Sound::load_kept` says
  what was kept). A run decodes whole (`Sound::load_whole`), and a longer file is refused, not cut.
- A run is cut by its own names (`split`), each cut checked against `NAME_TOLERANCE_CENTS`; a roll is
  not cut. **Cuts are derived audio: `target/`, never a commit.**
- **A pitched note is declared, never detected** (`--family note`; `sustained`, `voice` likewise);
  detecting would move drums out of the percussive family. A caller may name the note (`--expect`).
- A note's fundamental, virtual pitch, string series, two-stage decay, held-note `sustain`, laws and
  nearest object are defined in NOTES.md. The partial table is the reading, the object a summary;
  two notes compare partial by partial; `keymap` fits a Theil–Sen line per reading.

**Stimuli, responses and voices** ([detail](NOTES.md#stimuli-responses-and-effects), [voice](NOTES.md#a-synth-voice))
- A stimulus is rendered from its sidecar alone; `respond` refuses a sidecar whose fingerprint this
  build does not reproduce. Only the `setting NAME VALUE` lines may be edited.

**For the listener's window** ([detail](NOTES.md#for-the-listeners-window), [words](NOTES.md#words-and-phrases))
- A reading in stages (`describe_staged`, `Staged::perceive`) is the reading made at once.
- **A name is a claim, never a reading.** `name::check` suggests a family, never applies it; a name's
  note becomes `expect_hz` only when the owner asks.
- Every reading, table and curve id has a `data/glossary.tsv` row, and every row an id in the
  source; `tests/glossary.rs` scans `src/`, so ids are `&'static str`, never made at runtime.
- Curves are read-outs under the readings' contracts, from the report's own onset.
- Words for the timbre are read, never compared: `compare` leaves `words.*` out.

**Research boundary and clean room** ([detail](NOTES.md#research-boundary-clean-room-and-platform))
- **No recording, derived audio, per-file report, recording path or stimulus made from a recording
  is committed.** Automated tests use synthetic signals only, never a recording or the research
  checkout; recording-backed runs are local and manual.
- Every technique is written from its paper or standard and cited at the top of its file; no
  existing implementation is opened.
- Verified on Windows; Linux and macOS are checked later, together, and by CI on all
  three on `v*` tags. Nothing here may be Windows-only.

# Work Guidance

- Measure with this crate before asking the owner what a sound does, and read the whole report: a
  difference the owner heard is usually already in it. On an A/B page, run `listen site` before the
  owner listens. The guide's traps (`docs/drum-model-fitting.md` in the
  [mxm-drum-machine](https://github.com/mxm-audio/mxm-drum-machine/blob/main/docs/drum-model-fitting.md)
  repository) are why the readings carry windows, resolutions and validity.
- A new reading, table or curve takes its row in `data/glossary.tsv` in the same change: what it
  tells the owner about the sound, in plain words.
- When the owner uses a new phrase, add it to `data/vocabulary.tsv` and to the guide's §7 table, and
  the case to `data/golden.tsv`. Before acting on one, run `listen explain "<phrase>"` with the two
  sounds: what it has meant, and those readings' differences in them.
- A new measure comes with a closed-form or synthetic test whose answer is known by construction,
  and names its source.
- A tolerance in a test is an argument about the measure's resolution; write the reason beside it.
- Levenberg–Marquardt arrives with its first user, not before; L1 and L2 needed none (the glide is a
  line track, not a fitted law).

# Verification

```bash
cargo test -p mxm-listening --release          # closed forms and synthetic ground truth
cargo clippy -p mxm-listening --all-targets
cargo +1.87.0 test -p mxm-listening --release  # the MSRV floor, library and tests
cargo run -p mxm-listening --release --bin listen -- describe <file> [--room HZ,HZ] [--family note]
cargo run -p mxm-listening --release --bin listen -- set <manifest.tsv>
cargo run -p mxm-listening --release --bin listen -- compare <original> <render>
cargo run -p mxm-listening --release --bin listen -- selftest <file>
cargo run -p mxm-listening --release --bin listen -- golden --files <local map.tsv>
cargo run -p mxm-listening --release --bin listen -- site <site>
cargo run -p mxm-listening --release --bin listen -- session <file> [--operators pitch,decay,band-3200-6400]
cargo run -p mxm-listening --release --bin listen -- session <note> --family note --operators tilt,decay-law,inharmonicity
cargo run -p mxm-listening --release --bin listen -- calibrate
cargo run -p mxm-listening --release --bin listen -- split <run>… [--held] --out target/tuned-notes/<set>
cargo run -p mxm-listening --release --bin listen -- keymap <note>… [--title T] [--md OUT]
cargo run -p mxm-listening --release --bin listen -- describe <voice.wav> --family voice [--off S]
cargo run -p mxm-listening --release --bin listen -- explain "rings a little long" [<original> <render>]
cargo run -p mxm-listening --release --bin listen -- stimulus sweep --out target/stim/sweep [--set cutoff_hz=1000]
cargo run -p mxm-listening --release --bin listen -- respond target/stim/sweep.stimulus.tsv <response.wav> [--against <reference.wav>]
```

**Release-only suites.** `calibration` (the owner, 2026-09-30) and every suite that took over 30 s
in the merge gate — `sweep`, `note`, `respond`, `compare`, `voice`, `describe`, `parts_l1`,
`glossary`, `resynth`, `perception` (the owner, 2026-10-05) — are ignored with the reason
`release gate`, so a merge runs only the listener's quick tests. `scripts/merge_gate.py --full` (the
monorepo's, now in the private archive) ran them. Since the split there is no merge gate: a plain
`cargo test` runs the quick tests, and these run by hand before a release or while changing what
they cover:

```bash
cargo test -p mxm-listening --release --test <suite> -- --ignored
```

The page equivalence is re-proved whenever `src/prep.rs` changes: render
`drum_machine_ab_page` (an example of `mxm-drum-machine-dsp`, run in the mxm-drum-machine repository)
into a scratch folder before and after with the same mapping and compare every file's hash.

# Child DOX Index

No child AGENTS.md files.
