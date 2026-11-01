import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { HostInfo } from "./types";
import {
  experiments,
  fallbackAuth,
  fallbackExecution,
  fallbackFederation,
  fallbackInference,
  fallbackOverview,
  scenarios,
  type AuditRow,
  type AuthStatus,
  type DomainOverview,
  type EntityView,
  type ExecutionSnapshot,
  type ExperimentEntry,
  type FederationSnapshot,
  type InferenceResult,
  type InferenceStatus,
  type NavKey,
} from "./domain";
import { detectLang, messagesFor, type Lang } from "./i18n";
import {
  AlertIcon,
  ArrowRightIcon,
  ChevronIcon,
  CloseIcon,
  CloudIcon,
  FileIcon,
  FlaskIcon,
  GlobeIcon,
  GridIcon,
  LanguagesIcon,
  LayersIcon,
  LockIcon,
  MoonIcon,
  PlayIcon,
  SearchIcon,
  SettingsIcon,
  ShieldIcon,
  SparkleIcon,
  SunIcon,
  TargetIcon,
  UserIcon,
  WindowIcon,
} from "./icons";

type Theme = "light" | "dark";
type Mode = "model" | "sim" | "shadow" | "live";
type SelectionKind = "service" | "asset" | "resource" | "process" | "action" | "conflict" | "experiment" | "site";

interface Selection {
  kind: SelectionKind;
  id: string;
}

const NAV: { key: NavKey; icon: React.ReactNode }[] = [
  { key: "overview", icon: <GridIcon size={18} /> },
  { key: "capabilities", icon: <LayersIcon size={18} /> },
  { key: "plan", icon: <TargetIcon size={18} /> },
  { key: "execution", icon: <PlayIcon size={17} /> },
  { key: "federation", icon: <GlobeIcon size={18} /> },
  { key: "evidence", icon: <FlaskIcon size={18} /> },
];

const MODES: Mode[] = ["model", "sim", "shadow", "live"];
const MODE_ENABLED: Record<Mode, boolean> = { model: true, sim: false, shadow: false, live: false };

const NAV_LABEL: Record<NavKey, string> = {
  overview: "nav.overview",
  capabilities: "nav.capabilities",
  plan: "nav.plan",
  execution: "nav.execution",
  federation: "nav.federation",
  evidence: "nav.evidence",
};

const GRAPH_NODES: { id: string; label: string; kind: "service" | "process" | "asset" | "resource"; x: number; y: number }[] = [
  { id: "potable-water", label: "potable-water", kind: "service", x: 780, y: 80 },
  { id: "filter-water", label: "filter-water", kind: "process", x: 550, y: 80 },
  { id: "replace-sleeve-p1", label: "replace-sleeve-p1", kind: "process", x: 550, y: 215 },
  { id: "robot-m1", label: "robot-m1", kind: "asset", x: 320, y: 80 },
  { id: "pump-p1", label: "pump-p1", kind: "asset", x: 320, y: 215 },
  { id: "electric-energy", label: "electric-energy", kind: "resource", x: 90, y: 80 },
  { id: "sleeve-p1", label: "sleeve-p1", kind: "resource", x: 90, y: 215 },
];
const GRAPH_EDGES: [number, number][] = [
  [0, 1],
  [0, 2],
  [1, 3],
  [1, 4],
  [2, 3],
  [2, 5],
  [2, 6],
  [3, 5],
  [4, 6],
];

function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function initialNav(): NavKey {
  if (typeof window === "undefined") return "overview";
  const view = new URLSearchParams(window.location.search).get("view");
  const allowed: NavKey[] = ["overview", "capabilities", "plan", "execution", "federation", "evidence"];
  return allowed.includes(view as NavKey) ? (view as NavKey) : "overview";
}

function initialTheme(): Theme {
  const saved = localStorage.getItem("poie-theme");
  if (saved === "light" || saved === "dark") return saved;
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function initialLang(): Lang {
  const saved = localStorage.getItem("poie-lang");
  if (saved === "zh" || saved === "en") return saved;
  return detectLang();
}

function initialFocusMode(): boolean {
  return new URLSearchParams(window.location.search).get("focus") === "1";
}

function formatHours(seconds: number): string {
  if (!Number.isFinite(seconds)) return "—";
  return `${(seconds / 3600).toFixed(seconds % 3600 === 0 ? 0 : 1)} h`;
}

function shortDigest(digest: string): string {
  return digest.length > 18 ? `${digest.slice(0, 18)}…` : digest;
}

function statusTone(status: string): string {
  if (status === "available" || status === "online" || status === "candidate") return "ok";
  if (status === "degraded" || status === "unknown" || status === "partitioned" || status === "search_limit") return "warn";
  if (status === "failed" || status === "blocked") return "bad";
  return "neutral";
}

function verdictLabel(lang: Lang, verdict: string): string {
  const key = `verdict.${verdict}`;
  const label = messagesFor(lang)[key];
  return label ?? verdict;
}

function defaultSelection(nav: NavKey, overview: DomainOverview): Selection | null {
  if (nav === "overview") {
    const service = overview.services[0];
    return service ? { kind: "service", id: service.id } : null;
  }
  if (nav === "capabilities") {
    const asset = overview.assets.find((item) => item.id === "pump-p1") ?? overview.assets[0];
    return asset ? { kind: "asset", id: asset.id } : null;
  }
  if (nav === "plan" || nav === "execution") {
    const action = overview.plan.actions[0];
    return action ? { kind: "action", id: action.id } : null;
  }
  if (nav === "federation") {
    return fallbackFederation.conflicts[0]
      ? { kind: "conflict", id: fallbackFederation.conflicts[0].kind }
      : null;
  }
  return experiments[0] ? { kind: "experiment", id: experiments[0].id } : null;
}

function Rail({
  lang,
  nav,
  theme,
  onNav,
  onTheme,
  onLang,
  onSettings,
}: {
  lang: Lang;
  nav: NavKey;
  theme: Theme;
  onNav: (nav: NavKey) => void;
  onTheme: () => void;
  onLang: () => void;
  onSettings: () => void;
}) {
  const t = (key: string) => messagesFor(lang)[key] ?? key;
  return (
    <aside className="rail">
      <div className="brand" title="Poieweave">
        <span className="brand__mark">P</span>
      </div>
      <nav className="rail__nav" aria-label={t("top.workspace")}>
        {NAV.map((item) => (
          <button
            key={item.key}
            type="button"
            className={`rail__button${nav === item.key ? " rail__button--active" : ""}`}
            onClick={() => onNav(item.key)}
            title={t(NAV_LABEL[item.key])}
            aria-label={t(NAV_LABEL[item.key])}
          >
            {item.icon}
          </button>
        ))}
      </nav>
      <div className="rail__spacer" />
      <button className="rail__button" type="button" onClick={onTheme} title={t("nav.theme")} aria-label={t("nav.theme")}>
        {theme === "light" ? <MoonIcon size={17} /> : <SunIcon size={17} />}
      </button>
      <button className="rail__button" type="button" onClick={onLang} title={t("nav.language")} aria-label={t("nav.language")}>
        <LanguagesIcon size={17} />
      </button>
      <button className="rail__button" type="button" onClick={onSettings} title={t("nav.settings")} aria-label={t("nav.settings")}>
        <SettingsIcon size={17} />
      </button>
      <button className="rail__button rail__button--account" type="button" onClick={onSettings} title={t("nav.account")} aria-label={t("nav.account")}>
        <UserIcon size={17} />
      </button>
    </aside>
  );
}

function Sidebar({
  lang,
  nav,
  overview,
  query,
  onQuery,
  selection,
  onSelect,
  host,
  federation,
}: {
  lang: Lang;
  nav: NavKey;
  overview: DomainOverview;
  query: string;
  onQuery: (value: string) => void;
  selection: Selection | null;
  onSelect: (selection: Selection) => void;
  host: HostInfo | null;
  federation: FederationSnapshot;
}) {
  const t = (key: string) => messagesFor(lang)[key] ?? key;
  const needle = query.trim().toLowerCase();
  const matches = (...values: string[]) => !needle || values.join(" ").toLowerCase().includes(needle);

  const serviceItems = overview.services.filter((service) => matches(service.id));
  const assetItems = overview.assets.filter((asset) => matches(asset.id, asset.status));
  const resourceItems = overview.resources.filter((resource) => matches(resource.id));
  const processItems = overview.processes.filter((process) => matches(process.id));
  const dependencyItems = overview.externalDependencies.filter((dependency) => matches(dependency.resource));
  const actionItems = overview.plan.actions.filter((action) => matches(action.id, action.process, action.target));
  const conflictItems = federation.conflicts.filter((conflict) => matches(conflict.kind, conflict.subject));
  const experimentItems = experiments.filter((entry) => matches(entry.id, entry.title, entry.result));

  const row = (selected: boolean, key: string, onClick: () => void, content: React.ReactNode) => (
    <button key={key} type="button" className={`side-row${selected ? " side-row--active" : ""}`} onClick={onClick}>
      {content}
    </button>
  );

  return (
    <aside className="sidebar">
      <div className="sidebar__scenario">
        <span className="eyebrow">{t("top.scenario")}</span>
        <select className="scenario-select" value="base-01" aria-label={t("top.scenario")}>
          {scenarios.map((scenario) => (
            <option key={scenario.id} value={scenario.id} disabled={scenario.state !== "active"}>
              {scenario.name}{scenario.state === "planned" ? " · planned" : ""}
            </option>
          ))}
        </select>
        <span className="scenario-detail">{scenarios[0]?.detail}</span>
      </div>
      <label className="search-field">
        <SearchIcon size={14} />
        <input id="cockpit-search" value={query} onChange={(event) => onQuery(event.target.value)} placeholder={t("top.search")} aria-label={t("top.search")} />
      </label>

      <div className="sidebar__scroll">
        {nav === "overview" ? (
          <>
            <div className="side-section">
              <h3>{t("sidebar.overview")}</h3>
              {serviceItems.map((service) =>
                row(
                  selection?.kind === "service" && selection.id === service.id,
                  service.id,
                  () => onSelect({ kind: "service", id: service.id }),
                  <>
                    <span className={`dot dot--${statusTone(service.verdict)}`} />
                    <span className="side-row__label">{service.id}</span>
                    <span className="side-row__meta">{verdictLabel(lang, service.verdict)}</span>
                  </>,
                ),
              )}
            </div>
            <div className="side-section">
              <h3>{t("sidebar.external")} <span className="count">{dependencyItems.length}</span></h3>
              {dependencyItems.map((dependency) =>
                row(
                  selection?.kind === "resource" && selection.id === dependency.resource,
                  dependency.resource,
                  () => onSelect({ kind: "resource", id: dependency.resource }),
                  <>
                    <span className={`dot dot--${dependency.inRootCauses ? "bad" : "neutral"}`} />
                    <span className="side-row__label">{dependency.resource}</span>
                    <span className="side-row__meta">{dependency.maxSi} SI</span>
                  </>,
                ),
              )}
            </div>
          </>
        ) : null}

        {nav === "capabilities" ? (
          <>
            <EntityGroup lang={lang} title={t("sidebar.assets")} items={assetItems} kind="asset" selection={selection} onSelect={onSelect} />
            <EntityGroup lang={lang} title={t("sidebar.resources")} items={resourceItems} kind="resource" selection={selection} onSelect={onSelect} />
            <EntityGroup lang={lang} title={t("sidebar.processes")} items={processItems} kind="process" selection={selection} onSelect={onSelect} />
          </>
        ) : null}

        {nav === "plan" || nav === "execution" ? (
          <div className="side-section">
            <h3>{nav === "plan" ? t("sidebar.plan") : t("sidebar.execution")}</h3>
            {actionItems.length === 0 ? <p className="side-empty">{t("plan.empty")}</p> : null}
            {actionItems.map((action) =>
              row(
                selection?.kind === "action" && selection.id === action.id,
                action.id,
                () => onSelect({ kind: "action", id: action.id }),
                <>
                  <span className="side-row__label">{action.process}</span>
                  <span className="side-row__meta">{action.target}</span>
                </>,
              ),
            )}
          </div>
        ) : null}

        {nav === "federation" ? (
          <>
            <div className="side-section">
              <h3>{t("federation.sites")}</h3>
              {federation.sites.map((site) =>
                row(
                  selection?.kind === "site" && selection.id === site.id,
                  site.id,
                  () => onSelect({ kind: "site", id: site.id }),
                  <>
                    <span className={`dot dot--${site.status === "online" ? "ok" : "warn"}`} />
                    <span className="side-row__label">{site.id}</span>
                    <span className="side-row__meta">{t(`federation.${site.status}`)}</span>
                  </>,
                ),
              )}
            </div>
            <div className="side-section">
              <h3>{t("sidebar.conflicts")} <span className="count">{conflictItems.length}</span></h3>
              {conflictItems.map((conflict) =>
                row(
                  selection?.kind === "conflict" && selection.id === conflict.kind,
                  conflict.kind,
                  () => onSelect({ kind: "conflict", id: conflict.kind }),
                  <>
                    <span className="dot dot--bad" />
                    <span className="side-row__label">{conflict.kind}</span>
                  </>,
                ),
              )}
            </div>
          </>
        ) : null}

        {nav === "evidence" ? (
          <div className="side-section">
            <h3>{t("evidence.experiments")}</h3>
            {experimentItems.map((entry) =>
              row(
                selection?.kind === "experiment" && selection.id === entry.id,
                entry.id,
                () => onSelect({ kind: "experiment", id: entry.id }),
                <>
                  <span className={`dot dot--${entry.replay === "live" ? "warn" : "ok"}`} />
                  <span className="side-row__label">{entry.id}</span>
                  <span className="side-row__meta">{entry.milestone}</span>
                </>,
              ),
            )}
          </div>
        ) : null}
      </div>

      <div className="sidebar__foot">
        <span className={`tail-dot${host ? " tail-dot--ok" : ""}`} />
        <span>{host ? t("top.connected") : t("top.preview")}</span>
      </div>
    </aside>
  );
}

function EntityGroup({
  lang,
  title,
  items,
  kind,
  selection,
  onSelect,
}: {
  lang: Lang;
  title: string;
  items: EntityView[];
  kind: "asset" | "resource" | "process";
  selection: Selection | null;
  onSelect: (selection: Selection) => void;
}) {
  const t = (key: string) => messagesFor(lang)[key] ?? key;
  return (
    <div className="side-section">
      <h3>{title} <span className="count">{items.length}</span></h3>
      {items.map((item) => (
        <button
          key={item.id}
          type="button"
          className={`side-row${selection?.kind === kind && selection.id === item.id ? " side-row--active" : ""}`}
          onClick={() => onSelect({ kind, id: item.id })}
        >
          <span className={`dot dot--${statusTone(item.status)}`} />
          <span className="side-row__label">{item.id}</span>
          <span className="side-row__meta">{item.status === "declared" ? t(`common.${kind}`) : item.status}</span>
        </button>
      ))}
    </div>
  );
}

function Topbar({
  lang,
  nav,
  mode,
  onMode,
  onSettings,
  onWindow,
  theme,
  onTheme,
  onLang,
  onSearch,
}: {
  lang: Lang;
  nav: NavKey;
  mode: Mode;
  onMode: (mode: Mode) => void;
  onSettings: () => void;
  onWindow: () => void;
  theme: Theme;
  onTheme: () => void;
  onLang: () => void;
  onSearch: () => void;
}) {
  const t = (key: string) => messagesFor(lang)[key] ?? key;
  return (
    <header className="topbar">
      <div className="topbar__crumbs">
        <span className="app-name">Poieweave</span>
        <span className="scenario-badge">Base-01</span>
        <span className="crumb-sep">/</span>
        <strong>{t(NAV_LABEL[nav])}</strong>
        <span className="horizon-chip">{t("top.horizon")} 4 h</span>
      </div>
      <div className="mode-switch" role="group" aria-label={t("top.mode")}>
        {MODES.map((item) => (
          <button
            key={item}
            type="button"
            className={`mode${mode === item ? " mode--active" : ""}`}
            disabled={!MODE_ENABLED[item]}
            title={MODE_ENABLED[item] ? t(`top.mode.${item}`) : `${t(`top.mode.${item}`)} · later`}
            onClick={() => onMode(item)}
          >
            {t(`top.mode.${item}`)}
          </button>
        ))}
      </div>
      <div className="topbar__actions">
        <button className="icon-button" type="button" onClick={onSearch} title={t("top.search")} aria-label={t("top.search")}>
          <SearchIcon size={16} />
        </button>
        <button className="icon-button" type="button" onClick={onWindow} title={t("nav.window")} aria-label={t("nav.window")}>
          <WindowIcon size={16} />
        </button>
        <button className="icon-button" type="button" onClick={onTheme} title={t("nav.theme")} aria-label={t("nav.theme")}>
          {theme === "light" ? <MoonIcon size={16} /> : <SunIcon size={16} />}
        </button>
        <button className="icon-button" type="button" onClick={onLang} title={t("nav.language")} aria-label={t("nav.language")}>
          <LanguagesIcon size={16} />
        </button>
        <button className="icon-button" type="button" onClick={onSettings} title={t("nav.settings")} aria-label={t("nav.settings")}>
          <SettingsIcon size={16} />
        </button>
      </div>
    </header>
  );
}

function Kpi({ label, value, hint, tone }: { label: string; value: string; hint: string; tone?: string }) {
  return (
    <div className="kpi">
      <span className="kpi__label">{label}</span>
      <strong className={`kpi__value${tone ? ` kpi__value--${tone}` : ""}`}>{value}</strong>
      <span className="kpi__hint">{hint}</span>
    </div>
  );
}

function CapabilityGraph({ lang, selection, onSelect }: { lang: Lang; selection: Selection | null; onSelect: (selection: Selection) => void }) {
  const t = (key: string) => messagesFor(lang)[key] ?? key;
  return (
    <section className="card graph-card">
      <div className="card__head">
        <div>
          <h2>{t("overview.graph")}</h2>
          <p>{t("overview.graphNote")}</p>
        </div>
        <SparkleIcon size={15} />
      </div>
      <svg className="graph" viewBox="0 0 900 300" role="img" aria-label={t("overview.graph")}>
        {GRAPH_EDGES.map(([from, to]) => {
          const a = GRAPH_NODES[from];
          const b = GRAPH_NODES[to];
          return <line key={`${from}-${to}`} x1={a.x} y1={a.y} x2={b.x} y2={b.y} className="graph__edge" />;
        })}
        {GRAPH_NODES.map((node) => {
          const width = Math.max(120, node.label.length * 8 + 30);
          const active = selection?.id === node.id;
          return (
            <g
              key={node.id}
              className={`graph__node graph__node--${node.kind}${active ? " graph__node--active" : ""}`}
              onClick={() => onSelect({ kind: node.kind, id: node.id })}
            >
              <rect x={node.x - width / 2} y={node.y - 17} width={width} height={34} rx={8} />
              <circle cx={node.x - width / 2 + 14} cy={node.y} r={3.5} />
              <text x={node.x - width / 2 + 25} y={node.y + 4}>{node.label}</text>
            </g>
          );
        })}
      </svg>
      <div className="graph__legend">
        <span><i className="legend-dot legend-dot--service" />{t("common.service")}</span>
        <span><i className="legend-dot legend-dot--process" />{t("common.process")}</span>
        <span><i className="legend-dot legend-dot--asset" />{t("common.asset")}</span>
        <span><i className="legend-dot legend-dot--resource" />{t("common.resource")}</span>
      </div>
    </section>
  );
}

function Overview({ lang, overview, selection, onSelect }: { lang: Lang; overview: DomainOverview; selection: Selection | null; onSelect: (selection: Selection) => void }) {
  const t = (key: string) => messagesFor(lang)[key] ?? key;
  const available = overview.services.filter((service) => service.verdict === "candidate").length;
  const alerts = overview.services.filter((service) => service.verdict === "blocked").length + overview.externalDependencies.filter((dependency) => dependency.inRootCauses).length;
  return (
    <div className="view">
      <header className="view__head">
        <h1>{t("overview.title")}</h1>
        <p>{t("overview.subtitle")}</p>
      </header>
      <div className="kpis">
        <Kpi label={t("kpi.services")} value={`${available}/${overview.services.length}`} hint={overview.scenario} tone="ok" />
        <Kpi label={t("kpi.actions")} value={String(overview.plan.actions.length)} hint={overview.plan.goal} />
        <Kpi label={t("kpi.dependencies")} value={String(overview.externalDependencies.length)} hint={t("overview.external")} />
        <Kpi label={t("kpi.alerts")} value={String(alerts)} hint={t("overview.alerts")} tone={alerts > 0 ? "bad" : "ok"} />
      </div>
      <CapabilityGraph lang={lang} selection={selection} onSelect={onSelect} />
      <div className="grid-2">
        <section className="card">
          <div className="card__head">
            <div>
              <h2>{t("overview.plan")}</h2>
              <p>{overview.plan.planId} · {t("plan.completion")} {formatHours(overview.plan.completionS)}</p>
            </div>
            <span className="digest">{shortDigest(overview.plan.digest)}</span>
          </div>
          {overview.plan.actions.length === 0 ? <p className="empty">{t("overview.emptyPlan")}</p> : null}
          <div className="plan-list">
            {overview.plan.actions.map((action) => (
              <button
                key={action.id}
                type="button"
                className={`plan-row${selection?.kind === "action" && selection.id === action.id ? " plan-row--active" : ""}`}
                onClick={() => onSelect({ kind: "action", id: action.id })}
              >
                <span className="plan-row__time">{formatHours(action.startS)}</span>
                <span className="plan-row__body">
                  <strong>{action.process}</strong>
                  <span>{action.target} · {action.verification}</span>
                </span>
                <ChevronIcon size={13} />
              </button>
            ))}
          </div>
        </section>
      </div>
      <div className="grid-2">
        <section className="card">
          <div className="card__head"><h2>{t("overview.services")}</h2></div>
          <div className="service-list">
            {overview.services.map((service) => (
              <button
                key={service.id}
                type="button"
                className={`service-row${selection?.kind === "service" && selection.id === service.id ? " service-row--active" : ""}`}
                onClick={() => onSelect({ kind: "service", id: service.id })}
              >
                <span className={`dot dot--${statusTone(service.verdict)}`} />
                <span className="service-row__label">{service.id}</span>
                <span className={`pill pill--${statusTone(service.verdict)}`}>{verdictLabel(lang, service.verdict)}</span>
                <span className="service-row__steps">{service.steps} {t("inspector.steps")}</span>
              </button>
            ))}
          </div>
        </section>
        <section className="card">
          <div className="card__head"><h2>{t("overview.alerts")}</h2><AlertIcon size={15} /></div>
          {alerts === 0 ? <p className="empty">{t("overview.noAlerts")}</p> : null}
          <div className="alert-list">
            {overview.services.filter((service) => service.verdict !== "candidate").map((service) => (
              <div key={service.id} className="alert-row">
                <span className="dot dot--bad" />
                <div>
                  <strong>{service.id}</strong>
                  <span>{service.blockers.join(" · ") || verdictLabel(lang, service.verdict)}</span>
                </div>
              </div>
            ))}
            {overview.externalDependencies.filter((dependency) => dependency.inRootCauses).map((dependency) => (
              <div key={dependency.resource} className="alert-row">
                <span className="dot dot--warn" />
                <div><strong>{dependency.resource}</strong><span>external dependency in root causes</span></div>
              </div>
            ))}
          </div>
        </section>
      </div>
      <section className="card">
        <div className="card__head"><h2>{t("overview.external")}</h2><span className="digest">{overview.externalDependencies.length}</span></div>
        <div className="dep-grid">
          {overview.externalDependencies.map((dependency) => (
            <button key={dependency.resource} type="button" className="dep-chip" onClick={() => onSelect({ kind: "resource", id: dependency.resource })}>
              <span className={`dot dot--${dependency.inRootCauses ? "bad" : "neutral"}`} />
              <span>{dependency.resource}</span>
              <span className="muted">{dependency.maxSi} SI</span>
            </button>
          ))}
        </div>
      </section>
    </div>
  );
}

function Capabilities({ lang, overview, selection, onSelect }: { lang: Lang; overview: DomainOverview; selection: Selection | null; onSelect: (selection: Selection) => void }) {
  const t = (key: string) => messagesFor(lang)[key] ?? key;
  const lists: { key: "asset" | "resource" | "process"; title: string; items: EntityView[] }[] = [
    { key: "asset", title: t("capabilities.assets"), items: overview.assets },
    { key: "resource", title: t("capabilities.resources"), items: overview.resources },
    { key: "process", title: t("capabilities.processes"), items: overview.processes },
  ];
  return (
    <div className="view">
      <header className="view__head">
        <h1>{t("capabilities.title")}</h1>
        <p>{t("capabilities.subtitle")}</p>
      </header>
      <div className="grid-3">
        {lists.map((list) => (
          <section className="card" key={list.key}>
            <div className="card__head"><h2>{list.title}</h2><span className="digest">{list.items.length}</span></div>
            <div className="chips">
              {list.items.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  className={`chip chip--${list.key}${selection?.kind === list.key && selection.id === item.id ? " chip--active" : ""}`}
                  onClick={() => onSelect({ kind: list.key, id: item.id })}
                >
                  <span className={`dot dot--${statusTone(item.status)}`} />
                  {item.id}
                </button>
              ))}
            </div>
          </section>
        ))}
      </div>
      <CapabilityGraph lang={lang} selection={selection} onSelect={onSelect} />
      <section className="card">
        <div className="card__head"><h2>{t("overview.external")}</h2><span className="digest">{overview.externalDependencies.length}</span></div>
        <div className="dep-grid">
          {overview.externalDependencies.map((dependency) => (
            <div key={dependency.resource} className="dep-chip">
              <span className={`dot dot--${dependency.inRootCauses ? "bad" : "neutral"}`} />
              <span>{dependency.resource}</span>
              <span className="muted">{dependency.maxSi} SI</span>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}

function Plan({ lang, overview, selection, onSelect }: { lang: Lang; overview: DomainOverview; selection: Selection | null; onSelect: (selection: Selection) => void }) {
  const t = (key: string) => messagesFor(lang)[key] ?? key;
  const horizon = Math.max(1, overview.plan.horizonS);
  const selectedAction = overview.plan.actions.find((action) => action.id === selection?.id);
  return (
    <div className="view">
      <header className="view__head">
        <h1>{t("plan.title")}</h1>
        <p>{t("plan.subtitle")}</p>
      </header>
      <div className="grid-3">
        <div className="card meta-card"><span>{t("plan.goal")}</span><strong>{overview.plan.goal}</strong></div>
        <div className="card meta-card"><span>{t("plan.completion")}</span><strong>{formatHours(overview.plan.completionS)}</strong></div>
        <div className="card meta-card"><span>{t("plan.digest")}</span><strong className="mono">{shortDigest(overview.plan.digest)}</strong></div>
      </div>
      <section className="card">
        <div className="card__head"><h2>{t("plan.timeline")}</h2><span className="digest">{formatHours(horizon)}</span></div>
        <div className="gantt">
          <div className="gantt__axis">
            {[0, 0.25, 0.5, 0.75, 1].map((fraction) => <span key={fraction}>{formatHours(horizon * fraction)}</span>)}
          </div>
          <div className="gantt__track">
            {overview.plan.actions.map((action) => {
              const left = Math.min(100, Math.max(0, (action.startS / horizon) * 100));
              const width = Math.min(100 - left, Math.max(2, ((action.endS - action.startS) / horizon) * 100));
              return (
                <button
                  key={action.id}
                  type="button"
                  className={`gantt__bar${selectedAction?.id === action.id ? " gantt__bar--active" : ""}`}
                  style={{ left: `${left}%`, width: `${width}%` }}
                  onClick={() => onSelect({ kind: "action", id: action.id })}
                  title={action.process}
                >
                  {action.process}
                </button>
              );
            })}
            {overview.plan.actions.length === 0 ? <span className="gantt__empty">{t("plan.empty")}</span> : null}
          </div>
        </div>
      </section>
      <section className="card">
        <div className="card__head"><h2>{t("plan.actions")}</h2></div>
        <div className="action-table">
          <div className="action-table__head">
            <span>{t("common.action")}</span><span>{t("common.process")}</span><span>{t("common.asset")}</span><span>{t("plan.window")}</span><span>{t("plan.verification")}</span>
          </div>
          {overview.plan.actions.map((action) => (
            <button
              key={action.id}
              type="button"
              className={`action-table__row${selectedAction?.id === action.id ? " action-table__row--active" : ""}`}
              onClick={() => onSelect({ kind: "action", id: action.id })}
            >
              <span className="mono">{action.id}</span>
              <span>{action.process}</span>
              <span>{action.target}</span>
              <span>{formatHours(action.startS)} – {formatHours(action.endS)}</span>
              <span>{action.verification}</span>
            </button>
          ))}
        </div>
      </section>
    </div>
  );
}

function Execution({ lang, execution, selection, onSelect, gate, onGate }: { lang: Lang; execution: ExecutionSnapshot; selection: Selection | null; onSelect: (selection: Selection) => void; gate: { takeover: boolean; degraded: boolean }; onGate: (gate: { takeover: boolean; degraded: boolean }) => void }) {
  const t = (key: string) => messagesFor(lang)[key] ?? key;
  const stateFor = (): string => {
    if (gate.takeover) return t("execution.recorded");
    if (gate.degraded) return t("execution.fallback");
    return t("execution.authorized");
  };
  return (
    <div className="view">
      <header className="view__head">
        <h1>{t("execution.title")}</h1>
        <p>{t("execution.subtitle")}</p>
      </header>
      <section className="card gate-card">
        <div className="card__head">
          <div>
            <h2>{t("execution.gate")}</h2>
            <p>{gate.takeover ? t("execution.takeoverOn") : gate.degraded ? t("execution.degradedOn") : t("execution.gateOpen")}</p>
          </div>
          <ShieldIcon size={17} />
        </div>
        <div className="gate-row">
          <button type="button" className={`toggle${gate.takeover ? " toggle--on" : ""}`} onClick={() => onGate({ ...gate, takeover: !gate.takeover })}>
            <span className="toggle__knob" />
            {t("execution.takeover")}
          </button>
          <button type="button" className={`toggle${gate.degraded ? " toggle--on" : ""}`} onClick={() => onGate({ ...gate, degraded: !gate.degraded })}>
            <span className="toggle__knob" />
            {t("execution.degraded")}
          </button>
          <span className="gate-note">{t("execution.notConnected")}</span>
        </div>
      </section>
      <section className="card">
        <div className="card__head">
          <div><h2>{t("execution.checks")}</h2><p>{t("execution.ledgerFromSap")}</p></div>
          <span className={`pill pill--${execution.reviewStatus === "ready" ? "ok" : "bad"}`}>{execution.reviewStatus === "ready" ? t("execution.reviewReady") : t("execution.reviewBlocked")}</span>
        </div>
        <div className="check-grid">
          {execution.checks.map((check) => (
            <div key={check.check} className={`check-item${check.passed ? "" : " check-item--bad"}`}>
              <span className={`dot dot--${check.passed ? "ok" : "bad"}`} />
              <span>{t(`check.${check.check}`)}</span>
              <span className="muted">{check.detail}</span>
            </div>
          ))}
        </div>
      </section>
      <section className="card">
        <div className="card__head"><h2>{t("execution.ledger")}</h2><span className="digest">{execution.actions.length}</span></div>
        <div className="action-table action-table--exec">
          <div className="action-table__head">
            <span>{t("common.action")}</span><span>{t("common.process")}</span><span>{t("execution.state")}</span><span>{t("execution.verification")}</span><span>{t("execution.dispatch")}</span>
          </div>
          {execution.actions.map((action) => (
            <button
              key={action.id}
              type="button"
              className={`action-table__row${selection?.id === action.id ? " action-table__row--active" : ""}`}
              onClick={() => onSelect({ kind: "action", id: action.id })}
            >
              <span className="mono">{action.id}</span>
              <span>{action.process}</span>
              <span><span className={`pill pill--${gate.takeover || gate.degraded ? "warn" : "ok"}`}>{stateFor()}</span></span>
              <span className="muted">{action.verification}</span>
              <span className="muted">—</span>
            </button>
          ))}
        </div>
      </section>
    </div>
  );
}

function Federation({ lang, federation, selection, onSelect }: { lang: Lang; federation: FederationSnapshot; selection: Selection | null; onSelect: (selection: Selection) => void }) {
  const t = (key: string) => messagesFor(lang)[key] ?? key;
  return (
    <div className="view">
      <header className="view__head">
        <h1>{t("federation.title")}</h1>
        <p>{t("federation.subtitle")}</p>
      </header>
      <div className="grid-2">
        {federation.sites.map((site) => (
          <button
            key={site.id}
            type="button"
            className={`card site-card${selection?.kind === "site" && selection.id === site.id ? " site-card--active" : ""}`}
            onClick={() => onSelect({ kind: "site", id: site.id })}
          >
            <div className="card__head">
              <h2>{site.id}</h2>
              <span className={`pill pill--${site.status === "online" ? "ok" : "warn"}`}>{t(`federation.${site.status}`)}</span>
            </div>
            <div className="site-stats">
              <div><span>{t("federation.hlc")}</span><strong className="mono">{site.hlc}</strong></div>
              <div><span>{t("federation.records")}</span><strong>{site.records}</strong></div>
              <div><span>{t("federation.reservations")}</span><strong>{site.reservations}</strong></div>
              <div><span>{t("federation.leases")}</span><strong>{site.leases}</strong></div>
            </div>
          </button>
        ))}
      </div>
      <section className="card">
        <div className="card__head"><h2>{t("federation.conflicts")}</h2><span className="digest">{federation.conflicts.length}</span></div>
        <p className="card-note">{t("federation.conflictNote")} · {t("federation.merged")} {federation.mergedRecords} · {t("federation.exchange")} {federation.transferredAb + federation.transferredBa}</p>
        <div className="conflict-list">
          {federation.conflicts.map((conflict) => (
            <button
              key={conflict.kind}
              type="button"
              className={`conflict-row${selection?.kind === "conflict" && selection.id === conflict.kind ? " conflict-row--active" : ""}`}
              onClick={() => onSelect({ kind: "conflict", id: conflict.kind })}
            >
              <span className="pill pill--warn">{conflict.kind}</span>
              <span className="conflict-row__subject mono">{conflict.subject}</span>
              <span className="conflict-row__detail">{conflict.detail}</span>
              <span className="conflict-row__sites">{conflict.sites.join(" ↔ ")}</span>
            </button>
          ))}
        </div>
      </section>
    </div>
  );
}

function Evidence({ lang, selection, onSelect }: { lang: Lang; selection: Selection | null; onSelect: (selection: Selection) => void }) {
  const t = (key: string) => messagesFor(lang)[key] ?? key;
  return (
    <div className="view">
      <header className="view__head">
        <h1>{t("evidence.title")}</h1>
        <p>{t("evidence.subtitle")}</p>
      </header>
      <div className="grid-2">
        <section className="card">
          <div className="card__head"><h2>{t("evidence.experiments")}</h2><FlaskIcon size={15} /></div>
          <div className="experiment-list">
            {experiments.map((entry: ExperimentEntry) => (
              <button
                key={entry.id}
                type="button"
                className={`experiment-row${selection?.kind === "experiment" && selection.id === entry.id ? " experiment-row--active" : ""}`}
                onClick={() => onSelect({ kind: "experiment", id: entry.id })}
              >
                <span className="mono">{entry.id}</span>
                <span className="experiment-row__title">{entry.title}</span>
                <span className="experiment-row__result">{entry.result}</span>
                <span className={`pill pill--${entry.replay === "live" ? "warn" : "ok"}`}>{t(`evidence.${entry.replay}`)}</span>
              </button>
            ))}
          </div>
        </section>
        <section className="card">
          <div className="card__head"><h2>{t("evidence.contracts")}</h2><FileIcon size={15} /></div>
          <div className="contract-list">
            <div className="contract-row"><strong>Physical Ontology</strong><span>v0.3-draft · 2026-11-01 revision (#105/#106/#111)</span></div>
            <div className="contract-row"><strong>State &amp; Action Protocol</strong><span>v0.2-draft · 15 acceptance cases (#107/#108/#111)</span></div>
            <div className="contract-row"><strong>Viability &amp; Expansion</strong><span>v0.2-draft · revised accounting/statistics (#109/#110)</span></div>
          </div>
          <p className="card-note">384 listed Rust tests · CI replay chain · AI-preview findings triaged (22 accepted / 3 deferred).</p>
        </section>
      </div>
    </div>
  );
}

function Inspector({
  lang,
  nav,
  overview,
  selection,
  onNavigate,
  gate,
  execution,
  federation,
}: {
  lang: Lang;
  nav: NavKey;
  overview: DomainOverview;
  selection: Selection | null;
  onNavigate: (nav: NavKey, selection: Selection) => void;
  gate: { takeover: boolean; degraded: boolean };
  execution: ExecutionSnapshot;
  federation: FederationSnapshot;
}) {
  const t = (key: string) => messagesFor(lang)[key] ?? key;
  let title = t("inspector.noSelection");
  let kind = "";
  let body: React.ReactNode = <p className="empty">{t("inspector.noSelection")}</p>;

  if (selection?.kind === "service") {
    const service = overview.services.find((entry) => entry.id === selection.id);
    if (service) {
      title = service.id;
      kind = t("common.service");
      body = (
        <>
          <Row label={t("inspector.status")} value={<span className={`pill pill--${statusTone(service.verdict)}`}>{verdictLabel(lang, service.verdict)}</span>} />
          <Row label={t("inspector.steps")} value={String(service.steps)} />
          {service.blockers.length > 0 ? <Row label={t("inspector.blockers")} value={service.blockers.join(" · ")} /> : null}
          <button className="primary" type="button" onClick={() => onNavigate("plan", { kind: "action", id: overview.plan.actions[0]?.id ?? "" })}>
            {t("inspector.openPlan")} <ArrowRightIcon size={14} />
          </button>
        </>
      );
    }
  } else if (selection?.kind === "asset" || selection?.kind === "resource" || selection?.kind === "process") {
    const entity =
      selection.kind === "asset"
        ? overview.assets.find((entry) => entry.id === selection.id)
        : selection.kind === "resource"
          ? overview.resources.find((entry) => entry.id === selection.id)
          : overview.processes.find((entry) => entry.id === selection.id);
    if (entity) {
      title = entity.id;
      kind = t(`common.${selection.kind}`);
      body = (
        <>
          <Row label={t("inspector.status")} value={<span className={`pill pill--${statusTone(entity.status)}`}>{entity.status}</span>} />
          <Row label={t("inspector.kind")} value={selection.kind} />
          {selection.id === "pump-p1" ? (
            <>
              <Row label={t("inspector.blockers")} value="pump degraded · repair route uses sleeve-p1" />
              <Row label={t("inspector.dependencies")} value="sleeve-p1 · electric-energy · robot-m1" />
            </>
          ) : null}
          <button className="primary" type="button" onClick={() => onNavigate("plan", { kind: "action", id: overview.plan.actions[0]?.id ?? "" })}>
            {t("inspector.openPlan")} <ArrowRightIcon size={14} />
          </button>
        </>
      );
    }
  } else if (selection?.kind === "action") {
    const action = overview.plan.actions.find((entry) => entry.id === selection.id);
    if (action) {
      title = action.process;
      kind = t("common.action");
      body = (
        <>
          <Row label="id" value={<span className="mono">{action.id}</span>} />
          <Row label={t("common.asset")} value={action.target} />
          <Row label={t("inspector.window")} value={`${formatHours(action.startS)} – ${formatHours(action.endS)}`} />
          <Row label={t("plan.verification")} value={action.verification} />
          <Row label={t("inspector.runs")} value={String(action.runs)} />
          <Row label={t("plan.preconditions")} value={action.preconditions.join(" · ")} />
          <Row label={t("plan.allocations")} value={action.allocations.map(([resource, amount]) => `${resource} ${amount}`).join(" · ")} />
          <Row label={t("execution.state")} value={gate.takeover ? t("execution.recorded") : gate.degraded ? t("execution.fallback") : (execution.actions.find((entry) => entry.id === action.id)?.state ?? t("execution.authorized"))} />
          <button className="primary" type="button" onClick={() => onNavigate("execution", { kind: "action", id: action.id })}>
            {t("inspector.openExecution")} <ArrowRightIcon size={14} />
          </button>
        </>
      );
    }
  } else if (selection?.kind === "conflict") {
    const conflict = federation.conflicts.find((entry) => entry.kind === selection.id);
    if (conflict) {
      title = conflict.kind;
      kind = t("federation.conflict");
      body = (
        <>
          <Row label={t("inspector.status")} value={<span className="pill pill--warn">{t("federation.reported")}</span>} />
          <Row label={t("inspector.subject")} value={<span className="mono">{conflict.subject}</span>} />
          <Row label={t("inspector.sites")} value={conflict.sites.join(" ↔ ")} />
          <Row label={t("inspector.details")} value={conflict.detail} />
          <p className="empty">{t("federation.conflictNote")}</p>
        </>
      );
    }
  } else if (selection?.kind === "experiment") {
    const entry = experiments.find((experiment) => experiment.id === selection.id);
    if (entry) {
      title = entry.id;
      kind = t("evidence.experiments");
      body = (
        <>
          <Row label={t("inspector.milestone")} value={entry.milestone} />
          <Row label={t("inspector.status")} value={entry.result} />
          <Row label={t("evidence.replay")} value={t(`evidence.${entry.replay}`)} />
        </>
      );
    }
  } else if (selection?.kind === "site") {
    const site = federation.sites.find((entry) => entry.id === selection.id);
    if (site) {
      title = site.id;
      kind = t("federation.sites");
      body = (
        <>
          <Row label={t("inspector.status")} value={<span className={`pill pill--${site.status === "online" ? "ok" : "warn"}`}>{t(`federation.${site.status}`)}</span>} />
          <Row label={t("federation.hlc")} value={<span className="mono">{site.hlc}</span>} />
          <Row label={t("federation.records")} value={String(site.records)} />
          <Row label={t("federation.reservations")} value={String(site.reservations)} />
          <Row label={t("federation.leases")} value={String(site.leases)} />
        </>
      );
    }
  }

  return (
    <aside className="inspector">
      <header className="inspector__head">
        <div>
          <span className="eyebrow">{t("inspector.title")}</span>
          <h2>{title}</h2>
        </div>
        <span className="pill pill--neutral">{kind || nav}</span>
      </header>
      <div className="inspector__body">{body}</div>
    </aside>
  );
}

function Row({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="kv">
      <span>{label}</span>
      <span>{value}</span>
    </div>
  );
}

function SettingsDialog({
  lang,
  theme,
  host,
  auth,
  inference,
  audit,
  hosted,
  onClose,
  onTheme,
  onLang,
  onWindow,
  onAuthChange,
  onInferenceChange,
  onAuditRefresh,
}: {
  lang: Lang;
  theme: Theme;
  host: HostInfo | null;
  auth: AuthStatus;
  inference: InferenceStatus;
  audit: AuditRow[];
  hosted: boolean;
  onClose: () => void;
  onTheme: (theme: Theme) => void;
  onLang: (lang: Lang) => void;
  onWindow: () => void;
  onAuthChange: (status: AuthStatus) => void;
  onInferenceChange: (status: InferenceStatus) => void;
  onAuditRefresh: () => void;
}) {
  const t = (key: string) => messagesFor(lang)[key] ?? key;
  const [displayName, setDisplayName] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [confirm, setConfirm] = useState("");
  const [endpoint, setEndpoint] = useState(inference.endpoint ?? "");
  const [model, setModel] = useState(inference.model ?? "");
  const [apiKey, setApiKey] = useState("");
  const [testPrompt, setTestPrompt] = useState("Reply with the single word: pong.");
  const [testResult, setTestResult] = useState<InferenceResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (inference.endpoint) setEndpoint(inference.endpoint);
    if (inference.model) setModel(inference.model);
  }, [inference.endpoint, inference.model]);

  const run = async <T,>(action: () => Promise<T>): Promise<T | undefined> => {
    setBusy(true);
    setError("");
    try {
      return await action();
    } catch (caught) {
      setError(String(caught));
      return undefined;
    } finally {
      setBusy(false);
    }
  };

  const createOperator = async () => {
    if (passphrase !== confirm) {
      setError(t("settings.auth.error") + ": passphrases do not match");
      return;
    }
    const status = await run(() => invoke<AuthStatus>("auth_register", { displayName, passphrase }));
    if (status) {
      onAuthChange(status);
      setPassphrase("");
      setConfirm("");
      onAuditRefresh();
    }
  };

  const signIn = async () => {
    const status = await run(() => invoke<AuthStatus>("auth_sign_in", { passphrase }));
    if (status) {
      onAuthChange(status);
      setPassphrase("");
      onAuditRefresh();
    }
  };

  const signOut = async () => {
    const status = await run(() => invoke<AuthStatus>("auth_sign_out"));
    if (status) {
      onAuthChange(status);
      onAuditRefresh();
    }
  };

  const saveInference = async () => {
    const status = await run(() => invoke<InferenceStatus>("inference_configure", { endpoint, model, apiKey }));
    if (status) {
      onInferenceChange(status);
      setApiKey("");
      onAuditRefresh();
    }
  };

  const testInference = async () => {
    const result = await run(() => invoke<InferenceResult>("inference_test", { prompt: testPrompt }));
    if (result) {
      setTestResult(result);
      onAuditRefresh();
    }
  };

  const forgetInference = async () => {
    const status = await run(() => invoke<InferenceStatus>("inference_forget"));
    if (status) {
      onInferenceChange(status);
      setTestResult(null);
      onAuditRefresh();
    }
  };

  const authDisabled = !hosted || busy;
  const inferenceDisabled = !hosted || !auth.signedIn || busy;

  return (
    <div className="modal-scrim" role="dialog" aria-modal="true" aria-label={t("settings.title")}>
      <div className="modal">
        <header className="modal__head">
          <h2>{t("settings.title")}</h2>
          <button className="icon-button" type="button" onClick={onClose} aria-label={t("settings.close")}>
            <CloseIcon size={16} />
          </button>
        </header>
        <div className="modal__body">
          <section className="settings-section">
            <h3>{t("settings.appearance")}</h3>
            <div className="segmented">
              <button type="button" className={theme === "light" ? "segmented__item segmented__item--active" : "segmented__item"} onClick={() => onTheme("light")}>{t("settings.theme.light")}</button>
              <button type="button" className={theme === "dark" ? "segmented__item segmented__item--active" : "segmented__item"} onClick={() => onTheme("dark")}>{t("settings.theme.dark")}</button>
            </div>
          </section>
          <section className="settings-section">
            <h3>{t("settings.language")}</h3>
            <div className="segmented">
              <button type="button" className={lang === "zh" ? "segmented__item segmented__item--active" : "segmented__item"} onClick={() => onLang("zh")}>中文</button>
              <button type="button" className={lang === "en" ? "segmented__item segmented__item--active" : "segmented__item"} onClick={() => onLang("en")}>English</button>
            </div>
          </section>
          <section className="settings-section">
            <h3><WindowIcon size={14} /> {t("settings.window")}</h3>
            <button className="secondary" type="button" onClick={onWindow}>{t("settings.openWindow")}</button>
          </section>

          <section className="settings-section">
            <h3><LockIcon size={14} /> {t("settings.auth")} <span className="pill pill--neutral">{t("settings.auth.local")}</span></h3>
            {!hosted ? <p className="muted">{t("settings.auth.hostOnly")}</p> : null}
            {hosted && auth.signedIn ? (
              <>
                <div className="kv"><span>{t("settings.auth.signedInAs")}</span><strong>{auth.displayName}</strong></div>
                {auth.sessionExpiresAtMs ? (
                  <div className="kv"><span>{t("settings.auth.expires")}</span><span>{new Date(auth.sessionExpiresAtMs).toLocaleTimeString()}</span></div>
                ) : null}
                <button className="secondary" type="button" onClick={signOut} disabled={busy}>{t("settings.auth.signOut")}</button>
              </>
            ) : null}
            {hosted && !auth.signedIn && !auth.hasOperator ? (
              <>
                <p className="muted">{t("settings.auth.passphraseHint")}</p>
                <div className="field-grid">
                  <label>{t("settings.auth.displayName")}<input value={displayName} onChange={(event) => setDisplayName(event.target.value)} disabled={authDisabled} autoComplete="username" /></label>
                  <label>{t("settings.auth.passphrase")}<input type="password" value={passphrase} onChange={(event) => setPassphrase(event.target.value)} disabled={authDisabled} autoComplete="new-password" /></label>
                  <label>{t("settings.auth.confirm")}<input type="password" value={confirm} onChange={(event) => setConfirm(event.target.value)} disabled={authDisabled} autoComplete="new-password" /></label>
                </div>
                <button className="secondary" type="button" onClick={createOperator} disabled={authDisabled}>{t("settings.auth.createButton")}</button>
              </>
            ) : null}
            {hosted && !auth.signedIn && auth.hasOperator ? (
              <>
                <label className="field-single">{t("settings.auth.passphrase")}<input type="password" value={passphrase} onChange={(event) => setPassphrase(event.target.value)} disabled={authDisabled} autoComplete="current-password" /></label>
                <button className="secondary" type="button" onClick={signIn} disabled={authDisabled}>{t("settings.auth.signInButton")}</button>
              </>
            ) : null}
            {error ? <p className="form-error">{error}</p> : null}
          </section>

          <section className="settings-section">
            <h3><CloudIcon size={14} /> {t("settings.inference")} <span className={`pill pill--${inference.configured ? "ok" : "neutral"}`}>{inference.configured ? t("settings.inference.configured") : t("settings.inference.notConfigured")}</span></h3>
            {!hosted ? <p className="muted">{t("settings.inference.hostOnly")}</p> : null}
            {hosted && !auth.signedIn ? <p className="muted">{t("settings.inference.needsAuth")}</p> : null}
            <div className="field-grid">
              <label>{t("settings.inference.endpoint")}<input value={endpoint} onChange={(event) => setEndpoint(event.target.value)} disabled={inferenceDisabled} placeholder="https://provider.example/v1" /></label>
              <label>{t("settings.inference.model")}<input value={model} onChange={(event) => setModel(event.target.value)} disabled={inferenceDisabled} placeholder="provider / model" /></label>
              <label>{t("settings.inference.key")}<input type="password" value={apiKey} onChange={(event) => setApiKey(event.target.value)} disabled={inferenceDisabled} placeholder={t("settings.inference.keyPlaceholder")} autoComplete="off" /></label>
            </div>
            <p className="muted">{t("settings.inference.note")}</p>
            <div className="form-actions">
              <button className="secondary" type="button" onClick={saveInference} disabled={inferenceDisabled || !endpoint || !model || !apiKey}>{t("settings.inference.saveButton")}</button>
              <button className="secondary" type="button" onClick={testInference} disabled={inferenceDisabled || !inference.configured}>{t("settings.inference.test")}</button>
              {inference.configured ? <button className="secondary" type="button" onClick={forgetInference} disabled={inferenceDisabled}>{t("settings.inference.forget")}</button> : null}
            </div>
            {auth.signedIn ? (
              <>
                <label className="field-single">{t("settings.inference.testPrompt")}<input value={testPrompt} onChange={(event) => setTestPrompt(event.target.value)} disabled={inferenceDisabled} /></label>
                {testResult ? (
                  <div className="inference-result">
                    <span className="eyebrow">{t("settings.inference.result")} · {t("settings.inference.latency")} {testResult.latencyMs} ms</span>
                    <p>{testResult.text}</p>
                  </div>
                ) : null}
              </>
            ) : null}
          </section>

          {hosted && auth.signedIn ? (
            <section className="settings-section">
              <h3>{t("settings.audit.title")}</h3>
              {audit.length === 0 ? <p className="muted">{t("settings.audit.empty")}</p> : null}
              <ul className="audit-list">
                {audit.slice(0, 8).map((row) => (
                  <li key={row.id}>
                    <span className="mono">{new Date(row.atMs).toLocaleTimeString()}</span>
                    <span>{row.actor}</span>
                    <strong>{row.event}</strong>
                    <span className="muted">{row.detail}</span>
                  </li>
                ))}
              </ul>
            </section>
          ) : null}

          <section className="settings-section">
            <h3>{t("settings.about")}</h3>
            <div className="kv"><span>{t("settings.version")}</span><span className="mono">{host?.version ?? "0.1.0"}</span></div>
            <div className="kv"><span>{t("settings.sandbox")}</span><span className="mono truncate">{host?.sandboxRoot ?? "preview"}</span></div>
          </section>
        </div>
      </div>
    </div>
  );
}

function Statusbar({ lang, overview, host, auth }: { lang: Lang; overview: DomainOverview; host: HostInfo | null; auth: AuthStatus }) {
  const t = (key: string) => messagesFor(lang)[key] ?? key;
  return (
    <footer className="statusbar">
      <span className="statusbar__item"><span className={`tail-dot${host ? " tail-dot--ok" : ""}`} />{host ? t("top.connected") : t("top.preview")}</span>
      <span className="statusbar__item">{t("status.model")} · {overview.modelName}</span>
      <span className="statusbar__item">{t("status.plan")} · {shortDigest(overview.plan.digest)}</span>
      <span className="statusbar__spacer" />
      <span className="statusbar__item">{t("status.operator")} · {auth.signedIn ? auth.displayName : t("status.signedOut")}</span>
      <span className="statusbar__item">{t("status.host")} {host?.version ?? "—"}</span>
      <span className="statusbar__item">{t("status.sandbox")} {host ? host.sandboxRoot.split("/").slice(-1)[0] : "preview"}</span>
      <span className="statusbar__item">{new Date().toLocaleTimeString(lang === "zh" ? "zh-CN" : "en-GB", { hour: "2-digit", minute: "2-digit" })}</span>
    </footer>
  );
}

export default function App() {
  const [nav, setNav] = useState<NavKey>(initialNav);
  const [theme, setTheme] = useState<Theme>(initialTheme);
  const [lang, setLang] = useState<Lang>(initialLang);
  const [overview, setOverview] = useState<DomainOverview>(fallbackOverview);
  const [execution, setExecution] = useState<ExecutionSnapshot>(fallbackExecution);
  const [federation, setFederation] = useState<FederationSnapshot>(fallbackFederation);
  const [host, setHost] = useState<HostInfo | null>(null);
  const [selection, setSelection] = useState<Selection | null>(() => defaultSelection(initialNav(), fallbackOverview));
  const [query, setQuery] = useState("");
  const [mode, setMode] = useState<Mode>("model");
  const [gate, setGate] = useState({ takeover: false, degraded: false });
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [focusMode] = useState(initialFocusMode);
  const [auth, setAuth] = useState<AuthStatus>(fallbackAuth);
  const [inference, setInference] = useState<InferenceStatus>(fallbackInference);
  const [audit, setAudit] = useState<AuditRow[]>([]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem("poie-theme", theme);
  }, [theme]);

  useEffect(() => {
    document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
    localStorage.setItem("poie-lang", lang);
  }, [lang]);

  useEffect(() => {
    if (!isTauri()) return;
    void invoke<DomainOverview>("domain_overview").then((loaded) => {
      setOverview(loaded);
      setSelection((current) => current ?? defaultSelection(nav, loaded));
    }).catch(() => undefined);
    void invoke<HostInfo>("host_info").then(setHost).catch(() => undefined);
    void invoke<ExecutionSnapshot>("execution_snapshot").then(setExecution).catch(() => undefined);
    void invoke<FederationSnapshot>("federation_snapshot").then(setFederation).catch(() => undefined);
    void invoke<AuthStatus>("auth_status").then(setAuth).catch(() => undefined);
    void invoke<InferenceStatus>("inference_status").then(setInference).catch(() => undefined);
  }, []);

  const refreshAudit = () => {
    if (!isTauri()) return;
    void invoke<AuditRow[]>("audit_list", { limit: 12 }).then(setAudit).catch(() => undefined);
  };

  const applyAuth = (status: AuthStatus) => {
    setAuth(status);
    if (status.signedIn) refreshAudit();
    else setAudit([]);
  };

  const navigate = (next: NavKey) => {
    setNav(next);
    setSelection(defaultSelection(next, overview));
    setQuery("");
  };

  const select = (next: Selection) => setSelection(next);

  const openWindow = () => {
    if (isTauri()) {
      void invoke("open_workspace_window", { view: nav }).catch(() => undefined);
    } else {
      window.open(`?view=${nav}`, "_blank", "noopener,noreferrer");
    }
  };

  const toggleTheme = () => setTheme((current) => (current === "light" ? "dark" : "light"));
  const toggleLang = () => setLang((current) => (current === "zh" ? "en" : "zh"));

  const view = useMemo(() => {
    if (nav === "overview") return <Overview lang={lang} overview={overview} selection={selection} onSelect={select} />;
    if (nav === "capabilities") return <Capabilities lang={lang} overview={overview} selection={selection} onSelect={select} />;
    if (nav === "plan") return <Plan lang={lang} overview={overview} selection={selection} onSelect={select} />;
    if (nav === "execution") return <Execution lang={lang} execution={execution} selection={selection} onSelect={select} gate={gate} onGate={setGate} />;
    if (nav === "federation") return <Federation lang={lang} federation={federation} selection={selection} onSelect={select} />;
    return <Evidence lang={lang} selection={selection} onSelect={select} />;
  }, [nav, lang, overview, execution, federation, selection, gate]);

  return (
    <div className={`cockpit${focusMode ? " cockpit--focus" : ""}`}>
      {!focusMode ? (
        <Rail
          lang={lang}
          nav={nav}
          theme={theme}
          onNav={navigate}
          onTheme={toggleTheme}
          onLang={toggleLang}
          onSettings={() => setSettingsOpen(true)}
        />
      ) : null}
      {!focusMode ? (
        <Sidebar
          lang={lang}
          nav={nav}
          overview={overview}
          query={query}
          onQuery={setQuery}
          selection={selection}
          onSelect={select}
          host={host}
          federation={federation}
        />
      ) : null}
      <main className="workspace">
        <Topbar
          lang={lang}
          nav={nav}
          mode={mode}
          onMode={setMode}
          onSettings={() => setSettingsOpen(true)}
          onWindow={openWindow}
          theme={theme}
          onTheme={toggleTheme}
          onLang={toggleLang}
          onSearch={() => document.getElementById("cockpit-search")?.focus()}
        />
        <div className="workspace__scroll">
          <div className="workspace__inner">{view}</div>
        </div>
      </main>
      {!focusMode ? (
        <Inspector
          lang={lang}
          nav={nav}
          overview={overview}
          selection={selection}
          gate={gate}
          execution={execution}
          federation={federation}
          onNavigate={(nextNav, nextSelection) => {
            setNav(nextNav);
            setSelection(nextSelection);
          }}
        />
      ) : null}
      <Statusbar lang={lang} overview={overview} host={host} auth={auth} />
      {settingsOpen ? (
        <SettingsDialog
          lang={lang}
          theme={theme}
          host={host}
          auth={auth}
          inference={inference}
          audit={audit}
          hosted={isTauri()}
          onClose={() => setSettingsOpen(false)}
          onTheme={setTheme}
          onLang={setLang}
          onWindow={openWindow}
          onAuthChange={applyAuth}
          onInferenceChange={setInference}
          onAuditRefresh={refreshAudit}
        />
      ) : null}
    </div>
  );
}
