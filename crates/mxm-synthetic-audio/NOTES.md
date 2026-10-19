# NOTES — crates/mxm-synthetic-audio

The measurements and reasoning behind [AGENTS.md](AGENTS.md)'s rules.

## Why two engines (2026-10-19)

The owner asked for the strings to come from Applied Acoustics Systems' physical models, rendered
outside a DAW ("the AAS has the most complex sounds as they are physically modelled, so closest to
real instruments"), and for additive synthesis where AAS has nothing (voices, winds). The installed
String Studio VS-3 covers violin, viola, cello, double bass and plucked bass; Chromaphone, Strum,
Lounge Lizard and Multiphonics fit none of the sets' instruments.

String Studio was then measured, and can't give an exact f0 for a pitch that moves while a string
sounds:

- **A bend up waits, then jumps.** Its pitch-bend parameter (1271, the controller its
  `IMidiMapping` gives MIDI pitch bend on channel 1; 0.5 is centre) was stepped by 2 semitones
  every 0.237 s for 5 s on a held cello C3: each step up held about 75 ms and then jumped within
  10 ms, overshooting 20 to 30 cents; each step down glided over about 20 ms at once.
- **Its Tune parameter moves a sounding string about 90 cents at most**: a step of 3 semitones on a
  held note stopped near +100 cents, though the same value set before the note plays it correctly.
- **Its own vibrato is a clean sine** (5.47 Hz, 26 cents at 50 %, 2.6 cents RMS from a fitted sine),
  but tops out near ±52 cents, short of the set's ±100 and ±150.

So every case whose truth is a moving pitch is additive, and String Studio plays only notes that
hold one pitch: the strings' bow changes (set 1's file 03), and sets 3 to 8.

## How String Studio is driven

- The bundle loads through `mxm-vst3-host` as MXM Player and newDAWn load it; a fresh instance per
  render, the state loaded first. Two renders of one note are **bit for bit the same**.
- A factory preset is loaded by writing its pack record's `version`, `schema` and `values` into the
  saved state's `parameters` table, with its `programName`; the values are matched by name through
  the schema, so `Setup/Bend` (n is n - 1 semitones; 3 is ±2, 13 and above ±12) can be set.
- Voicings: cello and (with a medium body) viola from *Baroque Cello*, (with the largest body) the
  bowed double bass too; the violin from *Brushed violin 1*; the plucked bass from *Upright Pluck
  1* with its inharmonicity at 12 %. The library has no viola or bowed double bass; the violin's
  preset with a larger body was scratchy below G3 (noise 5 to 21 dB under the tone at D#3).
  *Upright Pluck 2* plucks at the string's midpoint (no even harmonics) with 61 % inharmonicity:
  its sixth partial sounded 3.8 % sharp, where the brief asks for slight.

## Why a note is annotated at the pitch it sounds

A bowed note's steady pitch locks to one of a few values near the one its bend asks, and hops
between them as the bend changes: a cello G4 swept from -9 to +1 cents of bend in 0.25-cent steps
sounded +0.4, +2.0 (eight times), -2.4, +6.1, -0.2, +4.4 … cents, each one steady to hundredths of
a cent; the violin preset's G4 with a larger body sat on plateaus of +17 and +24.5 cents whatever
the bend (a body resonance, as a wolf tone does). A note is therefore tuned near its target and
annotated with the pitch it sounds, measured by the harmonic sum over its steady part
(`measure::steady_pitch`, partials locked in a bowed string: all eight measured at the same cents).

## Settling and release, measured not annotated

The model's attacks settle: in the legato set's bow changes, the largest deviation in a note's
first 150 ms reached 50 to 200 cents for some violin and viola notes (clear pitch, correlation 0.8
to 0.9; a violin G4 sat 50 cents flat for 60 ms), most settled within 40 ms. The clean room scores
notes (onset within 50 ms, pitch within 50 cents) and asked for the F0 to stay held from the onset,
the settling recorded per note: `settle_cents`, `settle_ms` (within `SETTLE_WINDOW`).

VS-3's gated damper bent a released string's pitch 40 to 100 cents as it stopped it; with each note
ending by a gain fade before its note-off, the release drift fell to 16 cents at most (mostly under
10), recorded per note as `release_drift_cents`.

## Noise and the harmonic band

Some of the violin preset's low notes held 93 to 97 % of their energy at their harmonics (noise 12
to 15 dB under the tone), against the brief's bow noise of 30 to 45 dB under: `MAX_NOISE_DB` turns
such a render away and another key or bow is tried. A low cello note (C#2) showed sidebands about
10 Hz either side of every harmonic, 25 to 30 dB down (a slow tremolo), which a band of a tenth of
f0 counted as noise; the band is at least 12 Hz, which still leaves a raucous regime's f0 / 2 and
f0 / 3 outside it down to about 30 Hz.

## The octave guard

**The *Upright* presets sound an octave below their key**, as a double bass sounds below its
written note: keyed at F3, a note held as much energy at 87.5 Hz (f0 / 2) and 1.5 f0 as at
174.6 Hz, from E2 to G3 up to 12 dB more. A tuner asking for the key's pitch found its second
harmonic there and would have annotated every plucked note an octave high; the noise gate refused
them (energy at f0 / 2 is outside every harmonic's band). The voicing's `transpose` (-12) fixes the
key; afterwards nothing sat at f0 / 2 (40 to 88 dB down).

**A bowed string can flip to its second mode.** One violin A#3 render, cleaner than any other try
(noise 48 dB under), held its fundamental 55 dB under its loudest harmonic: the string sounded the
octave above, and the pitch measured was an octave low. A note whose fundamental is more than
`MIN_FUNDAMENTAL_DB` under its loudest of the first six harmonics is refused (the weakest real one
met, a violin's lowest notes, whose body radiates little below 275 Hz: about 10 dB).

## Gates, by register and length

The violin preset's lowest notes sit near -20 dB of noise at their cleanest, a bowed double bass's
G#1 wobbles 3 to 14 cents with 15 to 23 dB of noise, and a cello's 0.1 to 0.2 s notes are mostly
attack, 13 to 21 dB of bow noise: so `MAX_NOISE_DB` is -20, eased to -17 dB and 7 cents of spread
by 40 Hz (`limit`) and to -12 dB for a quarter-second note (`noise_limit`). A raucous regime, at
-12 dB and with its subharmonics, is still refused above a quarter second.

## Harmonic levels

Set 4's levels are read as the power within a quarter of f0 of each harmonic: a single DFT bin
read a jittering additive partial up to 5 dB low (a bassoon's H5 and H7, asked 40 dB under the
loudest, read 45), and the String Studio shaping loop corrects by the same reading.

## Checking a render

An independent check, outside the crate: a harmonic-sum f0 at 10 ms frames over a 40 ms window,
searched within 100 cents of the truth, every third voiced frame. The final render, 2026-10-19
(79 files): additive files median 0.02 to 0.10 cent, 99 % within 0.2 to 4 cents (dynamics' quiet
notes and glides excepted, where the window smears); wide vibrato median 1.1 to 1.6 cents (the
window smoothing ±150 cents at 7 Hz); String Studio files median 0.02 to 0.9 cent, 95 % within 2 to
24 cents, the tail in attacks and where a note hands over to the next; the room copies diverge by
design, the last note ringing under the next (rt30's 95 % at the search's edge). Below about 50 Hz
the 40 ms window holds too few periods: double-bass-06's E1 and C1, checked over 160 ms, sat
within 1 cent median and 12 at most. The even-level vibrato control measured within 0.2 dB between
its turns; the asymmetric notes 90 % of the difference asked, which is the readback's average over
±45° of each turn. The clean room's survey of all 79 files the same day found three analysis
faults the real recordings had only hinted at.
