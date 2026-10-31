//! Capability-graph queries (Physical Ontology §6).
//!
//! The solver builds one *structural candidate* route on a single ledger:
//! consumables are allocated at most once, reusable assets are occupied
//! without being consumed, asset usability is checked recursively through
//! `requires`, and a goal that reappears while it is being resolved is a
//! `cycle`. Time, capacity and concurrency are not checked in v0.1, so a
//! `candidate` is not a schedule.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde::Deserialize;

use crate::model::{AssetStatus, LotQuality, Model, Verification};
use crate::units::{self, Quantity};

pub const DEFAULT_MAX_DEPTH: usize = 48;
/// Upper bound on repeated runs of one process for a single shortfall.
const MAX_RUNS: f64 = 10_000.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Goal {
    Obtain { resource: String, quantity: Quantity },
    Restore(String),
    Service(String),
}

#[derive(Clone, Debug)]
pub struct Query {
    pub goal: Goal,
    pub min_verification: Verification,
    pub max_depth: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Candidate,
    Blocked,
    Unknown,
    SearchLimit,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockerKind {
    MissingResource,
    AssetUnavailable,
    UnknownState,
    Cycle,
    BelowVerification,
    NoCapabilityProvider,
    DepthExceeded,
}

impl BlockerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BlockerKind::MissingResource => "missing_resource",
            BlockerKind::AssetUnavailable => "asset_unavailable",
            BlockerKind::UnknownState => "unknown_state",
            BlockerKind::Cycle => "cycle",
            BlockerKind::BelowVerification => "below_verification",
            BlockerKind::NoCapabilityProvider => "no_capability_provider",
            BlockerKind::DepthExceeded => "depth_exceeded",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Blocker {
    pub kind: BlockerKind,
    pub subject: String,
    pub detail: String,
    /// Goal path at which the blocker was found, outermost first.
    pub path: Vec<String>,
    /// True for root causes; false for summaries of alternatives that all failed.
    pub root: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Lot(String),
    /// Output of an earlier step in the same route (index into `steps`).
    Step(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Allocation {
    pub resource: String,
    pub si: f64,
    pub source: Source,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub process: String,
    pub runs: u32,
    pub verification: Verification,
    pub allocations: Vec<Allocation>,
    pub occupies: Vec<String>,
    /// (capability, chosen provider)
    pub providers: Vec<(String, String)>,
    pub outputs: Vec<(String, f64)>,
    pub restores: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct QueryResult {
    pub verdict: Verdict,
    pub steps: Vec<Step>,
    pub blockers: Vec<Blocker>,
}

impl QueryResult {
    pub fn root_causes(&self) -> impl Iterator<Item = &Blocker> {
        self.blockers.iter().filter(|b| b.root)
    }

    /// Weakest verification level among the route's processes.
    pub fn route_verification(&self) -> Option<Verification> {
        self.steps.iter().map(|s| s.verification).min()
    }
}

#[derive(Clone, Debug)]
struct Balance {
    source: Source,
    remaining: f64,
}

#[derive(Clone, Debug, Default)]
struct Ledger {
    stock: BTreeMap<String, Vec<Balance>>,
    usable: BTreeSet<String>,
    steps: Vec<Step>,
    /// Total already allocated per resource on this route (for reporting).
    allocated: BTreeMap<String, f64>,
}

impl Ledger {
    fn new(model: &Model) -> Ledger {
        let mut ledger = Ledger::default();
        for lot in model.lots.iter().filter(|l| l.quality == LotQuality::Accepted && l.si > 0.0) {
            ledger
                .stock
                .entry(lot.resource.clone())
                .or_default()
                .push(Balance { source: Source::Lot(lot.id.clone()), remaining: lot.si });
        }
        ledger
    }

    fn available(&self, resource: &str) -> f64 {
        self.stock.get(resource).map_or(0.0, |b| b.iter().map(|b| b.remaining).sum())
    }

    /// Allocates `amount` first-in first-out. Caller must have checked availability.
    fn allocate(&mut self, resource: &str, amount: f64) -> Vec<Allocation> {
        let mut left = amount;
        let mut out = Vec::new();
        if let Some(balances) = self.stock.get_mut(resource) {
            for b in balances.iter_mut() {
                if left <= units::rounding_tolerance(amount) {
                    break;
                }
                let take = b.remaining.min(left);
                if take <= 0.0 {
                    continue;
                }
                b.remaining -= take;
                left -= take;
                *self.allocated.entry(resource.to_string()).or_default() += take;
                out.push(Allocation { resource: resource.to_string(), si: take, source: b.source.clone() });
            }
        }
        out
    }

    fn add(&mut self, resource: &str, source: Source, amount: f64) {
        self.stock.entry(resource.to_string()).or_default().push(Balance { source, remaining: amount });
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Frame {
    Service(String),
    Restore(String),
    Obtain(String),
    Process(String),
    Capability(String),
}

impl Frame {
    fn label(&self) -> String {
        match self {
            Frame::Service(s) => format!("service:{s}"),
            Frame::Restore(a) => format!("restore:{a}"),
            Frame::Obtain(r) => format!("obtain:{r}"),
            Frame::Process(p) => format!("process:{p}"),
            Frame::Capability(c) => format!("capability:{c}"),
        }
    }
}

type Outcome<T = ()> = Result<T, Vec<Blocker>>;

struct Solver<'m> {
    model: &'m Model,
    min: Verification,
    max_depth: usize,
    path: Vec<Frame>,
}

impl<'m> Solver<'m> {
    fn blocker(&self, kind: BlockerKind, subject: &str, detail: String, root: bool) -> Blocker {
        Blocker { kind, subject: subject.to_string(), detail, path: self.path.iter().map(Frame::label).collect(), root }
    }

    fn enter(&mut self, frame: Frame) -> Outcome {
        if self.path.len() >= self.max_depth {
            let label = frame.label();
            return Err(vec![self.blocker(
                BlockerKind::DepthExceeded,
                &label,
                format!("search depth limit {} reached", self.max_depth),
                true,
            )]);
        }
        self.path.push(frame);
        Ok(())
    }

    fn leave(&mut self) {
        self.path.pop();
    }

    fn on_path(&self, frame: &Frame) -> bool {
        self.path.contains(frame)
    }

    fn service(&mut self, id: &str, ledger: &mut Ledger) -> Outcome {
        self.enter(Frame::Service(id.to_string()))?;
        let mut errs = Vec::new();
        for a in &self.model.services[id].requires {
            if let Err(e) = self.restore(a, ledger) {
                errs.extend(e);
            }
        }
        self.leave();
        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs)
        }
    }

    /// Makes asset `id` usable: itself available (or restored) and every
    /// asset it `requires` usable.
    fn restore(&mut self, id: &str, ledger: &mut Ledger) -> Outcome {
        if ledger.usable.contains(id) {
            return Ok(());
        }
        let model = self.model;
        let asset = &model.assets[id];
        let frame = Frame::Restore(id.to_string());
        let needs_repair = matches!(asset.status, AssetStatus::Degraded | AssetStatus::Failed);
        if needs_repair && self.on_path(&frame) {
            let mut b = self.blocker(
                BlockerKind::Cycle,
                id,
                format!("restoring {id} ({}) requires {id} itself to be usable", asset.status),
                true,
            );
            b.path.push(frame.label());
            return Err(vec![b]);
        }
        self.enter(frame)?;
        if asset.status == AssetStatus::Unknown {
            let b = self.blocker(
                BlockerKind::UnknownState,
                id,
                format!("{id} has no valid status evidence; unknown is not available"),
                true,
            );
            self.leave();
            return Err(vec![b]);
        }

        let mut errs = Vec::new();
        for r in &asset.requires {
            if let Err(e) = self.restore(r, ledger) {
                errs.extend(e);
            }
        }

        if needs_repair {
            let restorers = model.restorers.get(id).map(Vec::as_slice).unwrap_or(&[]);
            if restorers.is_empty() {
                errs.push(self.blocker(
                    BlockerKind::AssetUnavailable,
                    id,
                    format!("{id} is {} and no process restores it", asset.status),
                    true,
                ));
            } else {
                let mut alt_errs = Vec::new();
                let mut done = false;
                for &pi in restorers {
                    let mut trial = ledger.clone();
                    match self.run_process(pi, 1, &mut trial) {
                        Ok(()) => {
                            *ledger = trial;
                            done = true;
                            break;
                        }
                        Err(e) => alt_errs.extend(e),
                    }
                }
                if !done {
                    let names: Vec<&str> = restorers.iter().map(|&i| model.processes[i].id.as_str()).collect();
                    errs.push(self.blocker(
                        BlockerKind::AssetUnavailable,
                        id,
                        format!("{id} is {}; every restoring process is blocked: {}", asset.status, names.join(", ")),
                        false,
                    ));
                    errs.extend(alt_errs);
                }
            }
        }
        self.leave();
        if errs.is_empty() {
            ledger.usable.insert(id.to_string());
            Ok(())
        } else {
            Err(errs)
        }
    }

    /// Allocates `amount` (SI) of `resource`, producing any shortfall.
    fn obtain(&mut self, resource: &str, amount: f64, ledger: &mut Ledger) -> Outcome<Vec<Allocation>> {
        let model = self.model;
        let res = &model.resources[resource];
        let available = ledger.available(resource);
        if units::approx_le(amount, available) {
            return Ok(ledger.allocate(resource, amount));
        }
        let shortfall = amount - available;
        let frame = Frame::Obtain(resource.to_string());
        if self.on_path(&frame) {
            let mut b = self.blocker(
                BlockerKind::Cycle,
                resource,
                format!("producing {resource} requires {resource} that is not in stock"),
                true,
            );
            b.path.push(frame.label());
            return Err(vec![b]);
        }
        self.enter(frame)?;
        let already = ledger.allocated.get(resource).copied().unwrap_or(0.0);
        let mut need = format!("need {}, available {}", res.fmt_amount(amount), res.fmt_amount(available));
        if already > 0.0 {
            need.push_str(&format!(" ({} already allocated on this route)", res.fmt_amount(already)));
        }
        let producers = model.producers.get(resource).map(Vec::as_slice).unwrap_or(&[]);
        if producers.is_empty() {
            let b = self.blocker(
                BlockerKind::MissingResource,
                resource,
                format!("{need}; no local process produces it (external dependency)"),
                true,
            );
            self.leave();
            return Err(vec![b]);
        }

        let mut alt_errs = Vec::new();
        for &pi in producers {
            let per_run: f64 =
                model.processes[pi].outputs.iter().filter(|f| f.resource == resource).map(|f| f.si).sum();
            let runs = (shortfall / per_run - 1e-9).ceil().max(1.0);
            if runs > MAX_RUNS {
                alt_errs.push(self.blocker(
                    BlockerKind::DepthExceeded,
                    &model.processes[pi].id,
                    format!("would need {runs} runs to cover the shortfall"),
                    true,
                ));
                continue;
            }
            let mut trial = ledger.clone();
            match self.run_process(pi, runs as u32, &mut trial) {
                Ok(()) if units::approx_le(amount, trial.available(resource)) => {
                    let alloc = trial.allocate(resource, amount);
                    *ledger = trial;
                    self.leave();
                    return Ok(alloc);
                }
                Ok(()) => alt_errs.push(self.blocker(
                    BlockerKind::MissingResource,
                    resource,
                    format!("{} consumed the {resource} it was meant to replenish", model.processes[pi].id),
                    true,
                )),
                Err(e) => alt_errs.extend(e),
            }
        }
        let mut errs = vec![self.blocker(
            BlockerKind::MissingResource,
            resource,
            format!("{need}; every local producing process is blocked"),
            false,
        )];
        errs.extend(alt_errs);
        self.leave();
        Err(errs)
    }

    fn run_process(&mut self, pi: usize, runs: u32, ledger: &mut Ledger) -> Outcome {
        let model = self.model;
        let p = &model.processes[pi];
        self.enter(Frame::Process(p.id.clone()))?;
        if p.verification < self.min {
            let b = self.blocker(
                BlockerKind::BelowVerification,
                &p.id,
                format!("{} is {}, query requires at least {}", p.id, p.verification, self.min),
                true,
            );
            self.leave();
            return Err(vec![b]);
        }

        let mut errs = Vec::new();
        for a in &p.occupies {
            if let Err(e) = self.restore(a, ledger) {
                errs.extend(e);
            }
        }

        let mut chosen = Vec::new();
        for cap in &p.requires_capabilities {
            if let Err(e) = self.enter(Frame::Capability(cap.clone())) {
                errs.extend(e);
                continue;
            }
            let candidates = model.providers.get(cap).map(Vec::as_slice).unwrap_or(&[]);
            if candidates.is_empty() {
                errs.push(self.blocker(
                    BlockerKind::NoCapabilityProvider,
                    cap,
                    format!("no asset provides {cap}"),
                    true,
                ));
            } else {
                let mut alt_errs = Vec::new();
                let mut found = false;
                for a in candidates {
                    let mut trial = ledger.clone();
                    match self.restore(a, &mut trial) {
                        Ok(()) => {
                            *ledger = trial;
                            chosen.push((cap.clone(), a.clone()));
                            found = true;
                            break;
                        }
                        Err(e) => alt_errs.extend(e),
                    }
                }
                if !found {
                    errs.push(self.blocker(
                        BlockerKind::NoCapabilityProvider,
                        cap,
                        format!("every provider of {cap} is unusable: {}", candidates.join(", ")),
                        false,
                    ));
                    errs.extend(alt_errs);
                }
            }
            self.leave();
        }

        let mut allocations = Vec::new();
        for f in &p.inputs {
            match self.obtain(&f.resource, f.si * runs as f64, ledger) {
                Ok(a) => allocations.extend(a),
                Err(e) => errs.extend(e),
            }
        }

        self.leave();
        if !errs.is_empty() {
            return Err(errs);
        }
        let index = ledger.steps.len();
        let outputs: Vec<(String, f64)> = p.outputs.iter().map(|f| (f.resource.clone(), f.si * runs as f64)).collect();
        for (r, amount) in &outputs {
            ledger.add(r, Source::Step(index), *amount);
        }
        ledger.usable.extend(p.restores.iter().cloned());
        ledger.steps.push(Step {
            process: p.id.clone(),
            runs,
            verification: p.verification,
            allocations,
            occupies: p.occupies.clone(),
            providers: chosen,
            outputs,
            restores: p.restores.clone(),
        });
        Ok(())
    }
}

fn dedup(blockers: Vec<Blocker>) -> Vec<Blocker> {
    let mut seen = BTreeSet::new();
    blockers.into_iter().filter(|b| seen.insert((b.kind, b.subject.clone(), b.path.clone(), b.root))).collect()
}

pub fn solve(model: &Model, query: &Query) -> QueryResult {
    let mut solver = Solver { model, min: query.min_verification, max_depth: query.max_depth, path: Vec::new() };
    let mut ledger = Ledger::new(model);
    let outcome = match &query.goal {
        Goal::Service(s) => solver.service(s, &mut ledger),
        Goal::Restore(a) => solver.restore(a, &mut ledger),
        Goal::Obtain { resource, quantity } => solver.obtain(resource, quantity.si, &mut ledger).map(|_| ()),
    };
    match outcome {
        Ok(()) => QueryResult { verdict: Verdict::Candidate, steps: ledger.steps, blockers: Vec::new() },
        Err(blockers) => {
            let blockers = dedup(blockers);
            let verdict = if blockers.iter().any(|b| b.kind == BlockerKind::DepthExceeded) {
                Verdict::SearchLimit
            } else if blockers.iter().any(|b| b.kind == BlockerKind::UnknownState) {
                Verdict::Unknown
            } else {
                Verdict::Blocked
            };
            QueryResult { verdict, steps: Vec::new(), blockers }
        }
    }
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Candidate => "candidate",
            Verdict::Blocked => "blocked",
            Verdict::Unknown => "unknown",
            Verdict::SearchLimit => "search_limit",
        }
    }
}

/// Human-readable report of a query result.
pub fn render(model: &Model, result: &QueryResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "verdict: {}", result.verdict.as_str());
    if result.verdict == Verdict::Candidate {
        if result.steps.is_empty() {
            let _ = writeln!(out, "  goal already satisfied; no steps needed");
        } else {
            let _ = writeln!(
                out,
                "  structural candidate (time, capacity and concurrency NOT checked); weakest step: {}",
                result.route_verification().map(|v| v.to_string()).unwrap_or_default()
            );
        }
        for (i, s) in result.steps.iter().enumerate() {
            let runs = if s.runs > 1 { format!(" x{}", s.runs) } else { String::new() };
            let _ = writeln!(out, "  {}. {}{} [{}]", i + 1, s.process, runs, s.verification);
            if !s.occupies.is_empty() {
                let _ = writeln!(out, "       occupies: {}", s.occupies.join(", "));
            }
            for (cap, a) in &s.providers {
                let _ = writeln!(out, "       capability {cap} <- {a}");
            }
            for a in &s.allocations {
                let from = match &a.source {
                    Source::Lot(l) => format!("lot {l}"),
                    Source::Step(k) => format!("output of step {}", k + 1),
                };
                let _ = writeln!(
                    out,
                    "       consumes {} {} ({from})",
                    model.resources[&a.resource].fmt_amount(a.si),
                    a.resource
                );
            }
            for (r, amount) in &s.outputs {
                let _ = writeln!(out, "       produces {} {r}", model.resources[r].fmt_amount(*amount));
            }
            if !s.restores.is_empty() {
                let _ = writeln!(out, "       restores: {}", s.restores.join(", "));
            }
        }
    } else {
        let _ = writeln!(out, "  root causes:");
        for b in result.root_causes() {
            let _ = writeln!(out, "    - {} {}: {}", b.kind.as_str(), b.subject, b.detail);
            let _ = writeln!(out, "      at {}", b.path.join(" > "));
        }
        let mut seen = BTreeSet::new();
        let context: Vec<&Blocker> =
            result.blockers.iter().filter(|b| !b.root && seen.insert((b.kind, &b.subject, &b.detail))).collect();
        if !context.is_empty() {
            let _ = writeln!(out, "  context:");
            for b in context {
                let _ = writeln!(out, "    - {} {}: {}", b.kind.as_str(), b.subject, b.detail);
            }
        }
    }
    out
}
