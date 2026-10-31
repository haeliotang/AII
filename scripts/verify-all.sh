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
echo "== end-to-end pipeline demo =="
cargo run --locked -q -p poie-e2e -- --write "$tmp/e2e"
diff "$tmp/e2e/REPORT.md" experiments/e2e-pipeline/REPORT.md
diff "$tmp/e2e/raw/trace.txt" experiments/e2e-pipeline/raw/trace.txt
echo "== localhost TCP federation demo =="
cargo run --locked -q -p poie-federation-tcp -- --write "$tmp/fed-tcp"
diff "$tmp/fed-tcp/REPORT.md" experiments/federation-tcp/REPORT.md
diff "$tmp/fed-tcp/raw/exchange.txt" experiments/federation-tcp/raw/exchange.txt
echo "== two-site shadow integration =="
cargo run --locked -q -p poie-two-site-shadow -- --write "$tmp/two-site"
diff "$tmp/two-site/REPORT.md" experiments/two-site-shadow/REPORT.md
diff "$tmp/two-site/raw/trace.txt" experiments/two-site-shadow/raw/trace.txt
echo "== rustdoc (warnings as errors) =="
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --offline
echo "== dependency policy =="
cargo deny check licenses bans sources
echo "ALL GATES PASSED"
