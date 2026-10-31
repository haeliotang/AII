#!/usr/bin/env python3
"""Check that relative Markdown links and image paths point at real files.

Scans every tracked Markdown file (plus tracked Markdown discovered on disk),
skips fenced code blocks and inline code, ignores external URLs, anchors and
mailto links, and fails when a repository-relative target does not exist.

Usage: python3 scripts/check-docs.py [repo-root]
"""

import os
import re
import sys
from pathlib import Path
from urllib.parse import unquote

SKIP_DIRS = {".git", "target", "node_modules", "__pycache__", ".cargo-cache", ".rustup-cache"}
LINK = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")
REFERENCE = re.compile(r"^\s*\[[^\]]+\]:\s*(\S+)", re.MULTILINE)
FENCE = re.compile(r"^(```|~~~).*?^\1", re.MULTILINE | re.DOTALL)
INLINE_CODE = re.compile(r"`[^`\n]*`")


def markdown_files(root):
    for directory, dirnames, filenames in os.walk(root):
        dirnames[:] = [name for name in dirnames if name not in SKIP_DIRS]
        for filename in filenames:
            if filename.endswith(".md"):
                yield Path(directory) / filename


def targets_in(path):
    text = path.read_text(encoding="utf-8")
    text = FENCE.sub("", text)
    text = INLINE_CODE.sub("", text)
    for match in LINK.finditer(text):
        yield match.group(1).strip()
    for match in REFERENCE.finditer(text):
        yield match.group(1).strip()


def check(root):
    problems = []
    for path in markdown_files(root):
        base = path.parent
        for raw in targets_in(path):
            target = raw.split()[0] if raw else ""
            if not target or target.startswith(("#", "http://", "https://", "mailto:", "tel:")):
                continue
            target = unquote(target)
            anchor = target.find("#")
            if anchor >= 0:
                target = target[:anchor]
            if not target:
                continue
            if target.startswith("/"):
                resolved = root / target.lstrip("/")
            else:
                resolved = base / target
            if not resolved.exists():
                problems.append(f"{path.relative_to(root)}: missing target {raw!r}")
    return problems


def main():
    root = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent
    problems = check(root)
    for problem in problems:
        print(problem)
    if problems:
        print(f"FAILED: {len(problems)} missing link target(s)")
        return 1
    print(f"OK: all relative Markdown links resolve in {root}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
