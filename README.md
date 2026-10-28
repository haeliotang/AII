# AII — Autonomous Infrastructure Intelligence

### Build the open foundation for self-sustaining industrial systems.

**From autonomous machines to autonomous industry. From remote outposts on Earth to the industrial foundations of a multiplanetary civilization.**

AII is a long-horizon research and engineering project toward an open autonomous industrial platform: a system that can represent, simulate, plan, coordinate, and verify the operation, maintenance, production, and eventual expansion of physical infrastructure.

Our central question is simple:

> Can an industrial system preserve and increase its ability to survive, repair, and produce when resources are limited, communications disappear, and equipment keeps wearing out?

**Status: research and architecture stage.** This repository currently contains the founding documentation. There is no executable platform, validated benchmark, hardware deployment, or implemented integration yet. All capabilities below are design goals.

[中文研究文档 / Research & architecture](docs/research/FOUNDATIONS.zh-CN.md)

## The ambition

A collection of capable robots does not automatically become a capable industrial system. A facility can complete every scheduled task while consuming the spare part, energy reserve, or manufacturing capacity it will need to survive tomorrow.

AII aims to connect energy, water, materials, machines, logistics, maintenance, and production into one accountable operational model. Its objective is to preserve essential services while developing the capacity to recover, replace critical equipment, and reduce external dependencies.

The horizon is a complete autonomous industrial platform. Earth-based microgrids, remote factories, polar stations, and isolated research facilities provide environments in which its foundations can be tested. Lunar and Martian industry extend that horizon; they are not current deployment claims or promised timelines.

We distinguish three frontiers:

1. **Autonomous operation:** sustain essential functions without continuous supervision.
2. **Autonomous maintenance:** diagnose degradation and restore verified capabilities.
3. **Autonomous expansion:** build additional productive capacity within physical, resource, and human-authorized constraints.

Full industrial self-sufficiency remains an open research problem. AII must expose irreducible dependencies, including electronics, precision tools, consumables, and specialist knowledge.

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

AII will investigate typed, temporal production hypergraphs to represent these joint dependencies. A path through the graph is only a candidate: scheduling, capacity, conservation, quality, and bootstrap dependencies must also be satisfied.

A machine that could theoretically manufacture its own replacement is not self-sufficient if its failure removes the only available means of manufacturing that replacement.

## A different control objective

Task throughput is one outcome. AII's proposed objective is to expand the range of conditions under which infrastructure can continue to provide essential services and recover productive capacity.

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

[Palantir Ontology](https://www.palantir.com/docs/foundry/ontology/overview) is an architectural reference for connecting operational objects, logic, actions, and permissions. AII's proposed research emphasis is the combination of physical constraints, capability dependencies, local autonomy, and long-term viability. This is not a claim that existing platforms lack related capabilities, and no affiliation is implied.

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

These are capability gates, not delivery dates. Every stage below is planned.

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

Use [issues](https://github.com/haeliotang/AII/issues) to discuss research questions and scoped implementation proposals. Proposed architecture should earn stability through experiments rather than being treated as a frozen specification.

**Licensing:** Apache-2.0 is proposed for the future core. No project license has been adopted in this initial documentation commit; public visibility alone does not grant an open-source license. Selecting and adding the license is an explicit next governance decision. Third-party components retain their own terms.

---

**Build infrastructure that can keep building the future.**
