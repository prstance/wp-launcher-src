#!/usr/bin/env python3
"""Turns a Riivolution package (zip, folder or older package) into a .wpmod: the same files with
a mod.json. With an --out ending in .wpver and a --theme, it makes a version: a version.json instead.

  scripts/make-wpmod.py paquet.zip --id auteur.mon-mod --name "Mon mod" --version 1.0.0 \
      --game SUPP01 --out "Mon-mod-1.0.0.wpmod"

  scripts/make-wpmod.py Mode-Hiver.wpmod --id prstance.mode-hiver --name "Mode Hiver" --version 2026.11.5 \
      --game SUPP01 --theme themes/mode-hiver --out src-tauri/versions/Mode-Hiver-2026.11.5.wpver

Every XML of riivolution/ goes in the manifest, with the first choice of each of its options, except
those that a setting of --manifest picks.

With --disc (the original disc image, any format dolphin-tool reads), every file of the package that
replaces one of the disc's becomes a delta against it (src-tauri/src/delta.rs): the package then
carries none of the game's files, and the launcher makes them again from the player's disc before
the first launch. It needs dolphin-tool from the fork (sys/main.dol) and cargo, for
src-tauri/examples/wpdelta.rs.
"""

import argparse
import json
import os
import subprocess
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


HERE = Path(__file__).resolve().parent
TAURI = HERE.parent / "src-tauri"
# The launcher version that knows the "delta" patches.
DELTA_LAUNCHER = ">=0.2"


def dolphin_tool():
    """The launcher's own (src-tauri/tools, npm run tools), or WP_DOLPHIN_TOOL."""
    if os.environ.get("WP_DOLPHIN_TOOL"):
        return os.environ["WP_DOLPHIN_TOOL"]
    tools = TAURI / "tools/dolphin"
    installed = json.loads((tools / "installed.json").read_text())
    return str(tools / installed.get("tool", installed["bin"].replace("dolphin-emu", "dolphin-tool")))


def replaced_files(root):
    """(disc path, package path) of each <file> and of each file of a <folder>, in every XML."""
    found = []
    for xml in sorted((root / "riivolution").glob("*.xml")):
        for patch in ElementTree.parse(xml).getroot().iterfind("patch"):
            base = patch.get("root", "").strip("/")
            def at(external):
                return root / external.lstrip("/") if external.startswith("/") else root / base / external
            for node in patch:
                disc, external = node.get("disc"), node.get("external")
                if not disc or not external or "{$" in disc + external:
                    continue
                if node.tag == "file":
                    found.append((disc, at(external)))
                elif node.tag == "folder" and at(external).is_dir():
                    for f in sorted(at(external).rglob("*")):
                        if f.is_file():
                            found.append((f"{disc.rstrip('/')}/{f.relative_to(at(external)).as_posix()}", f))
    return found


def make_deltas(root, image, work):
    """Turns the package's files that the disc has into deltas: the "delta" patch, and the files
    the package no longer carries."""
    tool = dolphin_tool()
    listing = subprocess.run([tool, "extract", "-l", "-i", image, "-p", "DATA"], capture_output=True, text=True, check=True).stdout
    on_disc = {l.strip().lower(): l.strip() for l in listing.splitlines() if l.strip() and not l.strip().endswith("/") and not l.startswith("///")}
    jobs, entries, dropped = [], [], set()
    for disc, file in replaced_files(root):
        single = "sys/main.dol" if disc.lower() == "main.dol" else on_disc.get(disc.lstrip("/").lower())
        if single is None or not file.is_file() or file in dropped:
            continue
        out = work / "disc" / str(len(jobs))
        subprocess.run([tool, "extract", "-q", "-i", image, "-p", "DATA", "-s", single, "-o", str(out)], check=True)
        source = out / "DATA" / ("sys/main.dol" if single == "sys/main.dol" else f"files/{single}")
        if not source.is_file():
            sys.exit(f"dolphin-tool n'a pas extrait {single} (il faut celui du fork pour sys/main.dol)")
        inside = file.relative_to(root).as_posix()
        delta = f"delta/{inside}.wpd"
        lz = file.name.lower().endswith(".lz") and source.read_bytes()[:1] in (b"\x10", b"\x11") and file.read_bytes()[:1] in (b"\x10", b"\x11")
        jobs.append({"source": str(source), "result": str(file), "out": str(work / "out" / delta), "lz": lz})
        entries.append({"disc": disc, "file": inside, "delta": delta})
        dropped.add(file)
    if not jobs:
        return None, set()
    (work / "jobs.json").write_text(json.dumps(jobs))
    made = subprocess.run(["cargo", "run", "--quiet", "--release", "--manifest-path", str(TAURI / "Cargo.toml"), "--example", "wpdelta", "--", str(work / "jobs.json")],
                          capture_output=True, text=True, check=True).stdout
    sizes = [line.split(" ", 2) for line in made.splitlines()]
    print(f"{len(jobs)} deltas : {sum(int(d) for d, _, _ in sizes) // 1024} Ko au lieu de {sum(int(r) for _, r, _ in sizes) // 1024} Ko")
    return {"type": "delta", "files": entries}, dropped


def add_theme(z, folder):
    """The folder's images go in theme/; its theme.json goes in the manifest, by the caller, and so
    does its manifest.json (texts per language, settings…) when --manifest doesn't name another."""
    for f in sorted(folder.iterdir()):
        if f.is_file() and f.name not in ("theme.json", "manifest.json"):
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
    p.add_argument("--disc", help="image du disque d'origine : les fichiers du jeu deviennent des deltas")
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
        if args.theme and not args.manifest and (args.theme / "manifest.json").is_file():
            args.manifest = args.theme / "manifest.json"
        if args.manifest:
            manifest.update(json.loads(args.manifest.read_text()))
            # An option a setting picks is the player's: it leaves the fixed ones.
            bound = {(b["xml"], b["section"], b["option"]) for b in (s.get("riivolution") for s in manifest.get("settings", [])) if b}
            for patch in manifest["patches"]:
                patch["options"] = [o for o in patch["options"] if (patch["xml"], o["section"], o["option"]) not in bound]
        dropped = set()
        if args.disc:
            work = Path(tmp) / ".deltas"
            delta, dropped = make_deltas(root, args.disc, work)
            if delta:
                manifest["patches"].append(delta)
                manifest["launcher"] = DELTA_LAUNCHER
        args.out.parent.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(args.out, "w", zipfile.ZIP_DEFLATED) as z:
            z.writestr("version.json" if is_version else "mod.json", json.dumps(manifest, ensure_ascii=False, indent=2))
            for f in sorted(root.rglob("*")):
                # riivolution/config holds the choices saved by Riivolution itself.
                if f.is_file() and f.name not in ("mod.json", "version.json") and "riivolution/config" not in f.relative_to(root).as_posix() \
                        and ".deltas" not in f.relative_to(root).parts and f not in dropped:
                    z.write(f, f.relative_to(root))
            if args.disc and dropped:
                for f in sorted((work / "out").rglob("*")):
                    if f.is_file():
                        z.write(f, f.relative_to(work / "out"))
            for f in args.extra:
                z.write(f, f.name)
            if args.theme:
                add_theme(z, args.theme)
    print(f"{args.out} ({args.out.stat().st_size // 1024} Ko)")


if __name__ == "__main__":
    main()
