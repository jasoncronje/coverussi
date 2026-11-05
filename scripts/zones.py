#!/usr/bin/env python3
"""Write crates/app/src/zones.rs: every IANA zone, its POSIX TZ rule (the
footer of its tzfile), which the board's libc understands, and its country
(from zone.tab, through tzdata.zi's links; empty for UTC and the like).

    python3 scripts/zones.py <tzdata zoneinfo dir>

The tzdata package has one:
    uv run --with tzdata python3 -c 'import tzdata, pathlib; print(pathlib.Path(tzdata.__file__).parent / "zoneinfo")'
"""

import sys
from pathlib import Path

root = Path(sys.argv[1])
zones = {}
for path in sorted(root.rglob("*")):
    if not path.is_file() or path.name.startswith((".", "_")):
        continue
    data = path.read_bytes()
    if not data.startswith(b"TZif"):
        continue
    lines = data.rstrip(b"\n").rsplit(b"\n", 1)
    if len(lines) == 2 and lines[1] and not lines[1].startswith(b"TZif"):
        zones[path.relative_to(root).as_posix()] = lines[1].decode()

countries = {}
for line in (root / "zone.tab").read_text().splitlines():
    if line and not line.startswith("#"):
        code, _, name = line.split("\t")[:3]
        countries[name] = code
links = {}
for line in (root / "tzdata.zi").read_text().splitlines():
    if line.startswith("L "):
        _, target, name = line.split()
        links[name] = target


def country(name):
    for _ in range(5):
        if name in countries:
            return countries[name]
        name = links.get(name, "")
    return ""


out = Path(__file__).resolve().parent.parent / "crates/app/src/zones.rs"
with out.open("w") as f:
    f.write("// Written by scripts/zones.py from tzdata; don't edit.\n")
    f.write("// Each IANA zone, its POSIX TZ rule and its country, sorted by name.\n\n")
    f.write("pub const ZONES: &[(&str, &str, &str)] = &[\n")
    for name in sorted(zones):
        f.write(f'    ("{name}", "{zones[name]}", "{country(name)}"),\n')
    f.write("];\n")
print(f"{len(zones)} zones → {out}")
