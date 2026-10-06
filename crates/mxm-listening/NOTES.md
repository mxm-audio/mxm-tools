# NOTES.md — crates/mxm-listening

The detail behind this folder's AGENTS.md: history, measurements, rationale and worked examples. AGENTS.md is the contract; this file is the reference it links to.

**References.** `plans/plan-mxm-listening.md`, `plans/plan-mxm-listener-hud.md` and
`scripts/merge_gate.py` are paths in the monorepo this crate came from; they now live only in the
private archive (`archive/01-mxm-collection/`), not in this repository. `docs/drum-model-fitting.md`
(the guide) and `crates/mxm-drum-machine-dsp/tools/ab_metrics.py` were monorepo paths too and are
now public in the mxm-drum-machine repository:
[`docs/drum-model-fitting.md`](https://github.com/mxm-audio/mxm-drum-machine/blob/main/docs/drum-model-fitting.md)
and
[`crates/mxm-drum-machine-dsp/tools/ab_metrics.py`](https://github.com/mxm-audio/mxm-drum-machine/blob/main/crates/mxm-drum-machine-dsp/tools/ab_metrics.py).
There is no merge gate since the split (2026-10-06). A `research:<path>` citation names a page in
MXM's private research repository.

## Why the listener exists, and how it was built

It exists because the owner hears a difference between a render and a recording at once and the
agents that tune the collection cannot listen: fine-tuning had become the owner describing sounds
("washy", "rings a little long", "higher pitched") and an agent hunting for the measurement behind
each phrase. The product plan is `plans/plan-mxm-listening.md` (in the private archive);
its order is the owner's: **percussion first, other physically modelled sounds second**, then synth
voices and effects, which the owner called for with "finish the listener" (the plan's revision 29).

**Status: finished** (the plan's revision 36), built slice by slice. **L2:** preparation, the numerics, the percussive family; `describe` for a hit's attack,
level, decay, pitch and modes, tone over time, tonal or noise, texture, modulation and artefacts;
`set` for round robins and velocity layers; `compare` of two sounds, or of a sound against a set's
takes, with thresholds, ranking, the owner's vocabulary and the unexplained-difference map; the
resynthesis self-test; the golden cases; `site` for a whole A/B page. **L3's machinery is built**
— perturbations of a real sound, staircases, a simulated listener recovered, the session page,
`calibrate` — and the owner's first sessions gave the owner's own thresholds, kept locally; further
sessions only when a decision needs one (the plan's revision 17).
**L4's perceptual models are built**: loudness, tonality, roughness and fluctuation strength after
ECMA-418-2, and sharpness on its specific loudness, read over a hit's stages. **L5 is built**: `split`
cuts a recorded run into its notes, struck or held; the **pitched note** family reads a note's
partials, its decay and level laws, its nearest ideal object, a string's series, a guitar's and an
electric piano's own readings, and compares two notes partial by partial; the **sustained note** reads
how a held pitch holds; `keymap` reads how those move across an instrument's compass. **The families
deferred at first are built**: `stimulus` writes what an instrument or an effect is played, with a
sidecar recording exactly what it was, and `respond` reads a response back against it — an impulse
response's frequency response, taps and space, and an effect's distortion, intermodulation,
dynamics, noise, moving delay and stereo; the **synth voice** reads a held note's aliasing,
harmonics, envelope stages, clicks, zipper, unison and filter peak; a note has calibration operators
of its own. Every description carries the **words** for its timbre with the numbers behind them, and
`explain` says what a phrase of the owner's has meant. **For the listener's window**
(`apps/mxm-listener-hud`, `plans/plan-mxm-listener-hud.md`): `describe` in two stages, what a file's
name claims, a glossary of every reading, and the curves a window draws.

## Ownership, in full

| Path | Scope |
|---|---|
| `src/prep.rs` | Decode to mono, onset, trim, K-weighting and the body loudness two sounds are matched on — **the drum A/B page's own definitions**, which the page now calls; the whole-file decode a run of notes needs; and what a hit's decode kept (`Kept`: the file's channels, a cut at the limit, the codec), which `Sound::load_kept` returns beside the same samples |
| `src/split.rs` | Cutting a run of struck notes into single notes, named from the run's range, and the check of each name against the cut's own peak |
| `src/numeric/` | `Complex`; the one-sided Jacobi SVD, Hessenberg–Francis QR eigenvalues and Householder least squares |
| `src/repr/` | Zero-phase Butterworth band-passes, window levels and envelopes, Hann-windowed spectra and the statistics read off them (centroid, roll-off, A-weighted median, resonance and Q); anticausal FIR filters and the analytic signal (`fir`); the high-resolution mode analysis and the joint refit (`modes`); band-limited resampling (`resample`); Hann-windowed complex demodulation from running sums (`demod`); steady lines at known frequencies through a Blackman–Harris window (`lines`) |
| `src/stimulus.rs` | The stimuli — impulse, Farina's sweep, bursts, level steps, one and two sines, noise, silence, a note — rendered deterministically, and their sidecars: written, read, and checked by fingerprint |
| `src/respond.rs` | A response read against its stimulus: deconvolution into an impulse response, the true-stereo matrix, and readings against a setting |
| `src/perception/` | ECMA-418-2 from its text: the hearing model (`hearing`), tonality and loudness (`sottek`), roughness (`roughness`), fluctuation strength (`fluctuation`), and sharpness on the specific loudness (`mod`) |
| `src/family.rs` | The sound's family, declared or detected (a pitched note and an impulse response are declared only; an effect response comes from `respond`) |
| `src/objects.rs` | The ideal objects a note is read against — string, tubes, bars, tuned bars, membrane, plates — as partial ratios, computed from their equations where the research table was |
| `src/reading.rs` | `Reading`, `Section`, `Table`, `Report`: unit, window, band, resolution, validity, source |
| `src/parts/` | One analyser per part of the sound (`sustain`: a held note's pitch holding, drift, vibrato, tremolo and noise): `attack`, `level`, `decay`, `pitch` (modes per window, rest pitch, the attack's pitch, glide, salience), `tone`, `tonality` (lines against resonant noise), `texture` (buzz, the rattle's share, wires, clustering, slap sharpness, rhythm), `modulation` (the ring's wobble, width and share, beating pairs, roughness), `artefacts` (click, floor, hum), `perception` (loudness, sharpness, tonality and roughness over the stages), `note` (a pitched note's onset, partials, fundamental, decay and level laws, nearest object), `impulse` (an impulse response's frequency response, taps and repeats, and its space through `mxm-classic-verb-fit`), `effect` (distortion by order and DC, a tone's wobble, intermodulation, a compressor's curve and times, noise and runaway on silence, bursts, a moving delay on noise, and a stereo output), `voice` (a synth voice's aliasing and its audibility under masking, harmonics by order, DC, envelope stages, clicks, zipper, a unison's beating, a peak on the series and a line off it), `words` (brightness, depth, warmth and hardness, as the numbers behind the words; boominess is read in `perception`) |
| `src/set.rs` | Sets: each group's readings as mean and spread, the velocity map, and the room's lines |
| `src/describe.rs` | `describe`: the parts in order, and the recording-chain warts as notes; in two stages, the perceptual models second (`describe_staged`, `Staged::perceive`) |
| `src/name.rs` | What a file's name claims — a note, a run, a family word, a tempo, a dynamic, a loop — and each claim held against a report |
| `src/glossary.rs`, `data/glossary.tsv` | What every reading, table and curve means, in plain words |
| `src/curves.rs` | What a window draws: the decay envelope, the spectrum in 1/24-octave bands, and a spectrogram on the unexplained map's frames |
| `src/audibility.rs`, `data/thresholds.tsv`, `data/vocabulary.tsv` | Each reading's audibility threshold with its source, the owner's own laid over from `.listening/` (`Thresholds::with_owner`, which `listen` and the window share), the owner's phrases for each direction, a phrase explained as the rows that hold it, and a reading's every phrase (`Vocabulary::for_id`) |
| `src/keymap.rs` | Key tracking: each note reading as a line against octaves from middle C over a set of notes |
| `src/compare.rs` | Findings, ranking, established differences against a set, the headline, the explanations of rule readings, and two notes' partials matched by ratio |
| `src/unexplained.rs` | The auditory map: where two sounds differ and no audible finding says why; its ERB frames, which the spectrogram shares |
| `src/resynth.rs` | The self-test's rebuild from the readings; `tracks`, its modes linked window to window, public for the drum models' analysis (`mxm-model-drums-dsp`'s synth kick fit) |
| `src/golden.rs`, `data/golden.tsv` | The golden cases: what the owner heard and what approved sounds keep, scored |
| `src/perturb.rs` | Known changes of a real sound — level, pitch, decay, a band, attack, noise, wobble, a click; and a note's tilt, decay law, inharmonicity and vibrato — each naming the reading it moves |
| `src/staircase.rs` | Levitt's 1-up-2-down staircase, a seeded random source, and the simulated listener |
| `src/session.rs` | A calibration session's trials, answers, outcomes in each reading's unit, its files, and `calibrate` |
| `src/bin/listen/serve.rs`, `data/session.html` | The session served to a local page |
| `src/report/` | Reports, sets and comparisons as hand-written JSON and as Markdown |
| `src/bin/listen/main.rs` | The `listen` command |
| `tests/` | Closed forms and synthetic ground truth: `numeric`, `prep_and_bands`, `describe`, `modes`, `parts_l1`, `tonality`, `compare` (perturbations and their sizes), `resynth` (the early window, the refit, the rebuild — a drum's and a note's — the explanations, the unexplained map), `golden`, `calibration` (the simulated listener recovered, a session's answer in the reading's unit, each perturbation's reading — a drum's and a note's — the owner's table and its precedence) — **release-only**, ignored with the reason `release gate` and run by `scripts/merge_gate.py --full` (`cargo test -p mxm-listening --test calibration -- --ignored`), because it takes minutes in a debug build (the owner, 2026-09-30), `perception` (the standard's calibration points, the acum reference, a gain changing nothing, the vacil reference and the fluctuation band-pass), `split` (a run through a deep tremolo; a pianissimo strike beside a loud low knock and a second strike of the note before; a run of soft-started held notes), `note` (the objects against the research table; a free bar's and a split tuned bar's laws, object and decays; a note struck over the last one; two notes compared partial by partial; a key map of a decay halving each octave; a note named by the caller over a sympathetic line; a tubular bell heard at its strike note; a stiff string's B and pluck point; a bar without a series; a two-stage decay told from a damper; a body resonance that stays put across a set; a held note's vibrato, drift and settling; a body line found across a set and kept out; two polarisations' beating, a strum's strings, a pickup's harmonics; a steady rich tone's own fundamental; a pluck's and a clarinet's harmonic deficit), `respond` (every stimulus through its sidecar and back, an edit refused; Farina's sweep; a resonant low-pass's peak, Q and latency, a high-pass's edge; a feedback delay's time, feedback and darkening; a decaying noise's T60; a polynomial's harmonics and DC exactly; tremolo and vibrato; a square law's intermodulation exactly; a compressor's threshold, ratio, attack and release; a line and a runaway on silence; a stereo chorus's sweep and the channels' phase; a fixed filter heard through noise; width, correlation and the true-stereo matrix; readings against a setting; bursts; every stimulus and a voice keeping the invariants at every rate), `voice` (a naive saw's audible aliasing against an additive one's none; an ADSR's stages; a hard note-off's click; a gain stepped every 64 samples; a detuned unison; a resonant filter's peak and a singing filter's line; a bent sine's distortion and an offset's DC), `sweep` (the §6 sweep after the drums: a struck note's tuning, T60, tilt, decay law and inharmonicity, a held note's tuning and vibrato, a voice's attack and sustain, an impulse response's edge, delay and T30, each at 0.5, 2 and 4 thresholds), `words` (each word's band energy exactly; a fast attack harder than a slow one; a low tone boomier than a high one; the words never compared; a phrase explained; a reading's phrases and the owner's thresholds from a local folder), `glossary` (every id in the source explained and every row an id, every id-shaped literal classified, no id made at runtime, every id a report carries found by the scan), `name` (a name's claims; each held against a report), `curves` (a damped sine's envelope rate, spectrum and spectrogram peak at three rates; the report's onset; silence draws nothing); `describe` also holds a reading made in stages |

## Local contracts, in full

Each contract as it was written, with its reasons and measurements.

### What lives here, and what it depends on

- **Interpretation lives here; rulers live in `mxm-measure`.** Three unshipped measurement crates,
  three jobs: `dsp-lab` holds harnesses, `mxm-measure` named computations with units and never a
  threshold, and this crate the onset and level-match policy, analysis windows, thresholds, verdicts
  and vocabulary. A computation whose answer does not change when someone changes their mind about
  what is acceptable, and that a second crate needs, belongs in `mxm-measure` under its extraction
  gate. Filter banks and estimators stay here: `mxm-measure` refuses a ruler built from the thing
  under test.
- **Never in a shipped graph.** `mxm-measure` is a normal dependency here, as in `dsp-lab`, because
  this crate has no shipped graph to protect; everything else takes this crate as a dev-dependency,
  except the unshipped `apps/mxm-listener-hud`, which takes it normally. Shipping that app is the
  owner's deferred decision, and the leak check forces it first.
  `mxm-classic-verb-fit` is a normal dependency too, for its analyser alone (`analyse`): an impulse
  response is read as a space the way that crate reads one, never fitted here.
  `mxm-measure/AGENTS.md`'s leak check (mxm-kit's `crates/mxm-measure/AGENTS.md`; the check itself
  runs in the product repositories since the split) covers both names.
- **No external crate.** The owner chose hand-rolled numerics (the plan's revision 1). JSON is
  written by hand, and read only as far as an A/B site's flat mapping needs; data files are plain
  TSV. The internal crates bring their own pinned dependencies (hound, symphonia through
  `mxm-audio-file-decode`).
- **The A/B page's definitions are part of the contract.** `prep.rs` holds the drum A/B page's decode,
  trim, K-weighting and body gating operation for operation, and the page calls them: the move was
  proved by rendering the page before and after (98 files — 94 models and four recordings — byte for
  byte). Changing a definition there changes every page and report: it is a **correction**, measured
  on every page row and approved, never a tidy-up. The same holds for every reading's definition.

### Onsets, readings and the decay envelope

- **One onset per family.** A percussive hit's onset is the first sample at 1 % of the peak
  (`prep::ONSET_FRACTION`), and every window is measured from it. Two sounds are compared reading by
  reading, each from its own onset and against its own level, so no waveform alignment is needed;
  sub-sample alignment arrives with the first measure that compares waveforms.
- **Never NaN; absent rather than zero.** A reading's value is finite or absent, and an absent reading
  says why. A buffer holding a non-finite sample is not measured (a DSP failure is not laundered into
  a measurement); silence is reported, not measured; a band under the numerical floor (−100 dB) is
  numerically silent, not a level.
- **Every reading says where and how finely.** Its window (ms from the onset), band, resolution —
  every spectrum's window and bin spacing, every Q's smoothing span — its validity (`BelowFloor`
  under −45 dB of the sound's peak, `Sample` for a single render's draw of a random quantity) and
  the source of its definition.
- **The decay envelope is one period long** (2–20 ms, sliding every 1 ms), not `ab_metrics.py`'s fixed
  2 ms: on a 220 Hz tone a 2 ms window swings by several dB with the phase and moved `t20` by 3 %.
  A correction, stated on every decay reading's resolution.

### Modes, the rest pitch and the drum readings

- **The mode analysis follows `research:listening/modal-estimation.md`.** Anticausal FIR filters only
  (an IIR adds its own poles as modes; a centred FIR puts its transient on the onset); each band kept
  in its first Nyquist zone so the analysis stays real; ESPRIT on a Hankel matrix a third of the
  window tall; the order by ESTER, deliberately generous, then pruned: growing poles, poles outside
  the band, a mode holding more energy over the window than its band does (part of a cancelling
  structure), or more than twice what the file itself has in that band and window. At least 120
  samples per fit: a 34-sample fit invented a mode 37 dB above the sound. The mode windows start
  after the stick's contact and are short early, because a glide inside a window leaves a phantom
  mode; a mode is **believed** when it recurs in another window.
- **A window reads itself.** Each filter stage spans at most a quarter of its window
  (`FILTER_SHARE`), so the analysis lags the window by a quarter of it at most. Uncapped, a low band's
  filters spanned 200–500 ms and "10–60 ms" read about 110–160 ms: the self-test found the snare's
  strongest early mode missing. **The bands find the modes; the window's own samples say how strong
  each is:** every window's modes are refitted together by least squares on the window's band-limited
  samples (`modes::refit`), because a band's own fit, carried back along its decay to the window's
  start, overshot a fast early mode by 6.5 dB.
- **The rest pitch is the lowest believed mode within 12 dB of the strongest, never a room line.** A set's
  room lines are late lines recurring across its files that are **not** one of the drum's main modes
  — round robins of one drum share its ring too — and `describe` keeps them out of the pitch and the
  ring when the caller knows them (`--room`, or `set`). The ring's strongest mode is its own reading
  (`pitch.strongest`, `ab_metrics.py`'s `late Hz`). A rule reading moves by an interval for a dB or
  two near its line: when two sounds' rest pitches are different modes, the comparison says which
  modes and by what margin, and the readings taken at the rest pitch join its headline group.
- **The glide is a line track**, the guide's two-mode measure (40 ms windows against the 180–260 ms
  median): mode matching across windows cannot see a glide.
- **Definitions kept for continuity, with their reach stated.** The ring's wobble is the snare
  scripts' (the ring's ±15 % band, a 30 ms average removed, 15–300 Hz): it passes modulation only up to
  15 % of the ring's frequency and part of a slow one, and the reading says so. A line narrower than
  its window resolves is marked `Unresolved`, not reported as the sound's width.
- **Fall times count a lasting fall**: the envelope must stay under the level for 10 ms, on a window
  one period of the strongest early line long. `ab_metrics.py` took the first dip, which a snare's
  wires make early (a correction, recorded in the plan's revision 10).
- **Texture and modulation readings of one render are samples** (`Validity::Sample`): a single draw
  of a random quantity, compared across round robins, never trusted alone.

### The perceptual models

- **The perceptual models follow ECMA-418-2's text** (4th edition; `research:listening/perceptual-measures.md`
  §7), the one perceptual standard published in full; no implementation was opened. **Proved on its
  own calibration points:** a 1 kHz tone at 40 dB SPL reads 0.9988 sone_HMS and 0.9991 tu_HMS; a 1 kHz
  tone fully modulated at 70 Hz at 60 dB SPL reads 1.04 asper — a 4 % deviation not yet found, where
  the standard allows 0.25 %, recorded on every roughness reading rather than calibrated away. The
  Moore–Glasberg model (ISO 532-3) is not built: its filter and gain tables are in no document read.
  **Sharpness** uses DIN 45692's weighting in its unverified textbook form on this model's specific
  loudness, calibrated here to the acum's definition (1 kHz narrow-band noise at 60 dB SPL reads 1
  acum; the textbook constant read 1.043).
- **The models hear the sound as the A/B page plays it.** A file carries no level: each sound is
  scaled so its body loudness (`prep::body_rms`, what the page matches on) is −20 dB K-weighted against
  full scale, and the owner's rule maps a full-scale sine to 94 dB SPL, diotic, free field. A gain
  alone changes no perceptual reading (tested); the file's own level stays in the `level` readings.
- **For a hit, the time-dependent values, never the single values.** Every ECMA single value
  discards the first 300 ms. The standard's loudness (Clause 8) smooths its tonal and noise parts at
  3.5 Hz and lags a snare's peak by about 100 ms, so it is quoted for its peak only; timing, the stage
  windows and sharpness come from the hearing model's basis loudness (Clause 5), which only its blocks
  smooth (21 ms above 340 Hz, 171 ms below 85 Hz). Tonality lags as the loudness does; roughness's
  341 ms blocks smear the first 100 ms. **Fluctuation strength** (Clause 9) reads a held note
  (`sustain.fluctuation`), its 1.37 s blocks too long for a hit: a 1 kHz tone fully modulated at
  4 Hz at 60 dB SPL reads 1.025 vacil (a 2.5 % deviation, recorded as roughness's 4 % is), its
  band-pass falls away at 1 and 16 Hz, and Iowa's flute E5 reads 0.40 vacil with vibrato, 0.21
  without. Two readings of the extracted text are stated in `src/perception/fluctuation.rs`: the
  window's exact Dirichlet kernel, and the fine tuning's step bounds in normalised frequency.
- **`describe` can leave the models out** (`Options::without_perception`): they cost about as much as
  everything else together, and a caller that reads other parts only — a calibration's operators,
  most tests — skips them.

### Comparison, the unexplained map and the self-test

- **A difference is measured in audibility thresholds.** Each reading's threshold comes from
  `data/thresholds.tsv` with its source (literature, or chosen and said so); the owner's own
  thresholds will stay local (the plan's revision 8). A relative threshold may carry a floor, the
  smallest value perceived at all (0.07 asper, 0.01 sone), under which values compare as the floor.
  A finding ranks by its thresholds, capped at
  twenty, times how loud its window is, and halves when an EQ or a fader could make it — **ranked
  lower, never hidden** (the owner's full-band rule). Octave levels are compared as spectral shape,
  each band against the whole at that moment. They start at 25 Hz: the octave under 50 Hz, where a
  kick's lowest push sounds, is read over 40 ms, one of its own periods, and says so in its
  resolution (a 10 ms window holds a quarter of one, and its level would swing with the phase); the
  tonal share keeps its bands from 50 Hz (the kick fine-tuning plan, 2026-09-28). Against a set, a difference is **established** only
  when it exceeds both its threshold and the takes' spread and holds against two thirds of them.
  The file's own properties (its end, floor, hum) are listed apart.
- **What no reading explains is reported.** The auditory map compares both sounds patch by patch on
  an ERB-spaced spectrogram; a patch differing by 3 dB is explained only by an audible finding whose
  window meets the frames that differ and whose band meets the patch's. A gap in the analysers shows
  instead of hiding.
- **The self-test rebuilds a sound from its readings** (the owner's revision 9): the recorded contact,
  the modes as tracked partials with cubic phase through every window's measured frequency and phase
  (McAulay–Quatieri), and the rest as third-octave noise on 2 ms envelopes, corrected once; then
  `compare` reads the rebuild against the sound. **Its known limits**: a mode is stationary inside
  its window, so a partial that glides or changes its decay there leaves a coherent residual, which
  the rebuild plays as noise at the same level; ESTER then finds fewer modes in the rebuild than in
  the sound, and a weak mode near the rest-pitch line can vanish from it. Read a self-test's findings
  with that in mind; the two faults it found in the listener are recorded under *A window reads
  itself*. A note, a held note or a voice is rebuilt from a note's analysis (`selftest --family
  note`): the drum analysis stops at 2 kHz and keeps only decaying modes. A mode the next window lacks
  because it decayed under that window's −60 dB floor keeps decaying to the end, and only one the next
  window should have held (a gliding mode) fades out: cutting a decayed mode off bent a note's fastest
  partials' fall, and the line through it put their strike 2 dB high.

### Golden cases, calibration and the session

- **Golden cases are data and never recordings.** `data/golden.tsv` holds what the owner said about
  which sounds (the plan's revision 8: the finished snare and the model-drums revisions 18–21), each
  sound a logical name; a local map, never committed, names the files. A phrase counts when a reading
  it means lands in the first three headline groups, a known deviation of an approved sound when it
  is audible at all, and an approved pair's every audible difference is a false alarm. A miss is
  reported, not tuned away: the thresholds and ranking are set by the literature and the owner's
  calibration, not by the cases.
- **Calibration measures the owner's ears the way the thresholds are used** (the plan's §5).
  Trials are odd-one-out of three: the owner's own sound twice and once changed by one operator,
  loudness-matched unless loudness is the change, played at one gain so their levels stay the trial.
  One 1-up-2-down staircase per operator, the ones with fewest trials taking turns, walks towards the
  size answered 70.7 % correct; every eighth trial is an obvious check. **Every answer counts:**
  `calibrate` measures each answered change in its reading's own unit (the sound changed by it,
  measured — an operator never encodes its reading's formula) and fits the owner's threshold to all
  answers of all sessions by maximum likelihood, with a 95 % range; a reading enters the owner's table
  once it has eight answers and a range bounded both sides. A staircase's own reversal estimate needed
  more trials than a six-minute session holds: the owner's first session finished none of three.
  The session's clock starts at its first trial, and its local log names the sound's file so
  `calibrate` can measure it again. **The machinery is proved before the owner sits:**
  a simulated listener's threshold comes back within 15 % over 200 runs, and a whole session over a
  real sound gives back a simulated listener's level threshold in dB. **A note has operators of its
  own** (the plan's revision 29, part C), for a session declared `--family note` or `sustained`: its
  tilt (a zero-phase gain rising by octaves above the fundamental), its decay law (a gain falling with
  time and frequency, frame by frame over a short-time transform), its inharmonicity (each harmonic's
  band, as an analytic signal, shifted by the stiff-string law) and a vibrato (a swept delay read by
  cubic interpolation). Each moves its note reading past its threshold at its starting size and leaves
  the loudness and the tuning where they were. The sessions on notes are the owner's to sit.
- **The owner's thresholds stay local** (the plan's revision 8): sessions and the table `calibrate`
  writes live in `.listening/`, ignored by git, and `compare`, `selftest`, `golden` and `site` lay
  that table over the literature's when it exists, saying so in their notes. Precedence: the owner's
  value in the sound's context (its family), then the owner's in any, then the literature's.
- **The session server binds 127.0.0.1 only**, answers one request at a time, and serves nothing but
  the page, the pending trial's three sounds and the session's answers; it is never published. A
  session defaults to three staircases and six minutes, which a few minutes can finish; the session's
  length is the owner's to set before the first one.
- **`site` writes beside the page, never into the repository.** It reads the drum tools' mapping and
  audio and writes `listen/` into the site folder, which is local tool state like the site itself.
- **A report names a sound, never a path.** A report may be kept, and a path into a private folder
  must not travel with it.

### Decoding a hit and cutting a run

- **A hit is decoded to 30 s at 48 kHz; a run, whole.** `prep::decode_mono` keeps the page's limit,
  which counts frames and stops silently, so a 96 kHz file keeps 15 s. `Sound::load_kept` reads the
  same samples and says what was kept — the file's channels before they were averaged, and whether
  it was cut — so a caller that shows a sound can say so (the A/B page, re-rendered when it was
  added, stayed byte for byte). A run of notes goes through
  `decode_mono_whole` (`Sound::load_whole`), which keeps up to ten minutes at 96 kHz and refuses a
  longer file rather than cutting it: the first cut of Iowa's runs read a truncated file and cut
  re-strikes as notes.
- **A run is cut by its own names** (`split`). The name's range says which notes come in which
  order; each note takes the spectral-flux candidate where its own band rises most, in scale order
  (a dynamic programme), and the flux then places the strike. The strongest flux peaks alone missed
  a pianissimo strike and took a knock at 50 Hz; a rectangular band window let the semitone below
  beat against a quiet note and lift it where nothing was struck, so the band is a Hann window four
  cycles of the semitone difference long. Every cut is checked: its strongest peak within three
  semitones of its name must sit within half a semitone of it (`NAME_TOLERANCE_CENTS`), and `split`
  names each note that does not. A roll is not a run of strikes and is not cut. The cuts are derived
  audio: they go to `target/`, never into a commit. **Held runs** (`--held`: bowed, blown) are also cut
  a second way — every 50 ms a candidate, each note scored on the rise of its first four harmonics'
  bands, since a bow starts softly and a held note's strongest line jumps between harmonics — and
  each run keeps the cut that names more of its notes: Iowa's bowed cello then names 92 of 102, its
  bowed violin 82 of 103, their pizzicati 73 and 85 of 104.

### Pitched notes

- **A pitched note is declared, never detected** (`--family note`). A struck note's envelope is a
  hit's — a xylophone note passes every test a drum does — so detecting notes would move drums out
  of the percussive family and change their readings. A note keeps every percussive part but the
  texture (a snare's wires mean nothing on a bar) and adds the `note` part. Its onset is the
  percussive one raised 12 dB over whatever rings in the file's first 5 ms: a note cut from a run
  starts with the one before it still sounding.
- **A note's partials are the mode analysis's, followed by demodulation.** Its mode bands reach
  20 kHz (a drum's stop at 2 kHz), and a pole growing slower than 1/s is a steady mode, its T60 held
  at 1000 s: a partial that neither decays nor grows sits on the unit circle, rounding puts it
  outside as often as in, and rejecting every growing pole — a drum's rule, kept for drums — dropped a
  noise-free 220 Hz tone's harmonics window by window until it read 660 Hz. A mode another window
  also holds is a partial, as for drums. A
  mode only the attack window holds — a xylophone's brightest partials die inside it — is kept above
  the fundamental when its band, followed from the strike, decays within a factor of two of the
  analysis's own fit; a mode only a late window (from 160 ms) holds is kept anywhere when its band
  falls at least 10 dB as a line does — a glide is long over, a steady line does not fall, and a late
  window cannot fit a long decay (a vibraphone's Db3 and a crotale's E6 at pp were found only there).
  Either way the band's level at the window's start must be within 10 dB of the analysis's: a
  synthetic onset's click, smeared through a long band, once decayed like a mode fitted 51 dB down
  and read 26 dB louder. Lines
  within 2 % are one partial, and two in one window are its split (a bar and its resonator). Each
  partial is followed by a Hann window sixteen cycles of the gap to its nearest neighbour long, so the
  neighbour sits about 80 dB down (four cycles let a bar's overtones sink into the fundamental's leak
  and read three times their decay); its T60 is a line through its level over its first 40 dB of
  fall, or down to 10 dB above the noise a probe on the window's first null reads. Its level at the
  strike is that line carried back to the onset — a mode's initial amplitude, as a modal model is set;
  the envelope's largest value read a fast partial low, its window half over the silence before the
  strike.
- **A note's fundamental is not a drum's rest pitch.** It is the most energetic partial over the whole
  note, unless a partial 1.4 to 8 times below it holds energy within 30 dB of it; then the lowest such.
  The drums' rule took a crotale's 3.95 kHz partial for its C6. On Iowa's 531 cut notes the rule
  names 530 as their files do; the one (a sympathetic line under a marimba's B6) is explained in
  `src/parts/note.rs`, and `--room` names such a line away. A set's room lines reach a note's
  partials as they reach a drum's pitch. The bounds
  are for struck objects: a string's brightest harmonic can sit far above its fundamental, and strings
  take a harmonic comb when they come. **A caller can name the note** (`--expect A4`; `keymap
  --expect-from-name` reads it from the names `split` writes): the fundamental is then the most
  energetic believed partial within a semitone of it, and the report says when none answers. A
  kalimba's high tines need it — its other tines ring in sympathy and outlast them, and level and
  decay cannot tell a neighbour from a note's own weak fundamental (a crotale's); comparing a model's
  note with a recording of the same note is the same case.
- **A note's pitch is its fundamental; a bell's is its virtual pitch.** Beside the fundamental the
  note reads a virtual pitch by subharmonic summation (Hermes 1988) over its partials, each weighted
  by the dominance region (a bell's strike note is formed from its
  nominal, superquint and octave nominal, and an audible partial's level hardly moves it; a mild
  weight, falling to nothing 30 dB down, keeps a tube's weak low modes from taking it). On Iowa's 531
  notes the fundamental names 530 and the virtual pitch 461 — a low bar's upper partials can outvote
  it — while on VCSL's tubular bells the fundamental names 1 of 40 and the virtual pitch 31 by pitch
  class. Two notes whose virtual pitches are different notes are said to be heard as different notes,
  not compared in cents.
- **A string reads as a harmonic series.** When the partials hold at least six of the first eight
  harmonics of a pitch (or five with the first), and the note's fundamental is among them, the note's
  fundamental is the series' first harmonic (a low piano string's second can outweigh its first), and
  it reads the stiff-string law's B, fitted as a line through (f_n/n)² against n², and where it was
  plucked or struck, the comb |sin(nπβ)| matched to the harmonics' tilt-free levels, a harmonic
  missing between found ones counted 10 dB under the weakest. Candidates are scored by the harmonics
  they hold less the partials they leave unexplained below their eighth (a stiff string's third
  harmonic otherwise read as its fundamental); the series must hold the note's own fundamental (a
  xylophone's dense upper field holds six of eight harmonics of a pitch above it), and either hold
  ten harmonics or have a first harmonic within 30 dB of the most energetic partial's energy (frame
  lines under a marimba's D4 at pp formed a short series from a faint line; a piano's lowest C holds
  27 harmonics over a fundamental further down than that). On VCSL's Kawai grand the listener reads
  B rising up the keyboard and the hammer at 0.06–0.13 of the string.
- **A decay in two stages, and a damper.** Two lines through the fundamental's energy decay curve
  (Schroeder's backward integration, the noise's power taken out; the fall ends where the envelope
  stays under the noise margin for 200 ms), taken when they leave at most half one line's error, each
  falls 5 dB and their decays differ 1.5-fold: a slower second stage is `note.two_stage` (a piano's
  aftersound: VCSL's Kawai C5 reads 1.76), one three times faster a damper (`note.release_at`,
  `note.release_t60`). On the envelope itself a piano's unison strings beat ±3–4 dB and no error
  bar let its stages through. Two lines average across a two-decay bend (0.5 s over 4 s reads about
  3.8), the curve's knee sits about 0.1 s before a damper, and only its first 50 dB are read (a
  piano sample's note-off 9.5 s in lies below them). A damper's own decay is read on the envelope
  from 0.15 s after the knee: the curve runs out under it.

### Sustained notes, guitars and electric pianos

- **A sustained note is a note held** (`--family sustained`: a bow, a breath). It reads everything a
  note does, but takes no weaker line below its most energetic partial for its fundamental (a struck
  object's rule; a held note holds every line as long), and adds a `sustain` part from one pitch track
  over the note: until the pitch holds within 20 c for 100 ms — or its mean over 250 ms does, since a
  vibrato deeper than 20 c never stays inside it and a 30 c one had read "never holds" — then over
  the held part a line (drift,
  c/s), the peak of the track's spectrum between 3 and 10 Hz with the power within 1 Hz of it (vibrato
  rate and peak depth; a player's vibrato wanders in rate, and one fitted sinusoid caught little of a
  flute's), what the two leave (jitter), the same on the level track (tremolo), and harmonics against
  the noise between them. Iowa's flute with vibrato reads 4.7–5.3 Hz at ±10–11 c; without, and its
  clarinet, none. A series whose odd harmonics sit 15 dB under its even ones is the series an octave up.
  A held note's **tuning** is its held pitch's, the track's median over the held stretch: the
  fundamental's own frequency comes from short early windows, which catch a vibrato at one point of its
  swing, and a 30-cent vibrato had moved a note's tuning three thresholds with its mean unchanged.
- **A guitar's and an electric piano's readings.** A partial's level over its fall, its line taken
  out, is read for a slow swing between 0.3 and 15 Hz as a held note's vibrato is: the fundamental's
  beating, a string's two polarisations or a bar and its resonator. A strum's strings are the
  spectral-flux peaks within 400 ms of the first, at least 0.15 of the largest and four times the
  median of the 20 ms before (a beating note's swings ripple the flux amid their own ripple); the
  flux starts a frame before the onset, padded with silence where the file does not hold one, or the
  first string, lying in the first frame only, went uncounted. The flux is summed over third-octave
  bands, not bins: a bin between two partials holds both one's and the other's window leakage, which
  beats at their spacing and, sampled every hop, aliased into a 30 ms ripple that read a clean pluck
  as a strum. A pickup's nonlinearity and symmetry read as the second and third harmonics against the
  first and the even harmonics against the odd. A series' **harmonic deficit** is the harmonics among its first
  twelve that are missing or 15 dB under a line through its levels against log₂ n, the line refitted
  without those 6 dB under it: a clarinet's sunk even half drew the first line halfway down.

### Laws, objects, note comparison and key tracking

- **The laws and the object.** The decay law is a line through ln T60 against ln (f/f₁) (AAS's
  *Material*, constant Q at −1), the tilt a line through the strike levels against octaves (its
  *Tone*), each over at least three partials within 60 dB of the strongest. The nearest ideal object
  weighs the partials within 40 dB by their energy and charges for the object's ratios left unmet; a
  note is called **like** an object only within 30 c, otherwise like none, with the nearest named.
  Real bars sit off every ideal (a concert marimba's top register holds 1 : 4 : 8.4, a vibraphone's
  resonator adds odd harmonics): the partial table is the reading, the object a summary.
- **Two notes compare partial by partial.** The reference's partials above its fundamental are slots;
  each sound's partial nearest a slot's ratio to its own fundamental, within 3 %, gives readings for
  its level against the fundamental, its T60 and its tuning against the slot, keyed by the slot's band,
  so they are thresholded, phrased and ranked like any reading. Partials only one side holds are
  listed in the comparison's notes. A drum carries no partial table and compares as before.
- **Key tracking is a line per reading.** `keymap` fits each note reading against octaves from
  middle C (AAS scales every parameter from there): its value at middle C, its change for each octave
  up, and the notes' spread around the line; a decay time on a log scale, so its change is a factor.
  The line is Theil–Sen's and the spread a scaled median deviation: a least-squares line let one note
  read wrong move a vibraphone's tilt by 15 dB an octave. It describes the notes without the
  perceptual models, on every core. Its **body** table pools every note's partial levels, each against
  its own note's tilt, by third octave of absolute frequency: a note's laws move with its pitch, and
  what stays put across the compass is the soundboard or the resonator (VCSL's concert harp: +5.7 dB
  near 200 Hz, +5 dB at 500–630 Hz). Before it reads, `keymap` finds the **lines under at least half
  the notes** (within 1.5 %: a body's resonances, open strings ringing in sympathy, the room) and
  describes the notes again with them kept out of every note's partials: a guitar's body lines at
  104.6 and 243.8 Hz took its notes' fundamentals until then.

### Stimuli, responses and effects

- **Stimuli and sidecars** (the plan's §2 item 10). A stimulus is rendered from its sidecar alone,
  and the sidecar carries a 64-bit FNV-1a fingerprint of the samples as written: `respond` renders it
  again and refuses a sidecar this build does not reproduce, so a response is never read against a
  signal other than the one played. Only the settings (`setting NAME VALUE`: what the device was set
  to) may be edited. The sweep is Farina's exponential sweep, faded over 5 ms; the steps are a 30 dB
  jump each way and then 6 dB steps up; a note is a sidecar with no audio, the instrument playing it.
  `Both` is written as one channel, `Left` and `Right` as two with the other silent — the columns of a
  true-stereo matrix.
- **Responses** (`respond`, the plan's §2 item 9). An impulse, a sweep and noise are deconvolved by
  spectral division, regularised at 10⁻⁶ of the stimulus's largest power, and kept from 5 ms before
  lag zero: a band-limited impulse rings on both sides of its peak, and cutting the ringing ahead of
  it took 0.7 dB off a flat response. The sweep's harmonic responses fall before the linear one and
  wrap to the end of the circular result, out of the window read. Noise is deconvolved only where its
  coherence reaches 0.9; a chorus reads 0.5. A frequency response is smoothed over 1/24 octave; an end
  whose first octave moves less than 1.5 dB is a passband, the edges are read 3 dB under it, and where
  neither end is flat the response is a band-pass read from its peak (a resonant low-pass's 12 dB
  peak had passed for a band-pass under a rule of 6 dB over both ends). Taps are energy peaks the
  largest within 2 ms, within 50 dB of the strongest and 20 times the median within ±20 ms, so a dense
  tail has none; repeats are the longest run spaced within 5 %.
- **An effect's readings.** Lines at known frequencies are read through a four-term Blackman–Harris
  window over the steady stretch (the signal less its first tenth, at least 100 ms, and half as much at
  its end): a polynomial's harmonics and DC, and a square law's intermodulation, read to 0.05 dB of
  their closed forms. A compressor's **attack and release** are the times its gain takes to go 63 % of
  the way, in dB, to its settled level — a one-pole gain in dB's time constant — after the step up and
  the step down that move the gain most (the largest *input* step down was a quiet one that moved it
  not at all), timed from where the response's own level jumps rather than from a waveform lag, which a
  compressed sine leaves a period uncertain. A burst's **hangover** runs until the response *stays*
  40 dB under it: an echo 40 ms later had read as none. A **moving delay** is tracked by
  cross-correlating the response with the noise in 5 ms frames: in 20 ms frames a chorus swept ±2 ms
  at 0.8 Hz moved 10 samples and its peak sank under the dry path's. Lags held in 70 % of frames are
  the dry path or fixed taps. The right channel's sweep is fitted at the left's rate, or a rate a
  little off turns their phases apart over the seconds of the track.

### A synth voice

- **A synth voice** is declared (`--family voice`, or a note stimulus's response), read as a held
  note and then as a voice, released where the sidecar or `--off` says. Its spectrum over the held
  note's steady stretch uses Nuttall's four-term window with a continuous first derivative, whose
  sidelobes fall 18 dB an octave: Blackman–Harris's summed over a rich wave's bins held an additive
  saw's aliasing at −66 dB. The fundamental is refined on the harmonic grid, each harmonic found where
  the fit so far predicts it. **Aliasing** is the energy off the grid — every harmonic masked ±4 of the
  *window's* bins and 0.3 % (a mask four of the padded spectrum's bins wide left the main lobe's
  skirts as −57 dB) — and its audibility the most audible off-grid peak over the harmonics' masked
  threshold (Schroeder's spreading function, Johnston's tonal offset, the threshold in quiet, a full-
  scale sine at 94 dB SPL): a naive saw's reads over 10 dB, an additive one's under zero. The
  **envelope** is the RMS over whole periods every millisecond: 2.2 periods rippled, and a ripple's
  crest in a slow decay was taken for the peak. Near a short attack's top the window blends the rise into
  the decay: an attack under about twice the window (18 ms at 220 Hz) is resolved to a few
  milliseconds only — a 20 ms attack 2 ms longer read 0.9 of a threshold, a 60 ms one its 0.5. **Zipper** is a line in the strongest partial's
  envelope between 50 Hz and 0.45 of the fundamental, the envelope's slow motion taken out first and
  its band tapered (a rectangular band edge read as a line 63 dB down on a clean saw); a control rate
  above that is not seen. A **unison** is read only when the second harmonic beats at twice the
  fundamental's rate. The filter's peak is the harmonic standing most over a Theil–Sen line through
  the series; its Q and its level against the cutoff are the sweep's, through `respond` with a setting.

### For the listener's window

- **A reading in stages is the reading made at once.** `describe_staged` reads every part but the
  perceptual models; `Staged::perceive` fills a held note's fluctuation strength in its place and
  appends the `perception` section, each through the function `describe_with` uses, so the two agree
  reading for reading. A window shows the first stage while the second runs.
- **A name is a claim, never a reading** (the owner, 2026-09-28: *"often usefull. Though it can be
  misleading"*). `name::claims` reads a note, a run, a family word, a tempo, a dynamic and a loop from
  a file's stem; `name::check` holds a note against a pitched note's fundamental or virtual pitch,
  or a hit's rest pitch, within `split`'s half-semitone tolerance, names an octave off as such, and
  **suggests** a family only a caller may declare rather than applying it. A claim never changes a
  reading; a caller passes the name's note as `expect_hz` only when the owner asks.
- **Every reading, table and curve id has a glossary row**, and every row an id the source holds.
  `tests/glossary.rs` finds ids by scanning `src/`, not by reading reports: an id is a `&'static
  str` and nothing here makes one at runtime (the test forbids a leak). Every id-shaped literal is
  classified — its prefix in the test's `PARTS`, or the literal in `NOT_IDS` — so a new part cannot
  slip past; the synthetic reports are checked against the scan as a second net.
- **Curves are read-outs, not measurements.** `curves` reads out the decay part's envelope, a spectrum
  and the unexplained map's frames, under the readings' contracts — finite or absent, each with its
  window, resolution and source — from the report's own onset, so they line up with its windows.

### Words and phrases

- **Words for the timbre are read, never compared** (the plan's §2 item 8). The Audio Commons words
  (Pearce, Brookes & Mason 2017) come as the numbers behind them, never a score: brightness the
  energy above 3 kHz against the whole and where it sits; depth the energy from 30 to 200 Hz and where
  it sits; warmth 150–600 Hz against 600 Hz–6 kHz (chosen); hardness the attack's steepest rise on the
  linear envelope, as the time a rise that steep takes to the peak — in dB any ramp rises steepest at
  its very start, and a 1 ms and a 25 ms fade read alike — and the first 10 ms's centroid; boominess
  the basis loudness's share under 280 Hz (Hatano & Hashimoto's range), where the hearing model spreads
  an 80 Hz tone's loudness a quarter above it. Each restates readings compared in their own right, so
  `compare` leaves `words.*` out: one difference is never counted twice.
- **A phrase is explained** (`listen explain`, the plan's §4): the vocabulary's rows and the golden
  cases holding every word of it, each with its reading, where and which way; with two sounds, those
  readings' differences in them, audible or not.

### Research boundary, clean room and platform

- **Research boundary.** No recording, derived audio, per-file report, recording path or stimulus
  made from a recording is committed. Every automated test runs on synthetic signals and never reads
  a recording or the research checkout; recording-backed runs are local and manual.
- **Clean room.** Every technique is written from its paper or standard, cited at the top of the file
  that uses it. No existing implementation is opened (the Timbre Toolbox, Audio Commons, ViSQOL,
  Zimtohrli, PEAQ, the ISO or ECMA reference code).
- **Windows is the platform this is verified on.** Linux and macOS are unverified; nothing here may
  be Windows-only. *Since the split (2026-10-06):* the fast tier is checked on Windows and on Linux
  in WSL before a push, and CI runs it on Windows, macOS and Linux on `v*` release tags or when
  started by hand; the `release gate` suites stay a manual run.
