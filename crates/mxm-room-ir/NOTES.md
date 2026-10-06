# NOTES.md — crates/mxm-room-ir

The detail behind this folder's AGENTS.md: history, measurements, rationale and worked examples. AGENTS.md is the contract; this file is the reference it links to.

**References.** `plans/plan-mxm-room-ir.md` (its revisions and §4.5) was the monorepo's plan;
*root* below means the monorepo's root `AGENTS.md` (*Don't open existing implementations*,
the crate rules), which now lives only in the private archive (`archive/01-mxm-collection/`).
*Since the split (2026-10-06):* the rules it held are public: *Don't open existing
implementations* and *Don't pre-generalise* in mxm-kit's
[`docs/collection-rules.md`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/collection-rules.md),
and the crate rules in mxm-kit's root
[`AGENTS.md`](https://github.com/mxm-audio/mxm-kit/blob/main/AGENTS.md), *The crates*.
`plugins/mxm-fx-convolution/AGENTS.md` and `crates/mxm-fx-convolution-dsp` are in the
mxm-fx-convolution repository. A `research:<path>` citation names a page in MXM's private
research repository.

## How it was built

The plan that built it, `plans/plan-mxm-room-ir.md` (revisions 1–13), was deleted at R5's closeout:
its durable decisions are in this file, and its history is in git. Method, sources and evidence:
`research:effects/room-acoustics-simulation.md`. Every algorithm is implemented from
a paper's or standard's equations. **No room-acoustics software source was opened and no software
material library was used** (root *Don't open existing implementations*).

**Milestones R1–R5 exist.**
- **R1:** the scene, image sources in arbitrary polyhedra, early rendering, the ISO 3382 analyser,
  WAV and sidecar.
- **R2:** the geometric hybrid. That is stochastic rays with scattering, late synthesis, ray-assisted
  image sources for non-diffuse scenes, directional sources and capsules, and every output format.
- **R3:** positive-real boundary models, the leapfrog wave solver, first-order edge diffraction, and
  the causal crossover that joins the wave solver to the geometric solvers.
- **R4:** the BRAS reference scenes against their measurements (V8), with measured directivity
  tables, free-field scenes built from prisms, and a WAV reader.
- **R5:** the catalogue. That is 100 generic archetypes in nine families as code (`src/catalogue/`),
  a render binary, V9 against published class ranges, and the release folder `impulses/`: 100
  stereo WAVs, one far position per room (about 190 MB, rebuilt locally, not committed), with their
  committed sidecars and manifest. Still open: the owner's listening (V11) and
  a decode through `mxm-fx-convolution`'s own reader (V12).

## Ownership, in full

| Path | Scope |
|---|---|
| `src/geometry.rs` | `Vec3`, planar polygons, `Room` validation (planar, watertight, inward), ray casting, generators: `shoebox`, `extruded`, `extruded_regions`, `stepped_regions` (regions of different heights), `box_with_prisms` (plates, partitions and blocks in a free field), `with_solids` (solids floating in any room) |
| `src/material.rs`, `src/bands.rs` | Per-octave absorption and scattering, an optional boundary model; the band set and its extension rule |
| `src/boundary.rs` | Boundary models as branch admittances: panel, Miki porous layer, fit to band absorption; Paris' integral; the non-negative least-squares fit |
| `src/complex.rs` | A small complex type |
| `src/air.rs` | ISO 9613-1 attenuation, humidity conversion, speed of sound, density |
| `src/scene.rs` | `Scene` (with source directivity and the non-diffuse flag), its canonical text and FNV-1a hash |
| `src/ism.rs` | Image sources: validity, visibility, safe pruning, incidence angles; one named face sequence to an arrival |
| `src/rays.rs` | Ray tracer: energy estimators, direction cells, the no-double-counting rule, discovery rays |
| `src/late.rs` | Late synthesis: envelope, Poisson events, per-capsule band rendering |
| `src/fdtd.rs` | The wave solver: voxelisation, the scheme and its boundary branches, source, probes, worker pool |
| `src/diffraction.rs` | Diffracting edges and the Biot–Tolstoy–Medwin line integral as nodes |
| `src/directivity.rs` | First-order and band-dependent patterns, measured tables on a front-pole grid, frames, arrays (mono, spaced omni, near-coincident cardioids, Blumlein, mid-side, FOA) |
| `src/render.rs` | Early (with phase-carrying walls), diffracted, late and wave parts; the crossover; time zero, lead, level, cap |
| `src/simulation.rs` | The hybrid for one scene (time seam, frequency seam, wave field), and rendering a set through one array |
| `src/fft.rs`, `src/par.rs` | Private FFT and minimum-phase design; order-preserving parallel map |
| `src/analysis.rs` | ISO 3382 parameters, octave band energies, echo density, band correlation, diffuse-field correlation |
| `src/wav.rs`, `src/sidecar.rs` | Float WAV writer, PCM and float WAV reader, JSON sidecar |
| `src/rng.rs` | SplitMix64, with per-index streams |
| `src/catalogue/mod.rs` | The catalogue: `FAMILIES`, the 100 `ARCHETYPES`, the wave-solving limit, each space's sources, positions and class reference; V9 comparison; each file's normalizing reference distance, the sidecar level patch and the consumer check the release uses |
| `src/catalogue/materials.rs` | The research page's absorption rows, further rows from its notes' full tables, and surfaces blended from them by area with the depth rule's scattering and the detail share combined into it |
| `src/catalogue/spaces/` | `mod.rs`: the generators the spaces share (a box and its standard layout, a box holding solids, pilasters and ceiling ribs as solids, a swept cross-section, a domed polygon, a sloping ceiling). One module per family (`rooms`, `studios`, `chambers`, `vehicles`, `halls`, `venues`, `worship`, `industrial`, `transit`): each space's geometry, blends, sources, positions, reference and notes |
| `src/bin/catalogue.rs` | `estimate`, `geometry` (each room as a Wavefront OBJ under `target/mxm-room-ir/geometry/`, for looking at; nothing reads it back), `render` (the far positions into `target/mxm-room-ir/catalogue/`, resumable; `--near` adds the near positions, `--true-stereo` the pair), `v9` (`--near` checks the near positions too), `release` (into `impulses/`; `--near` and `--true-stereo` release what they add) |
| `impulses/` | **Project-generated; never edited by hand.** The catalogue release: per archetype the far position's stereo file (100 32-bit float WAVs at 48 kHz) in `<family>/<slug>/`, a JSON sidecar each in the room's `metadata/` subfolder, and `manifest.json`, which lists both paths and each file's family; the near position only when released with `--near`, a true-stereo pair per position only with `--true-stereo`. Written only by `catalogue -- release`. The sidecars and manifest are committed; the WAVs are ignored and rebuilt locally |
| `tests/geometric_hybrid.rs` | R2's proofs: V4, V5, V6, V7, V10 ray convergence, time zero with late synthesis |
| `tests/wave_hybrid.rs` | R3's proofs of solver agreement (V3), and the small cabin's sub-audio check |
| `examples/render_church.rs` | The R2 end-to-end render (a large space, geometric only) |
| `examples/render_small_room.rs` | The R3 end-to-end render (a small room, wave-solved); both write under ignored `target/mxm-room-ir/` |
| `examples/bras_benchmark.rs` | V8: the BRAS reference scenes against their measurements, read from the research checkout; writes `target/mxm-room-ir/bras-benchmark.md` and exits 1 if a gate fails |
| `viewer/` | A static page that draws a room from its OBJ export, published as an artifact: `index.html` (no libraries, a painter's-algorithm canvas), `pack_rooms.py` (packs chosen `target/mxm-room-ir/geometry/*.obj` into `rooms.js`) and the packed `rooms.js`. Outside the crate's build; nothing reads it back |

## Local contracts, in full

Each contract as it was written, with its reasons and measurements.

### Geometry

- **Vertices run counter-clockwise seen from inside the room**, so every polygon's normal points
  inward. A room that is not planar, watertight and positive-volume under that convention is
  rejected, never repaired. A zero-thickness partition and a T-junction are both not watertight;
  `extruded_regions` makes openings between regions that share an edge vertex to vertex.
- **`stepped_regions` opens a shared edge up to the lower ceiling** and walls the taller region above
  it; every wall carries a vertex at each ceiling height meeting its vertical edges, so no edge
  ends in a T-junction. Equal heights reduce to `extruded_regions`, face for face.
- **`box_with_prisms` avoids T-junctions by construction.** The floor is split into a grid at every
  standing footprint's edges, and wall feet, prism feet and footprint ends carry the grid's
  vertices; a coplanar seam has open angle π and never diffracts. Prisms may not touch each other.
- **A thin partition is a knife edge.** Its section tapers from the real thickness at the floor to
  a crest. A flat top two edges wide would reach a shadowed receiver only by second-order
  diffraction, which v1 does not represent.
- Point-in-polygon is a crossing-number test and is ambiguous exactly on an edge. Scenes keep
  sources, receivers and paths off edges and corners. A ray that escapes through an edge ends and
  is counted in the sidecar's `leaked`.
- "Into a face" is decided by containment just off the edge, never by a vertex average, which lies
  outside a non-convex face (an L-shaped floor's lies in the notch).

### Image sources

- A candidate image must pass **validity** (it is mirrored from the interior side of the plane) and
  **visibility** (tracing back from the receiver, each segment crosses its polygon and nothing
  blocks it).
- Pruning by path length is safe in any geometry. An ancestor image is never farther from the
  receiver than the complete path, because the straight line to it is shorter than the folded
  path, which has the full length.
- For a rigid box the result equals Allen & Berkley's image set exactly, and a test proves it image
  by image. Borish 1984 was read as an abstract only, so the tests are the evidence for polyhedra.
- A wall's specular pressure reflection is `sqrt((1 − α)(1 − s))`, except in a wave-solved scene,
  where absorption and phase come from the boundary model at the arrival's incidence angle.

### The time seam

- **By order, not by time.** Image sources own every purely specular path up to their order; rays
  own everything else. `simulate` never cuts image sources by a length shorter than the rays'
  reach, or that ownership would leave a gap. The test that image energy plus the hybrid deposit
  equals a nothing-excluded sphere reference guards the rule.
- **One estimator per leg.** A leg leaving a scattering event toward the receiver is the diffuse
  rain's; every other non-image leg is the receiver sphere's.
- **Scatter or specular is sampled per band strategy, weighted by the balance heuristic.** A ray
  draws one band and continues with that band's scattering probability; each band divides its
  physical energy by the mean of every band's probability for the path. One band-mean probability
  for all bands was unbiased but useless once scattering varied with frequency: its weights
  multiplied without bound, a few rays carried a low band's whole tail, and a church's 125 Hz T30
  fell from 4.3 s to 2.3 s in steps. A box with uniform scattering never shows it; the
  frequency-dependent V4 test does.
- **Non-diffuse scenes** keep late specular paths discrete: separate purely specular discovery
  rays name face sequences, and `ism::arrival_for_sequence` renders each exactly or rejects it.
  Energy rays deposit nothing purely specular there.

### Boundaries and the wave solver

- **Every boundary is a sum of branches with `ℓ, r, κ ≥ 0`**, so it is positive real and the scheme
  cannot gain energy through it. A fit that produced a negative coefficient is refused, never
  clamped. A material without an explicit model gets one fitted to its band absorption, labelled
  *fitted*.
- **A fit to band absorption may carry reactance.** Demanding none contradicts Kramers–Kronig for any
  resistance that varies with frequency, and flattens the fit.
- **The boundary discretisation is derived, not read.** Its evidence is the reflection coefficient
  measured in the solver against the model, at normal and at 45° incidence, and monotone decay over
  40,000 steps. A change to the boundary update re-runs those tests before anything else.
- **The source is omnidirectional and radiates `excitation(t − r/c)/(4πr)`.** A directional source
  cannot be wave-solved, and `simulate` refuses one.
- **Probes are fixed when the solver runs.** A wave-solved scene declares every capsule position it
  will be rendered through (`WaveOptions::probe_offsets`), and a render through an undeclared capsule
  is refused.
- **Bit-identical at any thread count.** Each worker owns a contiguous cell range with its boundary
  cells and source cells; a test enforces identity.

### Edge diffraction

- **The model is the Biot–Tolstoy–Medwin line integral** (plan revision 10): time-domain, exact for
  a rigid wedge and valid for finite edges, so it converges to the wave solver across the
  transition band. UTD was rejected as asymptotic, with infinite edges.
- **Only edges whose open angle is neither π nor π/m diffract.** A box has none, so a diffraction
  test needs an exposed edge: a re-entrant corner or an obstacle.
- **The model assumes rigid wedge faces.** Absorbing faces only scale the specular legs. Behind an
  absorbing corner the geometric level runs high (measured: 3.7 dB with 90 % absorbing walls), and
  that is recorded, not corrected.
- **Diffracted nodes take part in time zero.** A shadowed receiver's first sound is diffracted.
- **A path may reflect once on each side of its edge.** A partition on a floor has four coherent
  crest paths at low frequencies. Without the fourth, BRAS RS5's floor-level pair ran 2.3 dB quiet
  at 250 Hz; with it, 0.4 dB.
- **The quadrature has a floor.** Next to a zone boundary the integrand peaks so high that rounding
  in `cosh η` keeps a split from converging, and the recursion grew to twelve million nodes on one
  RS5 path before the process ran out of memory. A relative convergence floor and a smallest piece
  bound it; a test holds the node count and the half-direct limit a few micro-radians from the
  boundary.
- **Specular reflection off a face narrower than the wavelength is under-cancelled.** First-order
  diffraction does not cancel it where double-edge diffraction would: grazing 8 cm over RS6's
  0.72 m top ran 4.2 dB loud at 250 Hz, and RS7's 0.12 m block tops 6–13 dB loud. Recorded, not
  corrected.

### Directivity

- **A measured directivity is used as published, magnitude only.** `DirectivityTable` holds gains
  per band on a front-pole grid (polar angle from forward, rotation from up toward left) and is
  looked up bilinearly in the source's or capsule's own right-handed frame. Phase is not
  represented.
- **A wave-solved scene takes an omnidirectional source** (`simulate` refuses another) **and omni
  or first-order capsules** (the render refuses a band-dependent or tabulated one).

### The frequency seam

- **The wave part passes a minimum-phase lowpass; the early part its exact complement; the late part
  a power-complementary minimum-phase highpass** (plan revision 10). All three are causal. A test
  proves the lowpass and its complement return the input exactly, and the lowpass and highpass pass
  unit power. Zero-phase crossovers were rejected because they ring before onsets.
- **A seam mismatch is fixed in the boundary model, never by retuning a solver.** If band absorption
  and the boundary model describe different surfaces, the solvers disagree at the crossover, and V3
  shows it.
- **In a direct-free render the wave part loses the analytic free-field direct sound**, pressure and
  near-field velocity, because the solver cannot omit it.
- **The wave part is gated at time zero**, blocked below 10 Hz, faded out at the end of the solver's
  run, and resampled with a windowed sinc.
- **A wave-solved render passes a sixth-order highpass at 30 Hz.** In a 4 m³ car cabin the fitted
  boundaries against the air's stiffness made a mode near 7 Hz that carried most of the render's
  energy; the 10 Hz drift block barely touched it, the wave part stopped mid-swing when the run
  ended, and the 125 Hz and 250 Hz decays measured 7–13 s. The filter acts on the merged output,
  so the solvers keep their relative phase at the seam.
- Solver agreement is judged on renders windowed on absolute time. Two renders of one scene can
  have different time zeros, because diffraction moves it; comparing each from its own time zero
  once hid a working diffraction model.

### Rendering

- **Time zero is the earliest arrival over every source and capsule of a set**, on a sample, even
  when the direct path is excluded. Inter-channel and inter-source delays are kept.
- **`lead_samples` is shared by the whole set** and is whatever keeps anything from ringing before
  time zero. It is zero for a mono render whose direct path lands on a sample; with a spaced pair
  it is normally positive.
- **Renders are normalized by default** (owner, 2026-09-15): each rendered file is scaled so its
  loudest sample peaks at −1 dBFS (`RenderOptions::normalize_peak_dbfs`), and `Rendered::gain` and the
  sidecar's `reference_distance_m` record the scale. With `normalize_peak_dbfs: None` the level is
  physical: pressure `reference_distance_m / path_length`, with late and wave energy scaled the
  same way. Tests of level and solver agreement, and the BRAS benchmark, render physically.
- **The result is bit-identical at any thread count on one platform.** Rays accumulate in fixed
  batches in ray order; renders accumulate serially in a fixed order. Tests enforce both.
- **The final sample is exactly zero.** A cap ends a file with a half-Hann fade that the sidecar
  records.
- **Late synthesis is seeded per scene**: the options' seed mixed with the simulation's ray seed,
  which hashes the scene, so near and far, stereo and true-stereo never share an event sequence.
- **Late synthesis is causal.** Events start no earlier than the histogram's earliest deposit
  (smoothing would otherwise lift energy before it, which put a pre-echo in the first church render),
  band filters are minimum-phase, and only an event's 8-sample placement kernel precedes it. A mono
  render whose direct path lands on a sample has no lead; a test holds it.

### Outputs, level and release

- **The catalogue is the far position, in stereo** (owner, 2026-09-15: "There is very little
  difference between near and far, with the mix turned down"). One stereo file per room: the centre
  source through the pair at the far position. The near position stays defined in each space and
  is rendered, checked and released only with `--near`.
  **True stereo is rendered and released only when asked** (`render --true-stereo`, `release
  --true-stereo`; owner, 2026-09-15): a pair of 2-channel files, one per side source. Our
  consumer loads one- or two-channel files and has no true-stereo mode, and the pair tripled the
  render. There is no four-channel file, because WAVE leaves the channel order unspecified and
  the consumer has not defined one. Mono and first-order ambisonics (ACN/SN3D) are library outputs
  only.
- **Each released file is normalized: its loudest sample peaks at −1 dBFS** (owner, 2026-09-15), as
  every render is by default.
  The catalogue is a creative tool, and the plugin or the DAW sets the reverb level; delivering
  rooms at their physical levels, a far cathedral near −54 dBFS, made no sense for that. Each
  sidecar's and manifest entry's `reference_distance_m` still records the file's gain, as the
  distance at which the direct sound has pressure 1. Scaling a 32-bit float file costs no
  resolution: the rounding scales with the signal, about 140 dB under it.
- **The direct sound is excluded from committed files**, because the consumer carries the dry signal
  through Mix. Time zero stays at the earliest arrival, so the gap before the first reflection, the
  room's distance cue, survives.
- **Committed files end by 10 s** with a fade the generator applies and records: the consumer's
  soft limit (owner, 2026-09-14), so the consumer never fades a file a second time. The library
  itself renders uncapped, and metrics come from the uncapped full-response render.
- **Format:** 32-bit float WAV at 48 kHz for the committed folder; the library renders any rate.
- **The WAVs are not committed** (owner, 2026-09-15): they would weigh on every clone and grow with
  every re-release (about 190 MB for the 100 far positions). They are rebuilt locally with
  `catalogue -- render` (about 96 minutes of simulation on one machine, resumable: the seams raised
  to Schroeder on 2026-09-15 cost about a sixth of it) and
  `catalogue -- release`. The sidecars and `manifest.json` are committed, so a release is still
  reviewed as a diff: scene hashes, gains and measured parameters per file.
- **When the plugins ship, a release is installed, not bundled** (owner, 2026-09-16). The released
  folder — WAVs, `metadata/` sidecars and `manifest.json`, in this layout — ships as a versioned
  download beside the plugins and is installed under the platform's local data directory at
  `mxm/impulses/`: `%LOCALAPPDATA%\mxm\impulses` on Windows, `~/Library/Application
  Support/mxm/impulses` on macOS, `~/.local/share/mxm/impulses` on Linux. Not inside a plugin: on
  Windows and Linux a plugin is one binary, and every instance would carry the catalogue. The folder
  is the collection's, not one plugin's, and does not depend on the plugin format: a VST3 build reads
  it too. This crate's ignored `impulses/` stays the build output a download is packaged from;
  `plugins/mxm-fx-convolution/AGENTS.md` owns how the plugin reads the installed folder.

### The catalogue

- **Owner decisions (2026-09-14):** the name `mxm-room-ir`; v1's 21 archetypes (grown to 100 on
  2026-09-15); a near and a far position each (only the far one released since 2026-09-15); generic rooms only. **The target is plausible**:
  convincing, pleasing reverb, not a copy of a real building, which is the bar the round robin found
  geometric simulators meet.
- **Owner decisions (2026-09-15):** each file is normalized, because the catalogue is a creative
  tool whose plugin or DAW sets the reverb level; the WAVs stay out of git, which carries only
  their sidecars and the manifest; the catalogue is stereo, with true stereo only on request; it
  holds 100 rooms of "a typical variety of reverbs"; and it releases only each room's far position.
- **The 100 follow the survey of shipped catalogues** (`research:sources/room-acoustics-simulation/notes/survey-shipped-ir-catalogue.md`):
  size and material variants of the classes most products ship (rooms, studios, chambers, halls,
  churches) and the niche spaces a closed room can hold (tank, cistern, silo, cave, catacomb,
  swimming hall, mausoleum, cinema). Nine families, each a release folder. Forest, quarry,
  courtyard and amphitheatre stay out: they have no closed room. Plates and springs are not rooms.
- **An opening is an anechoic face.** A tunnel's cut ends, an underpass's and a platform's ends and
  an open parking deck's sides are `Material::anechoic()`, standing for the space beyond; that is
  the only open boundary the catalogue has.
- **Where no row exists, a named row stands in and the note says so:** the hard row for steel and
  ice, the concrete floor for asphalt, rough sandstone and coarse block for rock, wood lining for
  books, velour for clothes and towels.
- **Scenes are code** (`src/catalogue/`), not files under `rooms/`: a scene format would need a
  parser. Geometry is invented and generic; no archetype copies a real building.
- **A surface is a blend of published rows by area fraction.** The fractions are the only per-room
  choice, and each blend's name lists them, so every sidecar says what a wall was. Rows are never
  edited to hit a class range; fractions and geometry are.
- **Scattering follows the research page's depth rule**, `0.5·d·f/c` clamped to 0.05–0.99, blended
  by reflected energy.
- **Then every surface scatters half of what the rule leaves specular** (`DETAIL_SCATTERING`,
  owner, 2026-09-16: "bake the extra scattering into the generator's material rules"). It stands for
  the detail no row, depth or solid represents, and combines with the rule as an independent
  mechanism, `s = 1 − (1 − s_rule)(1 − 0.5)`, under the same 0.99 ceiling: a flat part scatters
  0.525 in every band. **The evidence is a benchmark against recordings** (2026-09-16): ten real
  spaces from 49 m³ to 68,000 m³, from a commercial impulse-response library, each rebuilt from its
  published dimensions and these rows, then calibrated per octave to its recorded T30. The recordings
  and the harness stay outside this repository. Calibrated with the rule alone, the renders were too
  specular. In six of the ten spaces, echo density at 25 ms was 0.07–0.36 against 0.68–1.05
  recorded; the other four matched. Early decay fell short of late: EDT ran
  22–33 % under the recording in an opera house, a church converted to a concert hall and a seminar
  room. With the detail share, and absorption recalibrated:
  - the church hall's T30 error fell from 16 % to 4 % and its EDT error from 25 % to 12 %, with echo
    density at 25 ms 0.17 → 0.64 (recorded 0.77);
  - the opera house's EDT error fell from 22 % to 13 %, density 0.29 → 0.46 (0.68);
  - a domed mausoleum's density rose from 0.09 to 0.64 (0.90);
  - a power station hall's barely moved, 0.07 → 0.13 (1.05).
  **It made one space worse:** a roofless walled court, whose long decay is flutter between parallel
  walls, went from 13 % to 45 % T30 error, because scattering sent that flutter out of the open top.
  The seminar room kept a two-slope decay either way. The share is flat in frequency because only a
  flat share was measured. It is not the whole-part relief the owner heard as every room alike
  (2026-09-15, below): that put walls at the 0.99 ceiling from 2 kHz, which this does not.
- **A surface standing for left-out geometry carries that geometry's depth.** Balconies, coffers,
  piers, trusses and kiosks are not modelled, so their walls and ceilings take a characteristic
  depth of 0.5–1.5 m. At the rule's floor a flat box scatters almost nothing below 500 Hz: sound
  skims between parallel walls over an absorbing floor, and halls decayed up to 1.7 times Eyring
  at 125 Hz. Where depth was not enough the geometry is modelled: the halls' raised stages and
  side balconies are solids, and the arena is a bowl of raked tiers on all four sides. Tiers on
  two sides left the ends facing each other over open floor, and its 125 Hz decayed for 9–13 s.
- **In a room too large to wave-solve, that depth covers a quarter of the part** (`relief`,
  `RELIEF_SHARE`), and the rest reflects as the flat row (owner, 2026-09-15: "they sound very much
  the same… They are all very bright"). Over the whole part the depth put a big room's walls at the
  rule's 0.99 ceiling from 2 kHz, so every church and hall had the same diffuse early field there:
  the stone church without relief kept 4 kHz reflections discrete for 38 ms, with it for none. Seats,
  pews, drapes, carpet, ballast and absorbers keep their depth over their whole area, and so do the
  wave-solved rooms: a tiled bathroom's 1–8 kHz field was as dense as noise from its first
  milliseconds with or without relief, and with image sources to order 12.
- **Twenty-nine of the 49 geometric rooms keep the split; eighteen were given whole-part relief
  back** (owner, 2026-09-16: keep it per room). Splitting it let a horizontal field build again
  between the flat walls: seventeen rooms decayed more than 1.2 times longer at 500 Hz–1 kHz (an
  open-plan office 1.06 s → 3.0 s, an airport terminal 1.6×, a station concourse 1.4×, the recital,
  shoebox, chamber and small concert halls 1.25–1.5×, which failed V9), and the stone church, gated,
  left its class at 125 Hz (4.72 s against 4.40 s). The rooms that keep it are the churches, the
  cathedral and the refectory, the scoring stages, opera, drama and fan auditorium, the ballroom,
  club, arena, velodrome and village hall, the hangar and sawtooth factory, both tunnels, the metro
  platform, car park, parking deck and atrium; the other two geometric rooms have no structural
  relief to split. A room that keeps it gains 1–3 dB in its first 100 ms at 4–8 kHz.
- **A room whose absorption lies on one surface needs relief and some absorption on its walls.**
  Flat, hard walls over an absorbing floor or ceiling hold a horizontal field Eyring does not see:
  an open-plan office rendered 1.9 s against Eyring's 0.3 s, a lecture theatre 1.9 s against 0.9 s,
  a swimming hall 5.5 s against 2.4 s, a school corridor 3.9 s against 0.8 s and a train carriage
  1.7 s against 0.3 s. Columns, mullions, pilasters, lockers, seat backs and a tenth to a fifth of
  the walls in panels brought them to 1.1 s, 1.0 s, 2.5 s, 1.5 s and 0.45 s.
- **A long corridor is diffuse, not non-diffuse.** Ray-assisted discovery between two parallel
  walls 2.4 m apart ran for half an hour on one position without finishing; tunnels and platforms,
  whose ends absorb, and the stairwell finish in seconds to minutes.
- **Geometry does not replace the depth rule; it is added to it** (owner, 2026-09-16: model the
  relief as geometry "with a few so I can AB them"). A chamber music hall with thirty-two pilasters
  and ten ceiling ribs as solids, its rows flat, decayed 2.40 s at 1 kHz against its class's 1.7 s,
  and with only twelve pilasters 2.91 s: a dozen solids scatter far less than a wall the rule calls
  rough. The same hall with its rows keeping a quarter's relief measures 1.60 s. So a room carries
  both, and `pilasters_and_ribs` sizes the solids while the rows keep their published absorption.
  Ten rooms carry them: the recital, shoebox, chamber, small concert and large shoebox halls, the
  ballroom, village hall, sports hall, swimming hall and ice rink. Their decays moved 0.90-1.07
  times against the release before them, and no V9 verdict changed.
- **Only a box can hold solids.** `Room::box_with_prisms` is the one generator that takes them, so
  the churches and the opera (floor-plan regions), the cinema, lecture theatre and tunnels (swept
  sections) and the domed rooms carry their relief as depth alone until a region or swept generator
  learns to hold prisms.
- **A solid clears the floor.** A prism standing on the floor cuts it into a grid at every footprint
  edge — sixteen pilasters a side would leave over a thousand floor faces and an image-source search
  to match. Pilasters start 0.35 m up, and a 270-face hall still renders in about two seconds. That
  clearance is what lets `Room::with_solids` put furniture, pews or boulders in **any** room,
  whatever generator built it: a floating solid is a closed shell inside a watertight room and
  touches none of its faces. Only a solid that must stand on the floor needs
  `Room::box_with_prisms`, and only a box's floor is a rectangle to grid.
- **A solid replaces the floor part that stood for it, never doubles it.** Where seats, pews, desks
  or racking were a fraction of a floor blend, that fraction leaves the blend, which renormalises,
  and the solid carries the row on its top face at the same area — the published rows for seating
  are per floor area, so the top face is where they belong. The solid's sides take the frame's row
  (wood lining, hard), and the top keeps the row's depth, because a block of seats is still rough.
  Counting a row twice, once in the floor and once on a solid, is the quiet way to make a room too
  dead.
- **Twenty-one rooms carry their contents as solids** (owner, 2026-09-16: "put interior with their
  reflective properties into any room where it makes sense… churches are filled with benches"):
  furniture in the kitchen, bedroom, meeting room, open-plan office, library and museum; seat rows
  in the van, bus and carriage; pew rows across the naves of the baroque, romanesque and basilica
  churches; boulders and hanging slabs at angles in the rock cave; a bar, riser and tables in the
  pub, jazz club and basement club; plant and ducts in the plant room; columns in the mill loft;
  kiosks and benches in the station concourse and airport terminal. Two were taken back out: the
  living room, gated, left its class at 250 Hz, and the empty warehouse is empty — racking
  contradicts the archetype's own occupancy. The gated cathedral and stone church keep their pews
  as a floor blend until the reported churches have been listened to.
- **The solids' footprint matches the floor area the blend gave up.** A block's faces come to about
  four times its footprint, so the row over a quarter of them absorbs what its footprint did — but
  only if the footprints add up. Eighteen pew rows covering 190 m² replaced 450 m² of a baroque
  nave's pew fraction and the church rang a fifth longer; a pub whose floor gave up 38 m² of seating
  and gained 12 m² of bar and tables ran 1.28 times long at 1 kHz. Size the solids to the area, then
  measure: a room whose decay moves more than a fifth against the release before it goes back as it
  was, and a gated room that leaves its class always does.
- **Edge diffraction is not rendered in the catalogue.** Its first-order field matters most at the
  direct sound's shadow boundary; committed files exclude the direct sound, every receiver sees its
  centre source, and every sidecar says so.
- **V12 is a rules check, not a decode.** `release` checks every file against the consumer's WAV
  rules as read on its factory branch: one or two channels, 8–384 kHz, finite samples, at most 10 s.
  A decode through its reader waits for the plugin on `main`.
- **Sources:** omni. `centre` feeds the stereo file; `left` and `right` feed the true-stereo pair,
  rendered only when asked, and lie on the listener's left and right at both positions (a test
  holds it).
- **V9 gates only classes with a range measured in more than one room**, per octave or as a band
  mean or maximum (eight of
  100, all among v1's 21). Every added archetype is reported: against its class's range where it
  lies outside that class's size band, against a neighbour class, or against an estimate; a value passes inside the range widened by the 5 % JND, or a figure read-off's stated
  tolerance. The rest are reported against their estimate or single room.
- **Rendering and releasing are separate.** `render` writes every file at a 1 m reference into
  `target/`, with a full-response render and analysis for each rendered source, so every released
  file's metrics come from its own simulation. `release` confirms each file peaks at −1 dBFS (a render
  is linear in the reference distance, and a test holds it), checks every file against the consumer's WAV path, and
  replaces `impulses/` with the result and a manifest only after every file has passed.
- **A cached render is reused only if its key matches**: the three scene hashes, the options, the
  seam the wave solver will use (not only a room's override), the generator version and `RECIPE_EPOCH`, which a code change that alters renders must bump. A
  centre-only render ends its key with `sources centre`; a key without that line holds the pair.
  `v9` and `release` refuse a stale position rather than read it, and `release --true-stereo`
  refuses a position rendered without the pair.

### Rejected

- **Image sources alone:** no scattering, cost exponential in order, a metallic tail, no modes.
- **Statistical synthesis alone:** generic, with no room-specific early reflections, modes or
  coupled-volume double slopes. It survives only as the late renderer driven by traced histograms.
- **Full-band wave simulation:** a cathedral at audio bandwidth is about 10¹³ cells, and material data
  above a few kHz does not deserve it.
- **Existing room-acoustics software or commercial IR libraries:** third-party code or audio (root
  *Don't open existing implementations*).
- **Neural IR generation:** no rightful training data, and not physically verifiable.
- **A GPU wave solver in v1:** the CPU fits the offline budget.
- **A scene file format under `rooms/`:** it would need a parser; scenes are code.

### Chosen, not read

Each is marked where it is defined. Better evidence replaces it, never a guess.

- **Bands:** nine octaves, 63 Hz–16 kHz. Material data is extended outside 125 Hz–4 kHz by holding
  the edge values.
- **Fractional delay:** Blackman-windowed sinc, half-width 32 samples for early arrivals, 8 for late
  events, 16 solver samples for the wave part.
- **Minimum-phase design:** early filters on a grid of at least 4096 bins; phase-carrying wall
  filters on 16,384; late band and crossover filters on a 0.75 Hz grid.
- **Rays:** 128 equal-area direction cells; 2 ms bins; a ray ends 100 dB below its start (air
  included) or at the maximum time. Receiver radius `0.1·V^(1/3)`, clamped to 0.2–1.0 m and 80 % of
  clearance. Each ray scatters with one band's coefficient, drawn per ray, clamped to 0.01–0.99.
  Diffuse rain is derived from Lambert's law, not read.
- **Discovery:** its own purely specular rays, a sphere of `0.3·V^(1/3)` clamped between the receiver
  radius and 3 m, ending 60 dB down.
- **Late:** event rate `4πc³t²/V` clamped to 2,000–20,000 per second; envelope smoothing half-width 2 %
  of propagation time; a band ends 90 dB below its peak.
- **Boundary fits:** resistance, spring, mass and resonant branches at half-octave steps from 16 Hz
  to 24 kHz with damping ratios 0.3, 1 and 3; 120 log-spaced fit frequencies; reactance weighted
  0.05 in a band-absorption fit; absorption above 0.95 held at 0.95.
- **Frequency seam:** the Schroeder frequency from Eyring at 500 Hz–1 kHz, clamped to 60–1,000 Hz (400 Hz until 2026-09-15, which left the
  smallest rooms' 400–900 Hz modes to the geometric solvers);
  transition band half an octave either side; solver rate = top of band / 0.03. The crossover
  lowpass is a raised cosine in log-frequency across the band.
- **Wave solver:** excitation a unit-sum Blackman sinc, half-width 40, cutoff 0.1·fs; drift blocked
  by a second-order highpass at 10 Hz; the wave part faded out over the solver run's last 50 ms;
  a wave-solved render's merged output through a sixth-order highpass at 30 Hz; at most 16 threads
  and at least 32,768 cells per thread (measured: 1.07 × 10⁹ cell updates per second at 8–16
  threads, 0.40 × 10⁹ at 64).
- **Diffraction:** adaptive Simpson along the edge, split at the apex, each node spanning at most a
  quarter of a sample at the options' rate (48 kHz by default); leg visibility every 5 cm; a term
  exactly on a zone boundary dropped; at most one specular reflection on each side of the edge; a
  relative convergence floor of 1e-9, and a piece shorter than 1e-7 of the edge accepted.
- **Benchmark (V8):** the round robin's window (46 ms from 3 ms before the first arrival, 3 ms Hann
  in, 10 ms out); shape normalised over 250 Hz–4 kHz; a gated group passes when its median shape
  error is within 1 dB in every gated band, and a band gates only where its centre lies inside every
  shaping surface dataset's published range. BRAS third-octaves become octaves by the mean of three
  (absorption, scattering) or the power mean (directivity); the absorber's angle file is the one
  nearest each path's incidence; a thin partition is a knife edge; stands and transmission are
  left out.
- **Catalogue:** ORTF pair (17 cm, ±55°); 160,000 rays, by convergence (at 40,000 a hall's
  125–500 Hz decay times moved 5–10 %, the JND's size); image-source order 3; every render
  normalized to −1 dBFS by default; a 0.5 s fade at the 10 s cap; blend fractions per room;
  relief over a quarter of a part's area in a room too large to wave-solve (`RELIEF_SHARE`); half of
  every surface's remaining specular reflection scattered as detail (`DETAIL_SCATTERING`), chosen
  by the recorded-room benchmark above, not read.
- **Wave solving:** every room up to 1,000 m³ and no other (`WAVE_MAX_VOLUME_M3`): v1's small
  spaces reach 600 m³ and its halls start at 2,500 m³. The rule moved the small chapel (570 m³)
  and the stairwell (249 m³) to wave-solved. The seam is the Schroeder frequency, up to
  1,000 Hz, except in the four echo and tiled chambers, the water tank and the grain silo, where it
  stays below for cost and the room's note says so: cost grows with volume, the seam's fourth power
  and the run's length, and at Schroeder those runs would take one to twelve hours a source. The
  other six cost seams were raised on 2026-09-15; raising a lift car's seam from 400 to 738 Hz
  (1.04 million cells) took its simulation from 57 s to 322 s.
- **Air absorption:** evaluated at exact band centres. The ISO band method (clause 8) was not read.
  The humidity conversion (ISO 9613-1 Annex B) is as reproduced by secondary pages, not read in the
  standard. Speed of sound and density are ideal-gas forms. No ISO table value was available to test
  against, so the tests check structure and magnitude only.
- **Analyser:** time zero is the first sample within 20 dB of the energy peak. Band filters are
  zero-phase, power-complementary octave weights, not IEC 61260. Decay times are ordinary least
  squares over the ISO spans. A truncated response is not compensated. Band correlation divides
  by a 50 ms common envelope so a decaying tail weighs evenly.

### Boundaries of the crate

- Zero dependencies, MSRV 1.87, `publish = false`, a workspace member but **not** a default member.
  *Since the split (2026-10-06):* mxm-tools' workspace has no `default-members`, so a plain
  `cargo build` builds it.
- The workspace builds this crate optimised in the dev profile (root `Cargo.toml`,
  `[profile.dev.package.mxm-room-ir]`), because its tests trace hundreds of thousands of rays and run
  the wave solver.
- Library renders, the examples' output and the catalogue's render cache live under ignored
  `target/`. Only `catalogue -- release` writes `impulses/`.
- **Nothing depends on this crate.** Rolling it back removes content, not a plugin feature.
- Metrics are taken from a full-response render (direct path included, uncapped), never from a
  direct-free or capped file (plan §4.5). The sidecar field is named for that.
- **Wave solving costs minutes per small-room source.** Measured: a 63 m³ living room with fitted
  boundaries, 584,100 cells at 12.5 kHz for 3 s, took 136 s. Boundary cells dominate because
  fitted boundaries carry many branches.

## Work guidance, as written

- **Trust the analyser only through its controls.** A change to `analysis.rs` re-runs its synthetic
  decay tests first. A wrong analyser passes every room.
- **A new mechanism proves itself against a closed form first.** Image sources against Allen &
  Berkley's box; rays against the direct sound and the image-plus-hybrid identity; late synthesis
  against Eyring, the two-room model and `sin(kd)/kd`; discovery against the exact image sum; the
  wave solver against analytic modes and the boundary model's `R(θ)`; diffraction against the
  zone-boundary limit and reciprocity.
- **Change an estimator only with the no-double-counting test green.** It is the one place an
  energy leak or a doubling shows before a listener hears it.
- **Test diffraction where it carries the field.** In a live room reflections fill the window and a
  wrong diffraction model passes; use absorbing walls and a shadowed receiver.
- **Small rooms need furnished materials.** Empty-room data gave a plaster living room T30 4.2 s at
  125 Hz against 0.52–0.62 s measured in furnished rooms. The solver is right about the materials it
  was given; the catalogue must give it the room (plan revision 11).
- **Keep the FFT private.** `crates/mxm-fx-convolution-dsp` has its own, and a shared one waits for a
  second shipped consumer (root *crate rules*).

## What the tests prove

The tests prove:

- **V1:** free-field direct level `1/r` on sample zero, a sub-sample-exact fractional delay, the ray
  sphere estimator's calibration to the direct sound, and the wave solver's source level `1/(4πr)`
  within 3 %.
- **V2:**
  - Image sources: Allen & Berkley's image set for a rigid box through order 6, occlusion and the
    hand-worked first-order set in an L-shaped room, traced path lengths equal to image distances.
  - Modes: a rigid box's peaks within the scheme's dispersion bound.
  - Boundaries: the solver's reflection coefficient within 0.05 of the model at normal incidence
    and 0.08 at 45°.
  - Band absorption: fitted boundaries reproduce it within 0.02.
  - Passivity: no energy growth over 40,000 steps with resonant and fitted walls.
- **V3:**
  - In the transition band, image sources with the boundary models' `R(θ)` and the wave solver
    agree with correlation above 0.9 and level within 1.5 dB (measured 0.94, −0.5 dB).
  - Behind a corner with absorbing walls, diffraction raises that correlation above 0.6 and by more
    than 0.05 (measured 0.59 → 0.91).
  - The merged response's band energy lies between the two solvers' own in every sixth of the
    transition band.
  - The wave and geometric decay times in the seam's octave agree within 20 %: measured 15 %, the
    geometric tail running short, as the mean over six late-synthesis seeds. One draw lands anywhere
    from 5 % to 19 % short, so the 12 % first recorded was a single lucky draw.
  - A 4 m³ cabin's direct-free render keeps its energy below 12 Hz 30 dB under its whole energy and
    its 125 Hz and 250 Hz decays under 1 s, and its wave part fades out rather than stopping
    (measured on the retuned cabin: 56 dB under, 0.42 s and 0.21 s, the last 5 ms 46 dB under the
    50 ms before the fade; before the filter, 7.0 s and 12.0 s).
- **V4:** a uniformly absorbing, scattering box decays per Eyring within 8 % in every octave from
  250 Hz to 8 kHz; and with scattering rising from 0.3 to 0.99 across the bands.
- **V5:** weakly coupled live and dead rooms give the two-room model's late slope within 15 %, with
  an early slope under 60 % of it.
- **V6:** late echo density of a diffuse scene within 0.85–1.1, with no step between image sources
  and rays.
- **V7:** a spaced omni pair's octave correlation within 0.1 of the band-averaged `sin(kd)/kd`.
- **V10:**
  - Thread-count bit identity for renders, rays and the wave solver.
  - No non-finite samples, silence before the first kernel, and an exact final zero.
  - Time zero on the direct arrival, with late synthesis included.
  - Renders normalized to a −1 dBFS peak by default, equal to the physical render times the
    recorded gain, and physical on request.
  - Ray-assisted specular energy converging to 95 % of the exact image sum in a corridor.
- **No double counting:** image energy plus the hybrid deposit equals the sphere reference within 4 %.
- **Diffraction:** a box has no diffracting edge and an L-room exactly one; the onset tends to half
  the direct sound with opposite signs either side of the shadow boundary, also a few micro-radians
  from it with a bounded node count; responses are reciprocal; a crest on a floor has its path
  reflected on both sides, at its unfolded length.
- **Geometry and data:** prisms cut the floor into a closed room of the right volume whose seams
  do not diffract; measured directivity tables follow their grid and frame; WAVs read back in float
  and 16-bit PCM; octave band energies sum to the signal's.
- **V8** (`examples/bras_benchmark.rs`, not a test: its inputs live in the research checkout). Median
  octave shape error against BRAS measurements, dB, in each group's gated bands:
  - RS1 tiled floor ≤ 0.2; RS1 absorber ≤ 0.7;
  - RS2 plates, 1 m and 2 m, rigid and absorbing, ≤ 1.0;
  - RS3's first window ≤ 0.9;
  - RS5 1.0 at 250 Hz, ≤ 0.5 at 500 Hz, 2 kHz and 4 kHz, and 1.2 at 1 kHz, the one gated miss. The
    excess is unexplained and sits in the diffracted paths: the shadowed responses carry the most,
    and RS1, with the same loudspeaker, chamber and floor, is within 0.2 dB at 1 kHz.
  - Absolute median levels in RS1–RS3 are within 1 dB from 250 Hz to 2 kHz with BRAS's calibration.
- **V9** (`catalogue -- v9`: a report from the far positions' full-response renders, not a test,
  because rendering takes over an hour). Gated classes, against their range widened by the 5 % JND
  or a figure's tolerance:
  - all eight within: furnished living room, hard small room, car cabin, recital hall, shoebox
    concert hall, club venue, stone parish church, gothic cathedral. The recital hall is the closest,
    1.64 s at 500 Hz against 1.70 s (1.615 s widened) and 1.71 s at 1 kHz against 1.70 s (1.785 s
    widened). Successive blends move it between bands by about the ray noise.
  - Reported archetypes: 66 of 92 inside their reference. Outside it: rooms against a single measured
    room or a neighbour class (the horseshoe and drama theatres, small chapel, piano and rehearsal
    rooms, metro platform, both tunnels, bedroom, chamber and small concert halls, ice rink,
    concrete stairwell); the churches larger than the class's rooms (baroque, romanesque, basilica,
    domed, concrete); the vocal booth, deader than the EBU specification; estimates missed (cave,
    pub, car park, atrium, school corridor, city bus, pedestrian underpass).
  - **The detail share's render (2026-09-16) moved thirteen verdicts.** In came the recital hall
    (gated) and nine reported rooms: kitchen, walk-in closet, attic, open-plan office, string room,
    fan auditorium, ballroom, domed prayer hall and empty warehouse. Out went three estimates that
    are now short: the school corridor (0.66 s against 0.80 s), the city bus (0.36 s against 0.40 s)
    and the pedestrian underpass (0.90 s against 1.00 s). Without re-tuning it failed three gated
    classes. The living room ran 6–9 % under its floor at 500 Hz–4 kHz, the car cabin short at 250 Hz, and
    the recital hall further under at 500 Hz. Their blends were moved, not their rows:
    - the living room's drapes from 8 % to 5 % of the walls and seating from 24 % to 20 % of the
      floor;
    - the car cabin's trim from 35 % velour and 40 % carpet to 30 % and 45 %;
    - the recital hall's walls from 65 % to 30 % wood lining, with plasterboard for the rest, and
      its aisles from bare wood to carpet.
  - **Against the release before it**, 36 rooms moved more than a fifth in some octave, and the
    mid-band median ratio is 0.96. Mean EDT/T30 at 500 Hz–1 kHz went from 0.95 to 1.04. Rooms whose
    length came from sound skimming between flat walls shortened most: the school corridor from
    1.58 s to 0.66 s; the amp booth, broadcast and dead studios, lecture theatre and cinema 20–34 %.
    Open-ended spaces grew longer, because scattering keeps energy that used to leave along the
    axis: the road and rail tunnels 1.22–1.25 times at 500 Hz–1 kHz, the parking deck 1.34–1.42
    times, with C80 down 5–7 dB. The road tunnel, already long against its one measured tunnel, is
    further from it: 7.15 s against 5.6–5.9 s at 500 Hz.
- **V12** (`catalogue -- release`): every committed file passes the consumer's WAV rules before
  anything is written.
- **The analyser controls:** broadband noise decay against analytic T20, T30, EDT, C50, C80, D50 and
  Ts, and four octave-separated decays against their T.
- **Minimum-phase design, the crossover's complementarity, WAV header, ISO 9613-1 structure,** and
  the array patterns and frames.

## Not yet proved

Not yet proved:
- the added rooms against measurements: most have only an estimate, and the road tunnel has moved
  away from the one it has;
- a dome's tessellation: the domed rooms use five rings, and no finer tessellation was compared;
- the detail share's frequency dependence (only a flat 0.5 was measured), and whether rooms whose
  character is flutter between parallel walls (the tunnels, corridor, stairwell and echo chambers)
  lose it as the benchmark's open court did;
- the owner's listening (V11); a decode through `mxm-fx-convolution`'s own
  reader (V12), which is private on an unmerged branch;
- V8 at RS5's 1 kHz, and V8 below the frequency seam: the benchmark's source is directional, which
  the wave solver refuses, so no BRAS response is wave-solved;
- diffraction level against absorbing faces, and transmission through thin partitions;
- fitted boundaries below their fit range: their low-frequency branches let a small room's air
  leak through the walls, and a 4 m³ cabin carried a whole-room mode near 7 Hz. Wave-solved
  renders remove it with a sixth-order highpass at 30 Hz; the fit itself is not corrected;
- higher-order diffraction, and grazing incidence over faces narrower than the wavelength;
- Linux and macOS.
