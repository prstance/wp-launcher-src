#!/usr/bin/env python3
# riivo2wpmod turns a Wii Party riivolution mod (zip or folder) into a .wpmod for PartyLauncher
#
#   python riivo2wpmod.py SomeMod.zip
#
# it asks a few things like the name and the version and the author. just hit enter to keep what's in [brackets]
# and -y skips every question. the .wpmod ends up right next to the mod
# only needs python 3.8 or newer and nothing to pip install. on windows u can also just drag the zip onto it

import argparse
import json
import re
import sys
import tempfile
import unicodedata
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

# wii party's ids per region (same as what the launcher reads off the disc)
REGIONS = {"P": ("SUPP01", "Europe"), "E": ("SUPE01", "USA"), "J": ("SUPJ01", "Japan"), "K": ("SUPK01", "Korea")}

quiet = False
double_clicked = False


def ask(question, default=""):
    if quiet:
        return default
    answer = input(f"{question} [{default}] > " if default else f"{question} > ").strip()
    return answer or default


def slug(text):
    # plain lowercase ascii with dashes between the words so it works in ids and file names
    text = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode().lower()
    return re.sub(r"[^a-z0-9]+", "-", text).strip("-")


def find_root(folder):
    # zips love having an extra folder around everything so dig for the riivolution folder instead of guessing
    for xml in sorted(folder.rglob("*.xml"), key=lambda p: len(p.parts)):
        if xml.parent.name.lower() == "riivolution":
            return xml.parent.parent, xml.parent.name
    sys.exit("there's no riivolution folder with an xml in it so this doesn't look like a riivolution mod")


def games_of(xml):
    # most xmls just say <id game="SUPP"/> but some go <id game="SUP"> with <region type="P"/> inside
    games = []
    for node in xml.iterfind("id"):
        game = node.get("game", "").upper()
        if game and not game.startswith("SUP"):
            print(f"heads up this mod says it's for {game} and wii party's ids start with SUP")
        if len(game) >= 4:
            regions = [game[3]]
        else:
            regions = [r.get("type", "").upper() for r in node.iterfind("region")] or list(REGIONS)
        games += [REGIONS[r][0] for r in regions if r in REGIONS]
    return games


def pick(option, choices):
    # one choice means it's just an on switch so no point asking
    if len(choices) == 1:
        return 1
    print(f"\noption \"{option}\" has a few choices")
    for n, choice in enumerate(choices, 1):
        print(f"  {n}) {choice}")
    print("  0) off")
    print("  p) let the player pick in the launcher")
    while True:
        answer = ask("pick one", "1").lower()
        if answer == "p":
            return "player"
        if answer.isdigit() and int(answer) <= len(choices):
            return int(answer)
        print("type one of the numbers or p")


def main():
    global quiet, double_clicked
    parser = argparse.ArgumentParser(description="turns a riivolution mod into a .wpmod for PartyLauncher")
    parser.add_argument("mod", nargs="?", help="the riivolution mod as a zip or a folder")
    parser.add_argument("-y", action="store_true", help="keep every default and ask nothing")
    args = parser.parse_args()
    quiet = args.y
    double_clicked = args.mod is None

    # dragging a file into a terminal sometimes pastes it with quotes around it
    source = Path((args.mod or input("drag the mod's zip or folder here and press enter > ")).strip().strip("\"'"))
    if not source.exists():
        sys.exit(f"can't find {source}")
    if source.is_file() and source.suffix.lower() != ".zip":
        sys.exit("only zips and folders work so unzip the rar or 7z first and give me the folder")

    with tempfile.TemporaryDirectory() as tmp:
        folder = source
        if source.is_file():
            with zipfile.ZipFile(source) as z:
                z.extractall(tmp)
            folder = Path(tmp)
        root, riivo = find_root(folder)

        patches, settings, games = [], [], []
        for xml_path in sorted((root / riivo).glob("*.xml")):
            xml = ET.parse(xml_path).getroot()
            if xml.tag != "wiidisc":
                continue
            games += [g for g in games_of(xml) if g not in games]
            path = xml_path.relative_to(root).as_posix()

            options = []
            for section in xml.iterfind("options/section"):
                for option in section.iterfind("option"):
                    choices = [c.get("name", "") for c in option.iterfind("choice")]
                    if not choices:
                        continue
                    where = {"section": section.get("name"), "option": option.get("name")}
                    picked = pick(option.get("name"), choices)
                    if picked == "player":
                        # shows up as a dropdown on the mod's page in the launcher btw
                        settings.append({
                            "id": slug(f"{where['section']} {where['option']}") or f"option{len(settings) + 1}",
                            "type": "choice",
                            "label": where["option"],
                            "default": 1,
                            "choices": [{"value": 0, "label": "Off"}] + [{"value": n, "label": c or str(n)} for n, c in enumerate(choices, 1)],
                            "riivolution": {"xml": path, **where},
                        })
                    elif picked:
                        options.append({**where, "choice": picked})

            # the launcher refuses an xml with nothing turned on so just leave those out
            if options or any(s["riivolution"]["xml"] == path for s in settings):
                patches.append({"type": "riivolution", "xml": path, "options": options})

        if not patches:
            sys.exit("every option is off so there's nothing to put in the .wpmod lol")

        print()
        name = ask("mod name", source.stem if source.is_file() else source.name)
        version = ask("version", "1.0.0")
        author = ask("author")
        description = ask("description (one sentence)")
        default_id = ".".join(s for s in (slug(author), slug(name)) if s) or "mod"
        mod_id = ask("unique id made of lowercase letters and digits and dots", default_id).lower()
        mod_id = re.sub(r"[^a-z0-9._-]+", "-", mod_id).strip("._-") or default_id
        print("game versions are " + " / ".join(f"{g} ({region})" for g, region in REGIONS.values()))
        games = ask("which ones (spaces between them)", " ".join(games or ["SUPP01"]))
        games = [g.upper() for g in re.split(r"[\s,;]+", games) if g]

        manifest = {
            "format": 1,
            "id": mod_id,
            "name": name,
            "version": version,
            "authors": [author] if author else [],
            "description": description,
            "games": games,
            "patches": patches,
        }
        if settings:
            manifest["settings"] = settings

        out = source.parent / f"{slug(name) or 'mod'}-{slug(version) or '1'}.wpmod"
        count = 0
        with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
            z.writestr("mod.json", json.dumps(manifest, ensure_ascii=False, indent=2))
            for f in sorted(root.rglob("*")):
                inside = f.relative_to(root).as_posix()
                # riivolution/config only has riivolution's own saved choices and nobody needs that tho
                if f.is_file() and inside != "mod.json" and not inside.lower().startswith(f"{riivo.lower()}/config"):
                    z.write(f, inside)
                    count += 1

    print(f"\nsaved {out} with {count} files ({out.stat().st_size // 1024} KB)")
    print("to install it open PartyLauncher and drop the .wpmod on the Library page")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        sys.exit(1)
    except SystemExit as stop:
        # sys.exit prints its message by itself but a double clicked window would close before u can read it
        if isinstance(stop.code, str):
            print(stop.code)
            stop.code = 1
        if double_clicked:
            input("\npress enter to close")
        raise
    if double_clicked:
        input("\npress enter to close")
