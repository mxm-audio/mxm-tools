"""Pack exported rooms into `rooms.js`, the data the viewer's page loads.

    cargo run -p mxm-room-ir --release --bin catalogue -- geometry   # writes the OBJ files
    python crates/mxm-room-ir/viewer/pack_rooms.py [slug ...]        # packs them into rooms.js

With no slugs it packs the set the published page carries. Each room becomes one object: its name,
family, volume, surface area, face count, vertices, faces grouped by surface, and the source and
receiver points. Standard library only; nothing here is part of the crate's build.
"""

import json
import re
import sys
from pathlib import Path

VIEWER = Path(__file__).resolve().parent
GEOMETRY = VIEWER.parents[2] / "target" / "mxm-room-ir" / "geometry"

DEFAULT = [
    "recital-hall",
    "shoebox-hall",
    "chamber-music-hall",
    "baroque-church",
    "basilica",
    "rock-cave",
    "brick-pub",
    "jazz-club",
    "library-reading-room",
    "museum-gallery",
    "city-bus",
    "train-carriage",
    "station-concourse",
    "gothic-cathedral",
    "tiled-bathroom",
]

HEADER = re.compile(r"(.+?) \((.+?), (.+?)\): ([\d.]+) m3, ([\d.]+) m2, (\d+) faces")


def read_obj(path):
    """One room: the header's facts, its vertices, its faces by surface, its marked points."""
    vertices, faces, points, group, header = [], [], [], "room", ""
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("# "):
            header = line[2:].strip()
        elif line.startswith("v "):
            vertices.append([round(float(v), 3) for v in line.split()[1:]])
        elif line.startswith("g "):
            group = line[2:].strip()
        elif line.startswith("f "):
            faces.append({"i": [int(i) - 1 for i in line.split()[1:]], "g": group})
        elif line.startswith("p "):
            points.append(int(line.split()[1]) - 1)
    match = HEADER.match(header)
    if not match:
        raise SystemExit(f"{path.name}: no header line; re-export with `catalogue -- geometry`")
    name, slug, family, volume, area, count = match.groups()
    return {
        "name": name,
        "slug": slug,
        "family": family,
        "volume": float(volume),
        "area": float(area),
        "faces": int(count),
        "v": vertices,
        "f": faces,
        # The three sources first, in the order the catalogue names them, then the receivers.
        "src": [vertices[i] for i in points[:3]],
        "rec": [vertices[i] for i in points[3:]],
    }


def main(slugs):
    if not GEOMETRY.is_dir():
        raise SystemExit(f"no exports in {GEOMETRY}; run `catalogue -- geometry` first")
    rooms = []
    for slug in slugs:
        path = GEOMETRY / f"{slug}.obj"
        if not path.exists():
            raise SystemExit(f"{slug}: no export in {GEOMETRY}")
        rooms.append(read_obj(path))
    out = VIEWER / "rooms.js"
    out.write_text(
        "window.ROOMS = " + json.dumps(rooms, separators=(",", ":")) + ";\n", encoding="utf-8"
    )
    print(f"{out.name}: {len(rooms)} rooms, {out.stat().st_size // 1024} KB")
    for room in rooms:
        print(f"  {room['slug']:22} {room['faces']:4} faces  {room['volume']:8.0f} m3")


if __name__ == "__main__":
    main(sys.argv[1:] or DEFAULT)
