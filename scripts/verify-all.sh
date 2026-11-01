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
echo "== two-container partition preview (judgment replay) =="
mkdir -p "$tmp/fed-docker"
cargo run --locked -q -p poie-federation-docker -- report --raw experiments/federation-docker/raw --write "$tmp/fed-docker" > /dev/null
diff "$tmp/fed-docker/REPORT.md" experiments/federation-docker/REPORT.md
echo "== two-site shadow integration =="
cargo run --locked -q -p poie-two-site-shadow -- --write "$tmp/two-site"
diff "$tmp/two-site/REPORT.md" experiments/two-site-shadow/REPORT.md
diff "$tmp/two-site/raw/trace.txt" experiments/two-site-shadow/raw/trace.txt
echo "== Markdown relative links =="
python3 scripts/check-docs.py
echo "== experiment replay coverage =="
python3 scripts/check-experiment-replays.py
echo "== test matrix counts =="
python3 scripts/check-test-matrix.py
echo "== Python model and bridge tests =="
python3 -m unittest discover -s python/poie_models -p "test_*.py" -v
echo "== rustdoc (warnings as errors) =="
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --offline
echo "== shadow run demo (M9.4) =="
cargo run --locked -q -p poie-shadow-run -- --write "$tmp/shadow"
diff "$tmp/shadow/REPORT.md" experiments/shadow-run/REPORT.md
diff "$tmp/shadow/raw/trace.txt" experiments/shadow-run/raw/trace.txt
echo "== H1 structural replay =="
cargo run --locked -q -p poie-h1-baseline -- scenarios/base-01/cases.toml > "$tmp/h1-all.txt"
diff "$tmp/h1-all.txt" experiments/h1-structural/raw/final-all-2026-10-31.txt
dev=$(awk -F'\t' 'NR>1 && $1 ~ /^S[0-9]+$/ {print $1}' experiments/h1-structural/raw/dev-2026-10-31.txt | tr '\n' ' ')
cargo run --locked -q -p poie-h1-baseline -- scenarios/base-01/cases.toml $dev > "$tmp/h1-dev.txt"
diff "$tmp/h1-dev.txt" experiments/h1-structural/raw/dev-2026-10-31.txt
holdout=$(awk -F'\t' 'NR>1 && $1 ~ /^S[0-9]+$/ {print $1}' experiments/h1-structural/raw/final-holdout-2026-10-31.txt | tr '\n' ' ')
cargo run --locked -q -p poie-h1-baseline -- scenarios/base-01/cases.toml $holdout > "$tmp/h1-holdout.txt"
diff "$tmp/h1-holdout.txt" experiments/h1-structural/raw/final-holdout-2026-10-31.txt
echo "== H1 temporal replay =="
cargo run --locked -q -p poie-h1-temporal -- scenarios/base-01-timed/cases.toml > "$tmp/h1-temporal.txt"
diff "$tmp/h1-temporal.txt" experiments/h1-temporal/raw/dev-2026-11-01.txt
echo "== H2 development replay =="
cargo run --locked -q -p poie-h2-joint-planning -- --dev --write "$tmp/h2-dev"
diff "$tmp/h2-dev/dev-report.md" experiments/h2-joint-planning/dev-report.md
diff "$tmp/h2-dev/raw/dev-raw.txt" experiments/h2-joint-planning/raw/dev-raw.txt
echo "== H2 holdout replay (reproducibility only) =="
cargo run --locked -q -p poie-h2-joint-planning -- --holdout --write "$tmp/h2-holdout"
diff "$tmp/h2-holdout/holdout-report.md" experiments/h2-joint-planning/holdout-report.md
diff "$tmp/h2-holdout/raw/holdout-raw.txt" experiments/h2-joint-planning/raw/holdout-raw.txt
echo "== dependency policy =="
if [ "${SKIP_DENY:-0}" = "1" ]; then
  echo "skipped: SKIP_DENY=1 (the scheduled workflow runs cargo-deny in a separate job)"
else
  cargo deny check licenses bans sources
fi
echo "ALL GATES PASSED"
