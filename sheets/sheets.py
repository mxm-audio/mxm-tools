"""Contact sheets: a set of sounds side by side, each a reference and a candidate (a recording and
its model, an effect's input and its output), one sheet per view, for an agent to look at before
anything about the set reaches the owner (the owner, 2026-11-09: "It seems that this kind of visual
display of teh sounds ia really helping you. Is there som way we could make at a cannonical part of
youre process?"; "I expect we will be building a lot of instruments this way"). Sounds of one
instrument are one sound: what one has and its siblings do not is suspect until explained.

    python sheets.py set <manifest.tsv> <out dir> [--title T] [--names Recording,Model] [--span S]
                         [--views level,hz-difference]
    python sheets.py zoom <reference> <candidate or -> <out.png> [--from S] [--to S] [--lo HZ]
                          [--hi HZ] [--f0 HZ] [--q Q] [--names Recording,Model]

`set` reads a tab-separated manifest whose first line names its columns: `row` and `col` (the
sheets' labels, in the order first met: a strength and a note, a layer and a drum), `reference` and
`candidate` (WAV paths; an empty candidate draws the reference alone), and optionally `f0` (Hz;
empty: unpitched) and `group` (sounds that must agree: one note's strengths). It writes, each sheet
1560 x 736 (what Claude reads unscaled; sounds past one page go on more, `-2`, `-3`):

  hz-reference, hz-candidate, hz-difference   0.5 ms to the span on a log time axis, 20 Hz to
      20 kHz: what sits at one frequency whatever the note (a room's ring, a hum) lines up
  f0-reference, f0-candidate, f0-difference   the same in multiples of each sound's f0, 0.25x to
      48x (pitched sounds): every note's modes at one height
  strike-reference, strike-candidate, strike-difference   -2 to 40 ms, linear, in f/f0 where every
      sound is pitched, else in hertz
  line-1, line-2, line-3   the level of the reference's strongest, second and third line (at least
      10 % apart) over the span, reference and candidate, to 60 dB under the reference's peak there
  level                    both sounds' broadband level on the reference's scale

(`--views` draws only the sheets it names: a large set compared on a few views, as the workspace's
platform check compares the collection's renders across platforms), and `summary.txt`, printed too: per sound, the line its reference rings nearest its f0 and the
candidate's, in cents from the f0 given and from each other; per group, its references' spread; and
the sounds furthest from their siblings (the others in their row and column), which the sheets
frame: red, a candidate further from its siblings than its reference is from theirs; orange, a
reference unlike its siblings. The output ends with each sheet as `LOOK: <path>`, which the
workspace's `look_before_reporting.py` hook holds a turn on until each is read.

`zoom` draws one region of one pair large: the reference, the candidate and their difference over a
time range and a band, f0's multiples marked.

Levels are a constant-Q bank (Gaussian bands, sigma f/Q, Q 20 unless given), each file against its
own loudest on the level sheets; a difference is the candidate minus the reference as they are,
their loudness the caller's to match. Time zero is each file's onset, its first sample at 1 % of its
peak. Needs numpy and Pillow; reads WAV (PCM 16, 24 or 32-bit, float 32 or 64; channels averaged).
"""

import argparse
import csv
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

W, H = 1560, 736
LEFT, TOP, BOTTOM, GAP = 64, 52, 22, 4
# The smallest panel a sheet draws; more sounds than fit go on another page.
MIN_PW, MIN_PH = 140, 150
Q = 20.0
FLOOR_DB, DIFF_DB, FALL_DB = -72.0, 24.0, 60.0
TOP_HZ, LOW_HZ = 20000.0, 20.0
ONSET = 0.01
# A line within this of f0 is the sound's own (the strongest there is read as its pitch).
NEAR = 0.04
# A candidate's line this far from its reference's, or a reference's from the f0 given, is flagged.
PITCH_CENTS = 5.0
# A sound this much further from its siblings (mean dB over the cells either sounds in) is framed.
SIBLING_DB = 3.0
REF_RGB, CAND_RGB = (192, 98, 43), (31, 122, 140)
TEXT, AXIS, GRIDC, NONE = (30, 34, 40), (120, 128, 136), (222, 226, 231), (225, 225, 225)
MARK, RED, ORANGE = (80, 220, 235), (200, 30, 30), (235, 140, 20)
MAGMA = np.array([(0, 0, 4), (40, 11, 84), (101, 21, 110), (159, 42, 99), (212, 72, 66),
                  (245, 125, 21), (250, 193, 39), (252, 255, 164)], dtype=float)


def font(size, bold=False):
    names = (["segoeuib.ttf", "arialbd.ttf", "DejaVuSans-Bold.ttf", "Arial Bold.ttf"] if bold
             else ["segoeui.ttf", "arial.ttf", "DejaVuSans.ttf", "Arial.ttf"])
    folders = ["C:/Windows/Fonts", "/usr/share/fonts/truetype/dejavu", "/Library/Fonts",
               "/System/Library/Fonts/Supplemental"]
    for folder in folders:
        for name in names:
            try:
                return ImageFont.truetype(f"{folder}/{name}", size)
            except OSError:
                pass
    return ImageFont.load_default()


FONTS = {"title": font(16, True), "label": font(13, True), "tiny": font(10)}


# --- Sound ---------------------------------------------------------------------------------------

def read_wav(path):
    """A WAV's samples, channels averaged, and its rate."""
    data = Path(path).read_bytes()
    if data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        raise ValueError(f"{path}: not a WAV")
    i, fmt, raw = 12, None, None
    while i + 8 <= len(data):
        cid, size = data[i:i + 4], int.from_bytes(data[i + 4:i + 8], "little")
        body = data[i + 8:i + 8 + size]
        if cid == b"fmt ":
            fmt = body
        elif cid == b"data":
            raw = body
        i += 8 + size + (size & 1)
    if fmt is None or raw is None:
        raise ValueError(f"{path}: no fmt or data chunk")
    tag, channels = int.from_bytes(fmt[0:2], "little"), int.from_bytes(fmt[2:4], "little")
    rate, bits = int.from_bytes(fmt[4:8], "little"), int.from_bytes(fmt[14:16], "little")
    if tag == 0xFFFE:
        tag = int.from_bytes(fmt[24:26], "little")
    width = bits // 8
    raw = raw[:len(raw) // (width * channels) * width * channels]
    if tag == 3:
        x = np.frombuffer(raw, dtype="<f4" if width == 4 else "<f8").astype(np.float64)
    elif width == 2:
        x = np.frombuffer(raw, dtype="<i2") / 32768.0
    elif width == 3:
        b = np.frombuffer(raw, dtype=np.uint8).reshape(-1, 3).astype(np.int32)
        v = b[:, 0] | (b[:, 1] << 8) | (b[:, 2] << 16)
        x = np.where(v >= 1 << 23, v - (1 << 24), v) / float(1 << 23)
    elif width == 4:
        x = np.frombuffer(raw, dtype="<i4") / 2147483648.0
    else:
        raise ValueError(f"{path}: {bits}-bit samples")
    return x.reshape(-1, channels).mean(axis=1), rate


class Sound:
    def __init__(self, path):
        self.path = path
        self.x, self.rate = read_wav(path)
        self.peak = float(np.max(np.abs(self.x))) or 1.0
        self.on = int(np.argmax(np.abs(self.x) >= ONSET * self.peak))

    def bank(self, freqs, times, q=Q):
        if not hasattr(self, "_spec"):
            n = 1 << int(np.ceil(np.log2(len(self.x) + self.rate)))
            self._spec = (n, np.fft.fft(self.x, n))
        level = db(bank(self._spec, self.rate, self.on, freqs, times, q))
        # Past its last sample there is nothing: the transform's wrap would bring the strike back.
        level[:, self.on + np.asarray(times) * self.rate >= len(self.x)] = np.nan
        return level

    def ring_s(self):
        """How long it rings to 60 dB under its loudest 10 ms, s from the onset."""
        frame = max(int(0.01 * self.rate), 1)
        seg = self.x[self.on:]
        n = len(seg) // frame
        if n == 0:
            return 0.0
        rms = np.sqrt((seg[:n * frame].reshape(n, frame) ** 2).mean(axis=1))
        loud = np.nonzero(rms >= rms.max() * 1e-3)[0]
        return (loud[-1] + 1) * frame / self.rate

    def spectrum(self, start_s, end_s):
        """A Hann spectrum's magnitude over [start, end) s from the onset, padded to 2^21."""
        a = self.on + int(start_s * self.rate)
        b = min(self.on + int(end_s * self.rate), len(self.x))
        seg = self.x[a:b]
        if len(seg) < 64:
            return None, None
        n = 1 << max(int(np.ceil(np.log2(len(seg)))) + 2, 21)
        s = np.abs(np.fft.rfft(seg * np.hanning(len(seg)), n))
        return np.fft.rfftfreq(n, 1 / self.rate), s

    def line_near(self, f0):
        """The strongest line within NEAR of `f0`, Hz, over its ring (50 ms on, at most 2 s)."""
        end = min(max(self.ring_s(), 0.25), 2.05)
        f, s = self.spectrum(0.05 if end > 0.3 else 0.0, end)
        if f is None:
            return None
        band = (f > f0 * (1 - NEAR)) & (f < f0 * (1 + NEAR))
        if not band.any():
            return None
        k = int(np.argmax(np.where(band, s, 0)))
        if 0 < k < len(s) - 1 and s[k] > 0:
            # The peak between bins: a parabola through the three dB values.
            a, b, c = (20 * np.log10(max(v, 1e-30)) for v in s[k - 1:k + 2])
            d = 0.5 * (a - c) / (a - 2 * b + c) if a - 2 * b + c != 0 else 0.0
            return float(f[k] + d * (f[1] - f[0]))
        return float(f[k])

    def lines(self, span, f0=None, count=3):
        """Its strongest lines over its first second (or span), Hz, at least 10 % apart: from half
        its f0 up where it has one (under it, a recording's lines are the room's), else from 30 Hz."""
        f, s = self.spectrum(0.01, min(span, 1.0))
        if f is None:
            return []
        ok = (f >= (0.5 * f0 if f0 else LOW_HZ * 1.5)) & (f <= min(TOP_HZ, 0.45 * self.rate))
        peak = ok[1:-1] & (s[1:-1] >= s[:-2]) & (s[1:-1] >= s[2:])
        idx = np.nonzero(peak)[0] + 1
        out = []
        for k in idx[np.argsort(-s[idx])]:
            if all(abs(np.log(f[k] / o)) > np.log(1.1) for o in out):
                out.append(float(f[k]))
            if len(out) == count:
                break
        return out

    def level_db(self, times):
        """Broadband RMS level in frames centred at `times` s from the onset, dB re full scale."""
        half = max(int(0.0025 * self.rate), 1)
        out = []
        for t in times:
            c = self.on + int(t * self.rate)
            seg = self.x[max(c - half, 0):max(c + half, 0)]
            out.append(np.sqrt(np.mean(seg ** 2)) if len(seg) else 0.0)
        return db(np.array(out))


def bank(spectrum, rate, on, freqs, times, q=Q):
    """|analytic band signal| of a Gaussian band (sigma f/q) at each of `freqs`, at `times` s from
    the sample `on`, from the sound's whole spectrum `(n, fft of it padded to n)` (a second of zeros
    past its end at least): each band shifted to baseband and inverse-transformed short. NaN where a
    band is past TOP_HZ or the Nyquist."""
    n, spec = spectrum
    df = rate / n
    at = on + np.asarray(times) * rate
    out = np.full((len(freqs), len(at)), np.nan)
    for i, f in enumerate(freqs):
        if f > min(TOP_HZ, 0.45 * rate) or f < df * 4:
            continue
        s = f / q
        k0 = int(round(f / df))
        half = int(np.ceil(5 * s / df)) + 1
        m = np.arange(-min(half, k0 - 1), half + 1)
        c = spec[k0 + m] * np.exp(-0.5 * ((((k0 + m) * df) - f) / s) ** 2) * (2.0 / n)
        size = 1 << int(np.ceil(np.log2(2 * len(m) + 2)))
        buf = np.zeros(size, complex)
        buf[m % size] = c
        env = np.abs(np.fft.ifft(buf) * size)
        # The short transform is the band's envelope at n/size-sample steps over the whole n.
        out[i] = np.interp(at / (n / size), np.arange(size), env, left=0.0, right=0.0)
    return out


def db(a):
    return 20 * np.log10(np.maximum(a, 1e-9))


def cents(a, b):
    return 1200 * np.log2(a / b) if a and b else None


# --- Drawing -------------------------------------------------------------------------------------

def magma(v):
    v = np.clip(v, 0, 1) * (len(MAGMA) - 1)
    lo = np.floor(v).astype(int).clip(0, len(MAGMA) - 2)
    fr = (v - lo)[..., None]
    return MAGMA[lo] * (1 - fr) + MAGMA[lo + 1] * fr


def level_rgb(level):
    """Level in dB against the file's loudest, NaN grey."""
    rgb = magma((level - FLOOR_DB) / -FLOOR_DB)
    rgb[np.isnan(level)] = NONE
    return rgb


def diff_rgb(cand, ref):
    top = np.nanmax(ref)
    d = np.clip((np.maximum(cand, top + FLOOR_DB) - np.maximum(ref, top + FLOOR_DB)) / DIFF_DB, -1, 1)
    blue, white, red = np.array([33, 102, 172.]), np.array([247, 247, 247.]), np.array([178, 24, 43.])
    rgb = np.where(d[..., None] < 0, white + (blue - white) * (-d[..., None]), white + (red - white) * d[..., None])
    rgb[(cand < top + FLOOR_DB + 6) & (ref < top + FLOOR_DB + 6)] = (255, 255, 255)
    rgb[np.isnan(cand) | np.isnan(ref)] = NONE
    return rgb


def y_of(freqs, h, f):
    """The panel row of frequency `f` on a log axis whose rows are `freqs`, top down."""
    return int(np.interp(np.log(f), np.log(freqs[::-1]), np.arange(h)[::-1]))


class Sheet:
    """One page of one view: rows and columns of panels, labelled."""

    def __init__(self, title, rows, cols, note, pw=None, ph=None):
        self.rows, self.cols = rows, cols
        self.img = Image.new("RGB", (W, H), "white")
        self.d = ImageDraw.Draw(self.img)
        self.d.text((LEFT, 4), title, fill=TEXT, font=FONTS["title"])
        self.d.text((LEFT + 420, 8), note, fill=AXIS, font=FONTS["tiny"])
        self.pw = pw or (W - LEFT - 8) // len(cols)
        self.ph = ph or (H - TOP - BOTTOM) // len(rows)
        for c, n in enumerate(cols):
            self.d.text((LEFT + c * self.pw + 2, TOP - 18), str(n)[: self.pw // 7], fill=TEXT, font=FONTS["label"])
        for r, n in enumerate(rows):
            self.d.text((4, TOP + r * self.ph + 2), str(n)[:8], fill=TEXT, font=FONTS["label"])

    def box(self, c, r):
        x0, y0 = LEFT + c * self.pw, TOP + r * self.ph
        return x0, y0, x0 + self.pw - GAP, y0 + self.ph - GAP

    def size(self):
        x0, y0, x1, y1 = self.box(0, 0)
        return x1 - x0, y1 - y0

    def paste(self, c, r, rgb):
        x0, y0, _, _ = self.box(c, r)
        self.img.paste(Image.fromarray(rgb.astype(np.uint8)), (x0, y0))

    def frame(self, c, r, colour):
        x0, y0, x1, y1 = self.box(c, r)
        self.d.rectangle([x0, y0, x1 - 1, y1 - 1], outline=colour, width=3)

    def flag(self, c, r, text):
        x0, _, _, y1 = self.box(c, r)
        self.d.rectangle([x0 + 3, y1 - 17, x0 + 8 + 7 * len(text), y1 - 4], fill="white")
        self.d.text((x0 + 5, y1 - 17), text, fill=RED, font=FONTS["label"])

    def freq_ticks(self, freqs, ticks, fmt):
        _, h = self.size()
        for r in range(len(self.rows)):
            _, y0, _, _ = self.box(0, r)
            for f in ticks:
                if freqs[-1] <= f <= freqs[0]:
                    self.d.text((LEFT - 28, y0 + y_of(freqs, h, f) - 6), fmt(f), fill=AXIS, font=FONTS["tiny"])

    def mark(self, c, r, freqs, f):
        _, h = self.size()
        x0, y0, _, _ = self.box(c, r)
        if freqs[-1] <= f <= freqs[0]:
            y = y0 + y_of(freqs, h, f)
            self.d.line([(x0, y), (x0 + 5, y)], fill=MARK, width=2)

    def time_ticks(self, ticks, where):
        w, _ = self.size()
        for c in range(len(self.cols)):
            x0, _, _, y1 = self.box(c, len(self.rows) - 1)
            for k, (t, label) in enumerate(ticks):
                x = x0 + where(t) * (w - 1)
                if not x0 <= x <= x0 + w:
                    continue
                self.d.line([(x, y1), (x, y1 + 3)], fill=AXIS)
                if c == 0 or k > 0:
                    self.d.text((x + 1, y1 + 3), label, fill=AXIS, font=FONTS["tiny"])

    def graph(self, c, r, curves, top, span, label=None):
        """Curves (dB at even times over 0..span) under `top`, to FALL_DB below it."""
        x0, y0, x1, y1 = self.box(c, r)
        self.d.rectangle([x0, y0, x1, y1], outline=GRIDC)
        for g in (20, 40):
            yy = y0 + g / FALL_DB * (y1 - y0)
            self.d.line([(x0, yy), (x1, yy)], fill=GRIDC)
        for curve, colour in curves:
            if curve is None:
                continue
            ys = y0 + np.clip((top - curve) / FALL_DB, 0, 1) * (y1 - y0)
            xs = x0 + np.arange(len(curve)) / max(len(curve) - 1, 1) * (x1 - x0)
            # Drawn where the sound has samples: a NaN ends a stretch.
            ok = np.isfinite(ys)
            for run in np.split(np.arange(len(ys)), np.nonzero(np.diff(ok.astype(int)))[0] + 1):
                if ok[run[0]] and len(run) > 1:
                    self.d.line(list(zip(xs[run].tolist(), ys[run].tolist())), fill=colour, width=1)
        if label:
            self.d.text((x1 - 6 * len(label) - 4, y0 + 2), label, fill=AXIS, font=FONTS["tiny"])
        stamp = f"0-{span:.2f} s" if span < 1 else f"0-{span:.1f} s"
        self.d.text((x1 - 6 * len(stamp) - 4, y1 - 13), stamp, fill=AXIS, font=FONTS["tiny"])


def per_page(rows, cols):
    """Rows and columns on a page, and the panels' pitch (every page's the first's)."""
    rpp = min(len(rows), max(1, (H - TOP - BOTTOM) // MIN_PH))
    cpp = min(len(cols), max(1, (W - LEFT - 8) // MIN_PW))
    return rpp, cpp, (W - LEFT - 8) // cpp, (H - TOP - BOTTOM) // rpp


class View:
    """One view over every page: where each sound's panel is, and the sheets."""

    def __init__(self, name, title, note, rows, cols):
        self.name = name
        rpp, cpp, pw, ph = per_page(rows, cols)
        self.pages, self.where = [], {}
        for ri in range(0, len(rows), rpp):
            for ci in range(0, len(cols), cpp):
                rs, cs = rows[ri:ri + rpp], cols[ci:ci + cpp]
                k = len(self.pages)
                paged = len(rows) > rpp or len(cols) > cpp
                self.pages.append(Sheet(title + (f"  ·  page {k + 1}" if paged else ""), rs, cs, note, pw, ph))
                for r in rs:
                    for c in cs:
                        self.where[(r, c)] = (k, cs.index(c), rs.index(r))

    def at(self, row, col):
        k, c, r = self.where[(row, col)]
        return self.pages[k], c, r

    def each(self, fn):
        for p in self.pages:
            fn(p)

    def save(self, out):
        paths = []
        for k, p in enumerate(self.pages):
            path = out / (f"{self.name}.png" if k == 0 else f"{self.name}-{k + 1}.png")
            p.img.save(path, optimize=True)
            paths.append(path.resolve())
        return paths


# --- set -----------------------------------------------------------------------------------------

def read_manifest(path):
    with open(path, encoding="utf-8", newline="") as f:
        rows = [r for r in csv.DictReader(f, delimiter="\t") if any((v or "").strip() for v in r.values())]
    for k in ("row", "col", "reference"):
        if rows and k not in rows[0]:
            raise ValueError(f"{path}: no `{k}` column")
    base = Path(path).resolve().parent
    entries = []
    for r in rows:
        resolve = lambda p: str((base / p).resolve()) if p and not Path(p).is_absolute() else p
        f0 = (r.get("f0") or "").strip()
        entries.append({"row": r["row"].strip(), "col": r["col"].strip(), "reference": resolve(r["reference"].strip()),
                        "candidate": resolve((r.get("candidate") or "").strip()) or None,
                        "f0": float(f0) if f0 else None, "group": (r.get("group") or "").strip() or None})
    return entries


def ordered(values):
    seen = []
    for v in values:
        if v not in seen:
            seen.append(v)
    return seen


def run_set(args):
    entries = read_manifest(args.manifest)
    if not entries:
        sys.exit(f"{args.manifest}: no sounds")
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    ref_name, cand_name = args.names.split(",")
    rows, cols = ordered(e["row"] for e in entries), ordered(e["col"] for e in entries)
    pitched = [e for e in entries if e["f0"]]
    all_pitched = len(pitched) == len(entries)
    have_cand = any(e["candidate"] for e in entries)
    for e in entries:
        e["ref"] = Sound(e["reference"])
        e["cand"] = Sound(e["candidate"]) if e["candidate"] else None
    span = args.span or float(np.clip(max(e["ref"].ring_s() for e in entries) * 1.05, 0.2, 10.0))

    _, _, pw, ph = per_page(rows, cols)
    w, h = pw - GAP, ph - GAP
    hertz = np.exp(np.linspace(np.log(TOP_HZ), np.log(LOW_HZ), h))
    wide = np.exp(np.linspace(np.log(48), np.log(0.25), h))
    near = np.exp(np.linspace(np.log(48), np.log(0.5), h))
    t_log = np.exp(np.linspace(np.log(0.0005), np.log(span), w))
    t_lin = np.linspace(-0.002, 0.040, w)
    t_fall = np.linspace(0.0, span, 2 * w)

    title = args.title or Path(args.manifest).stem
    level_note = f"each file against its own loudest, black {FLOOR_DB:g} dB; Gaussian bands sigma f/{Q:g}"
    diff_note = f"{cand_name} minus {ref_name.lower()}: red louder, blue quieter, full at {DIFF_DB:g} dB; white: both {-FLOOR_DB - 6:g} dB under"
    fall_note = f"orange: {ref_name.lower()}, teal: {cand_name.lower()}; to {FALL_DB:g} dB under the {ref_name.lower()}'s peak there, lines at -20 and -40"
    views = {}

    def view(name, heading, note):
        views[name] = View(name, f"{heading}  ·  {title}", note, rows, cols)

    who = [("reference", ref_name)] + ([("candidate", cand_name)] if have_cand else [])
    for k, word in who:
        view(f"hz-{k}", f"{word}s in hertz", level_note + "; cyan: f0")
        if pitched:
            view(f"f0-{k}", f"{word}s in multiples of f0", level_note + "; cyan: 1x")
        view(f"strike-{k}", f"{word}s, the strike", level_note)
    if have_cand:
        view("hz-difference", "Difference in hertz", diff_note)
        if pitched:
            view("f0-difference", "Difference in multiples of f0", diff_note)
        view("strike-difference", "Difference, the strike", diff_note)
    for i in (1, 2, 3):
        view(f"line-{i}", f"The {ref_name.lower()}'s {['strongest', 'second', 'third'][i - 1]} line", fall_note)
    view("level", "Level", fall_note.replace("there", "loudest"))

    strike_freqs = near if all_pitched else hertz
    compare = {}
    for e in entries:
        key = (e["row"], e["col"])
        maps = {}
        for k, s in (("reference", e["ref"]), ("candidate", e["cand"])):
            if s is None:
                continue
            m = {"hz": s.bank(hertz, t_log)}
            if e["f0"]:
                m["f0"] = s.bank(wide * e["f0"], t_log)
            m["strike"] = s.bank(strike_freqs * (e["f0"] if all_pitched else 1.0), t_lin)
            maps[k] = m
            for kind, level in m.items():
                p, c, r = views[f"{kind}-{k}"].at(*key)
                p.paste(c, r, level_rgb(level - np.nanmax(level)))
                if kind == "hz" and e["f0"]:
                    p.mark(c, r, hertz, e["f0"])
                if kind == "f0":
                    p.mark(c, r, wide, 1.0)
        if "candidate" in maps:
            for kind in maps["candidate"]:
                p, c, r = views[f"{kind}-difference"].at(*key)
                p.paste(c, r, diff_rgb(maps["candidate"][kind], maps["reference"][kind]))
        # What the siblings are compared on: the f/f0 map where every sound is pitched.
        kind = "f0" if all_pitched else "hz"
        compare[key] = {k: np.maximum(m[kind] - np.nanmax(m[kind]), FLOOR_DB) for k, m in maps.items()}

        # The reference's lines, and both sounds' level at each, over its own ring: a high note's
        # fall on a low one's span is a line at the panel's left edge.
        own = float(np.clip(e["ref"].ring_s() * 1.05, 0.1, span))
        t_own = np.linspace(0.0, own, 2 * w)
        lines = e["ref"].lines(span, e["f0"])
        for i in (1, 2, 3):
            p, c, r = views[f"line-{i}"].at(*key)
            if len(lines) < i:
                p.graph(c, r, [], 0.0, own, "none")
                continue
            hz = lines[i - 1]
            ref_curve = e["ref"].bank([hz], t_own)[0]
            cand_curve = e["cand"].bank([hz], t_own)[0] if e["cand"] else None
            label = f"{hz / e['f0']:.2f}x" if e["f0"] else f"{hz:.0f} Hz"
            p.graph(c, r, [(ref_curve, REF_RGB), (cand_curve, CAND_RGB)], np.nanmax(ref_curve), own, label)
        p, c, r = views["level"].at(*key)
        ref_level = e["ref"].level_db(t_own)
        p.graph(c, r, [(ref_level, REF_RGB), (e["cand"].level_db(t_own) if e["cand"] else None, CAND_RGB)],
                np.max(ref_level), own)

        # Pitch: the line each sound rings at its f0.
        if e["f0"]:
            e["ref_line"] = e["ref"].line_near(e["f0"])
            e["cand_line"] = e["cand"].line_near(e["f0"]) if e["cand"] else None
        print("sheets:", e["row"], e["col"], file=sys.stderr)

    report = []
    if pitched:
        report.append("Pitch: the line each rings within 4 % of its f0, cents from the f0 given; a flag at "
                      f"{PITCH_CENTS:g} cents")
        for e in pitched:
            rc, cc = cents(e["ref_line"], e["f0"]), cents(e["cand_line"], e["f0"])
            apart = cents(e["cand_line"], e["ref_line"])
            bad = (rc is not None and abs(rc) > PITCH_CENTS) or (apart is not None and abs(apart) > PITCH_CENTS)
            report.append(f"  {e['row']} {e['col']}: f0 {e['f0']:.2f}; {ref_name.lower()} {e['ref_line'] or 0:.2f} "
                          f"({rc if rc is not None else float('nan'):+.1f} c)"
                          + (f", {cand_name.lower()} {e['cand_line'] or 0:.2f} ({cc:+.1f} c), apart {apart:+.1f} c"
                             if apart is not None else "") + ("   <<" if bad else ""))
            if bad:
                # What tripped it: the candidate off its reference, else the reference off the f0.
                text = (f"{apart:+.0f}c" if apart is not None and abs(apart) > PITCH_CENTS
                        else f"f0 {-rc:+.0f}c")
                for k in [k for k, _ in who] + (["difference"] if have_cand else []):
                    for v in (f"f0-{k}", f"hz-{k}"):
                        if v in views:
                            p, c, r = views[v].at(e["row"], e["col"])
                            p.flag(c, r, text)
        groups = ordered(e["group"] for e in pitched if e["group"])
        if groups:
            report.append("Groups: their references' lines, the spread in cents")
            for g in groups:
                hz = [e["ref_line"] for e in pitched if e["group"] == g and e["ref_line"]]
                if len(hz) > 1:
                    report.append(f"  {g}: {min(hz):.2f}-{max(hz):.2f} Hz, {cents(max(hz), min(hz)):.1f} c")

    # Siblings: each sound against the median of the others in its row and column.
    scores = []
    for e in entries:
        key = (e["row"], e["col"])
        sib = [k for k in compare if k != key and (k[0] == key[0] or k[1] == key[1])]
        if not sib:
            continue
        out_by = {}
        for k in compare[key]:
            others = [compare[s][k] for s in sib if k in compare[s]]
            if not others:
                continue
            med = np.nanmedian(np.stack(others), axis=0)
            mine = compare[key][k]
            live = (mine > FLOOR_DB + 6) | (med > FLOOR_DB + 6)
            out_by[k] = float(np.nanmean(np.abs(mine - med)[live])) if live.any() else 0.0
        scores.append((key, out_by))
    if scores:
        refs = [s["reference"] for _, s in scores]
        typical = float(np.median(refs))
        odd_refs = [(k, s["reference"] - typical) for k, s in scores if s["reference"] - typical > SIBLING_DB]
        odd_cands = [(k, s["candidate"] - s["reference"]) for k, s in scores
                     if "candidate" in s and s["candidate"] - s["reference"] > SIBLING_DB]
        report.append(f"Siblings: mean dB from the median of the others in the row and column (references "
                      f"typically {typical:.1f}); framed at {SIBLING_DB:g} dB over")
        report.append(f"  {cand_name.lower()}s further from theirs than their {ref_name.lower()}: "
                      + (", ".join(f"{k[0]} {k[1]} +{d:.1f}" for k, d in sorted(odd_cands, key=lambda x: -x[1])) or "none"))
        report.append(f"  {ref_name.lower()}s unlike theirs: "
                      + (", ".join(f"{k[0]} {k[1]} +{d:.1f}" for k, d in sorted(odd_refs, key=lambda x: -x[1])) or "none"))
        for key, _ in odd_cands:
            for v in ("hz-candidate", "f0-candidate", "hz-difference", "f0-difference"):
                if v in views:
                    p, c, r = views[v].at(*key)
                    p.frame(c, r, RED)
        for key, _ in odd_refs:
            for v in ("hz-reference", "f0-reference"):
                if v in views:
                    p, c, r = views[v].at(*key)
                    p.frame(c, r, ORANGE)

    # Axes.
    log_ticks = [(t, f"{t * 1000:g} ms" if t < 1 else f"{t:g} s") for t in (0.001, 0.01, 0.1, 1.0, 10.0) if t <= span]
    log_where = lambda t: np.log(t / t_log[0]) / np.log(t_log[-1] / t_log[0])
    hz_fmt = lambda f: f"{f // 1000:g}k" if f >= 1000 else f"{f:g}"
    for name, v in views.items():
        if name.startswith("hz-"):
            v.each(lambda p: (p.freq_ticks(hertz, (50, 100, 200, 500, 1000, 2000, 5000, 10000), hz_fmt),
                              p.time_ticks(log_ticks, log_where)))
        elif name.startswith("f0-"):
            v.each(lambda p: (p.freq_ticks(wide, (0.5, 1, 2, 4, 8, 16, 32), lambda q: f"{q:g}x"),
                              p.time_ticks(log_ticks, log_where)))
        elif name.startswith("strike-"):
            v.each(lambda p: (p.freq_ticks(strike_freqs, (0.5, 1, 2, 4, 8, 16, 32) if all_pitched
                                           else (50, 100, 200, 500, 1000, 2000, 5000, 10000),
                                           (lambda q: f"{q:g}x") if all_pitched else hz_fmt),
                              p.time_ticks([(0.0, "0 ms"), (0.01, "10"), (0.02, "20"), (0.03, "30")],
                                           lambda t: (t - t_lin[0]) / (t_lin[-1] - t_lin[0]))))
        # The graphs (line-*, level): each over its own ring, its span in its corner.

    paths = []
    for name, v in views.items():
        if args.views and name not in args.views:
            continue
        paths += v.save(out)
    text = "\n".join(report)
    (out / "summary.txt").write_text(text + "\n", encoding="utf-8")
    print(text)
    for p in paths:
        print(f"LOOK: {p}")


# --- zoom ----------------------------------------------------------------------------------------

def run_zoom(args):
    ref_name, cand_name = args.names.split(",")
    ref = Sound(args.reference)
    cand = Sound(args.candidate) if args.candidate != "-" else None
    rows = [ref_name, cand_name, "Difference"] if cand else [ref_name]
    title = f"{Path(args.reference).stem}: {args.start * 1000:g}-{args.end * 1000:g} ms, {args.lo:g}-{args.hi:g} Hz"
    sheet = Sheet(title, rows, [""], f"Gaussian bands sigma f/{args.q:g}; each file against its own loudest here, black {FLOOR_DB:g} dB")
    w, h = sheet.size()
    freqs = np.exp(np.linspace(np.log(args.hi), np.log(args.lo), h))
    times = np.linspace(args.start, args.end, w)
    a = ref.bank(freqs, times, args.q)
    sheet.paste(0, 0, level_rgb(a - np.nanmax(a)))
    if cand:
        b = cand.bank(freqs, times, args.q)
        sheet.paste(0, 1, level_rgb(b - np.nanmax(b)))
        sheet.paste(0, 2, diff_rgb(b, a))
    span = args.end - args.start
    step = next(s for s in (0.0005, 0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0) if span / s <= 12)
    ticks = [(args.start + k * step, f"{(args.start + k * step) * 1000:g} ms") for k in range(int(span / step) + 1)]
    sheet.time_ticks(ticks, lambda t: (t - args.start) / span)
    fticks = [f for f in np.exp(np.linspace(np.log(args.lo), np.log(args.hi), 9))]
    sheet.freq_ticks(freqs, fticks, lambda f: f"{f:.0f}")
    if args.f0:
        for r in range(len(rows)):
            for k in range(1, 64):
                sheet.mark(0, r, freqs, k * args.f0)
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    sheet.img.save(out, optimize=True)
    print(f"LOOK: {out.resolve()}")


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("set", help="contact sheets of a set of sounds")
    s.add_argument("manifest")
    s.add_argument("out")
    s.add_argument("--title")
    s.add_argument("--names", default="Recording,Model")
    s.add_argument("--span", type=float, help="seconds the whole-sound views span (default: the longest ring)")
    s.add_argument("--views", type=lambda text: text.split(","),
                   help="only these sheets, by name (level,hz-difference): a large set compared on a few")
    z = sub.add_parser("zoom", help="one region of one pair, large")
    z.add_argument("reference")
    z.add_argument("candidate", help="a WAV, or - for the reference alone")
    z.add_argument("out")
    z.add_argument("--from", dest="start", type=float, default=0.0)
    z.add_argument("--to", dest="end", type=float, default=0.1)
    z.add_argument("--lo", type=float, default=LOW_HZ)
    z.add_argument("--hi", type=float, default=TOP_HZ)
    z.add_argument("--f0", type=float)
    z.add_argument("--q", type=float, default=Q)
    z.add_argument("--names", default="Recording,Model")
    args = ap.parse_args(argv)
    if args.cmd == "set":
        run_set(args)
    else:
        if not (args.end > args.start and args.hi > args.lo > 0):
            sys.exit("zoom: --to must be past --from, and --hi over --lo")
        run_zoom(args)


if __name__ == "__main__":
    main()
