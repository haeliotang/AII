#!/usr/bin/env python3
"""Check that docs/status/test-matrix.zh-CN.md matches the actual test counts.

Parses the library-crate table, the total and the status-snapshot count, runs
`cargo test -q -p <crate> -- --list` for every listed crate and for every other
workspace package (experiment/adaptor runners), and fails when a count or a
total drifts. Keeps evidence documents honest as tests are added.

Usage: python3 scripts/check-test-matrix.py [repo-root]
"""

import json
import re
import subprocess
import sys
from pathlib import Path

ROW = re.compile(r"^\|\s*`(poie-[a-z0-9-]+)`\s*\|\s*(\d+)\s*\|", re.MULTILINE)
TOTAL = re.compile(r"\*\*库 crate 测试合计：(\d+) 个已列出的测试\*\*")
SNAPSHOT = re.compile(r"Rust 列表测试\s*\|\s*\*\*(\d+)\*\*（库 crate (\d+) \+ 其他包 (\d+)）")
LIST_END = ": test"


def workspace_packages(root: Path) -> list[str]:
    completed = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--offline"],
        cwd=root,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"cannot read cargo metadata: {completed.stderr.strip()}")
    return sorted(
        package["name"] for package in json.loads(completed.stdout)["packages"] if package["name"].startswith("poie-")
    )


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
    snapshot = root / "docs" / "status" / "status-snapshot.zh-CN.md"
    snapshot_match = SNAPSHOT.search(snapshot.read_text(encoding="utf-8")) if snapshot.exists() else None
    if not snapshot_match:
        print(f"FAILED: cannot find the test count in {snapshot}")
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
    matrix_packages = {package for package, _ in rows}
    other_packages = [package for package in workspace_packages(root) if package not in matrix_packages]
    other_total = sum(listed_tests(root, package) for package in other_packages)
    snap_total, snap_library, snap_other = (int(value) for value in snapshot_match.groups())
    if (snap_library, snap_other, snap_total) != (actual_total, other_total, actual_total + other_total):
        print(
            f"MISMATCH status snapshot: documented {snap_total} ({snap_library} library + {snap_other} other), "
            f"actual {actual_total + other_total} ({actual_total} library + {other_total} other: {other_packages})"
        )
        failures += 1
    else:
        print(f"ok status snapshot: {snap_total} ({snap_library} + {snap_other})")
    if failures:
        print(f"FAILED: {failures} test-matrix mismatch(es)")
        return 1
    print("OK: test matrix matches cargo test --list")
    return 0


if __name__ == "__main__":
    sys.exit(main())
