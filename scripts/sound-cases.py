#!/usr/bin/env python3
"""Cases for src-tauri/src/sound (test merges_as_merge_py): a package with a "sound" patch merged by
wp-sound's tools/wpsound/merge.py, alone and with two more mods, for the Rust merge to give the same bytes.

    python3 scripts/sound-cases.py <package .wpmod> <out dir> [--disc DIR] [--wp-sound DIR]
    WP_SOUND_CASES=<out dir> cargo test --manifest-path src-tauri/Cargo.toml merges_as_merge_py

Cases: one/ (the package), three/ (the package, a copy of it under another id with its names renamed,
a bank of the game in its group and edits of the game's sounds, then a mod of edits only: the same wave
and fields changed by several mods, so warnings). Each: mods/<n>-<id>/ in load order, out/ and
result.json (merge.py's output).
"""
import argparse, json, shutil, subprocess, sys, zipfile
from pathlib import Path

HOME = Path.home() / "wii-party-mod"


def sound_patches(mod):
    m = json.loads((mod / "mod.json").read_text(encoding="utf-8"))
    return m, [p for p in m["patches"] if p["type"] == "sound"]


def renamed(mod, dest, suffix):
    """A copy of the mod under another id, the names it adds renamed, plus a bank of the game and edits."""
    shutil.copytree(mod, dest)
    m, patches = sound_patches(dest)
    m["id"] += suffix
    (dest / "mod.json").write_text(json.dumps(m, indent=1, ensure_ascii=False), encoding="utf-8")
    for p in patches:
        path = dest / p["patch"]
        patch = json.loads(path.read_text(encoding="utf-8"))
        banks = {b["label"]: b["label"] + suffix.upper().replace(".", "_") for g in patch.get("groups", []) for b in g.get("banks", [])}
        for g in patch.get("groups", []):
            g["label"] += suffix.upper().replace(".", "_")
            for b in g.get("banks", []):
                b["label"] = banks[b["label"]]
            for s in g.get("sounds", []):
                s["label"] += suffix.upper().replace(".", "_")
                if "bank" in s:
                    s["bank"] = banks.get(s["bank"], s["bank"])
            g["files"].append({"bank": "BANK_SEBD001"})
        patch["sounds"] = {"SEQ_SEBD001_03": {"volume": 90, "player": "PLAYER_SE_GAME"}}
        patch["wsd"] = {"WSD_SEMG000_01": {"pitch": 1.1, "note": {"release": 100, "attack": 120}}}
        path.write_text(json.dumps(patch, indent=1), encoding="utf-8")


def edits_only(dest, ident, waves_from):
    """A mod that only edits: the wave the package replaces, and the fields the copy edits."""
    (dest / "sound").mkdir(parents=True)
    _, patches = sound_patches(waves_from)
    first = json.loads((waves_from / patches[0]["patch"]).read_text(encoding="utf-8"))
    waves = []
    for w in first.get("waves", [])[:1]:
        shutil.copy(waves_from / Path(patches[0]["patch"]).parent / w["rwav"], dest / "sound/w.rwav")
        waves.append({**w, "rwav": "w.rwav"})
    patch = {"format": 1, "waves": waves,
             "sounds": {"SEQ_SEBD001_03": {"volume": 70, "priority": 60}},
             "wsd": {"WSD_SEMG000_01": {"note": {"release": 90}, "fx_send": [10, 0, 0], "event": {"length": 0.5}}}}
    (dest / "sound/patch.json").write_text(json.dumps(patch, indent=1), encoding="utf-8")
    m = {"format": 1, "id": ident, "name": {"fr": ident}, "version": "1.0.0", "games": ["SUPP01"], "launcher": ">=0.6.0",
         "patches": [{"type": "sound", "archive": "/sound/wp_sound.brsar", "patch": "sound/patch.json"}]}
    (dest / "mod.json").write_text(json.dumps(m, indent=1), encoding="utf-8")


def main():
    a = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    a.add_argument("package")
    a.add_argument("out")
    a.add_argument("--disc", default=str(HOME / "original/fst"))
    a.add_argument("--wp-sound", default=str(HOME / "mods/wp-sound"))
    args = a.parse_args()
    out = Path(args.out)
    shutil.rmtree(out, ignore_errors=True)
    one = out / "one/mods/1-package"
    one.mkdir(parents=True)
    with zipfile.ZipFile(args.package) as z:
        z.extractall(one)
    ident = sound_patches(one)[0]["id"]
    three = out / "three/mods"
    shutil.copytree(one, three / "1-package")
    renamed(one, three / "2-renamed", ".copy")
    edits_only(three / "3-edits", ident + ".edits", one)
    for case in ("one", "three"):
        mods = sorted((out / case / "mods").iterdir())
        result = subprocess.run([sys.executable, Path(args.wp_sound) / "tools/wpsound/merge.py", args.disc, out / case / "out", *mods],
                                check=True, capture_output=True, text=True)
        (out / case / "result.json").write_text(result.stdout, encoding="utf-8")
        print(case, result.stdout.strip())


if __name__ == "__main__":
    main()
