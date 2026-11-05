#!/usr/bin/env python3
"""Writes the address of Mode Hiver's settings in its manifest (themes/mode-hiver/manifest.json).

The mod keeps them in main.dol, one byte each (wp-winter, include/wp/WinterSettings.hpp):
gWinterSettings moves with every build, so this reads it again in the build's main.MAP, then
checks that main.dol holds there the defaults of the manifest's settings.

  python3 scripts/winter-settings.py ../wp-winter/build/SUPP01 [--manifest themes/mode-hiver/manifest.json]
"""
import argparse
import json
import re
import struct
import sys
from pathlib import Path

# The settings of the manifest, in the order of WinterSettings' fields.
FIELDS = ["yeti", "sceau", "mur", "corde", "mini-jeux", "tempete", "tenues", "bonnets"]
SYMBOL = "gWinterSettings"


def address(map_text):
    # "  00000018 000008 802f55d8 0022dd18  8 gWinterSettings 	GameData.o"
    m = re.search(r"^\s+[0-9a-f]{8} ([0-9a-f]{6}) ([0-9a-f]{8}) [0-9a-f]{8}\s+\d+ " + SYMBOL + r"\s", map_text, re.M)
    if not m:
        sys.exit(f"{SYMBOL} est absent de main.MAP : ce build n'a pas les réglages")
    size, at = int(m.group(1), 16), int(m.group(2), 16)
    if size != len(FIELDS):
        sys.exit(f"{SYMBOL} fait {size} octets, {len(FIELDS)} attendus : FIELDS ne suit plus WinterSettings.hpp")
    return at


def dol_bytes(dol, at, n):
    offsets, addresses, sizes = (struct.unpack_from(">18I", dol, k) for k in (0, 0x48, 0x90))
    for offset, start, size in zip(offsets, addresses, sizes):
        if start <= at and at + n <= start + size:
            return dol[offset + at - start:offset + at - start + n]
    sys.exit(f"0x{at:08X} n'est dans aucune section de main.dol")


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("build", type=Path, help="build/SUPP01 de wp-winter (main.MAP, main.dol)")
    p.add_argument("--manifest", type=Path, default=Path(__file__).resolve().parents[1] / "themes/mode-hiver/manifest.json")
    args = p.parse_args()

    at = address((args.build / "main.MAP").read_text(errors="replace"))
    manifest = json.loads(args.manifest.read_text())
    settings = {s["id"]: s for s in manifest.get("settings", [])}
    missing = [f for f in FIELDS if f not in settings]
    if missing:
        sys.exit(f"le manifeste n'a pas les réglages {', '.join(missing)}")
    for i, field in enumerate(FIELDS):
        settings[field]["memory"] = {"offset": f"0x{at + i:08X}", "size": 1}

    found = dol_bytes((args.build / "main.dol").read_bytes(), at, len(FIELDS))
    defaults = bytes(settings[f]["default"] for f in FIELDS)
    if found != defaults:
        sys.exit(f"main.dol a {found.hex()} à 0x{at:08X}, le manifeste attend {defaults.hex()} : ses défauts ne sont plus ceux du mod")

    args.manifest.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    print(f"{SYMBOL} : 0x{at:08X}, défauts {defaults.hex()}, {args.manifest}")


if __name__ == "__main__":
    main()
