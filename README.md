# Poieweave — Autonomous Infrastructure Intelligence

> An open autonomous industrial platform initiated by **Inceptless**.

The name comes from Greek *poiesis* (making) and *autopoiesis* (a system that produces and maintains itself). The project was named **AII** until 2026-10-31; see [ADR-0002](docs/adr/0002-rename-to-poieweave.zh-CN.md).

[![CI](https://github.com/haeliotang/poieweave/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/haeliotang/poieweave/actions/workflows/ci.yml)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

### Build the open foundation for self-sustaining industrial systems.

**From autonomous machines to autonomous industry. From remote outposts on Earth to the industrial foundations of a multiplanetary civilization.**

Poieweave is a long-horizon research and engineering project toward an open autonomous industrial platform: a system that can represent, simulate, plan, coordinate, and verify the operation, maintenance, production, and eventual expansion of physical infrastructure.

Our central question is simple:

> Can an industrial system preserve and increase its ability to survive, repair, and produce when resources are limited, communications disappear, and equipment keeps wearing out?

**Status: research and architecture stage (Foundation, in progress).** The repository contains the founding documentation, three draft contracts, a structural capability-graph prototype in Rust, and the first structural version of the Autonomous Base-01 scenario. There is no time-domain model, executable platform, validated benchmark, hardware deployment, or implemented integration yet. Unless stated otherwise below, capabilities are design goals.

[中文研究文档 / Research & architecture](docs/research/FOUNDATIONS.zh-CN.md) · [开发文档 / Development plan](docs/DEVELOPMENT.zh-CN.md)

## What exists today

| Artifact | State |
| --- | --- |
| [Physical Ontology](specifications/physical-ontology.zh-CN.md) | v0.2-draft. Units, entities, process and capability verification levels, capability-graph semantics (alternative supports, multi-source top-ups, degraded occupation, conservative yield/rework, bounded route enumeration, external-dependency tracing), mass conservation. |
| [State & Action Protocol](specifications/state-action-protocol.zh-CN.md) | v0.1-draft. Record types, action lifecycle, partition rules, 11 failure-injection cases. No implementation yet. |
| [Viability & Expansion Contract](specifications/viability-expansion.zh-CN.md) | v0.1-draft. Query fields, verdict classes, four metric groups, expansion accounting. No implementation yet. |
| [`crates/poie-ontology`](crates/poie-ontology) | Structural prototype: dimensioned quantities, model validation, mass balance, single-ledger capability-graph solver with root-cause reporting, alternative (`requires_any`) supports, multi-source top-ups, a degraded-use occupation policy, conservative yield with explicit reject/rework flows, capability-level verification on top of process-level verification, bounded enumeration of distinct candidate routes, and external-dependency tracing across all candidate routes. |
| [`crates/poie-state`](crates/poie-state) | State & Action Protocol: the nine record kinds with the common envelope and schema validation; versioned projections with state versions and evidence expiry; the exhaustive action lifecycle table with an idempotency-key dispatch guard (10⁵-sequence property test) and redispatch policy; the six pre-dispatch review checks with traceable rejections; an append-only local log with crash-tail recovery ([ADR-0003](docs/adr/0003-append-only-local-log.zh-CN.md)); restart replay/decisions; and a simulated executor with ACK-loss, duplicate-delivery, NACK and lost-dedup fault injection. SAP-T1–T11 are all automated, including a crash at every persistent write point of the dispatch flow, command ownership/leases with clock-skew detection, partition-safe reservations, and a bridge feeding projected states into capability-graph queries. |
| [Base-01 scenario](scenarios/base-01/README.zh-CN.md) | Structural v0.1 with synthetic data and 40 stratified cases whose expectations were written before the first run. All 40 match. |
| [H1 structural experiment](experiments/h1-structural/PREREG.md) | Pre-registered protocol; 40 stratified cases with a committed 25% holdout; the per-device baseline misses 2/7 blocked holdout cases while the capability-graph solver misses 0/7. H1 is weakly supported, but the conclusion is internal evidence only: the ≥1/4 external-contribution quota is not met and the ground truth was authored with the same model. Details in the [report](experiments/h1-structural/REPORT.md). |

A `candidate` route from the prototype is structural only: it does not check time, capacity, concurrency, or physical execution.

```bash
cargo test
cargo run -p poie-ontology --bin poie-graph -- scenarios/base-01/cases.toml
cargo run -p poie-ontology --bin poie-graph -- scenarios/base-01/cases.toml --routes 3
cargo run -p poie-ontology --bin poie-graph -- scenarios/base-01/cases.toml --external-deps 3
```

## The ambition

A collection of capable robots does not automatically become a capable industrial system. A facility can complete every scheduled task while consuming the spare part, energy reserve, or manufacturing capacity it will need to survive tomorrow.

Poieweave aims to connect energy, water, materials, machines, logistics, maintenance, and production into one accountable operational model. Its objective is to preserve essential services while developing the capacity to recover, replace critical equipment, and reduce external dependencies.

The horizon is a complete autonomous industrial platform. Earth-based microgrids, remote factories, polar stations, and isolated research facilities provide environments in which its foundations can be tested. Lunar and Martian industry extend that horizon; they are not current deployment claims or promised timelines.

We distinguish three frontiers:

1. **Autonomous operation:** sustain essential functions without continuous supervision.
2. **Autonomous maintenance:** diagnose degradation and restore verified capabilities.
3. **Autonomous expansion:** build additional productive capacity within physical, resource, and human-authorized constraints.

Full industrial self-sufficiency remains an open research problem. Poieweave must expose irreducible dependencies, including electronics, precision tools, consumables, and specialist knowledge.

## What we intend to build

| Foundation | Purpose |
| --- | --- |
| **Physical Operational Ontology** | Give assets, resources, processes, capabilities, evidence, actions, and constraints a shared meaning. |
| **Physical Capability Graph** | Represent the joint inputs, equipment, skills, time, and quality requirements needed to produce, repair, or replace something. |
| **Industrial World Model** | Combine conservation laws, process models, discrete events, degradation, and uncertainty into inspectable predictions. |
| **Viability Intelligence** | Estimate which essential services remain feasible, for how long, under explicitly stated disturbances and assumptions. |
| **Planning & Expansion** | Compare production, maintenance, recovery, and construction strategies across competing resource demands. |
| **Execution & Verification** | Turn authorized plans into bounded actions and distinguish requested effects from observed outcomes. |
| **Distributed Runtime** | Retain local facts, policies, models, and decision capability during disconnection, then reconcile evidence safely. |

```mermaid
flowchart TB
    M[Human goals and authority] --> P[Planning and expansion]
    O[Physical ontology and capability graph] --> W[World models and scenario simulation]
    W --> V[Viability assessment]
    V --> P
    P --> E[Authorization, execution and verification]
    E --> A[Adapters and local controllers]
    A --> F[Energy, water, manufacturing and robotics]
    F --> S[Observed state and evidence]
    S --> O
    S --> E
    R[Local runtime and durable event history] -. supports .-> S
    R -. supports .-> E
```

This is a target architecture, not an implemented system. Device controllers and independent safety interlocks retain authority over physical protection.

## The capability graph

A useful industrial model must express more than “pump P1 belongs to station S1.” It must express:

```text
qualified material + available machine + energy + tooling + validated process
    + operating capability + inspection capacity + time
        -> inspected replacement part
        -> restored equipment
        -> recovered essential service
```

Poieweave will investigate typed, temporal production hypergraphs to represent these joint dependencies. A path through the graph is only a candidate: scheduling, capacity, conservation, quality, and bootstrap dependencies must also be satisfied.

A machine that could theoretically manufacture its own replacement is not self-sufficient if its failure removes the only available means of manufacturing that replacement.

## A different control objective

Task throughput is one outcome. Poieweave's proposed objective is to expand the range of conditions under which infrastructure can continue to provide essential services and recover productive capacity.

We will report separate, physically interpretable dimensions:

- **Viability:** service feasibility over a specified horizon and disturbance set.
- **Independence:** external resource requirements and unresolved critical dependencies.
- **Resilience:** service loss, failure propagation, and recovery time.
- **Sustainability:** material losses, energy balance, equipment depletion, and replenishment capacity.

No opaque universal autonomy score. Every estimate must carry its assumptions, horizon, model versions, uncertainty, and supporting evidence. Simulation success is not proof of physical reliability.

## Rust-first, ecosystem-aware

The proposed language boundary is **Rust for core state and execution semantics; Python for scientific models and optimization; ROS 2/C++ where robotics integration requires it; TypeScript for future interfaces.**

Rust offers a strong basis for explicit types, ownership, concurrency, and long-running local services. It does not by itself guarantee real-time behavior, safe control, or correct physical models. Cross-language integration adds deployment and versioning costs; it also avoids rebuilding mature scientific ecosystems.

We are evaluating the following foundations, with no integration implied:

| Project | Candidate role |
| --- | --- |
| [NASA ISAAC](https://github.com/nasa/isaac) | Reference for autonomous facility caretaking and robotics integration; distinct from NVIDIA Isaac. |
| [Open-RMF](https://openrmf.readthedocs.io/en/latest/) | Fleet task coordination and shared-resource scheduling. |
| [Eclipse BaSyx / AAS](https://eclipse.dev/basyx/) | Standardized asset descriptions and industrial interoperability. |
| [Eclipse Ditto](https://eclipse.dev/ditto/intro-overview.html) | Device twin state and connectivity integration. |
| [IDAES](https://idaes-pse.readthedocs.io/en/stable/) | Equation-oriented process models and optimization. |

[Palantir Ontology](https://www.palantir.com/docs/foundry/ontology/overview) is an architectural reference for connecting operational objects, logic, actions, and permissions. Poieweave's proposed research emphasis is the combination of physical constraints, capability dependencies, local autonomy, and long-term viability. This is not a claim that existing platforms lack related capabilities, and no affiliation is implied.

## Engineering commitments

- **Local autonomy:** essential operation must not require a cloud connection.
- **Physics before persuasion:** models respect units, resource accounting, and explicit validity limits.
- **Evidence before claims:** observations, estimates, predictions, and intentions remain distinct.
- **Simulation before actuation:** hypothetical worlds cannot acquire real device authority.
- **Bounded execution:** commands need preconditions, expiry, authorization, deduplication, and verified outcomes.
- **Human authority:** people define mission priorities, permitted actions, and escalation boundaries.
- **Open interfaces:** models, scenarios, protocols, and decision evidence should be portable and inspectable.

Language models may assist with interpretation and candidate planning. They do not replace physical solvers, authorization checks, or independent protective controls.

## Research roadmap

These are capability gates, not delivery dates. Foundation is in progress; every other stage is planned. Milestones, exit criteria, and current status are tracked in the [development plan](docs/DEVELOPMENT.zh-CN.md).

| Stage | Deliverable | Evidence required to advance |
| --- | --- | --- |
| **Foundation** | Ontology, capability dependencies, event semantics, and a small virtual industrial base. | Unit consistency, resource balance, dependency failure cases, and reproducible replay. |
| **Autonomy** | Joint production, maintenance, and recovery planning. | Fair comparisons against rules and short-horizon optimization; explicit infeasibility and uncertainty. |
| **Embodiment** | Adapters, shadow operation, then bounded execution in a controlled physical testbed. | Observed effects, safe degradation, disconnect/reconnect behavior, and operator recovery. |
| **Expansion** | Planning and verification of additional productive capacity. | Net capability gains after construction, upkeep, inspection, and external inputs are accounted for. |
| **Federation** | Cooperation between independently operable sites. | Local authority survives network partitions; resource commitments and evidence reconcile without duplicate physical actions. |

The first proposed research environment is **Autonomous Base-01**: power, storage, water treatment, a small workshop, logistics and maintenance capability, and a spare-parts inventory. It should expose a complete conceptual loop before accumulating integrations.

The first implementation focus is the ontology and capability graph, exercised by this scenario. We will not create a large empty service hierarchy before the core semantics can be tested.

## Participate

Start with the [founding research document](docs/research/FOUNDATIONS.zh-CN.md). Useful contributions include counterexamples to the capability model, auditable physical models, reproducible failure scenarios, interface proposals, and evidence that challenges our assumptions.

Use [issues](https://github.com/haeliotang/poieweave/issues) to discuss research questions and scoped implementation proposals. Proposed architecture should earn stability through experiments rather than being treated as a frozen specification.

Read [CONTRIBUTING.md](CONTRIBUTING.md) before sending a case or a pull request: contributions need a DCO sign-off (`git commit -s`), pre-registered expectations, and must pass the same gates CI runs. Report vulnerabilities privately, as described in [SECURITY.md](SECURITY.md) — not in a public issue.

## Open Source License

Poieweave is released under the **Apache License, Version 2.0**.

The root [LICENSE](LICENSE) applies to the project code and documentation unless otherwise noted. Third-party components retain their own licenses; preserve their notices when redistributing dependencies or binaries.

The proposed open-source boundary is recorded in [ADR-0001](docs/adr/0001-open-source-boundary.zh-CN.md): specifications, the core, and all safety- and authority-relevant semantics stay open; customer-specific adapters, calibrated customer models, and hosted services may be offered separately. The license does not grant trademark rights to the project name.

## Project Origin

Poieweave is initiated by **Inceptless**.

- Initiating Company: Inceptless
- Project: Poieweave — Autonomous Infrastructure Intelligence
- Copyright: (c) 2026 Inceptless.

---

**Build infrastructure that can keep building the future.**
