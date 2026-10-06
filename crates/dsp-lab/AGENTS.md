# AGENTS.md — crates/dsp-lab

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

Measurement harnesses for the `docs/` references, which are mxm-kit's since the split. **This crate
ships nothing.**

It exists so the evidence behind the long-form references lives *in the repository* and can be
reproduced from a clone, rather than in a scratch project on one machine. Every measured number in
[`docs/oscillators/`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/oscillators/README.md) and
[`docs/modulation/`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/modulation/README.md) comes from here.

# Ownership

| Path | Scope |
|---|---|
| `examples/osc_spike.rs` | Every measured number in `docs/oscillators/` — twenty sections. §9f, the sample-reading grain cost ladder, is the newest |
| `examples/mod_spike.rs` | Every measured number in `docs/modulation/` — four sections |
| `src/lib.rs` | **Documentation only.** The numeric core it used to hold — `N`, `fft`, `ifft`, `princarg`, `periods_for`, `a_weight`, `spectral_flatness_db` — moved to [`../mxm-measure`](https://github.com/mxm-audio/mxm-kit/blob/main/crates/mxm-measure/AGENTS.md); the file records where it went and why |

Does **not** own the shipped DSP, or anything that verifies it. `mono_01_filter_spike.rs`,
`resonance_gain.rs` and `mono_01_render_demo.rs` stay in
[`../mxm-mono-01-dsp/`](https://github.com/mxm-audio/mxm-mono-01/blob/main/crates/mxm-mono-01-dsp/AGENTS.md) — see the split below.

# Local Contracts

## This is not a shared-DSP crate

The root contract (the monorepo's; now *Don't pre-generalise* in mxm-kit's
[`docs/collection-rules.md`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/collection-rules.md))
says `crates/<plugin>-dsp` stays per-plugin "until a second instrument
demonstrates a genuinely shared API" and warns against pre-generalising. **That rule is untouched
by this crate and this crate is not an exception to it.** No shipped DSP lives here and none may.
`oscillator.rs` and `filter.rs` stay in `mxm-mono-01-dsp` until a second instrument exists to shape a
shared API against.

What moved here is *measurement code for collection-level documentation*, which was never
plugin-specific in the first place.

## What belongs here, and what does not

The test is **what a harness serves**, not what it measures:

- Serves a collection-level reference in `docs/` → **here**. `osc_spike` measures nine techniques
  mxm-mono-01 does not ship; `mod_spike` produces the numbers for a reference that will outlive
  mxm-mono-01.
- Verifies *this plugin's* shipped DSP → **stays with the plugin's crate**.
  `mono_01_filter_spike`'s figures
  are quoted inside `filter.rs` and in that crate's own verification section; it is a test with
  prose output.

When a new harness is written, ask which of those two it is before choosing a directory.

## The dependency runs one way

`dsp-lab` depends on `mxm-mono-01-dsp`, never the reverse. That is what lets the harnesses measure the
**shipped** code through its real public API rather than a copy — a contract the references rely on
— and it adds nothing to `mxm-mono-01-dsp`'s runtime graph, which is what earns its MSRV
override.

## Not in `default-members`

A plain `cargo build` must not build the harnesses. They are run deliberately, in release, when a
reference's numbers need regenerating. *Since the split (2026-10-06):* mxm-tools' workspace has no
`default-members`, so a plain `cargo build` here does build this crate; the rule held in the
monorepo and is not enforced in this repository.

## Shared code, and the line it is drawn on

The numeric core both harnesses need and neither owns — the radix-2 `fft` and its `ifft`, `princarg`,
the analysis length `N`, `periods_for` (the exactly-periodic frequency choice the whole method rests
on) and the two spectral weightings `a_weight` and `spectral_flatness_db` — **lives in
[`../mxm-measure`](https://github.com/mxm-audio/mxm-kit/blob/main/crates/mxm-measure/AGENTS.md)**, not here. It was in `src/lib.rs` until the rest of the
repository turned out to be re-implementing pieces of it by hand; none of it was ever specific to
these two references.

**This crate takes `mxm-measure` as a normal dependency, and it is the only crate that does.** Every
shipped crate takes it as a `[dev-dependencies]` entry. The asymmetry is the point: `dsp-lab` has no
shipped graph to protect and its harnesses *are* measurement.

**Measurement policy stays in the harnesses.** Deciding which bins count as wanted *is* the
experiment, and each one decides it differently — `analyse`, `analyse_multi`, `harmonic_deviation`,
`alias_db` and `splatter_above_db` therefore stay where they are used, even where two of them look
alike. The test for moving something down into `mxm-measure` is that it has no opinion about the
measurement: a transform, a window length, a weighting curve. A helper that encodes what a result
*means* does not qualify, and neither does a threshold.

**A change to `mxm-measure`'s spectrum module is a change to both references' evidence**, so it is
followed by a full re-run and a diff against both `measurements-run.txt` files.

**Strip compiler diagnostics before diffing, or the diff lies.** `cargo run` writes warnings to the
same stream as the output, so a single new warning shifts every line and the comparison reports that
hundreds of figures moved when none did. That happened once and cost a real scare: build first, then
run, and filter `Compiling`/`Finished`/`Running`/`warning`/`error` and their continuation lines before
comparing. That gate was run
when the core moved (2026-09-12): every spectral figure came back bit-identical, and the only
movement was in cost columns, by the few percent they already drift between sessions.

# Work Guidance

- **Release, always.** Cost figures from a debug build are meaningless and the spectra take minutes.
- **Measure the shipped code through its public API.** A harness that reimplements what it claims to
  measure is measuring itself. Where a technique is *not* shipped — most of `osc_spike` — implement
  it in the harness behind the same trait so quality and cost figures describe the same code.
- **Every number a reference quotes must come from a section here**, and the reference names which.
  If a chapter needs a figure this crate does not produce, add the measurement rather than
  estimating.
- **Record the run.** `docs/*/measurements-run.txt` (in mxm-kit) holds the verbatim output the documents quote,
  so a fresh run can be diffed rather than eyeballed. Regenerate it whenever the numbers move.
- **A recorded run belongs to the machine that produced it.** mxm-kit's `docs/oscillators/` holds two,
  because §9f was measured years of hardware later than the rest and the cost columns move about 2x
  between them. Record a new machine's run as its own file rather than overwriting a run other
  chapters still quote, and say at the claim which one a figure came from.
- **A negative result is a result.** §9f expected playback rate to cost cache lines and measured it
  free; that row stays in the harness and the finding stays in the chapter, because the argument it
  withdraws (a mipmap for speed) is one somebody would otherwise make again.

# Verification

```bash
cargo run -p dsp-lab --release --example osc_spike     # ~25 s, twenty sections
cargo run -p dsp-lab --release --example mod_spike     # four sections
cargo clippy -p dsp-lab --all-targets
```

A change to **`mxm-measure`'s spectrum module** is a change to both references' evidence — `src/lib.rs`
here holds no code any more. Re-run both harnesses and diff against the recorded runs; anything moving
outside the cost columns needs explaining before it is recorded.

There are no unit tests here and there should not be: the harnesses' own correctness is checked by
control rows *inside* their output — an additive sawtooth that must measure as having no aliasing, a
trivial waveform that must measure as having a lot, a one-voice case that establishes a metric's
floor. A harness whose controls read wrong is broken regardless of what the rest of the table says.

The properties those controls assert are listed in
[`../../docs/oscillators/MEASUREMENTS.md`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/oscillators/MEASUREMENTS.md).

# Child DOX Index

No child AGENTS.md files.
