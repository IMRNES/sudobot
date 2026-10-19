#!/usr/bin/env python3
"""Regenerate the bot's rung -> badge table from the ladder and the artwork.

`include_bytes!` takes a string literal, so the path cannot be computed from a rung
name at compile time: one line per rung has to be written out. Hand-maintaining
that list is how a new rung silently ships without a badge, so it is generated from
the single source of truth (`LADDER` in the domain) and the files actually present.

    scripts/gen-ladder-icons-table.py

Run it after adding a rung to `LADDER` or replacing the artwork. The Rust side
asserts at test time that the table, the ladder and `icon_path()` all agree, so
forgetting to run it fails `cargo test` rather than shipping.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LADDER_RS = ROOT / "apps/iam/src/domain/leveling/ladder.rs"
ICON_DIR = ROOT / "web/public/rank-icons"
TABLE_RS = ROOT / "apps/bot/src/ladder_icons_table.rs"

# The path as written in `include_bytes!`, relative to the table's own directory.
RELATIVE = "../../../web/public/rank-icons"


def rung_names() -> list[str]:
    """The ladder's rung names, in ladder order."""
    source = LADDER_RS.read_text(encoding="utf-8")
    names = re.findall(r'name:\s*"([^"]+)"', source)
    if not names:
        raise SystemExit(f"no rung names found in {LADDER_RS}")
    return names


def slug(name: str) -> str:
    """A rung name as a file slug, matching `LadderRung::icon_path`."""
    return re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")


def main() -> int:
    names = rung_names()
    missing = [
        name for name in names if not (ICON_DIR / f"{slug(name)}.png").is_file()
    ]
    if missing:
        print("missing artwork:", ", ".join(missing), file=sys.stderr)
        print(f"expected one PNG per rung in {ICON_DIR}", file=sys.stderr)
        return 1

    lines = [
        "// Generated table of rung -> badge file. Regenerate with",
        "// `scripts/gen-ladder-icons-table.py` after adding a rung or artwork.",
        "//",
        "// A separate file because `include_bytes!` needs a string literal — the path",
        "// cannot be computed from the rung name at compile time.",
        "//",
        "// Plain comments, not `//!`: this file is pulled in with `include!`, and an",
        "// inner doc comment is only legal at the top of a module, not at the point of",
        "// inclusion.",
        "",
        "/// One entry per ladder rung, in ladder order.",
        f"static ICONS: [Icon; {len(names)}] = [",
    ]
    for name in names:
        file = f"{slug(name)}.png"
        lines += [
            "    Icon {",
            f'        name: "{name}",',
            f'        file: "{file}",',
            f'        png: include_bytes!("{RELATIVE}/{file}"),',
            "    },",
        ]
    lines += ["];", ""]

    TABLE_RS.write_text("\n".join(lines), encoding="utf-8")
    print(f"wrote {TABLE_RS.relative_to(ROOT)} with {len(names)} entries")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
