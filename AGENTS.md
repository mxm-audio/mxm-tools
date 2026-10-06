# AGENTS.md — mxm-tools

DOX rail for this repository. Project-wide instructions, durable workflow rules, and the
top-level Child DOX Index.

---

# DOX framework

- DOX is a highly performant AGENTS.md hierarchy installed here
- Agents must follow DOX instructions across any edits

## Core Contract

- AGENTS.md files are binding work contracts for their subtrees
- Work products, source materials, instructions, records, assets, and durable docs must stay
  understandable from the nearest applicable AGENTS.md plus every parent AGENTS.md above it

## Read Before Editing

1. Read the root AGENTS.md
2. Identify every file or folder you expect to touch
3. Walk from the repository root to each target path
4. Read every AGENTS.md found along each route
5. If a parent AGENTS.md lists a child AGENTS.md whose scope contains the path, read that child and
   continue from there
6. Use the nearest AGENTS.md as the local contract and parent docs for repo-wide rules
7. If docs conflict, the closer doc controls local work details, but no child doc may weaken DOX

Do not rely on memory. Re-read the applicable DOX chain in the current session before editing.

## Update After Editing

Every meaningful change requires a DOX pass before the task is done.

Update the closest owning AGENTS.md when a change affects:

- purpose, scope, ownership, or responsibilities
- durable structure, contracts, workflows, or operating rules
- required inputs, outputs, permissions, constraints, side effects, or artifacts
- user preferences about behavior, communication, process, organization, or quality
- AGENTS.md creation, deletion, move, rename, or index contents

Update parent docs when parent-level structure, ownership, workflow, or child index changes. Update
child docs when parent changes alter local rules. Remove stale or contradictory text immediately.
Small edits that do not change behavior or contracts may leave docs unchanged, but the DOX pass
still must happen.

## Hierarchy

- Root AGENTS.md is the DOX rail
- Child AGENTS.md files own domain-specific instructions and their own Child DOX Index
- Each parent explains what its direct children cover and what stays owned by the parent
- The closer a doc is to the work, the more specific and practical it must be

## Child Doc Shape

- Create a child AGENTS.md when a folder becomes a durable boundary with its own purpose, rules,
  responsibilities, workflow, materials, or quality standards
- Work Guidance must reflect current project standards or user instructions; leave it empty if
  there are none yet
- Verification must reflect an existing check; leave it empty until one exists

Default section order: Purpose · Ownership · Local Contracts · Work Guidance · Verification ·
Child DOX Index

## Style

- Keep docs concise, current, and operational
- Document stable contracts, not diary entries
- **A value the code holds is named, not copied** (the owner, 2026-09-24: *"Why are you writing the
  opening pages size in such detail. Is that not already described in the code?"*). A size a test
  derives — an opening size, a minimum, a card floor — or a constant the code declares is stated in
  DOX as its rule, its constant and the test that holds it, never its number: a copied number goes
  stale the day the code moves. Two exceptions: the design system states its own tokens and
  rules, because it is the normative source the code implements; and a plan's revision history
  records what was measured when — a dated record, not the current value
- Put broad rules in parent docs and concrete details in child docs
- Prefer direct bullets with explicit names
- Do not duplicate rules across many files unless each scope needs a local version
- Delete stale notes instead of explaining history
- Trim obvious statements, repeated rules, misplaced detail, and warnings for risks that no longer
  exist

## Closeout

1. Re-check changed paths against the DOX chain
2. Update nearest owning docs and any affected parents or children
3. Refresh every affected Child DOX Index
4. Remove stale or contradictory text
5. Run existing verification when relevant
6. Report any docs intentionally left unchanged and why

---

---

# Purpose

**mxm-tools** is the tools the MXM instruments are tuned and measured with: the listener, the room simulator, the measurement harnesses and the listener HUD.

It is one of the MXM products, each in its own repository under
[github.com/mxm-audio](https://github.com/mxm-audio), built on the MIT-licensed
[mxm-kit](https://github.com/mxm-audio/mxm-kit) — the design system, keyboard navigation, presets,
modulation, the control map and the checks every plugin shares. Until 2026-10 all of it was one
repository (`mxm-collection`); references to `plans/` name its design history, which stays in
a private archive.

Reference-quality open source: clarity beats cleverness, and every nontrivial algorithm names the
technique or paper it comes from.

# Ownership

Root owns `Cargo.toml`, `Cargo.lock`, `LICENSE`, `NOTICE.md`, `TRADEMARKS.md`, `README.md`,
`CONTRIBUTING.md`, `.cargo/`, `.github/` and `xtask/`.
Each folder with an `AGENTS.md` owns its contents; the index is below.

**Dependencies are pinned exactly and `Cargo.lock` is committed.** The kit comes from mxm-kit at
`v0.3.0`, another product's crates from its repository at a tag, and nice-plug and
egui-baseview from their MXM forks (`[patch.crates-io]`).

## Windows, Linux and macOS — all three, always

**An absolute requirement.** Everything here runs on all three; a change that works on one and
breaks another is a broken change. CI builds and tests on all three.

- **Anything platform-specific is `cfg`-gated with every arm implemented**, never one arm and a
  silent nothing elsewhere.
- **Linux needs system libraries** the other two carry in their SDKs — ALSA (and JACK) for audio,
  and X11, xkbcommon and a GL loader for the window.
- **A dependency that does not support all three cannot be taken**, whatever else it offers.

## MSRV is per crate

| Crate | MSRV | Why |
|---|---|---|
| `crates/mxm-listening` | **1.87** | No external crate of its own; `mxm-measure`, `mxm-audio-file` and `mxm-audio-file-decode` all build there, so a DSP crate's harness can dev-depend on it. **Verified on 1.87**, library, tests and the `listen` binary |
| `crates/mxm-room-ir` | **1.87** | Zero dependencies and no GUI; an offline tool whose product is impulse-response content, not a plugin's DSP |
| `apps/mxm-listener-hud` | **1.95** | eframe + egui; unshipped, not a default member. `mxm-listening` below it stays at 1.87 |

## Licensing

**GPL-3.0-or-later** (`LICENSE`). `NOTICE.md` lists the third-party code in its builds. The MXM
name and logo are not covered by the licence: see `TRADEMARKS.md`.

- **MPL-2.0 is accepted** for symphonia, through `mxm-audio-file-decode` only, used unmodified;
  never vendor, patch or modify an MPL crate.
- Check the licence before porting any algorithm, and record source and licence in a comment at
  the top of the file. Cite techniques even when the implementation is original.

## Research citations

A citation written `` `research:<path>` `` names a page in MXM's private research repository. It
is plain text in a code span, never a link, and nothing here depends on it at build or test
time. Facts, numbers, our own measurements and short quotations cross into this repository;
third-party files, images and verbatim text never do.

# Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```

CI runs the same on Windows, macOS and Linux.

# Child DOX Index

| Doc | Scope |
|---|---|
| [`crates/dsp-lab/AGENTS.md`](crates/dsp-lab/AGENTS.md) | Non-shipping measurement harnesses for collection theory documents |
| [`crates/mxm-room-ir/AGENTS.md`](crates/mxm-room-ir/AGENTS.md) | Offline room simulation that writes impulse-response WAVs. R1: validated polygonal rooms, image sources in arbitrary polyhedra proved against Allen & Berkley, minimum-phase early rendering with physical level, an ISO 3382 analyser trusted through synthetic controls, float WAV and sidecar. R2: the geometric hybrid (rays with diffuse rain, late synthesis, ray-assisted image sources, capsules from mono to FOA) with its seam by image-source order, proved against Eyring, the two-room model, sin(kd)/kd and the exact image sum. R3: positive-real boundary models, the leapfrog wave solver with staircased impedance boundaries, Biot–Tolstoy–Medwin edge diffraction, and a causal crossover, proved against analytic modes, the boundary models' reflection coefficients and solver agreement. R4: the BRAS reference scenes against their measurements (V8), with measured directivity tables and free-field scenes built from prisms; 8 of 9 gated groups within the 1 dB JND. R5: the catalogue, 100 generic archetypes as code in nine families with V9 against published class ranges, and 100 stereo impulse responses, one far position per room, released into `impulses/<family>/<slug>/` (true stereo on request; project-generated: sidecars and manifest committed, WAVs rebuilt locally); band-dependent scattering exposed a ray-tracer variance fault, fixed with balance-heuristic weights, and the smallest room a sub-audio mode, filtered out. Every chosen constant is marked |
| [`crates/mxm-listening/AGENTS.md`](crates/mxm-listening/AGENTS.md) | The listener: preparation (the drum A/B page's decode, trim and body loudness, which the page calls), hand-rolled numerics, sound families, readings per part of a sound — attack, level, decay, pitch and high-resolution modes, tone over time, texture, modulation, artefacts — each with its window, resolution, validity and source, and sets of round robins with their room lines; comparison ranked by audibility in the owner's words, the unexplained-difference map, the resynthesis self-test, the golden cases and `listen site` for a whole A/B page; calibration to the owner's ears (sessions on a local page, staircases, the owner's thresholds kept local); loudness, sharpness, tonality and roughness after ECMA-418-2; for the listener's window, `describe` in stages, a name's claims, the glossary of every reading and the curves it draws |
| [`apps/mxm-listener-hud/AGENTS.md`](apps/mxm-listener-hud/AGENTS.md) | Drop a sound and see what the listener hears, as a HUD, and hear it and its rebuilt parts: the owner's rulings (its own design, shipping deferred), the numbers rule, the five roles, the wheel's zoom, the callback that only copies, the bounded worker, the no-path rule, the backend rule |
