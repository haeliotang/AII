# Security Policy

Poieweave is at the research and architecture stage. There is no stable release, no deployed service, and no hardware integration in this repository. The security policy still matters, because the project's purpose is to let software reason about — and eventually act on — physical infrastructure.

## Scope

**In scope**

- safety- and authority-relevant semantics that [ADR-0001](docs/adr/0001-open-source-boundary.zh-CN.md) requires to stay open and correct: action validation, authorization, dispatch review, verification, simulation isolation, evidence expiry, conservation checks, command ownership and leases (invariants I1–I6);
- implementation vulnerabilities in the crates in this repository, including parser and loader inputs (scenario and model TOML that an untrusted party can supply);
- dependency vulnerabilities, when the impact is specific to Poieweave (report upstream issues upstream);
- the CI configuration, especially any path by which a change could land without the quality gates.

**Out of scope**

- the physical protection, interlocks, or emergency stops of any testbed or customer site — these must remain independent of Poieweave by design;
- social engineering and other non-technical attacks;
- vulnerabilities in third-party services (GitHub, crates.io) — report them to those services.

## How to report

**Preferred: GitHub private vulnerability reporting.**

<https://github.com/haeliotang/poieweave/security/advisories/new>

This keeps the report private and creates a tracked advisory. Do **not** open a public issue, discussion, or pull request that describes the vulnerability.

If you cannot use GitHub, email **support@inceptless.com** with `Poieweave security` in the subject and the minimal details needed to establish contact. Do not include exploitation details in the first message; we will arrange a private channel.

Please include:

- the affected component (crate, contract clause, CI workflow) and commit;
- reproduction steps or a minimal input;
- expected vs. observed behaviour;
- your assessment of impact — in particular whether the issue could lead to an unintended physical action, a missed protection, a bypassed authorization check, or a false “verified” result;
- any suggested fix or mitigation.

## What to expect

- acknowledgement within **5 business days**;
- an initial assessment (valid / invalid / needs more information) within **10 business days**;
- coordinated disclosure: we will agree on a timeline with you, credit you in the advisory unless you prefer otherwise, and publish the fix and advisory together once users can act on it;
- a fix lands on `main` with a regression test; because there is no stable release, there is no backport branch to maintain yet.

These targets are best effort. If we cannot meet them, we will tell you and say why.

## Safety research rules

- Do not test against real equipment, customer sites, or any system you are not authorized to touch. This repository contains no actuators, and no report needs a physical test.
- Prefer the simulation and structural layers; never attempt to actuate anything on the basis of this project's code.
- Follow responsible-disclosure norms: no privacy violations, no service disruption, no data exfiltration. We will not pursue legal action for good-faith research that follows this policy.
