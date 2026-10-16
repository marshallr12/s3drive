#!/usr/bin/env python3
"""Check local Markdown link targets in the book and repository entry points."""
import pathlib
import re
import sys
from urllib.parse import unquote, urlsplit

root = pathlib.Path(__file__).resolve().parent.parent
files = [*root.glob("*.md"), *(root / "docs/src").rglob("*.md")]
errors = []
for source in files:
    for target in re.findall(r"\]\(([^\s)]+)\)", source.read_text()):
        parsed = urlsplit(target)
        if parsed.scheme or not parsed.path:
            continue
        if not (source.parent / unquote(parsed.path)).exists():
            errors.append(f"{source.relative_to(root)}: missing {target}")
if errors:
    print("\n".join(errors), file=sys.stderr)
    sys.exit(1)
print(f"Local documentation links OK ({len(files)} Markdown files).")
