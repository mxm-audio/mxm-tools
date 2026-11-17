"""The sheets on a set made here: six struck tones, two strengths of three notes, each with a candidate
that is its reference again, but one 15 cents flat and one under loud noise; both must be flagged,
and every sheet drawn at its size. Run: python -m unittest test_sheets (in this folder); it writes
under the repository's git-ignored `target/sheets-test/`."""

import contextlib
import io
import shutil
import unittest
from pathlib import Path

import numpy as np
from PIL import Image

import sheets

HERE = Path(__file__).resolve().parent
OUT = HERE.parent / "target" / "sheets-test"
RATE = 44100


def write_wav(path, x):
    y = np.clip(np.round(x * 32767), -32768, 32767).astype("<i2").tobytes()
    head = (b"RIFF" + (36 + len(y)).to_bytes(4, "little") + b"WAVEfmt " + (16).to_bytes(4, "little")
            + (1).to_bytes(2, "little") + (1).to_bytes(2, "little") + RATE.to_bytes(4, "little")
            + (RATE * 2).to_bytes(4, "little") + (2).to_bytes(2, "little") + (16).to_bytes(2, "little")
            + b"data" + len(y).to_bytes(4, "little"))
    path.write_bytes(head + y)


def tone(f0, level, cents=0.0, noise=0.0, seed=0):
    t = np.arange(int(1.2 * RATE)) / RATE
    f = f0 * 2 ** (cents / 1200)
    x = np.exp(-t / 0.3) * np.sin(2 * np.pi * f * t) + 0.3 * np.exp(-t / 0.08) * np.sin(2 * np.pi * 4 * f * t)
    x = np.concatenate([np.zeros(441), x])
    x += noise * np.random.default_rng(seed).standard_normal(len(x))
    return level * x / np.abs(x).max()


class Sheets(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        shutil.rmtree(OUT, ignore_errors=True)
        (OUT / "wav").mkdir(parents=True)
        lines = ["row\tcol\treference\tcandidate\tf0\tgroup"]
        for note, f0 in (("A3", 220.0), ("E4", 329.63), ("A4", 440.0)):
            for strength, level in (("soft", 0.3), ("loud", 0.9)):
                name = f"{note}-{strength}"
                write_wav(OUT / "wav" / f"{name}-ref.wav", tone(f0, level))
                cents = -15.0 if name == "E4-soft" else 0.0
                noise = 0.05 if name == "A4-loud" else 0.0
                write_wav(OUT / "wav" / f"{name}-cand.wav", tone(f0, level, cents, noise))
                lines.append(f"{strength}\t{note}\twav/{name}-ref.wav\twav/{name}-cand.wav\t{f0}\t{note}")
        (OUT / "set.tsv").write_text("\n".join(lines) + "\n", encoding="utf-8")
        cls.printed = io.StringIO()
        with contextlib.redirect_stdout(cls.printed), contextlib.redirect_stderr(io.StringIO()):
            sheets.main(["set", str(OUT / "set.tsv"), str(OUT / "sheets"), "--title", "test"])
        cls.text = cls.printed.getvalue()

    def test_every_sheet_is_drawn_at_its_size_and_listed(self):
        names = ["hz-reference", "hz-candidate", "hz-difference", "f0-reference", "f0-candidate",
                 "f0-difference", "strike-reference", "strike-candidate", "strike-difference",
                 "line-1", "line-2", "line-3", "level"]
        for n in names:
            path = OUT / "sheets" / f"{n}.png"
            self.assertTrue(path.exists(), n)
            with Image.open(path) as image:
                self.assertEqual(image.size, (sheets.W, sheets.H), n)
            self.assertIn(f"LOOK: {path.resolve()}", self.text, n)

    def test_the_flat_candidate_is_flagged_and_no_other(self):
        flagged = [l for l in self.text.splitlines() if l.endswith("<<")]
        self.assertEqual(len(flagged), 1, flagged)
        self.assertIn("soft E4", flagged[0])
        self.assertRegex(flagged[0], r"apart -1[45]\.\d c")

    def test_the_noisy_candidate_is_the_one_unlike_its_siblings(self):
        line = next(l for l in self.text.splitlines() if "further from theirs" in l)
        self.assertIn("loud A4", line)
        self.assertNotIn("soft A3", line)

    def test_a_group_s_references_agree(self):
        line = next(l for l in self.text.splitlines() if l.strip().startswith("E4:"))
        self.assertRegex(line, r", 0\.\d c$")

    def test_zoom_draws_one_region(self):
        with contextlib.redirect_stdout(io.StringIO()) as out:
            sheets.main(["zoom", str(OUT / "wav" / "E4-soft-ref.wav"), str(OUT / "wav" / "E4-soft-cand.wav"),
                         str(OUT / "zoom.png"), "--from", "0", "--to", "0.5", "--lo", "200", "--hi", "2000",
                         "--f0", "329.63"])
        self.assertIn("LOOK:", out.getvalue())
        with Image.open(OUT / "zoom.png") as image:
            self.assertEqual(image.size, (sheets.W, sheets.H))


if __name__ == "__main__":
    unittest.main()
