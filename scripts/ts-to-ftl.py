#!/usr/bin/env python3
"""Bring the 1.x translations (Qt `.ts`) to the Fluent files of lirtup-gui.

The 1.x GUI wrote its strings in English in the code (`self.tr(...)`) and
translated them to Spanish in `src/upscaler/gui/locale/upscale_gui_es.ts`
(526 messages). The 2.0 GUI has its own keys, one `.ftl` per screen, so
the strings cannot be copied key by key: they are matched by text.

    python3 scripts/ts-to-ftl.py extract [es.ts]
        Read the `.ts` and write `scripts/ts-to-ftl-1x.tsv` (context,
        English source, Spanish translation, comment; `{0}` becomes
        `{ $arg0 }`). The `.ts` leave the repository with Python in round 4:
        the TSV keeps their strings.

    python3 scripts/ts-to-ftl.py check [--write]
        Compare `crates/lirtup-ui/i18n/{es,en}/*.ftl` with the TSV:
        - a key defined in Spanish and missing in English gets the English
          of the 1.x string with the same Spanish text, if there is one
          (`--write` appends it to the English file of the same screen);
        - a key with the same text in both languages is listed with what
          1.x said, to tell a name (`MagpieFX`) from a missing translation;
        - a summary of how many 2.0 strings come from 1.x.
        Exits 1 when a key is missing in English and 1.x has no match.
"""

import argparse
import csv
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TS = ROOT / "src" / "upscaler" / "gui" / "locale" / "upscale_gui_es.ts"
TSV = Path(__file__).resolve().parent / "ts-to-ftl-1x.tsv"
I18N = ROOT / "crates" / "lirtup-ui" / "i18n"
MSG = re.compile(r"^([a-z][a-z0-9-]*)\s*=\s?(.*)$")


def qt_to_fluent(text):
    """`{0}` → `{ $arg0 }`, `%n` → `{ $count }`; braces of Fluent kept."""
    text = re.sub(r"\{(\d+)\}", r"{ $arg\1 }", text)
    return text.replace("%n", "{ $count }")


def extract(ts_path):
    root = ET.parse(ts_path).getroot()
    rows = []
    for context in root.iter("context"):
        name = context.findtext("name", "")
        for message in context.iter("message"):
            source = message.findtext("source", "")
            translation = message.find("translation")
            if translation is None or translation.get("type") in ("vanished", "obsolete"):
                continue
            numerus = translation.findall("numerusform")
            es = numerus[-1].text if numerus else translation.text
            rows.append(
                (
                    name,
                    qt_to_fluent(source),
                    qt_to_fluent(es or ""),
                    message.findtext("comment", "") or "",
                )
            )
    with TSV.open("w", newline="", encoding="utf-8") as f:
        w = csv.writer(f, delimiter="\t", lineterminator="\n")
        w.writerow(("context", "en", "es", "comment"))
        w.writerows(rows)
    print(f"{len(rows)} messages from {ts_path} → {TSV.relative_to(ROOT)}")


def load_tsv():
    with TSV.open(encoding="utf-8") as f:
        return list(csv.DictReader(f, delimiter="\t"))


def parse_ftl(lang):
    """key → (file name, value with continuation lines joined by `\n`)."""
    out = {}
    for f in sorted((I18N / lang).glob("*.ftl")):
        key = None
        for line in f.read_text(encoding="utf-8").splitlines():
            m = MSG.match(line)
            if m:
                key = m.group(1)
                out[key] = (f.name, m.group(2))
            elif key and line.startswith(" "):
                name, value = out[key]
                out[key] = (name, value + "\n" + line)
            else:
                key = None
    return out


def norm(text):
    """Compare texts without case, ellipses, trailing colons or `&`."""
    text = text.replace("…", "...").replace("&", "").strip().rstrip(":.").strip()
    return re.sub(r"\s+", " ", text).lower()


def check(write):
    rows = load_tsv()
    by_es = {}
    for r in rows:
        by_es.setdefault(norm(r["es"]), r)
    es, en = parse_ftl("es"), parse_ftl("en")

    missing, filled = [], {}
    for key, (fname, value) in es.items():
        if key in en:
            continue
        r = by_es.get(norm(value))
        if r is None:
            missing.append(f"en: missing {key} (es/{fname}: {value!r}); no 1.x match")
        else:
            filled.setdefault(fname, []).append((key, r["en"]))
    for fname, entries in filled.items():
        for key, text in entries:
            print(f"en/{fname}: {key} = {text}  (from 1.x)")
        if write:
            with (I18N / "en" / fname).open("a", encoding="utf-8") as f:
                f.write("\n# From the 1.x translations (scripts/ts-to-ftl.py).\n")
                for key, text in entries:
                    f.write(f"{key} = {text}\n")

    same = [(k, v) for k, (_, v) in es.items() if k in en and en[k][1] == v]
    if same:
        print(f"\n{len(same)} keys with the same text in both languages:")
        for key, value in same:
            r = by_es.get(norm(value))
            said = f"; 1.x: {r['en']!r} → {r['es']!r}" if r else ""
            print(f"  {key} = {value.splitlines()[0]}{said}")

    from_1x = sum(1 for _, v in es.values() if norm(v) in by_es)
    print(
        f"\n{len(es)} keys in es, {len(en)} in en; {from_1x} Spanish texts are 1.x "
        f"strings ({len(rows)} in the 1.x)"
    )
    for m in missing:
        print(m)
    return 1 if missing else 0


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = p.add_subparsers(dest="cmd", required=True)
    e = sub.add_parser("extract")
    e.add_argument("ts", nargs="?", type=Path, default=TS)
    c = sub.add_parser("check")
    c.add_argument("--write", action="store_true")
    args = p.parse_args()
    if args.cmd == "extract":
        extract(args.ts)
        return 0
    return check(args.write)


if __name__ == "__main__":
    sys.exit(main())
