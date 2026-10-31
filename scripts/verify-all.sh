#!/usr/bin/env bash
# Reproduce every gate that the repository claims in docs/status/stage-gates.zh-CN.md.
set -euo pipefail
cd "$(dirname "$0")/.."
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "== fmt =="
cargo fmt --all --check
echo "== clippy =="
cargo clippy --all-targets -- -D warnings
echo "== tests =="
cargo test --locked
echo "== Base-01 cases =="
cargo run --locked -q -p poie-ontology --bin poie-graph -- scenarios/base-01/cases.toml
echo "== Base-02 cases =="
cargo run --locked -q -p poie-ontology --bin poie-graph -- scenarios/base-02/cases.toml
echo "== timed Base-01 + replay digests =="
cargo run --locked -q -p poie-sim --bin poie-timed -- scenarios/base-01-timed/cases.toml
cargo run --locked -q -p poie-sim --bin poie-timed -- scenarios/base-01-timed/cases.toml --check-digests scenarios/base-01-timed/digests.toml
echo "== VEC report replay =="
cargo run --locked -q -p poie-vec-base01 -- scenarios/base-01-timed/cases.toml --check experiments/vec-base01/report.toml
echo "== H3 simulated replay =="
cargo run --locked -q -p poie-h3-bounded-execution -- --write "$tmp/h3"
diff "$tmp/h3/REPORT.sim.md" experiments/h3-bounded-execution/REPORT.sim.md
diff "$tmp/h3/raw/sim.txt" experiments/h3-bounded-execution/raw/sim.txt
echo "== H4 replay =="
cargo run --locked -q -p poie-h4-expansion -- --write "$tmp/h4"
diff "$tmp/h4/REPORT.md" experiments/h4-expansion/REPORT.md
diff "$tmp/h4/raw/h4.txt" experiments/h4-expansion/raw/h4.txt
echo "== federation partition replay =="
cargo run --locked -q -p poie-federation-partition -- --write "$tmp/fed"
diff "$tmp/fed/REPORT.sim.md" experiments/federation-partition/REPORT.sim.md
diff "$tmp/fed/raw/partition.txt" experiments/federation-partition/raw/partition.txt
echo "== dependency policy =="
cargo deny check licenses bans sources
echo "ALL GATES PASSED"
