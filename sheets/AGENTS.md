# AGENTS.md — sheets

## Purpose

Contact sheets: a set of sounds side by side, each a reference and a candidate (a recording and its
model, an effect's input and its output), drawn for an agent to look at before anything about the
set reaches the owner (the owner, 2026-11-09: "It seems that this kind of visual display of teh
sounds ia really helping you. Is there som way we could make at a cannonical part of youre
process?", and "I expect we will be building a lot of instruments this way"). The workspace's rule
is its `AGENTS.md`, "Look before reporting"; this folder is the tool every product's runs call.

## Ownership

- `sheets.py`: the tool. Its docstring is the manual: the manifest's columns, every sheet, the
  summary, `zoom`.
- `test_sheets.py`: its test, on a set it makes (six struck tones, one candidate flat, one noisy).

## Local Contracts

- **Python 3 with numpy and Pillow, nothing else**: no path into the workspace, no network. Runs on
  Windows, Linux and macOS alike; `font` falls back through each one's fonts to Pillow's own.
- **Every sheet is 1560 × 736**, the largest Claude reads unscaled. More sounds go on more pages,
  never on panels under `MIN_PW` × `MIN_PH`.
- **Every picture written is printed as `LOOK: <absolute path>`**, after the summary; progress goes
  to stderr. The workspace's `look_before_reporting.py` hook reads those lines.
- **A flag is a pointer, not a verdict**: its thresholds are named in the code (`PITCH_CENTS`,
  `SIBLING_DB`), and what it points at is looked at, then explained or fixed.
- It reads WAV; a caller with other audio decodes it first. A caller passing downloaded recordings
  runs it isolated (`python -E -P`).

## Work Guidance

- A new view: its sheet in `run_set`, its line in the docstring, its name in the test's list.
- A fault found by eye that no flag caught becomes a flag here, as the drums' gate takes every
  check the owner's ear found.

## Verification

- `python -m unittest test_sheets` in this folder; it writes under the repository's git-ignored
  `target/sheets-test/`.

## Child DOX Index

None.
