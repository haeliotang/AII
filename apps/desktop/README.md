# Poieweave desktop

A Tauri 2 desktop **operations cockpit** for Poieweave: capability graph, planning,
execution gate, federation reconciliation and the evidence chain. The Rust host
owns the app-data directory, SQLite state, an embedded Base-01 domain snapshot and
a size-limited sandbox; the frontend is React 19 + TypeScript + Vite.

![Poieweave cockpit, light](preview.png)
![Poieweave cockpit, dark](preview-dark.png)

This is deliberately **not** a notes/workline layout copied from Inceptless. It
keeps the Inceptless light/dark colour tokens, but the information architecture is
Poieweave's: services → capability entities → plan → SAP execution gate →
federation → evidence.

## Layout

```
rail        64px   Poieweave mark + section icons + theme/language/settings/account
sidebar    288px   scenario, search, contextual entities / actions / conflicts / experiments
workspace  1fr     topbar (scenario, horizon, run mode) + one of six views
inspector  360px   context for the selected service, entity, action, conflict or experiment
statusbar  26px    host, model, plan digest, sandbox, time
```

Views:

| View | Content |
| --- | --- |
| Overview | KPI strip, capability graph, current plan, service viability, external dependencies, open alerts. |
| Capabilities | Assets, resources and processes from the projected Base-01 model, plus the structural graph and external dependencies. |
| Planning | Goal/completion/digest, action timeline and action table; the inspector keeps preconditions, allocations and verification level. |
| Execution | SAP execution gate (human takeover / safe degradation), action ledger and the deliberate "executor not wired" warning. |
| Federation | Site cards (HLC, records, reservations, leases) and the preserved conflict list. Current snapshot is the Base-02 federation preview. |
| Evidence | Pre-registered experiments with replay badges, contract revisions and the 384-test evidence summary. |

## Theme and i18n

- Light and dark tokens are the Inceptless values: light `#e8e8ea / #f7f7f8 /
  #efeff1 / #d4d4d8 / #1a1a1a / #666 / #0369CC`; dark `#141414 / #1c1c1c /
  #242424 / #2e2e2e / #f2f2f2 / #9a9a9a / #0369CC`.
- Theme and language persist in `localStorage`; the language switch covers the
  shell (Chinese/English), while model ids, process names and evidence labels stay
  in their source language.
- No dark-mode colour is invented outside the token block in `src/styles.css`.

## Domain integration

The host embeds the Base-01 model (`scenarios/base-01/model.toml`), patches
`pump-p1` to `degraded`, and runs the real crates:

- `poie-ontology`: graph solve per service + external dependency report;
- `poie-plan`: a 24 h plan for `potable-water` with a 4 h deadline.

`domain_overview` returns a serializable snapshot:

```text
scenario, modelName, counts,
assets / resources / processes (id, kind, status),
services (verdict, steps, blockers),
plan (planId, goal, completion, digest, actions, goals met/unmet),
externalDependencies
```

The browser-only `pnpm dev` build uses `src/domain.ts`'s preview snapshot with the
same shape; the Tauri build replaces it with the host result. There is no SQL or
solver access from the webview.

## Host commands

| Command | Purpose |
| --- | --- |
| `host_info` | Version, app-data dir, DB path, sandbox root, platform. |
| `domain_overview` | Base-01 domain snapshot computed by `poie-ontology` + `poie-plan`. |
| `notes_all` / `events_list` / `event_append` | SQLite-backed workline data kept for the next integration step. |
| `sandbox_read` / `sandbox_write` / `sandbox_root` | Relative-path, 1 MiB-limited access under `<app-data>/sandbox`; traversal and absolute paths rejected. |
| `open_workspace_window` | Opens one of the six views in a second Tauri window. |

## Authentication and remote inference

The settings sheet exposes both surfaces, but they are **not wired**:

- authentication shows "local mode" and a disabled sign-in button;
- remote inference shows disabled endpoint/model fields and states that no data is
  sent to any remote endpoint.

No fake token, fake login or fake inference call is implemented. The next
iteration should add a host-side credential store (macOS Keychain / Windows
Credential Manager via a Tauri plugin), an inference provider interface and an
audit trail before enabling either surface.

## Run and verify

```bash
cd apps/desktop
pnpm install
pnpm tauri dev            # native window
pnpm dev                  # browser-only cockpit with preview data

pnpm exec tsc --noEmit -p tsconfig.json
pnpm build
cd src-tauri && cargo test --offline    # 6 tests: DB, sandbox, domain snapshot
cd .. && pnpm tauri build --no-bundle
```

The desktop crate is excluded from the root Cargo workspace, so the existing
Rust CI and `scripts/verify-all.sh` do not require WebKit.

## Next

1. Add `poie-state` command(s) so the execution ledger is driven by a real action
   ledger and SAP review instead of plan actions.
2. Add `poie-federation` command(s) so sites/conflicts are computed by the
   federation reconciler instead of the preview snapshot.
3. Wire authentication, the credential store and remote inference with explicit
   opt-in and audit events.
4. Give secondary windows real roles (federation wall, execution monitor) rather
   than duplicating the full cockpit.
