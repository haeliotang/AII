# Contributing to Poieweave

Poieweave is a research and engineering program, not a finished product. The most useful contributions are **counterexamples, auditable cases, sharpened contracts, and small implementations that can be checked by someone else** — not volume of code.

Chinese and English are both welcome. Choose one language per contribution; do not translate the other way without being asked.

## Ground rules

1. **Evidence before claims.** "Better", "robust", "autonomous" and similar words must point at a concrete run, test, or case. See [development plan §1.4](docs/DEVELOPMENT.zh-CN.md) for the experiment discipline.
2. **Pre-register expectations.** A scenario case must have its expected outcome written *before* the first run. Do not edit an expectation to match the output; first explain what is wrong — the model, the implementation, or the expectation — and record that reasoning.
3. **Contracts are versioned, not frozen.** They move through `draft` → `candidate` → `stable` ([§1.2](docs/DEVELOPMENT.zh-CN.md)). Normative changes need an ADR ([§1.5](docs/DEVELOPMENT.zh-CN.md)).
4. **Safety and authority semantics stay open.** Anything that decides whether an action may be validated, authorized, dispatched, or verified must live in the open core ([ADR-0001](docs/adr/0001-open-source-boundary.zh-CN.md), I1–I6). Do not propose a private shortcut around them.
5. **License and sign-off.** Everything is contributed under Apache-2.0, with a Developer Certificate of Origin sign-off (below). No CLA is required.

## Toolchain and MSRV

CI uses Rust **1.94.0** (pinned in the workflow to match the reference development environment; bumping it is a deliberate PR). This avoids clippy-version drift between local checks and CI.

The MSRV is **not pinned yet**. The code uses APIs such as `Option::is_none_or` (stabilized in 1.82) and `BTreeSet::pop_first` (1.66), so toolchains older than 1.82 will not compile; no older toolchain is verified in CI. If you need a specific MSRV, open an issue — pinning it requires a dedicated CI job on that toolchain before it can be claimed.

## What to contribute

| Kind | Start here | Good first examples |
| --- | --- | --- |
| Counterexample | [Counterexample issue](https://github.com/haeliotang/poieweave/issues/new?template=counterexample.yml) | A case where the capability graph says “candidate” but the route is physically wrong |
| Scenario case | [Base-01 cases](scenarios/base-01/cases.toml) | A dependency-failure case in `scenarios/*/cases.toml` |
| Contract change | [Contract change issue](https://github.com/haeliotang/poieweave/issues/new?template=contract-change.yml) | A clause that admits two incompatible readings |
| Experiment | [Experiment proposal](https://github.com/haeliotang/poieweave/issues/new?template=experiment-proposal.yml) | A fair baseline for H1, or a disturbance set for H2 |
| Implementation | This repository, `crates/` | A missing normative behaviour plus its tests |
| Documentation | `docs/`, `specifications/` | An ambiguity, a wrong cross-reference, a missing rationale |

### Counterexamples

The project gets stronger when its model is shown to be wrong. A useful counterexample includes:

- the **expected** result and the **observed** result;
- the smallest model / inventory / goal that shows the difference (a TOML snippet or a link is fine);
- the contract clause and version it violates;
- the commit or version you observed;
- why you believe the current answer is wrong — physical reasoning, conservation, or a logical inconsistency.

If we accept it, we register it as a case in the relevant scenario (or an issue) and keep your attribution. Do not “fix” a case by weakening its expectation.

### Scenario cases

Add cases to `scenarios/<name>/cases.toml` with their expected verdict and any expected root causes. Requirements:

- the expectation is written before the first run and committed in the same change;
- the case name and comment say which contract clause it exercises;
- the case is deterministic (no time-, order-, or machine-dependent behaviour);
- existing cases and their expectations are left unchanged unless you explain the change.

### Contract changes

Open a contract-change issue first. Describe the problem (ambiguity, incorrect, missing, unnecessary), propose the normative text, and show the case or counterexample that motivates it. A normative change lands with: the contract edit, at least one test, at least one scenario case, and — for behavioural or dependency changes — an ADR.

### Code

Before opening a pull request, run the same gates CI runs:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo run --locked -q -p poie-ontology --bin poie-graph -- scenarios/base-01/cases.toml
cargo deny check licenses bans sources
```

Rules that CI cannot enforce:

- every new normative behaviour has a test whose name states the clause it covers;
- no dependency without a reason — new runtime dependencies need an ADR;
- `main` stays green; a red `main` is fixed before new work is merged.

### Documentation

Keep status statements honest: if something is a design goal, say so; if it is implemented, link the test or run. When you change a normative clause, update every document that restates it (contract, development plan, README “What exists today”).

## DCO sign-off

Every commit must be signed off. Use:

```bash
git commit -s
```

This adds a line such as:

```text
Signed-off-by: Your Name <you@example.com>
```

The sign-off certifies the [Developer Certificate of Origin 1.1](DCO). Use your real name and a reachable address. Corporate contributors must be authorized to sign on their employer's behalf. Pull requests with unsigned commits will be asked to amend them (force-pushes on your branch are fine).

## Pull requests

- branch from `main`, keep the change focused; small pull requests are reviewed faster;
- conventional prefixes help: `docs:`, `feat:`, `fix:`, `ci:`, `spec:`, `scenario:`;
- describe what changed, why, and the evidence (test output, case name, run link);
- CI runs formatting, clippy, tests, the Base-01 cases, and the dependency-license policy; the `advisories` check is advisory and may not block unrelated work;
- maintainers may ask for a pre-registered experiment, a counterexample, or an ADR instead of accepting a claim.

## Security issues

Do not open a public issue for a vulnerability. Follow [SECURITY.md](SECURITY.md).

## Conduct

Argue with evidence about the model, the code, and the experiments; keep criticism aimed at the work, not at people. State assumptions and uncertainty instead of overstating results. Contributions that misrepresent what the system does, or that hide negative results, will be rejected regardless of quality.
