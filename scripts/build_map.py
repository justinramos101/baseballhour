#!/usr/bin/env python3
"""Generate bundled terminal geography from Natural Earth GeoJSON files."""

import argparse
import hashlib
import json
from pathlib import Path


def build(countries, states):
    polygons = []
    borders = []
    for feature in countries["features"]:
        geometry = feature["geometry"]
        shapes = geometry["coordinates"] if geometry["type"] == "MultiPolygon" else [geometry["coordinates"]]
        for rings in shapes:
            ring = rings[0]
            if any(-132 <= lon <= -58 and 12 <= lat <= 58 for lon, lat in ring):
                polygons.append(rings)
                borders.append([[round(x, 3), round(y, 3)] for x, y in ring])
    for feature in states["features"]:
        geom = feature["geometry"]
        lines = geom["coordinates"] if geom["type"] == "MultiLineString" else [geom["coordinates"]]
        for line in lines:
            if any(-132 <= x <= -58 and 12 <= y <= 58 for x, y in line):
                borders.append([[round(x, 3), round(y, 3)] for x, y in line])
    land = set()
    for rings in polygons:
        for yi in range(30, 146):
            y = yi * 0.4
            crossings = []
            for ring in rings:
                for a, b in zip(ring, ring[1:]):
                    if (a[1] > y) != (b[1] > y):
                        crossings.append(a[0] + (y - a[1]) * (b[0] - a[0]) / (b[1] - a[1]))
            crossings.sort()
            for left, right in zip(crossings[::2], crossings[1::2]):
                for xi in range(-330, -144):
                    x = xi * 0.4
                    if left <= x <= right:
                        land.add((round(x, 1), round(y, 1)))
    return {"land": sorted(land), "borders": borders}


def build_world(countries, step=0.5, tolerance=0.2):
    """Simplified coastline rings and a hex-encoded land mask, one row per latitude band."""
    rings, polygons = [], []
    for feature in countries["features"]:
        geometry = feature["geometry"]
        shapes = geometry["coordinates"] if geometry["type"] == "MultiPolygon" else [geometry["coordinates"]]
        for shape in shapes:
            polygons.append(shape)
            kept = [shape[0][0]]
            for x, y in shape[0][1:]:
                if abs(x - kept[-1][0]) + abs(y - kept[-1][1]) >= tolerance:
                    kept.append([x, y])
            kept.append(shape[0][-1])
            if len(kept) > 3:
                rings.append([[round(x, 2), round(y, 2)] for x, y in kept])
    columns, rows = int(360 / step), int(180 / step)
    land = []
    for row in range(rows):
        y = 90 - (row + 0.5) * step
        bits = [0] * columns
        for shape in polygons:
            crossings = []
            for ring in shape:
                for a, b in zip(ring, ring[1:]):
                    if (a[1] > y) != (b[1] > y):
                        crossings.append(a[0] + (y - a[1]) * (b[0] - a[0]) / (b[1] - a[1]))
            crossings.sort()
            for left, right in zip(crossings[::2], crossings[1::2]):
                first = max(0, int((left + 180) / step + 0.5))
                last = min(columns - 1, int((right + 180) / step - 0.5))
                for column in range(first, last + 1):
                    bits[column] = 1
        value = int("".join(map(str, bits)), 2)
        land.append(f"{value:0{columns // 4}x}")
    return {"step": step, "coast": rings, "land": land}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("countries", type=Path)
    parser.add_argument("states", type=Path)
    parser.add_argument("--output", type=Path, default=Path("assets/north-america.json"))
    parser.add_argument("--world-output", type=Path, default=Path("assets/world.json"))
    args = parser.parse_args()
    sources = [args.countries.read_bytes(), args.states.read_bytes()]
    result = build(*(json.loads(s) for s in sources))
    world = build_world(json.loads(sources[0]))
    # Build everything before writing, so a failure cannot leave one file regenerated.
    for path in (args.output, args.world_output):
        path.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, separators=(",", ":")) + "\n")
    print(f"{args.output}: {len(result['land'])} land points, {len(result['borders'])} boundaries")
    args.world_output.write_text(json.dumps(world, separators=(",", ":")) + "\n")
    print(f"{args.world_output}: {len(world['coast'])} coastline rings")
    for path, data in zip((args.countries, args.states), sources):
        print(f"{path.name}: sha256 {hashlib.sha256(data).hexdigest()}")


if __name__ == "__main__":
    main()
