#!/usr/bin/env python3
"""Check the Fluent keys of lirtup-gui.

- every key the Rust code names (a string literal that looks like a key and
  starts with the prefix of some defined key) exists in Spanish and English;
- both languages define the same keys;
- no message is defined twice.

    python3 scripts/check-ftl-keys.py

Exits 1 when something is missing. The round 4 test (front M) replaces it
with a cargo test; this is the reviewer's check for the round 3 merge.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "crates" / "lirtup-ui"
MSG = re.compile(r"^([a-z][a-z0-9-]*)\s*=", re.M)
LITERAL = re.compile(r'"([a-z][a-z0-9]*(?:-[a-z0-9]+)+)"')


def defined(lang):
    keys, dup = {}, []
    for f in sorted((ROOT / "i18n" / lang).glob("*.ftl")):
        for k in MSG.findall(f.read_text()):
            if k in keys:
                dup.append(f"{lang}/{f.name}: {k} (also in {keys[k]})")
            keys[k] = f.name
    return keys, dup


es, dup_es = defined("es")
en, dup_en = defined("en")
prefixes = {k.split("-")[0] for k in es} | {k.split("-")[0] for k in en}

used = {}
for f in sorted((ROOT / "src").rglob("*.rs")):
    for k in LITERAL.findall(f.read_text()):
        if k.split("-")[0] in prefixes:
            used.setdefault(k, f.relative_to(ROOT).as_posix())

problems = dup_es + dup_en
for k, where in sorted(used.items()):
    for lang, keys in (("es", es), ("en", en)):
        if k not in keys:
            problems.append(f"{lang}: missing {k} (used in {where})")
for k in sorted(set(es) - set(en)):
    problems.append(f"en: missing {k} (defined in es/{es[k]})")
for k in sorted(set(en) - set(es)):
    problems.append(f"es: missing {k} (defined in en/{en[k]})")

print(f"{len(es)} keys in es, {len(en)} in en, {len(used)} named in the code")
for p in problems:
    print(p)
sys.exit(1 if problems else 0)
