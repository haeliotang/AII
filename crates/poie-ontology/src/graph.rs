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
/// Upper bound on single-run topping-up steps when several processes share one shortfall.
const MAX_TOPPING_STEPS: usize = 10_000;

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
    CapabilityBelowVerification,
    NoAlternativeSupport,
    DegradedRestriction,
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
            BlockerKind::CapabilityBelowVerification => "capability_below_verification",
            BlockerKind::NoAlternativeSupport => "no_alternative_support",
            BlockerKind::DegradedRestriction => "degraded_restriction",
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
    /// (capability, chosen provider, provider capability verification level)
    pub providers: Vec<(String, String, Verification)>,
    pub outputs: Vec<(String, f64)>,
    pub restores: Vec<String>,
    /// Assets occupied while degraded under a policy (§6.3 rule 11).
    pub degraded: Vec<String>,
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

    /// Weakest verification level on the route: process levels and the
    /// capability levels of the providers each step relies on (§6.3 rule 13).
    pub fn route_verification(&self) -> Option<Verification> {
        let mut weakest: Option<Verification> = None;
        for step in &self.steps {
            let mut level = step.verification;
            for (_, _, capability_level) in &step.providers {
                level = level.min(*capability_level);
            }
            weakest = Some(weakest.map_or(level, |current| current.min(level)));
        }
        weakest
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
    /// Degraded assets used under a policy on this route.
    degraded_used: BTreeSet<String>,
    /// Occupations of degraded assets on this route (for `max_occupations`).
    degraded_occupations: BTreeMap<String, u32>,
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

/// Deterministic rotation of an alternative list (enumeration ordering).
fn rotated<T>(items: &[T], shift: usize) -> impl Iterator<Item = &T> {
    let offset = if items.is_empty() { 0 } else { shift % items.len() };
    items.iter().skip(offset).chain(items.iter().take(offset))
}

struct Solver<'m> {
    model: &'m Model,
    min: Verification,
    max_depth: usize,
    path: Vec<Frame>,
    /// Rotates the order in which OR alternatives are tried, for enumeration.
    shift: usize,
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
        self.restore_with(id, ledger, None)
    }

    /// `degraded_process` is `Some(process)` when the caller may occupy the asset
    /// rather than restore it; the policy decides whether that is allowed
    /// (§3.1, §6.3 rule 11). A degraded-use asset is never cached as usable, so
    /// every occupation is re-checked and counted.
    fn restore_with(&mut self, id: &str, ledger: &mut Ledger, degraded_process: Option<&str>) -> Outcome {
        if ledger.usable.contains(id) {
            return Ok(());
        }
        let model = self.model;
        let asset = &model.assets[id];
        let frame = Frame::Restore(id.to_string());
        let degraded_use = asset.status == AssetStatus::Degraded
            && degraded_process.is_some_and(|process| {
                asset.degraded.as_ref().is_some_and(|policy| {
                    policy.occupiable
                        && (policy.allowed_processes.is_empty()
                            || policy.allowed_processes.iter().any(|p| p == process))
                })
            });
        let needs_repair = !degraded_use && matches!(asset.status, AssetStatus::Degraded | AssetStatus::Failed);
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

        // Alternative supports (§6.3 rule 9): at least one must be usable.
        // Failures of individual alternatives are rolled back like OR processes.
        if errs.is_empty() && !asset.requires_any.is_empty() {
            let mut alt_errs = Vec::new();
            let mut supported = false;
            for alt in rotated(&asset.requires_any, self.shift) {
                let mut trial = ledger.clone();
                match self.restore(alt, &mut trial) {
                    Ok(()) => {
                        *ledger = trial;
                        supported = true;
                        break;
                    }
                    Err(e) => alt_errs.extend(e),
                }
            }
            if !supported {
                errs.push(self.blocker(
                    BlockerKind::NoAlternativeSupport,
                    id,
                    format!(
                        "{id} needs at least one usable alternative support ({}); none is usable",
                        asset.requires_any.join(", ")
                    ),
                    false,
                ));
                errs.extend(alt_errs);
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
                for &pi in rotated(restorers, self.shift) {
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
            if degraded_use {
                ledger.degraded_used.insert(id.to_string());
            } else {
                ledger.usable.insert(id.to_string());
            }
            Ok(())
        } else {
            Err(errs)
        }
    }

    /// Makes `asset` usable for `process`, honouring a degraded-use policy.
    fn occupy(&mut self, asset: &str, process: &str, ledger: &mut Ledger) -> Outcome {
        self.restore_with(asset, ledger, Some(process))
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
        for &pi in rotated(producers, self.shift) {
            let fraction = model.processes[pi].yield_fraction_for(resource);
            let per_run: f64 =
                model.processes[pi].outputs.iter().filter(|f| f.resource == resource).map(|f| f.si * fraction).sum();
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

        // Multi-source top-up (§6.3 rule 10): when no single process covers the
        // whole shortfall, several alternative processes may each contribute a
        // part of it. Greedy, deterministic and bounded; failed attempts never
        // mutate the ledger.
        let mut trial = ledger.clone();
        let mut progressed = true;
        let mut attempts = 0usize;
        while progressed && !units::approx_le(amount, trial.available(resource)) && attempts < MAX_TOPPING_STEPS {
            progressed = false;
            attempts += 1;
            for &pi in rotated(producers, self.shift) {
                let mut candidate = trial.clone();
                if self.run_process(pi, 1, &mut candidate).is_err() {
                    continue;
                }
                let produced = candidate.available(resource) - trial.available(resource);
                if produced > units::rounding_tolerance(amount) {
                    trial = candidate;
                    progressed = true;
                    break;
                }
            }
        }
        if units::approx_le(amount, trial.available(resource)) {
            let alloc = trial.allocate(resource, amount);
            *ledger = trial;
            self.leave();
            return Ok(alloc);
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
        let mut degraded_occupations: Vec<(String, u32)> = Vec::new();
        for a in &p.occupies {
            if let Err(e) = self.occupy(a, &p.id, ledger) {
                errs.extend(e);
                continue;
            }
            if !ledger.degraded_used.contains(a) {
                continue;
            }
            let Some(policy) = &model.assets[a].degraded else { continue };
            if let Some(max) = policy.max_occupations {
                let used = ledger.degraded_occupations.get(a).copied().unwrap_or(0);
                if used + runs > max {
                    errs.push(self.blocker(
                        BlockerKind::DegradedRestriction,
                        a,
                        format!(
                            "{a} is degraded; its policy allows at most {max} occupation(s) per route, \
                             this step needs {runs} more (already {used})"
                        ),
                        true,
                    ));
                    continue;
                }
            }
            degraded_occupations.push((a.clone(), runs));
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
                for a in rotated(candidates, self.shift) {
                    let level = model.assets[a].capability_verification(cap);
                    if level < self.min {
                        alt_errs.push(self.blocker(
                            BlockerKind::CapabilityBelowVerification,
                            cap,
                            format!("{a} provides {cap} at {level}; the query requires at least {}", self.min),
                            true,
                        ));
                        continue;
                    }
                    let mut trial = ledger.clone();
                    match self.restore(a, &mut trial) {
                        Ok(()) => {
                            *ledger = trial;
                            chosen.push((cap.clone(), a.clone(), level));
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
        let mut outputs: Vec<(String, f64)> = Vec::new();
        for f in &p.outputs {
            let fraction = p.yield_fraction_for(&f.resource);
            outputs.push((f.resource.clone(), f.si * runs as f64 * fraction));
            if let Some(spec) = &p.process_yield {
                if spec.output == f.resource && fraction < 1.0 {
                    outputs.push((spec.reject.clone(), f.si * runs as f64 * (1.0 - fraction)));
                }
            }
        }
        for (r, amount) in &outputs {
            ledger.add(r, Source::Step(index), *amount);
        }
        for (a, count) in &degraded_occupations {
            *ledger.degraded_occupations.entry(a.clone()).or_default() += count;
        }
        let degraded: Vec<String> = degraded_occupations.iter().map(|(a, _)| a.clone()).collect();
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
            degraded,
        });
        Ok(())
    }
}

fn dedup(blockers: Vec<Blocker>) -> Vec<Blocker> {
    let mut seen = BTreeSet::new();
    blockers.into_iter().filter(|b| seen.insert((b.kind, b.subject.clone(), b.path.clone(), b.root))).collect()
}

pub fn solve(model: &Model, query: &Query) -> QueryResult {
    solve_attempt(model, query, 0)
}

fn solve_attempt(model: &Model, query: &Query, shift: usize) -> QueryResult {
    let mut solver = Solver { model, min: query.min_verification, max_depth: query.max_depth, path: Vec::new(), shift };
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

/// One structural candidate route with the summary needed for later ranking
/// (PO §8.8, §6.3 rule 14).
#[derive(Clone, Debug)]
pub struct CandidateRoute {
    pub steps: Vec<Step>,
    pub verification: Option<Verification>,
    /// Total allocated quantity (SI) per resource on the route.
    pub consumed: BTreeMap<String, f64>,
    /// Consumed resources with no local producing process (§6.5).
    pub external_dependencies: Vec<String>,
}

impl CandidateRoute {
    /// Stable identity used to deduplicate enumerated routes.
    pub fn signature(&self) -> String {
        let mut out = String::new();
        for step in &self.steps {
            let providers: Vec<String> = step.providers.iter().map(|(cap, a, _)| format!("{cap}<{a}")).collect();
            let sources: Vec<String> = step
                .allocations
                .iter()
                .map(|a| match &a.source {
                    Source::Lot(lot) => format!("{}:lot:{lot}", a.resource),
                    Source::Step(index) => format!("{}:step{index}", a.resource),
                })
                .collect();
            out.push_str(&format!("{}x{}|{}|{};", step.process, step.runs, providers.join(","), sources.join(",")));
        }
        out
    }
}

fn summarize_route(model: &Model, result: &QueryResult) -> CandidateRoute {
    let mut consumed: BTreeMap<String, f64> = BTreeMap::new();
    for step in &result.steps {
        for allocation in &step.allocations {
            *consumed.entry(allocation.resource.clone()).or_insert(0.0) += allocation.si;
        }
    }
    let external: BTreeSet<&str> = model.external_resources().into_iter().collect();
    let external_dependencies =
        consumed.keys().filter(|resource| external.contains(resource.as_str())).cloned().collect();
    CandidateRoute {
        steps: result.steps.clone(),
        verification: result.route_verification(),
        consumed,
        external_dependencies,
    }
}

/// Enumerates up to `limit` distinct structural candidate routes (§8.8).
///
/// The solver is deterministic, so enumeration rotates the order in which OR
/// alternatives (producers, restorers, capability providers, alternative
/// supports) are tried across attempts and deduplicates the resulting routes.
/// The search is deliberately bounded (64 attempts): it returns the routes it
/// found, possibly fewer than `limit`, and does not claim exhaustiveness.
pub fn enumerate(model: &Model, query: &Query, limit: usize) -> Vec<CandidateRoute> {
    const MAX_ATTEMPTS: usize = 64;
    let mut routes: Vec<CandidateRoute> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    if limit == 0 {
        return routes;
    }
    for shift in 0..MAX_ATTEMPTS {
        let result = solve_attempt(model, query, shift);
        if result.verdict != Verdict::Candidate {
            continue;
        }
        let route = summarize_route(model, &result);
        if seen.insert(route.signature()) {
            routes.push(route);
            if routes.len() >= limit {
                break;
            }
        }
    }
    routes
}

/// Human-readable list of enumerated routes and their per-route summaries.
pub fn render_routes(model: &Model, routes: &[CandidateRoute]) -> String {
    let mut out = String::new();
    for (i, route) in routes.iter().enumerate() {
        let weakest = route.verification.map(|v| v.to_string()).unwrap_or_else(|| "-".into());
        let _ = writeln!(out, "  route {} ({} step(s), weakest verification: {weakest}):", i + 1, route.steps.len());
        for (j, step) in route.steps.iter().enumerate() {
            let runs = if step.runs > 1 { format!(" x{}", step.runs) } else { String::new() };
            let _ = writeln!(out, "    {}. {}{} [{}]", j + 1, step.process, runs, step.verification);
        }
        let consumed: Vec<String> =
            route.consumed.iter().map(|(r, si)| format!("{} {r}", model.resources[r].fmt_amount(*si))).collect();
        let _ =
            writeln!(out, "    consumes: {}", if consumed.is_empty() { "nothing".into() } else { consumed.join(", ") });
        let _ = writeln!(
            out,
            "    external dependencies: {}",
            if route.external_dependencies.is_empty() { "none".into() } else { route.external_dependencies.join(", ") }
        );
    }
    out
}

/// One external dependency found while tracing a query (PO §6.5, VEC §4.2).
#[derive(Clone, Debug, PartialEq)]
pub struct ExternalDependency {
    pub resource: String,
    /// 1-based indices of the enumerated candidate routes that consume it.
    pub routes: Vec<usize>,
    /// Largest single-route allocated quantity (SI).
    pub max_si: f64,
    /// True when the dependency appears in the root causes of a blocked query.
    pub in_root_causes: bool,
}

/// External dependencies of a query, aggregated over candidate routes and, when
/// there is no route, over the blocked root causes (PO §6.5, VEC §4.2).
#[derive(Clone, Debug, Default)]
pub struct ExternalDependencyReport {
    pub candidate_routes: usize,
    pub dependencies: Vec<ExternalDependency>,
}

impl ExternalDependencyReport {
    pub fn resources(&self) -> Vec<&str> {
        self.dependencies.iter().map(|dependency| dependency.resource.as_str()).collect()
    }
}

/// Traces every external dependency a query relies on: resources with no local
/// producing process that appear in any enumerated candidate route, or in the
/// root causes of a blocked query. Indirect consumables (cutting inserts,
/// gauges modelled as resources) are included because they appear in the route
/// ledger (PO §6.5, VEC §4.2).
pub fn external_dependency_report(model: &Model, query: &Query, limit: usize) -> ExternalDependencyReport {
    let routes = enumerate(model, query, limit);
    let mut dependencies: BTreeMap<String, ExternalDependency> = BTreeMap::new();
    for (index, route) in routes.iter().enumerate() {
        for resource in &route.external_dependencies {
            let si = route.consumed.get(resource).copied().unwrap_or(0.0);
            let dependency = dependencies.entry(resource.clone()).or_insert_with(|| ExternalDependency {
                resource: resource.clone(),
                routes: Vec::new(),
                max_si: 0.0,
                in_root_causes: false,
            });
            dependency.routes.push(index + 1);
            dependency.max_si = dependency.max_si.max(si);
        }
    }
    if routes.is_empty() {
        let external: BTreeSet<&str> = model.external_resources().into_iter().collect();
        for blocker in solve(model, query).root_causes() {
            if blocker.kind == BlockerKind::MissingResource && external.contains(blocker.subject.as_str()) {
                let dependency = dependencies.entry(blocker.subject.clone()).or_insert_with(|| ExternalDependency {
                    resource: blocker.subject.clone(),
                    routes: Vec::new(),
                    max_si: 0.0,
                    in_root_causes: true,
                });
                dependency.in_root_causes = true;
            }
        }
    }
    ExternalDependencyReport { candidate_routes: routes.len(), dependencies: dependencies.into_values().collect() }
}

/// Human-readable external-dependency trace.
pub fn render_external_dependencies(model: &Model, report: &ExternalDependencyReport) -> String {
    let mut out = String::new();
    if report.candidate_routes == 0 {
        let _ =
            writeln!(out, "  external dependency trace: no candidate route; dependencies from blocked root causes:");
    } else {
        let _ = writeln!(
            out,
            "  external dependency trace ({} candidate route(s), {} dependency(ies)):",
            report.candidate_routes,
            report.dependencies.len()
        );
    }
    for dependency in &report.dependencies {
        let where_used = if dependency.in_root_causes {
            "blocked root cause".to_string()
        } else {
            let routes: Vec<String> = dependency.routes.iter().map(|index| format!("route {index}")).collect();
            routes.join(", ")
        };
        let amount = if dependency.routes.is_empty() {
            String::new()
        } else {
            model
                .resources
                .get(&dependency.resource)
                .map(|resource| format!(" (max {})", resource.fmt_amount(dependency.max_si)))
                .unwrap_or_default()
        };
        let _ = writeln!(out, "    - {}: needs external input; in {}{}", dependency.resource, where_used, amount);
    }
    out
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
                "  structural candidate (time, capacity and concurrency NOT checked); weakest verification: {}",
                result.route_verification().map(|v| v.to_string()).unwrap_or_default()
            );
        }
        for (i, s) in result.steps.iter().enumerate() {
            let runs = if s.runs > 1 { format!(" x{}", s.runs) } else { String::new() };
            let _ = writeln!(out, "  {}. {}{} [{}]", i + 1, s.process, runs, s.verification);
            if !s.occupies.is_empty() {
                let _ = writeln!(out, "       occupies: {}", s.occupies.join(", "));
            }
            for (cap, a, level) in &s.providers {
                let _ = writeln!(out, "       capability {cap} <- {a} [{level}]");
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
            if !s.degraded.is_empty() {
                let _ = writeln!(out, "       occupied while degraded (policy): {}", s.degraded.join(", "));
            }
            if let Some(spec) = model.process(&s.process).and_then(|p| p.process_yield.as_ref()) {
                let _ = writeln!(
                    out,
                    "       planned yield: {} x {} (reject -> {})",
                    spec.output, spec.fraction, spec.reject
                );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Model;

    fn model_with(assets: &str, lots: &str, processes: &str) -> Model {
        let text = format!(
            r#"
schema = "poie.physical-ontology/0.3"
name = "or-support-test"

[[resource]]
id = "energy"
unit = "kWh"

{assets}
{lots}
{processes}
"#
        );
        Model::from_doc(toml::from_str(&text).unwrap()).unwrap()
    }

    fn restore_query(asset: &str) -> Query {
        Query {
            goal: Goal::Restore(asset.to_string()),
            min_verification: Verification::Declared,
            max_depth: DEFAULT_MAX_DEPTH,
        }
    }

    fn alternatives(first: &str, second: &str) -> Model {
        model_with(
            &format!(
                r#"
[[asset]]
id = "target"
kind = "machine"
status = "available"
requires_any = ["alt-a", "alt-b"]

[[asset]]
id = "alt-a"
kind = "feed"
status = "{first}"

[[asset]]
id = "alt-b"
kind = "feed"
status = "{second}"
"#
            ),
            "",
            "",
        )
    }

    #[test]
    fn alternative_support_accepts_any_usable_alternative() {
        let m = alternatives("failed", "available");
        let r = solve(&m, &restore_query("target"));
        assert_eq!(r.verdict, Verdict::Candidate, "{}", render(&m, &r));
    }

    #[test]
    fn no_usable_alternative_is_reported() {
        let m = alternatives("failed", "failed");
        let r = solve(&m, &restore_query("target"));
        assert_eq!(r.verdict, Verdict::Blocked);
        let roots: Vec<(&BlockerKind, &str)> = r.root_causes().map(|b| (&b.kind, b.subject.as_str())).collect();
        assert!(roots.contains(&(&BlockerKind::AssetUnavailable, "alt-a")), "{roots:?}");
        assert!(roots.contains(&(&BlockerKind::AssetUnavailable, "alt-b")), "{roots:?}");
        let summary = r.blockers.iter().find(|b| b.kind == BlockerKind::NoAlternativeSupport).expect("summary");
        assert_eq!(summary.subject, "target");
        assert!(!summary.root);
        assert!(summary.detail.contains("alt-a, alt-b"), "{}", summary.detail);
    }

    #[test]
    fn and_requirements_are_still_required_with_alternative_support() {
        let with_and = |and_status: &str| {
            model_with(
                &format!(
                    r#"
[[asset]]
id = "target"
kind = "machine"
status = "available"
requires = ["and-support"]
requires_any = ["alt-a", "alt-b"]

[[asset]]
id = "and-support"
kind = "feed"
status = "{and_status}"

[[asset]]
id = "alt-a"
kind = "feed"
status = "failed"

[[asset]]
id = "alt-b"
kind = "feed"
status = "available"
"#
                ),
                "",
                "",
            )
        };

        let m = with_and("available");
        assert_eq!(solve(&m, &restore_query("target")).verdict, Verdict::Candidate);

        let m = with_and("failed");
        let r = solve(&m, &restore_query("target"));
        assert_eq!(r.verdict, Verdict::Blocked);
        assert!(r.root_causes().any(|b| b.kind == BlockerKind::AssetUnavailable && b.subject == "and-support"));
        // The OR list is not probed once the AND support has failed.
        assert!(!r.blockers.iter().any(|b| b.kind == BlockerKind::NoAlternativeSupport));
    }

    fn model_from_toml(body: &str) -> Model {
        let text = format!(
            r#"
schema = "poie.physical-ontology/0.3"
name = "test"
{body}
"#
        );
        Model::from_doc(toml::from_str(&text).unwrap()).unwrap()
    }

    fn obtain_query(resource: &str, quantity: &str) -> Query {
        Query {
            goal: Goal::Obtain { resource: resource.to_string(), quantity: quantity.parse().unwrap() },
            min_verification: Verification::Declared,
            max_depth: DEFAULT_MAX_DEPTH,
        }
    }

    const TWO_PRODUCERS: &str = r#"
[[resource]]
id = "a"
unit = "kg"

[[resource]]
id = "b"
unit = "kg"

[[resource]]
id = "r"
unit = "item"
mass_per_unit = "1 kg"

[[lot]]
id = "lot-a"
resource = "a"
quantity = "1 kg"
quality = "accepted"

[[lot]]
id = "lot-b"
resource = "b"
quantity = "1 kg"
quality = "accepted"

[[process]]
id = "make-r-from-a"
inputs = [{ resource = "a", quantity = "1 kg" }]
outputs = [{ resource = "r", quantity = "1 item" }]
verification = "demonstrated"

[[process]]
id = "make-r-from-b"
inputs = [{ resource = "b", quantity = "1 kg" }]
outputs = [{ resource = "r", quantity = "1 item" }]
verification = "demonstrated"
"#;

    const DEGRADED_LATHE: &str = r#"
[[resource]]
id = "raw"
unit = "kg"

[[resource]]
id = "out"
unit = "item"
mass_per_unit = "1 kg"

[[lot]]
id = "lot-raw"
resource = "raw"
quantity = "4 kg"
quality = "accepted"

[[asset]]
id = "lathe"
kind = "lathe"
status = "degraded"
POLICY_PLACEHOLDER

[[process]]
id = "turn"
inputs = [{ resource = "raw", quantity = "1 kg" }]
outputs = [{ resource = "out", quantity = "1 item" }]
occupies = ["lathe"]
verification = "declared"
"#;

    fn degraded_lathe_with(policy: &str) -> Model {
        model_from_toml(&DEGRADED_LATHE.replace("POLICY_PLACEHOLDER", policy))
    }

    #[test]
    fn degraded_asset_is_not_occupied_without_a_policy() {
        let m = degraded_lathe_with("");
        let r = solve(&m, &obtain_query("out", "1 item"));
        assert_eq!(r.verdict, Verdict::Blocked, "{}", render(&m, &r));
        assert!(r.root_causes().any(|b| b.kind == BlockerKind::AssetUnavailable && b.subject == "lathe"));
    }

    #[test]
    fn policy_allows_degraded_occupation_and_marks_the_step() {
        let m = degraded_lathe_with("degraded = { occupiable = true }");
        let r = solve(&m, &obtain_query("out", "1 item"));
        assert_eq!(r.verdict, Verdict::Candidate, "{}", render(&m, &r));
        assert_eq!(r.steps[0].degraded, vec!["lathe".to_string()]);
        assert!(render(&m, &r).contains("occupied while degraded (policy): lathe"));
    }

    #[test]
    fn policy_restricted_to_other_processes_falls_back_to_repair() {
        let body = format!(
            "{}\n{}",
            DEGRADED_LATHE
                .replace("POLICY_PLACEHOLDER", "degraded = { occupiable = true, allowed_processes = [\"other\"] }"),
            r#"
[[resource]]
id = "raw2"
unit = "kg"

[[resource]]
id = "out2"
unit = "item"
mass_per_unit = "1 kg"

[[process]]
id = "other"
inputs = [{ resource = "raw2", quantity = "1 kg" }]
outputs = [{ resource = "out2", quantity = "1 item" }]
verification = "declared"
"#
        );
        let m = model_from_toml(&body);
        let r = solve(&m, &obtain_query("out", "1 item"));
        assert_eq!(r.verdict, Verdict::Blocked, "{}", render(&m, &r));
        // Falls back to restoring the asset, which no process does.
        assert!(r.root_causes().any(|b| b.kind == BlockerKind::AssetUnavailable && b.subject == "lathe"));
        assert!(!r.blockers.iter().any(|b| b.kind == BlockerKind::DegradedRestriction));
    }

    #[test]
    fn policy_occupation_limit_is_enforced_per_route() {
        let m = degraded_lathe_with("degraded = { occupiable = true, max_occupations = 2 }");
        let r = solve(&m, &obtain_query("out", "2 item"));
        assert_eq!(r.verdict, Verdict::Candidate, "{}", render(&m, &r));
        assert_eq!(r.steps[0].runs, 2);

        let m = degraded_lathe_with("degraded = { occupiable = true, max_occupations = 1 }");
        let r = solve(&m, &obtain_query("out", "2 item"));
        assert_eq!(r.verdict, Verdict::Blocked, "{}", render(&m, &r));
        assert!(
            r.root_causes().any(|b| b.kind == BlockerKind::DegradedRestriction && b.subject == "lathe"),
            "{:?}",
            r.root_causes().collect::<Vec<_>>()
        );
    }

    const YIELD_MAKE: &str = r#"
[[resource]]
id = "raw"
unit = "kg"

[[resource]]
id = "good"
unit = "item"
mass_per_unit = "0.8 kg"

[[resource]]
id = "reject"
unit = "item"
mass_per_unit = "0.8 kg"

[[process]]
id = "make"
inputs = [{ resource = "raw", quantity = "0.8 kg" }]
outputs = [{ resource = "good", quantity = "1 item" }]
verification = "declared"
yield = { output = "good", fraction = 0.5, reject = "reject" }
"#;

    #[test]
    fn yield_plans_extra_runs_and_records_rejects() {
        let body = format!(
            "{}\n{}",
            YIELD_MAKE,
            r#"
[[lot]]
id = "lot-raw"
resource = "raw"
quantity = "2 kg"
quality = "accepted"
"#
        );
        let m = model_from_toml(&body);
        let r = solve(&m, &obtain_query("good", "1 item"));
        assert_eq!(r.verdict, Verdict::Candidate, "{}", render(&m, &r));
        assert_eq!(r.steps[0].runs, 2, "50% yield needs two runs for one good unit");
        assert!(r.steps[0].outputs.iter().any(|(res, qty)| res == "reject" && (*qty - 1.0).abs() < 1e-9));
        assert!(render(&m, &r).contains("planned yield: good x 0.5"));
    }

    #[test]
    fn rework_route_recovers_rejects() {
        let body = format!(
            "{}\n{}",
            YIELD_MAKE,
            r#"
[[lot]]
id = "lot-reject"
resource = "reject"
quantity = "2 item"
quality = "accepted"

[[process]]
id = "rework"
inputs = [{ resource = "reject", quantity = "1 item" }]
outputs = [{ resource = "raw", quantity = "0.8 kg" }]
verification = "declared"
"#
        );
        let m = model_from_toml(&body);
        let r = solve(&m, &obtain_query("good", "1 item"));
        assert_eq!(r.verdict, Verdict::Candidate, "{}", render(&m, &r));
        let route: Vec<&str> = r.steps.iter().map(|s| s.process.as_str()).collect();
        assert_eq!(route, ["rework", "make"], "{}", render(&m, &r));
    }

    #[test]
    fn yield_capacity_is_not_overbooked() {
        let body = format!(
            "{}\n{}",
            YIELD_MAKE,
            r#"
[[lot]]
id = "lot-raw"
resource = "raw"
quantity = "1.2 kg"
quality = "accepted"
"#
        );
        let m = model_from_toml(&body);
        let r = solve(&m, &obtain_query("good", "1 item"));
        assert_eq!(r.verdict, Verdict::Blocked, "{}", render(&m, &r));
        assert!(r.root_causes().any(|b| b.kind == BlockerKind::MissingResource && b.subject == "raw"));
    }

    const CAPABILITY_MODEL: &str = r#"
[[asset]]
id = "worker"
kind = "robot"
status = "available"
provides = ["repair"]
LEVEL_PLACEHOLDER

[[asset]]
id = "machine"
kind = "pump"
status = "degraded"

[[process]]
id = "fix"
restores = ["machine"]
requires_capabilities = ["repair"]
verification = "demonstrated"
"#;

    fn capability_model(level: &str) -> Model {
        model_from_toml(&CAPABILITY_MODEL.replace("LEVEL_PLACEHOLDER", level))
    }

    #[test]
    fn capability_level_below_the_query_threshold_blocks_the_provider() {
        let m = capability_model("capability_verification = { \"repair\" = \"declared\" }");
        let mut query = restore_query("machine");
        query.min_verification = Verification::Demonstrated;
        let r = solve(&m, &query);
        assert_eq!(r.verdict, Verdict::Blocked, "{}", render(&m, &r));
        assert!(r.root_causes().any(|b| b.kind == BlockerKind::CapabilityBelowVerification && b.subject == "repair"));
        assert!(!r.steps.iter().any(|s| s.process == "fix"));
    }

    #[test]
    fn route_verification_includes_capability_levels() {
        let m = capability_model("capability_verification = { \"repair\" = \"modeled\" }");
        let mut query = restore_query("machine");
        query.min_verification = Verification::Modeled;
        let r = solve(&m, &query);
        assert_eq!(r.verdict, Verdict::Candidate, "{}", render(&m, &r));
        assert_eq!(r.route_verification(), Some(Verification::Modeled));
        assert!(r.steps[0]
            .providers
            .iter()
            .any(|(cap, worker, level)| { cap == "repair" && worker == "worker" && *level == Verification::Modeled }));
    }

    #[test]
    fn enumeration_finds_distinct_routes_with_their_external_dependencies() {
        let m = model_from_toml(TWO_PRODUCERS);
        let routes = enumerate(&m, &obtain_query("r", "1 item"), 4);
        assert_eq!(routes.len(), 2, "{routes:?}");
        let deps: Vec<Vec<String>> = routes.iter().map(|r| r.external_dependencies.clone()).collect();
        assert!(deps.contains(&vec!["a".to_string()]), "{deps:?}");
        assert!(deps.contains(&vec!["b".to_string()]), "{deps:?}");
        let mut signatures: Vec<String> = routes.iter().map(|r| r.signature()).collect();
        signatures.sort();
        signatures.dedup();
        assert_eq!(signatures.len(), 2);
    }

    #[test]
    fn enumeration_respects_the_limit_and_does_not_invent_routes() {
        let m = model_from_toml(TWO_PRODUCERS);
        let limited = enumerate(&m, &obtain_query("r", "1 item"), 1);
        assert_eq!(limited.len(), 1);

        // A model with a single producer has exactly one structural route.
        let single = format!(
            "{}\n{}",
            YIELD_MAKE,
            r#"
[[lot]]
id = "lot-raw"
resource = "raw"
quantity = "2 kg"
quality = "accepted"
"#
        );
        let m = model_from_toml(&single);
        let routes = enumerate(&m, &obtain_query("good", "1 item"), 8);
        assert_eq!(routes.len(), 1, "{routes:?}");
    }

    #[test]
    fn external_dependencies_are_unioned_over_candidate_routes() {
        let m = model_from_toml(TWO_PRODUCERS);
        let report = external_dependency_report(&m, &obtain_query("r", "1 item"), 8);
        assert_eq!(report.candidate_routes, 2);
        let resources = report.resources();
        assert!(resources.contains(&"a"), "{resources:?}");
        assert!(resources.contains(&"b"), "{resources:?}");
        let a = report.dependencies.iter().find(|d| d.resource == "a").expect("a");
        let b = report.dependencies.iter().find(|d| d.resource == "b").expect("b");
        assert_eq!(a.routes, vec![1]);
        assert_eq!(b.routes, vec![2]);
        assert!((a.max_si - 1.0).abs() < 1e-9);
        assert!(!a.in_root_causes);
    }

    #[test]
    fn blocked_query_reports_external_dependencies_from_root_causes() {
        let m = model_from_toml(TWO_PRODUCERS);
        let report = external_dependency_report(&m, &obtain_query("r", "3 item"), 8);
        assert_eq!(report.candidate_routes, 0);
        assert!(report.dependencies.iter().any(|d| d.resource == "a" && d.in_root_causes));
        assert!(report.dependencies.iter().any(|d| d.resource == "b" && d.in_root_causes));
    }

    #[test]
    fn a_shortfall_can_be_covered_by_two_processes() {
        let m = model_from_toml(TWO_PRODUCERS);
        let r = solve(&m, &obtain_query("r", "2 item"));
        assert_eq!(r.verdict, Verdict::Candidate, "{}", render(&m, &r));
        let processes: Vec<&str> = r.steps.iter().map(|s| s.process.as_str()).collect();
        assert_eq!(processes, ["make-r-from-a", "make-r-from-b"]);
    }

    #[test]
    fn one_process_is_preferred_when_it_can_cover_the_whole_shortfall() {
        let body = TWO_PRODUCERS.replace(
            r#"quantity = "1 kg"
quality = "accepted"

[[lot]]
id = "lot-b""#,
            r#"quantity = "3 kg"
quality = "accepted"

[[lot]]
id = "lot-b""#,
        );
        let m = model_from_toml(&body);
        let r = solve(&m, &obtain_query("r", "2 item"));
        assert_eq!(r.verdict, Verdict::Candidate, "{}", render(&m, &r));
        assert_eq!(r.steps.len(), 1, "single producer should cover the shortfall: {}", render(&m, &r));
        assert_eq!(r.steps[0].process, "make-r-from-a");
        assert_eq!(r.steps[0].runs, 2);
    }

    #[test]
    fn multi_source_still_fails_when_total_capacity_is_short() {
        let m = model_from_toml(TWO_PRODUCERS);
        let r = solve(&m, &obtain_query("r", "3 item"));
        assert_eq!(r.verdict, Verdict::Blocked, "{}", render(&m, &r));
        let roots: Vec<(&BlockerKind, &str)> = r.root_causes().map(|b| (&b.kind, b.subject.as_str())).collect();
        assert!(roots.contains(&(&BlockerKind::MissingResource, "a")), "{roots:?}");
        assert!(roots.contains(&(&BlockerKind::MissingResource, "b")), "{roots:?}");
    }

    #[test]
    fn an_alternative_can_be_restored_by_a_process() {
        let m = model_with(
            r#"
[[resource]]
id = "part"
unit = "item"
mass_per_unit = "1 kg"

[[asset]]
id = "target"
kind = "machine"
status = "available"
requires_any = ["alt-a", "alt-b"]

[[asset]]
id = "alt-a"
kind = "feed"
status = "failed"

[[asset]]
id = "alt-b"
kind = "feed"
status = "failed"
"#,
            r#"
[[lot]]
id = "lot-part"
resource = "part"
quantity = "1 item"
quality = "accepted"
"#,
            r#"
[[process]]
id = "fix-alt-b"
inputs = [{ resource = "part", quantity = "1 item" }]
restores = ["alt-b"]
asset_mass_change = "1 kg"
verification = "declared"
"#,
        );
        let r = solve(&m, &restore_query("target"));
        assert_eq!(r.verdict, Verdict::Candidate, "{}", render(&m, &r));
        assert_eq!(r.steps.len(), 1);
        assert_eq!(r.steps[0].process, "fix-alt-b");
    }

    // Issue #105 (P4, review A-01): a process whose mass balance is unchecked
    // must not appear as a step of a candidate route (PO §5, §6).
    #[test]
    fn po5_unchecked_process_is_not_a_candidate_step() {
        let text = r#"
schema = "poie.physical-ontology/0.3"
name = "conjure-test"

[[resource]]
id = "work-order"
unit = "item"

[[resource]]
id = "bar"
unit = "kg"

[[lot]]
id = "lot-orders"
resource = "work-order"
quantity = "5 item"
quality = "accepted"

[[process]]
id = "conjure-bar"
inputs = [{ resource = "work-order", quantity = "1 item" }]
outputs = [{ resource = "bar", quantity = "500 kg" }]
verification = "verified"
"#;
        let m = Model::from_doc(toml::from_str(text).unwrap()).unwrap();
        let r = solve(
            &m,
            &Query {
                goal: Goal::Obtain { resource: "bar".into(), quantity: "500 kg".parse().unwrap() },
                min_verification: Verification::Declared,
                max_depth: DEFAULT_MAX_DEPTH,
            },
        );
        assert_eq!(r.verdict, Verdict::Blocked, "{r:?}");
        assert!(r.root_causes().any(|b| b.kind == BlockerKind::MassUnchecked && b.subject == "conjure-bar"), "{r:?}");
    }
}
