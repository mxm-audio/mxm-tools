# AGENTS.md — apps/mxm-listener-hud

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

**Drop a sound, see what it is made of.** A window over `crates/mxm-listening`: any sound file
dropped on it is read by the listener and shown as a HUD — the pitch at the centre under a
crosshair, the parts of the sound orbiting it as glyphs, the sound through time along the bottom —
with every reading one wheel-scroll away, and the sound and its rebuilt parts to hear, a playhead
following. The plan is
`../../plans/plan-mxm-listener-hud.md` (`plans/plan-mxm-listener-hud.md` in the private archive); its phases name what
is built.

The owner's rulings (2026-09-28):

- **Its own identity.** *"It should have nothing to do with the design of the synths, player and
  plugins. It is its own thing. Made to be useful. And cool."* The design system and the
  standardisation preference govern plugin editors and the player; this app is the owner-named
  exception and takes nothing from `crates/ui`.
- **Unshipped, and the decision deferred.** *"If the interface is useful, it might be shipped. Defer
  that decision."* It is a bench, like `apps/mxm-layout-lab`: a workspace member, not a default
  member, with no bundle.

# Ownership

| Path | Scope |
|---|---|
| `src/worker.rs` | The one analysis thread: one running and one waiting, latest wins, cancelled at stage boundaries, a panic survived |
| `src/analysis.rs` | The stages asked of the listener (the load, its first stage, the curves, its perceptual models, the self-test's rebuild), the display name, and the notices worded from the load's facts and the report |
| `src/playback.rs` | Playback: the callback's transport, the window's controls, the queues between them, the sounds prepared for the device's rate, and the device itself |
| `src/scene.rs` | What the HUD shows, decided without drawing: the lock, the blips, the parts with their roles and key readings, the headline and the stages — each value a reading's, carried with its id |
| `src/theme.rs` | The palette, the roles' colours and shapes, and the drawing tools: additive glow, brackets, dashed rings, gradients, the sweep, the heat ramp |
| `src/scope.rs` | The centre: the note ring (pitch class around, octave outward) or the radar (time around, frequency outward), the crosshair and the coordinates under the cursor |
| `src/glyphs.rs` | The parts on their orbit, the wheel's zoom, the callout — its place, its leader line and its panel — and each reading's explanation (glossary, threshold, the owner's words) |
| `src/figures.rs` | What the inspector draws at each zoom, decided without drawing: a part in context, its readings over windows, across bands, as heat maps or on their units' scales, its tables as plots |
| `src/plot.rs` | Draws the figures: axes, curves, marks, bars, heat maps, gauges and table plots, each a widget whose hover explains the reading or row under the pointer |
| `src/timeline.rs` | The strip along the bottom: the spectrogram, the waveform, the envelope, the stages bracketed |
| `src/list.rs` | The LIST view: every reading at once |
| `src/hud.rs` | The window: its state, the family ring, the name's claims, the PLAY row and the playhead, the log, the layout and the style |
| `src/main.rs` | The window: eframe on wgpu with the player's backends |
| `examples/listener_hud_timing.rs` | The stages' times on real files, a local manual run |
| `examples/listener_hud_shot.rs` | The HUD rendered to PNGs with a sound dropped on it — one without a zoom and one per `--zoom PART:LEVEL`, the sound read once — for reviewing the look without the window; a local manual run |
| `tests/` | `worker` (the bounds, with a held analysis), `scene` (the lock, the blips, every number a reading's, NO SIGNAL), `figures` (every reading of a part once at zoom 2, each mark at its reading's own value, window or band, tables as the plots their columns call for, the waveform's columns the samples' own, a callout beside its glyph towards the centre), `playback` (the transport's start, switch, stop and end; a buffer given back, never freed in the callback; a full return queue holding a command back; a sound prepared for the device's rate; and, by hand, the default device playing silence) and `window` (a stale reading discarded, no path shown, a drop read end to end, the ring re-reading, the name held against the reading, the wheel's zoom in and out with a reading explained on hover, a suggested family one click away, the PLAY row enabled once the rebuild lands) |
| `LICENSE` | MIT |

# Local Contracts

- **Decoration may be invented; numbers may not.** Every number on screen is a reading, a table cell
  or a curve point the listener produced, or a conversion of where the cursor points (its frequency
  and note); this app formats them and never computes one about the sound. Motion that stands for
  work — the fast sweep, the blinking cursor, the elapsed time — runs only while the listener works;
  the turning rings are the look. `tests/scene.rs` traces every shown value to its reading.
- **Interpretation stays in the listener.** Anything the window needs that says what a sound does —
  a curve, a glossary line, a fact about the decode, a threshold, the owner's words — comes from
  `crates/mxm-listening`, not computed here. The note name under the crosshair is the listener's own
  conversion (`parts::note::nearest_note`).
- **Five roles, each one colour and one shape, fixed everywhere**: time amber triangles, the ring cyan
  circles, noise magenta diamonds, character violet hexagons, the recording chain steel squares
  (`scene::Role`, `theme`). A part's place on the orbit is fixed too (`glyphs::place`). **Colour
  never carries meaning alone** (root *User Preferences*: the owner is red-green colour-blind): no
  red/green pair, and an agreement is said in words.
- **The wheel zooms, a part at a time, and every zoom is graphs, not lists** (the owner: *"Cant they
  be graphs?"*). Over a glyph, up goes one level deeper, down one back, and down past zoom 1 closes.
  Zoom 1 is the part in context: the envelope with its windowed levels marked, or the spectrum with
  its frequencies marked and its modes or partials as stems (the hearing model's part: its curves
  over time), its times as bars on one scale, its key readings on gauges. Zoom 2 is every reading,
  **each exactly once** (`tests/figures.rs`): over its windows, across its bands, over both as a heat
  map, or — one value — on its unit's scale (`figures::scale`; a plain number stays a number). Zoom 3
  is the part's tables as plots — stems over frequency, lanes over time or level — or their numbers
  where no plot fits, and the sources. A mark's place is its reading's own value, window or band;
  every mark explains itself on hover: the glossary, where it was measured, the smallest change heard
  (the owner's thresholds from `.listening/` over the literature's, as `listen` loads them), the
  owner's words, and the number. LIST keeps every number at once.
- **The zoom is a callout over the scope, never a side panel** (the owner: *"it woul be better if they
  showed up as a window on top of the main graph. With a line going to the the thing we are zooming
  into"*). The scope keeps the whole middle; the zoomed part's figures float in a foreground area
  beside its glyph, on the side towards the centre so the glyph stays in sight, level with it where
  the middle allows (`glyphs::callout_place`), and a leader line runs from a ring round the glyph to
  the panel's edge. The panel is as tall as its figures up to most of the middle, then scrolls: an
  egui area gives its contents last frame's size unless told otherwise, so the panel sets its own
  allowance or it would never grow. Escape, × or the wheel down past zoom 1 closes it.
- **The waveform is the samples themselves**: each pixel column's lowest and highest, from the
  onset, on the strip's shared axis, scaled to the sound's peak and labelled with its `level.peak`
  reading; pooled once per sound and width, not every frame.
- **Glow is additive light.** A colour with alpha zero adds its light under egui's premultiplied
  blending; a closed glowing path must not repeat its first point, or the thick strokes' miter spikes
  (`theme::glow_circle`). A real bloom is the plan's H4.
- **The look animates with `request_repaint_after`**, never an immediate repaint: kittest's `run`
  waits only on immediate ones, so the tests stay deterministic.
- **wgpu with the player's features, never `glow`.** Features unify across one build graph; asking
  for `glow` here would switch the player to OpenGL (`apps/mxm-layout-lab/Cargo.toml` records the
  break). `cargo build --workspace` is the check that catches it. kittest's `wgpu` feature is a
  dev-dependency, for the shot example.
- **No path leaves the drop.** A sound is named by its file's stem; the path goes to the loader and
  is kept in memory to read the sound again when the ring turns, and never reaches a label, a
  notice, a report, the log or a file. The decoder's refusals carry no path.
- **One worker, bounded, latest wins.** A new drop replaces the waiting request and stops the running
  one at its next stage boundary; every update carries its request's sequence number and a stale one
  is discarded. A new drop waits at most one stage; the measured stage times are in the plan's
  revision table.
- **The callback only copies** (`playback::Transport`). Everything that costs — resampling to the
  device's rate with the listener's resampler, converting, allocating — happens on a thread of its
  own before a sound reaches the callback, whole, as a shared buffer through a lock-free queue
  (rtrb). The callback takes commands at the start of a block, writes the current buffer to every
  device channel in the device's sample format (f32, i16, u16, i32), and publishes where it is. It
  never allocates, frees, locks, logs or waits: a buffer it lets go of goes back through a second
  queue for the window to drop, and a command waits in its queue until there is room to give the
  current buffer back.
- **The device is opened on the first play**, never before, at its own rate and format; a stream
  error stops playback and says why, and the next play opens it again. Tests never open a device:
  the transport is tested without one, and the device by hand (`--ignored`), playing silence.
- **What plays is named for what it is**: LISTENED (the mono buffer the listener read, cut at its
  limit — not "the original"), MODES and NOISE (the self-test's rebuild in parts) and REBUILD (the
  whole). The rebuild keeps the listened sound's time base, so one playhead maps onto every buffer;
  the timeline's cursor, the radar's sweep and the part glyphs whose key windows it passes follow it.
- **The name is a claim, shown beside the reading, never in its place.** Its checks are said in
  words — AGREES, DIFFERS, SUGGESTS, NOT MEASURED. A family it suggests is offered as a button; its
  note reaches the listener (`expect_hz`) only while the owner ticks the checkbox, which starts off.
- **Notes, held notes, voices and impulse responses are declared, never detected.** The ring's AUTO is
  the listener's own detection; HIT, not DRUM, because the listener establishes a percussive
  envelope, not a drum.
- **Tests use synthetic sounds only.** A recording is read only by the manual examples, on the
  owner's machine; no recording, derived audio, rendered picture of a recording or recording path is
  committed.
- **Windows is verified; Linux and macOS are not.**

# Work Guidance

- Read the plan's current phase before adding to the window; a feature from a later phase waits.
- Test with `--release`: the listener's full reading is slow unoptimised.
- Look before and after a change to the look: render it with `listener_hud_shot` and read the PNG.
- A running window locks `target/release/mxm-listener-hud.exe` on Windows, and a release test build
  then fails to relink it. Never stop the owner's window to test: build into another target folder
  (`CARGO_TARGET_DIR=target/hud-test`).

# Verification

```bash
cargo test   -p mxm-listener-hud --release
cargo clippy -p mxm-listener-hud --all-targets
cargo build --workspace                 # REQUIRED: the only check that catches feature unification
cargo run    -p mxm-listener-hud --release
cargo run    -p mxm-listener-hud --release --example listener_hud_timing -- <file>… [--family note]
cargo run    -p mxm-listener-hud --release --example listener_hud_shot -- <file|-> <out.png> [--mode ring|radar|list] [--zoom PART:LEVEL[,PART:LEVEL…]] [--family …]
cargo test   -p mxm-listener-hud --release --test playback -- --ignored   # by hand: the default device plays (silence)
```

# Child DOX Index

No child AGENTS.md files.
