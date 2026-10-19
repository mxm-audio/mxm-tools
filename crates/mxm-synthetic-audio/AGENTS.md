# AGENTS.md — crates/mxm-synthetic-audio

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

Synthetic recordings with exact pitch ground truth, for newDAWn's pitch editor: its analysis is
scored against them (the brief: the clean room's `synthetic-test-audio.md`, 2026-10-19). Nine sets
(legato, wide-vibrato, hum, weak-fundamental, dynamics, re-entry, soft-attack, repeated, and
reverb copies of 1, 4 and 5), each file with its notes, its f0 every 10 ms and a sidecar naming
every note's case and parameters. **It ships nothing into a plugin.** The measurements behind its
rules are in [NOTES.md](NOTES.md).

# Ownership

| Path | Scope |
|---|---|
| `src/vst.rs` | A VST3 instrument rendered offline through mxm-kit's `mxm-vst3-host`: fresh instance per render, notes and parameter values at exact frames, mono out |
| `src/preset.rs` | String Studio VS-3's factory packs read, and a preset written into a saved state |
| `src/strings.rs` | The String Studio engine: voicings, the per-note tuning, the dry settings |
| `src/additive.rs` | The additive engine: instrument profiles and the renderer |
| `src/measure.rs` | Readings of a rendered String Studio note: steady pitch, harmonic levels and share, level, settling |
| `src/dsp.rs`, `src/reverb.rs` | Seeded noise, filters, convolution; set 9's room |
| `src/truth.rs`, `src/json.rs` | Cases, files, the URMP-format truth and the sidecar |
| `src/sets/` | One module per set: what each case plays |
| `src/bin/synthetic-audio.rs` | `render`, and the commands for looking at the model (`params`, `text`, `state`, `preset`, `try`, `sweep`) |

# Local Contracts

- **Output**: `render <newdawn>/examples/audio` writes `synthetic/<slug>.wav` (mono, 32-bit float,
  peak at most -1 dBFS) and `annotations/<slug>/` (`Notes_1_…`, `F0s_1_…`, `<slug>.case.json`),
  slugs `synth-<set>-<instrument>-<nn>`, a 44.1 kHz copy `-44k`, a room copy `-rt05|15|30`, and a
  `synthetic/listing.tsv`. All of it is local, git-ignored in newDAWn, and **never committed
  without the owner's say**.
- **No pitch tool in the chain.** Nothing shifts, stretches or corrects pitch. Only gain, linear
  filters (set 4's harmonic levels, set 6's bleed, set 9's room), a comb that cancels odd
  harmonics (set 4's even-only control) and added noise touch rendered audio.
- **The annotation holds the pitch that sounds** (the clean room, 2026-10-19). Additive: the f0 a
  tone is made from (`additive::render` integrates `k · f0(t)`), so truth is exact by construction.
  String Studio: the note's measured steady pitch, rendered alone.
- **A String Studio note holds one pitch.** It can't follow a pitch change once sounding (NOTES), so
  every glide, overshoot, vibrato and bend is additive. A note is tuned before it sounds (key, the
  voicing's `transpose`, and a static bend), rendered alone, and accepted only steady
  (`STEADY_CENTS`), clean (`noise_limit`, with `measure::harmonic_share`'s band), with its own
  fundamental (`MIN_FUNDAMENTAL_DB`: the octave guard) and near the asked pitch (`ACCEPT_CENTS`,
  else at most `FALLBACK_CENTS`, said on stderr); its onset and end are where its level, shaped,
  first and last come within 40 dB of its peak. **A gate is loosened only with the measurement that
  shows why** (NOTES), never to let a render through. Its F0 is held from onset to end; attack settling and release drift
  are measured into the sidecar, not the truth (the clean room agreed, 2026-10-19).
- **No note is heard being damped**: each String Studio note ends by a gain fade, its note-off sent
  after it (VS-3's damper bends the pitch). Vibrato, unison, arpeggiator, insert effects and every
  reverb are switched off (`DRY`); body, filter, EQ and compressor stay as the preset made them.
- **Everything is seeded.** A file's seed is FNV-1a of its slug without suffixes, every random draw
  a named stream of it (`dsp::Rng::stream`), so a part added never moves another's stream and a
  44.1 kHz copy plays the same cases. String Studio renders bit for bit the same (NOTES).
- **The 44.1 kHz copies are rendered at 44.1 kHz**, both engines, never resampled; which files get
  one is fixed by `copied` in the binary, by instrument and number.
- **A set's parameters are named for what they set** (`h1_under_h2_db`, `dip_db`, …), and what was
  achieved is measured beside them (`measured_…`), so an error can be traced to a parameter.

# Work Guidance

- Look at the model before changing a voicing: `try` renders a preset with timed notes and
  parameter values, `sweep` measures a note's pitch across bends, `MXM_SYNTH_DEBUG=1` prints every
  tuning render, `MXM_SYNTH_DUMP=<wav>` keeps the last one.
- String Studio is Windows-installed software, found at its standard paths (`MXM_VS3`,
  `MXM_VS3_PACK` choose others); the additive sets need nothing installed. The rendering is for
  this machine; the crate still builds everywhere.

# Verification

```bash
cargo test -p mxm-synthetic-audio
cargo clippy -p mxm-synthetic-audio --all-targets -- -D warnings
cargo run --release -p mxm-synthetic-audio -- render <newdawn>/examples/audio [--sets a,b] [--instruments a,b]
```

A render checks itself: a note that can't be tuned, a file over -1 dBFS or outside 20 to 90 s
fails or warns. After a change to an engine, check a file against its truth with an independent
tracker and look at its spectrogram (NOTES, *Checking a render*).

# Child DOX Index

None.
