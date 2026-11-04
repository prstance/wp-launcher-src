#!/usr/bin/env python3
"""Turns a Riivolution package (zip, folder or older package) into a .wpmod: the same files with
a mod.json. With an --out ending in .wpver and a --theme, it makes a version: a version.json instead.

  scripts/make-wpmod.py paquet.zip --id auteur.mon-mod --name "Mon mod" --version 1.0.0 \
      --game SUPP01 --out "Mon-mod-1.0.0.wpmod"

  scripts/make-wpmod.py Mode-Hiver.wpmod --id prstance.mode-hiver --name "Mode Hiver" --version 2026.11.5 \
      --game SUPP01 --theme themes/mode-hiver --out src-tauri/versions/Mode-Hiver-2026.11.5.wpver

Every XML of riivolution/ goes in the manifest, with the first choice of each of its options, except
those that a setting of --manifest picks.
"""

import argparse
import json
import sys
import tempfile
import xml.etree.ElementTree as ElementTree
import zipfile
from pathlib import Path


def patches_of(root):
    patches = []
    for xml in sorted((root / "riivolution").glob("*.xml")):
        options = [
            {"section": section.get("name"), "option": option.get("name"), "choice": 1}
            for section in ElementTree.parse(xml).getroot().iterfind("options/section")
            for option in section.iterfind("option")
        ]
        patches.append({"type": "riivolution", "xml": xml.relative_to(root).as_posix(), "options": options})
    if not patches:
        sys.exit(f"aucun XML dans {root / 'riivolution'}")
    return patches


def add_theme(z, folder):
    """The folder's images go in theme/; its theme.json goes in the manifest, by the caller."""
    for f in sorted(folder.iterdir()):
        if f.is_file() and f.name != "theme.json":
            z.write(f, f"theme/{f.name}")


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("source", type=Path, help="paquet Riivolution : un zip ou un dossier")
    p.add_argument("--id", required=True, help="identifiant unique : minuscules, chiffres, . _ -")
    p.add_argument("--name", required=True)
    p.add_argument("--version", required=True)
    p.add_argument("--game", action="append", required=True, help="identifiant du jeu (répétable)")
    p.add_argument("--author", action="append", default=[])
    p.add_argument("--description", default="")
    p.add_argument("--requires", action="append", default=[], metavar="ID=VERSIONS")
    p.add_argument("--conflicts", action="append", default=[], metavar="ID=VERSIONS")
    p.add_argument("--extra", action="append", default=[], type=Path, help="fichier à ajouter, comme des crédits")
    p.add_argument("--theme", type=Path, help="dossier du thème d'une version : theme.json et ses images")
    p.add_argument("--manifest", type=Path, help="JSON fusionné dans le manifeste : settings, icon, textes par langue…")
    p.add_argument("--out", type=Path, required=True)
    args = p.parse_args()
    is_version = args.out.suffix == ".wpver"
    if is_version != bool(args.theme):
        sys.exit("une version (.wpver) a besoin de --theme, et un .wpmod n'en a pas")

    with tempfile.TemporaryDirectory() as tmp:
        root = args.source
        if root.is_file():
            with zipfile.ZipFile(root) as z:
                z.extractall(tmp)
            root = Path(tmp)
        manifest = {
            "format": 1,
            "id": args.id,
            "name": args.name,
            "version": args.version,
            "authors": args.author,
            "description": args.description,
            "games": args.game,
            "requires": dict(r.split("=", 1) for r in args.requires),
            "conflicts": dict(c.split("=", 1) for c in args.conflicts),
            "patches": patches_of(root),
        }
        if args.theme:
            manifest["theme"] = json.loads((args.theme / "theme.json").read_text())
        if args.manifest:
            manifest.update(json.loads(args.manifest.read_text()))
            # An option a setting picks is the player's: it leaves the fixed ones.
            bound = {(b["xml"], b["section"], b["option"]) for b in (s.get("riivolution") for s in manifest.get("settings", [])) if b}
            for patch in manifest["patches"]:
                patch["options"] = [o for o in patch["options"] if (patch["xml"], o["section"], o["option"]) not in bound]
        args.out.parent.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(args.out, "w", zipfile.ZIP_DEFLATED) as z:
            z.writestr("version.json" if is_version else "mod.json", json.dumps(manifest, ensure_ascii=False, indent=2))
            for f in sorted(root.rglob("*")):
                # riivolution/config holds the choices saved by Riivolution itself.
                if f.is_file() and f.name not in ("mod.json", "version.json") and "riivolution/config" not in f.relative_to(root).as_posix():
                    z.write(f, f.relative_to(root))
            for f in args.extra:
                z.write(f, f.name)
            if args.theme:
                add_theme(z, args.theme)
    print(f"{args.out} ({args.out.stat().st_size // 1024} Ko)")


if __name__ == "__main__":
    main()
