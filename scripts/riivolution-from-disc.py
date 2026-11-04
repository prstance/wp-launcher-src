#!/usr/bin/env python3
"""Makes a Riivolution package out of a modded copy of an extracted disc: what differs
from the original disc, for scripts/make-wpmod.py.

  scripts/riivolution-from-disc.py copie/ original/ --id MonMod --name "Mon mod" --game SUPP --out paquet/

The copy and the original hold sys/ and files/. The game's main.dol goes in as
<file disc="main.dol">, which only Dolphin understands: a Wii wouldn't load it.
"""

import argparse
import shutil
import sys
from pathlib import Path
from xml.sax.saxutils import quoteattr


def differs(a, b):
    sa, sb = a.stat(), b.stat()
    if (sa.st_ino, sa.st_dev) == (sb.st_ino, sb.st_dev):
        return False
    return sa.st_size != sb.st_size or a.read_bytes() != b.read_bytes()


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("copy", type=Path)
    p.add_argument("original", type=Path)
    p.add_argument("--id", required=True, help="nom du patch et de son dossier : lettres et chiffres")
    p.add_argument("--name", required=True)
    p.add_argument("--game", required=True, help="identifiant du jeu pour Riivolution, comme SUPP")
    p.add_argument("--out", type=Path, required=True)
    args = p.parse_args()
    if args.out.exists():
        sys.exit(f"{args.out} existe déjà")

    entries = []

    def add(source, external, attributes):
        target = args.out / args.id / external
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        entries.append(f'    <file {attributes} external={quoteattr(external)} />')

    if differs(args.copy / "sys/main.dol", args.original / "sys/main.dol"):
        add(args.copy / "sys/main.dol", "sys/main.dol", 'disc="main.dol"')
    for f in sorted((args.copy / "files").rglob("*")):
        if not f.is_file():
            continue
        path = f.relative_to(args.copy / "files").as_posix()
        original = args.original / "files" / path
        if not original.is_file():
            add(f, path, f'disc={quoteattr("/" + path)} create="true"')
        elif differs(f, original):
            add(f, path, f'disc={quoteattr("/" + path)}')
    if not entries:
        sys.exit("aucune différence avec le disque d'origine")

    newline = "\n"
    xml = f"""<wiidisc version="1">
  <id game={quoteattr(args.game)} />
  <options>
    <section name="Wii Party">
      <option name={quoteattr(args.name)}>
        <choice name="Enabled">
          <patch id={quoteattr(args.id)} />
        </choice>
      </option>
    </section>
  </options>
  <patch id={quoteattr(args.id)} root={quoteattr("/" + args.id)}>
{newline.join(entries)}
  </patch>
</wiidisc>
"""
    (args.out / "riivolution").mkdir(parents=True)
    (args.out / "riivolution" / f"{args.id}.xml").write_text(xml)
    print(f"{len(entries)} fichiers dans {args.out}")


if __name__ == "__main__":
    main()
