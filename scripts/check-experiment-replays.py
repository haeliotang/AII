#!/usr/bin/env python3
"""Check that generated experiment artifacts are replayed in CI.

Rules:
- every file under experiments/*/raw/ must be referenced by CI (diff target);
- every generated report (REPORT.md, REPORT.sim.md, *-report.md, report.toml)
  must be referenced by CI, except reports listed in MANUAL_REPORTS below, which
  are hand-written analyses rather than generated outputs;
- every experiments/... path mentioned in CI must exist on disk;
- raw/ files of experiments listed in LIVE_RAW_EXPERIMENTS are captured from a
  live external environment (Docker) and cannot be regenerated in CI; they are
  exempt, but their REPORT.md must still be replayed from the committed raw/;
- reports in LIVE_REPORTS are judged against a live container and have no
  offline replay path; they are exempt and labelled as AI previews.

Usage: python3 scripts/check-experiment-replays.py [repo-root]
"""

import re
import sys
from pathlib import Path

ROOT = None
MANUAL_REPORTS = {
    "experiments/h1-structural/REPORT.md",
    "experiments/h1-temporal/REPORT.md",
    "experiments/h2-joint-planning/REPORT.md",
    "experiments/vec-base01/report.md",
}
LIVE_RAW_EXPERIMENTS = {
    "experiments/basyx-shadow",
    "experiments/federation-docker",
}
LIVE_REPORTS = {
    "experiments/basyx-shadow/REPORT.md",
}
PATH = re.compile(r"(experiments/[A-Za-z0-9_./-]+\.(?:md|txt|toml))")


def main():
    root = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent
    ci = (root / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
    cited = set(PATH.findall(ci))

    stale = sorted(path for path in cited if not (root / path).exists())
    raw = sorted(
        str(path.relative_to(root))
        for path in (root / "experiments").glob("*/raw/*")
        if str(path.parent.parent.relative_to(root)) not in LIVE_RAW_EXPERIMENTS
    )
    generated = set(raw)
    report = re.compile(r"^(REPORT\.md|REPORT\.sim\.md|.*-report\.md|report\.toml)$")
    for directory in (root / "experiments").iterdir():
        if not directory.is_dir():
            continue
        for path in list(directory.glob("*.md")) + list(directory.glob("*.toml")):
            # Match case-sensitively: on case-insensitive filesystems glob may
            # otherwise report a pattern-cased name that does not exist as such.
            if report.match(path.name):
                generated.add(str(path.relative_to(root)))
    unreplayed = sorted(path for path in generated if path not in cited and path not in MANUAL_REPORTS | LIVE_REPORTS)

    for path in stale:
        print(f"STALE: CI references a missing path: {path}")
    for path in unreplayed:
        print(f"UNREPLAYED: generated artifact is not replayed in CI: {path}")
    if stale or unreplayed:
        print(f"FAILED: {len(stale)} stale, {len(unreplayed)} unreplayed")
        return 1
    print(f"OK: {len(cited)} CI experiment paths exist; {len(generated)} generated artifacts are replayed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
