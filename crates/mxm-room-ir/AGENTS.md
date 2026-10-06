# AGENTS.md — crates/mxm-room-ir

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

Offline room-acoustics simulation that writes impulse-response WAVs. A room is built from
geometry, materials and air; sound propagation in it is simulated; each result is written as a
file with a sidecar recording how it was made. **It ships nothing into a plugin.** Its product is
content: the catalogue the convolution effect loads.

Method, sources and evidence: `research:effects/room-acoustics-simulation.md`. Every algorithm is
implemented from a paper's or standard's equations; **no room-acoustics software source is opened
and no software material library is used.** Milestones R1–R5 exist; still open are the owner's
listening (V11) and a decode through `mxm-fx-convolution`'s own reader (V12). What each milestone
built: [NOTES.md § How it was built](NOTES.md#how-it-was-built).

# Ownership

Each path's full scope: [NOTES.md § Ownership, in full](NOTES.md#ownership-in-full).

| Path | Scope |
|---|---|
| `src/geometry.rs`, `src/scene.rs` | `Vec3`, polygons, `Room` validation, ray casting, the room generators; `Scene`, its canonical text and FNV-1a hash |
| `src/material.rs`, `src/bands.rs`, `src/air.rs` | Band absorption and scattering; the band set; ISO 9613-1 air |
| `src/boundary.rs`, `src/complex.rs` | Boundary models as branch admittances and their fit; a small complex type |
| `src/ism.rs`, `src/rays.rs`, `src/late.rs` | Image sources; the ray tracer; late synthesis |
| `src/fdtd.rs`, `src/diffraction.rs` | The wave solver; Biot–Tolstoy–Medwin edge diffraction |
| `src/directivity.rs` | Source and capsule patterns, measured tables, arrays |
| `src/render.rs`, `src/simulation.rs` | Rendering the parts and the crossover; the hybrid for one scene and a set |
| `src/fft.rs`, `src/par.rs`, `src/rng.rs` | Private FFT and minimum-phase design; order-preserving parallel map; SplitMix64 |
| `src/analysis.rs`, `src/wav.rs`, `src/sidecar.rs` | ISO 3382 parameters and the other analyses; float WAV writer, WAV reader, JSON sidecar |
| `src/catalogue/` | The 100 archetypes (`mod.rs`), the material rows and blends (`materials.rs`), one module per family (`spaces/`) |
| `src/bin/catalogue.rs` | `estimate`, `geometry`, `render`, `v9`, `release` |
| `impulses/` | **Project-generated; never edited by hand.** Written only by `catalogue -- release`; sidecars and `manifest.json` committed, WAVs ignored |
| `tests/`, `examples/` | R2's and R3's proofs; the end-to-end renders and the BRAS benchmark (V8) |
| `viewer/` | A static page drawing a room from its OBJ export; outside the build, nothing reads it back |

# Local Contracts

Each section's reasons and measurements are under the same heading in
[NOTES.md § Local contracts, in full](NOTES.md#local-contracts-in-full).

## Geometry
- Vertices run counter-clockwise seen from inside, so normals point inward. A room not planar,
  watertight and positive-volume is **rejected, never repaired**; a zero-thickness partition and a
  T-junction are not watertight. The generators avoid T-junctions by construction; prisms may not touch.
- A thin partition is a knife edge. Keep sources, receivers and paths off edges and corners (the
  crossing-number test is ambiguous there); a ray escaping through an edge counts in `leaked`.
- "Into a face" is decided by containment just off the edge, never by a vertex average.

## Image sources
- A candidate passes validity and visibility; pruning by path length is safe in any geometry. A rigid
  box reproduces Allen & Berkley's image set exactly, image by image (the polyhedra evidence).

## The time seam
- **By order, not by time:** image sources own every purely specular path up to their order, rays
  everything else; `simulate` never cuts image sources shorter than the rays' reach.
- One estimator per leg. Scatter or specular is sampled per band strategy, weighted by the balance
  heuristic. Non-diffuse scenes keep late specular paths discrete (`ism::arrival_for_sequence`).

## Boundaries and the wave solver
- Every boundary is a sum of branches with `ℓ, r, κ ≥ 0` (positive real). A fit with a negative
  coefficient is **refused, never clamped**; a material without a model gets a fitted one, labelled
  *fitted*, which may carry reactance.
- A change to the boundary update re-runs the reflection-coefficient and 40,000-step decay tests first.
- The source is omnidirectional; `simulate` refuses a directional one in a wave-solved scene. Probes
  are fixed when the solver runs (`WaveOptions::probe_offsets`); an undeclared capsule is refused.
- Bit-identical at any thread count; a test enforces it.

## Edge diffraction
- The model is the Biot–Tolstoy–Medwin line integral (UTD rejected). Only edges whose open angle is
  neither π nor π/m diffract, so a diffraction test needs an exposed edge.
- Faces are assumed rigid, and the known errors (absorbing corners, faces narrower than the
  wavelength) are **recorded, not corrected**. Diffracted nodes take part in time zero; a path may
  reflect once on each side of its edge; the quadrature has a convergence floor, held by a test.

## Directivity
- A measured directivity is used as published, magnitude only, looked up bilinearly in the source's
  or capsule's own frame. A wave-solved scene takes an omni source and omni or first-order capsules.

## The frequency seam
- Wave part through a minimum-phase lowpass, early part its exact complement, late part a
  power-complementary minimum-phase highpass: all causal; zero-phase crossovers rejected.
- **A seam mismatch is fixed in the boundary model, never by retuning a solver.**
- The wave part is gated at time zero, blocked below 10 Hz, faded at the run's end, resampled with a
  windowed sinc; a wave-solved render's merged output passes a sixth-order highpass at 30 Hz.
- Judge solver agreement on renders windowed on absolute time, never each from its own time zero.

## Rendering
- Time zero is the earliest arrival over every source and capsule of a set, even with the direct path
  excluded; `lead_samples` is shared by the set. Inter-channel and inter-source delays are kept.
- Renders are normalized to a −1 dBFS peak by default (`RenderOptions::normalize_peak_dbfs`), with
  `Rendered::gain` and `reference_distance_m` recording the scale; `None` renders physically. Level
  and agreement tests and the BRAS benchmark render physically.
- Bit-identical at any thread count on one platform; the final sample exactly zero; late synthesis
  seeded per scene and causal.

## Outputs, level and release
- **The catalogue is the far position, in stereo** (the centre source through the pair). Near
  positions only with `--near`, true stereo only with `--true-stereo`; no four-channel file. Mono and
  FOA (ACN/SN3D) are library outputs only.
- Each released file peaks at −1 dBFS; `reference_distance_m` records its gain. The direct sound is
  excluded; time zero stays at the earliest arrival. Committed files end by 10 s with a recorded
  fade; metrics come from the uncapped full-response render. 32-bit float WAV at 48 kHz.
- **The WAVs are not committed**: rebuilt locally with `catalogue -- render` and `-- release`; the
  sidecars and `manifest.json` are, so a release is reviewed as a diff.
- When the plugins ship, a release is **installed, not bundled**: a versioned download installed
  under the platform's local data directory at `mxm/impulses/`; the plugin's reading of it is owned
  by `plugins/mxm-fx-convolution/AGENTS.md` in the mxm-fx-convolution repository.

## The catalogue
- The target is **plausible**, generic rooms only: no archetype copies a real building. Scenes are
  code (`src/catalogue/`), not files. An opening is an anechoic face, the only open boundary. The 100
  follow the survey of shipped catalogues; a space with no closed room, a plate or a spring is not a room.
- A surface is a blend of published rows by area fraction, named in the blend. **Rows are never edited
  to hit a class range**; fractions and geometry are. A stand-in row is named in the room's note.
- Scattering: the depth rule, then `DETAIL_SCATTERING` on what it leaves specular; `RELIEF_SHARE`
  in rooms too large to wave-solve, with whole-part relief kept per room. Geometry is added to the
  rule, never replaces it. A room whose absorption lies on one surface needs relief and some
  absorption on its walls. A long corridor is diffuse, not non-diffuse.
- Standing solids only in `Room::box_with_prisms`; floating solids anywhere via `Room::with_solids`.
  **A solid replaces the floor part that stood for it, never doubles it**, and its footprint matches
  the area the blend gave up. A room whose decay moves more than a fifth against the release before
  it goes back as it was; so does a gated room that leaves its class.
- Edge diffraction is not rendered in the catalogue. V12 is a rules check against the consumer's WAV
  rules. V9 gates only classes with a range measured in more than one room; the rest are reported.
  Sources: omni, `centre` for stereo, `left`/`right` for the true-stereo pair.
- **Rendering and releasing are separate**: `render` writes to `target/`; `release` checks every file
  and replaces `impulses/` only after all pass. A cached render is reused only if its key matches;
  **a code change that alters renders bumps `RECIPE_EPOCH`**. `v9` and `release` refuse a stale position.

## Rejected
Image sources alone; statistical synthesis alone; full-band wave simulation; existing room-acoustics
software or commercial IR libraries; neural IR generation; a GPU wave solver in v1; a scene file
format under `rooms/`. Reasons: [NOTES.md § Rejected](NOTES.md#rejected).

## Chosen, not read
Each chosen constant is marked where it is defined; better evidence replaces it, never a guess. The
list: [NOTES.md § Chosen, not read](NOTES.md#chosen-not-read).

## Boundaries of the crate
- Zero dependencies, MSRV 1.87, `publish = false`, a workspace member but **not** a default member,
  built optimised in the dev profile (`[profile.dev.package.mxm-room-ir]` in the root `Cargo.toml`).
- Renders, the examples' output and the render cache live under ignored `target/`; only
  `catalogue -- release` writes `impulses/`. **Nothing depends on this crate.**
- Metrics come from a full-response render (direct path included, uncapped), never a direct-free or
  capped file. Wave solving costs minutes per small-room source.

# Work Guidance

- **Trust the analyser only through its controls.** A change to `analysis.rs` re-runs its synthetic
  decay tests first. A wrong analyser passes every room.
- **A new mechanism proves itself against a closed form first** (the closed form each one uses:
  [NOTES.md § Work guidance](NOTES.md#work-guidance-as-written)).
- **Change an estimator only with the no-double-counting test green.** **Test diffraction where it
  carries the field**: absorbing walls and a shadowed receiver.
- **Small rooms need furnished materials.** The catalogue must give the solver the room.
- **Keep the FFT private.** `mxm-fx-convolution-dsp` has its own; a shared one waits for a second
  shipped consumer.

# Verification

```bash
cargo test -p mxm-room-ir --release
cargo test -p mxm-room-ir
cargo clippy -p mxm-room-ir --all-targets -- -D warnings
cargo fmt -p mxm-room-ir -- --check
cargo +1.87.0 check -p mxm-room-ir --all-targets
cargo run -p mxm-room-ir --release --example render_church
cargo run -p mxm-room-ir --release --example render_small_room
cargo run -p mxm-room-ir --release --example bras_benchmark   # needs the research checkout
cargo run -p mxm-room-ir --release --bin catalogue -- geometry        # every room as OBJ, for looking at
python crates/mxm-room-ir/viewer/pack_rooms.py recital-hall rock-cave  # repack the viewer's rooms.js
cargo run -p mxm-room-ir --release --bin catalogue -- render  # the far positions, about 96 minutes; resumes
cargo run -p mxm-room-ir --release --bin catalogue -- render --near  # adds the near positions, about as long again
cargo run -p mxm-room-ir --release --bin catalogue -- render --true-stereo  # adds the pair, about three times as long
cargo run -p mxm-room-ir --release --bin catalogue -- render --rays=640000 recital-hall  # V10 beside it
cargo run -p mxm-room-ir --release --bin catalogue -- v9      # exits 1 if a gated class misses
cargo run -p mxm-room-ir --release --bin catalogue -- release # writes impulses/: a release act
```

- The tests prove V1–V7, V10, no double counting, diffraction, geometry and data, and the analyser
  controls. **V8** (`examples/bras_benchmark.rs`) and **V9** (`catalogue -- v9`) are reports, not
  tests; **V12** runs inside `catalogue -- release`. Each bound and its last measured result:
  [NOTES.md § What the tests prove](NOTES.md#what-the-tests-prove).
- **Not yet proved**, so never claimed: the added rooms against measurements, a dome's tessellation,
  the detail share's frequency dependence and flutter rooms, the owner's listening (V11), a decode
  through the consumer's reader (V12), V8 at RS5's 1 kHz and below the seam, diffraction against
  absorbing faces and transmission, fitted boundaries below their fit range, higher-order diffraction
  and grazing incidence, Linux and macOS ([NOTES.md § Not yet proved](NOTES.md#not-yet-proved)).

# Child DOX Index

No child AGENTS.md files.
