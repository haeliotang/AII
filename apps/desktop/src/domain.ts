export type NavKey = "overview" | "capabilities" | "plan" | "execution" | "federation" | "evidence";

export interface Counts {
  services: number;
  assets: number;
  resources: number;
  processes: number;
  lots: number;
}

export interface EntityView {
  id: string;
  kind: string;
  status: string;
}

export interface ServiceView {
  id: string;
  label: string;
  verdict: string;
  steps: number;
  blockers: string[];
}

export interface DependencyView {
  resource: string;
  maxSi: number;
  inRootCauses: boolean;
}

export interface ActionView {
  id: string;
  process: string;
  target: string;
  startS: number;
  endS: number;
  runs: number;
  verification: string;
  preconditions: string[];
  allocations: [string, number][];
  occupies: string[];
}

export interface PlanView {
  planId: string;
  goal: string;
  horizonS: number;
  completionS: number;
  digest: string;
  actions: ActionView[];
  goalsMet: string[];
  goalsUnmet: string[];
}

export interface DomainOverview {
  scenario: string;
  modelName: string;
  counts: Counts;
  assets: EntityView[];
  resources: EntityView[];
  processes: EntityView[];
  services: ServiceView[];
  plan: PlanView;
  externalDependencies: DependencyView[];
}

export interface ScenarioEntry {
  id: string;
  name: string;
  detail: string;
  state: "active" | "planned";
}

export interface ExperimentEntry {
  id: string;
  title: string;
  milestone: string;
  result: string;
  replay: "ci" | "manual" | "live";
}

export interface ConflictEntry {
  kind: string;
  subject: string;
  detail: string;
  sites: string[];
}

export interface SiteEntry {
  id: string;
  label: string;
  status: "online" | "partitioned";
  hlc: string;
  records: number;
  reservations: number;
  leases: number;
}

export const fallbackOverview: DomainOverview = {
  scenario: "Base-01 · pump degraded",
  modelName: "Autonomous Base-01",
  counts: { services: 1, assets: 9, resources: 13, processes: 6, lots: 7 },
  assets: [
    { id: "pump-p1", kind: "asset", status: "degraded" },
    { id: "filter-1", kind: "asset", status: "available" },
    { id: "lathe-1", kind: "asset", status: "available" },
    { id: "robot-m1", kind: "asset", status: "available" },
    { id: "bus-1", kind: "asset", status: "available" },
    { id: "charger-1", kind: "asset", status: "available" },
  ],
  resources: [
    "sleeve-p1",
    "sleeve-p1-raw",
    "bronze-bar",
    "electric-energy",
    "filter-media",
    "cutting-insert",
    "breaker-b1",
  ].map((id) => ({ id, kind: "resource", status: "declared" })),
  processes: [
    "replace-sleeve-p1",
    "turn-sleeve-p1",
    "filter-water",
    "charge-battery",
    "inspect-sleeve",
  ].map((id) => ({ id, kind: "process", status: "declared" })),
  services: [
    { id: "potable-water", label: "potable-water", verdict: "candidate", steps: 1, blockers: [] },
  ],
  plan: {
    planId: "plan-base01",
    goal: "potable-water",
    horizonS: 86400,
    completionS: 10800,
    digest: "preview-digest",
    actions: [
      {
        id: "potable-water-1-replace-sleeve-p1",
        process: "replace-sleeve-p1",
        target: "pump-p1",
        startS: 0,
        endS: 10800,
        runs: 1,
        verification: "demonstrated",
        preconditions: ["pump-p1 available", "capability repair via robot-m1 ≥ demonstrated"],
        allocations: [
          ["sleeve-p1", 1],
          ["electric-energy", 1.2],
        ],
        occupies: ["lathe-1"],
      },
    ],
    goalsMet: ["potable-water"],
    goalsUnmet: [],
  },
  externalDependencies: [
    { resource: "breaker-b1", maxSi: 2, inRootCauses: false },
    { resource: "bronze-bar", maxSi: 2, inRootCauses: false },
    { resource: "cutting-insert", maxSi: 1, inRootCauses: false },
    { resource: "electric-energy", maxSi: 1.2, inRootCauses: false },
    { resource: "grease", maxSi: 0.2, inRootCauses: false },
  ],
};

export const scenarios: ScenarioEntry[] = [
  { id: "base-01", name: "Base-01", detail: "pump degraded · structural + timed", state: "active" },
  { id: "base-02", name: "Base-02", detail: "shared depot · heat treatment", state: "planned" },
  { id: "timed", name: "Base-01 timed", detail: "20 pre-registered cases", state: "planned" },
  { id: "expansion", name: "Expansion proposal", detail: "recycling furnace · modeled only", state: "planned" },
];

export const experiments: ExperimentEntry[] = [
  { id: "H1", title: "Structural capability graph", milestone: "M3", result: "weak support", replay: "ci" },
  { id: "H1-t", title: "Temporal re-test", milestone: "M5", result: "time-blind baseline misses 3/7", replay: "ci" },
  { id: "H2", title: "Joint planning vs baselines", milestone: "M8", result: "not supported (5/8 tie)", replay: "ci" },
  { id: "H3", title: "Bounded execution (simulated)", milestone: "M10", result: "14/14 simulated", replay: "ci" },
  { id: "H4", title: "Expansion accounting (re-run)", milestone: "M11", result: "net gain negative", replay: "ci" },
  { id: "M12.6", title: "Partition / reconnect (simulated)", milestone: "M12", result: "6/6 scenarios", replay: "ci" },
  { id: "BaSyx", title: "AAS read-only adapter preview", milestone: "M9.2", result: "9/9 (AI preview)", replay: "live" },
];

export const sites: SiteEntry[] = [
  { id: "site-a", label: "site-a", status: "online", hlc: "0000012a.0003", records: 2, reservations: 2, leases: 1 },
  { id: "site-b", label: "site-b", status: "partitioned", hlc: "0000012a.0004", records: 2, reservations: 2, leases: 1 },
];

export const conflicts: ConflictEntry[] = [
  {
    kind: "over_reservation",
    subject: "lot-bronze",
    detail: "4 kg + 3 kg > 5 kg available",
    sites: ["site-a", "site-b"],
  },
  {
    kind: "double_ownership",
    subject: "robot-m1",
    detail: "overlapping leases after reconnect",
    sites: ["site-a", "site-b"],
  },
  {
    kind: "duplicate_idempotency_key",
    subject: "shared-key",
    detail: "two sites dispatched the same key",
    sites: ["site-a", "site-b"],
  },
];
