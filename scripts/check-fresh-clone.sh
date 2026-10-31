#!/usr/bin/env bash
# Clone the repository into a temporary directory inside the workspace and run
# the full verification suite there. Proves that a clean checkout reproduces all
# claimed evidence without local, uncommitted state.
#
# Usage: scripts/check-fresh-clone.sh
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
tmp="$(mktemp -d "$root/.fresh-clone.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

echo "== cloning $root =="
git clone --quiet --no-hardlinks "$root" "$tmp"
cd "$tmp"
echo "== commit: $(git rev-parse HEAD) =="
scripts/verify-all.sh
echo "FRESH CLONE REPRODUCTION PASSED"
