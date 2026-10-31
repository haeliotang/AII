#!/usr/bin/env python3
"""Check that docs/status/test-matrix.zh-CN.md matches the actual test counts.

Parses the library-crate table and the total from the Markdown file, runs
`cargo test -q -p <crate> -- --list` for every listed crate, and fails when a
count or the total drifts. Keeps evidence documents honest as tests are added.

Usage: python3 scripts/check-test-matrix.py [repo-root]
"""

import re
import subprocess
import sys
from pathlib import Path

ROW = re.compile(r"^\|\s*`(poie-[a-z0-9-]+)`\s*\|\s*(\d+)\s*\|", re.MULTILINE)
TOTAL = re.compile(r"\*\*库 crate 测试合计：(\d+) 个已列出的测试\*\*")
LIST_END = ": test"


def listed_tests(root: Path, package: str) -> int:
    completed = subprocess.run(
        ["cargo", "test", "-q", "-p", package, "--", "--list"],
        cwd=root,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"cannot list tests for {package}: {completed.stderr.strip()}")
    return sum(1 for line in completed.stdout.splitlines() if line.endswith(LIST_END))


def main():
    root = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent
    matrix = root / "docs" / "status" / "test-matrix.zh-CN.md"
    text = matrix.read_text(encoding="utf-8")
    rows = ROW.findall(text)
    total_match = TOTAL.search(text)
    if not rows or not total_match:
        print(f"FAILED: cannot find the crate table or total in {matrix}")
        return 1

    expected_total = int(total_match.group(1))
    failures = 0
    actual_total = 0
    for package, documented in rows:
        actual = listed_tests(root, package)
        actual_total += actual
        if actual != int(documented):
            print(f"MISMATCH {package}: documented {documented}, actual {actual}")
            failures += 1
        else:
            print(f"ok {package}: {actual}")
    if actual_total != expected_total:
        print(f"MISMATCH total: documented {expected_total}, actual {actual_total}")
        failures += 1
    else:
        print(f"ok total: {actual_total}")
    if failures:
        print(f"FAILED: {failures} test-matrix mismatch(es)")
        return 1
    print("OK: test matrix matches cargo test --list")
    return 0


if __name__ == "__main__":
    sys.exit(main())
