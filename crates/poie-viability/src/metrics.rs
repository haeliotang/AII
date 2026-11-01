//! The four metric groups (VEC §4).
//!
//! A run produces a [`RunTrace`]: time-stamped service levels, hard-constraint
//! violations, external inputs, stock, cumulative output, recycling flows,
//! asset health and the affected services/assets. The functions here compute the
//! contract's metrics from that trace. Two rules are enforced structurally:
//! a censored survival time is kept as censored (never faked as the horizon),
//! and sustainability cannot be reported without production and service metrics.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use poie_ontology::graph::{self, Goal, Query, DEFAULT_MAX_DEPTH};
use poie_ontology::model::{Model, Patch, Verification};
use poie_ontology::units::Quantity;

#[derive(Clone, Debug, PartialEq)]
pub struct ConstraintViolation {
    pub constraint: String,
    pub at_s: f64,
}

/// Structural recovery-path finding at the first service violation (VEC §3.3).
#[derive(Clone, Debug, PartialEq)]
pub struct RecoveryPath {
    pub target: String,
    pub structural: String,
}

/// One timestamped observation of a run.
#[derive(Clone, Debug, PartialEq)]
pub struct TimeSample {
    pub at_s: f64,
    /// Service level as a fraction of nominal (0..=1).
    pub service_levels: BTreeMap<String, f64>,
    pub violations: Vec<ConstraintViolation>,
    /// Amounts supplied from outside since the previous sample (per resource).
    pub external_inputs: BTreeMap<String, f64>,
    pub stock: BTreeMap<String, f64>,
    /// Cumulative good output per resource.
    pub produced: BTreeMap<String, f64>,
    /// Cumulative recycling inputs and reusable outputs (kg).
    pub recycle_inputs_kg: BTreeMap<String, f64>,
    pub recycle_reusable_kg: BTreeMap<String, f64>,
    /// Asset health indicators (unit defined by the model).
    pub health: BTreeMap<String, f64>,
    pub affected_services: Vec<String>,
    pub affected_assets: Vec<String>,
}

/// The full trace of one scenario.
#[derive(Clone, Debug, PartialEq)]
pub struct RunTrace {
    pub scenario_id: String,
    pub horizon_s: f64,
    /// Minimum acceptable level per critical service.
    pub minimum_levels: BTreeMap<String, f64>,
    pub samples: Vec<TimeSample>,
    /// Recovery-path queries performed at the first violation of each service.
    pub recovery_paths: BTreeMap<String, RecoveryPath>,
}

impl RunTrace {
    pub fn validate(&self) -> Result<(), MetricError> {
        if self.scenario_id.trim().is_empty() {
            return Err(MetricError::InvalidTrace("scenario id must not be empty".into()));
        }
        if !self.horizon_s.is_finite() || self.horizon_s <= 0.0 {
            return Err(MetricError::InvalidTrace("horizon must be finite and positive".into()));
        }
        if self.minimum_levels.is_empty() {
            return Err(MetricError::InvalidTrace("at least one critical service is required".into()));
        }
        for (service, minimum) in &self.minimum_levels {
            if !minimum.is_finite() || !(0.0..=1.0).contains(minimum) {
                return Err(MetricError::InvalidTrace(format!("service {service:?} has an invalid minimum")));
            }
        }
        if self.samples.is_empty() {
            return Err(MetricError::InvalidTrace("a trace needs at least one sample".into()));
        }
        let mut previous = -1.0;
        for sample in &self.samples {
            if !sample.at_s.is_finite() || sample.at_s < 0.0 || sample.at_s > self.horizon_s {
                return Err(MetricError::InvalidTrace("sample time outside the horizon".into()));
            }
            if sample.at_s <= previous {
                return Err(MetricError::InvalidTrace("samples must be strictly increasing".into()));
            }
            previous = sample.at_s;
            for (service, level) in &sample.service_levels {
                if !level.is_finite() || !(0.0..=1.0).contains(level) {
                    return Err(MetricError::InvalidTrace(format!("service {service:?} level out of range")));
                }
            }
            for value in sample.external_inputs.values().chain(sample.stock.values()).chain(sample.health.values()) {
                if !value.is_finite() {
                    return Err(MetricError::InvalidTrace("non-finite metric input".into()));
                }
            }
        }
        Ok(())
    }

    pub fn start(&self) -> &TimeSample {
        &self.samples[0]
    }

    pub fn end(&self) -> &TimeSample {
        self.samples.last().expect("validated traces have samples")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetricError {
    InvalidTrace(String),
    AntiCheat(String),
}

impl fmt::Display for MetricError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetricError::InvalidTrace(detail) => write!(f, "invalid trace: {detail}"),
            MetricError::AntiCheat(detail) => write!(f, "sustainability reporting rejected: {detail}"),
        }
    }
}

impl std::error::Error for MetricError {}

// ---------------------------------------------------------------------------
// §4.1 Viability
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct ServiceViability {
    pub service: String,
    /// Time until the service first dropped below its minimum; `None` means
    /// censored (it never dropped within the horizon, or the reading went
    /// missing before it could be shown to drop).
    pub maintained_s: Option<f64>,
    pub censored: bool,
    /// Censoring time: the horizon when the service held throughout, or the
    /// time of the first missing reading (VEC §4: a missing reading is
    /// `unknown`, never full service). `None` when an event was observed.
    pub censored_at_s: Option<f64>,
    pub first_violation_s: Option<f64>,
    pub violation_count: usize,
    pub recovery_path: Option<RecoveryPath>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViabilitySummary {
    pub service: String,
    /// All runs, including undecided ones (VEC §4: no run leaves the distribution).
    pub scenarios: usize,
    /// Smallest observed survival time; `None` when every run is censored
    /// (report as "> follow-up").
    pub min_maintained_s: Option<f64>,
    /// Product-limit (Kaplan–Meier) quantiles over all runs, censored ones
    /// included (VEC §4.1). `None` means the estimated failure fraction never
    /// reaches the quantile: report it as "> `follow_up_s`", never as a value.
    pub median_maintained_s: Option<f64>,
    pub p90_maintained_s: Option<f64>,
    /// Largest observed time (event or censoring) — the bound for "> …".
    pub follow_up_s: f64,
    /// Censored runs, including undecided runs censored at their stop time.
    pub censored: usize,
    /// Runs that stopped undecided (timeout, numerical failure, unknown input).
    pub undecided: usize,
    /// Sensitivity analysis: undecided runs counted as failures at their stop time.
    pub worst_case_min_maintained_s: Option<f64>,
    pub worst_case_median_maintained_s: Option<f64>,
    /// Mean is only reported when no survival time is censored.
    pub mean_maintained_s: Option<f64>,
    pub first_violations: usize,
    pub violation_count: usize,
}

/// A run that ended undecided (timeout, numerical failure, unknown input)
/// before its horizon. It still enters the survival distribution: censored at
/// `stopped_at_s` for its services, and listed separately (VEC §4).
#[derive(Clone, Debug, PartialEq)]
pub struct UndecidedRun {
    pub scenario_id: String,
    pub stopped_at_s: f64,
    /// The verdict class, e.g. `timeout`.
    pub class: String,
    /// Services the run was evaluating; empty means every summarized service.
    pub services: Vec<String>,
}

impl UndecidedRun {
    fn covers(&self, service: &str) -> bool {
        self.services.is_empty() || self.services.iter().any(|item| item == service)
    }
}

pub fn viability(trace: &RunTrace) -> Vec<ServiceViability> {
    trace
        .minimum_levels
        .iter()
        .map(|(service, minimum)| {
            // Walk the samples in order: a reading below the minimum is the
            // event; a missing reading censors the run at that time.
            let mut event: Option<f64> = None;
            let mut missing: Option<f64> = None;
            for sample in &trace.samples {
                match sample.service_levels.get(service) {
                    Some(level) if level + 1e-9 < *minimum => {
                        event = Some(sample.at_s);
                        break;
                    }
                    Some(_) => {}
                    None => {
                        missing = Some(sample.at_s);
                        break;
                    }
                }
            }
            let violation_count = trace
                .samples
                .iter()
                .filter(|sample| sample.violations.iter().any(|v| v.constraint == *service))
                .count();
            ServiceViability {
                service: service.clone(),
                maintained_s: event,
                censored: event.is_none(),
                censored_at_s: if event.is_some() { None } else { Some(missing.unwrap_or(trace.horizon_s)) },
                first_violation_s: event,
                violation_count,
                recovery_path: trace.recovery_paths.get(service).cloned(),
            }
        })
        .collect()
}

pub fn summarize_viability(traces: &[RunTrace]) -> Vec<ViabilitySummary> {
    summarize_runs(traces, &[])
}

/// Summaries over all runs (VEC §4.1): completed traces plus undecided runs.
/// Undecided runs are censored at their stop time for each service they
/// cover, and a worst-case sensitivity counts them as failures there.
pub fn summarize_runs(traces: &[RunTrace], undecided: &[UndecidedRun]) -> Vec<ViabilitySummary> {
    let mut per_service: BTreeMap<String, Vec<ServiceViability>> = BTreeMap::new();
    for trace in traces {
        for entry in viability(trace) {
            per_service.entry(entry.service.clone()).or_default().push(entry);
        }
    }
    for run in undecided {
        for service in &run.services {
            per_service.entry(service.clone()).or_default();
        }
    }
    per_service
        .into_iter()
        .map(|(service, entries)| {
            let undecided: Vec<&UndecidedRun> = undecided.iter().filter(|run| run.covers(&service)).collect();
            // (time, is_event) for every run.
            let mut observations: Vec<(f64, bool)> = entries
                .iter()
                .map(|entry| match entry.maintained_s {
                    Some(time) => (time, true),
                    None => (entry.censored_at_s.unwrap_or(f64::INFINITY), false),
                })
                .collect();
            let undecided_times: Vec<f64> = undecided.iter().map(|run| run.stopped_at_s).collect();
            let mut worst_case = observations.clone();
            observations.extend(undecided_times.iter().map(|time| (*time, false)));
            worst_case.extend(undecided_times.iter().map(|time| (*time, true)));
            let events: Vec<f64> = entries.iter().filter_map(|entry| entry.maintained_s).collect();
            let censored = entries.iter().filter(|entry| entry.censored).count() + undecided.len();
            let mean = if censored == 0 && !events.is_empty() {
                Some(events.iter().sum::<f64>() / events.len() as f64)
            } else {
                None
            };
            let minimum = |items: &[(f64, bool)]| {
                items
                    .iter()
                    .filter(|(_, event)| *event)
                    .map(|(time, _)| *time)
                    .fold(None, |best: Option<f64>, time| Some(best.map_or(time, |current| current.min(time))))
            };
            let follow_up_s =
                observations.iter().map(|(time, _)| *time).filter(|time| time.is_finite()).fold(0.0, f64::max);
            ViabilitySummary {
                service,
                scenarios: observations.len(),
                min_maintained_s: minimum(&observations),
                median_maintained_s: km_quantile(&observations, 0.5),
                p90_maintained_s: km_quantile(&observations, 0.9),
                follow_up_s,
                censored,
                undecided: undecided.len(),
                worst_case_min_maintained_s: minimum(&worst_case),
                worst_case_median_maintained_s: km_quantile(&worst_case, 0.5),
                mean_maintained_s: mean,
                first_violations: entries.iter().filter(|entry| entry.first_violation_s.is_some()).count(),
                violation_count: entries.iter().map(|entry| entry.violation_count).sum(),
            }
        })
        .collect()
}

/// Product-limit (Kaplan–Meier) quantile: the first time at which the
/// estimated failure fraction reaches `fraction`. Events at a time are
/// processed before censorings at the same time. `None` when the estimate
/// never reaches `fraction` within the observed follow-up.
pub fn km_quantile(observations: &[(f64, bool)], fraction: f64) -> Option<f64> {
    let mut sorted: Vec<(f64, bool)> = observations.to_vec();
    sorted.sort_by(|left, right| {
        left.0.partial_cmp(&right.0).unwrap_or(std::cmp::Ordering::Equal).then_with(|| right.1.cmp(&left.1))
    });
    let mut at_risk = sorted.len() as f64;
    let mut survival = 1.0_f64;
    let mut index = 0;
    while index < sorted.len() {
        let time = sorted[index].0;
        let mut events = 0.0;
        let mut leaving = 0.0;
        while index < sorted.len() && sorted[index].0 == time {
            if sorted[index].1 {
                events += 1.0;
            }
            leaving += 1.0;
            index += 1;
        }
        if events > 0.0 && at_risk > 0.0 {
            survival *= 1.0 - events / at_risk;
            if 1.0 - survival + 1e-12 >= fraction {
                return Some(time);
            }
        }
        at_risk -= leaving;
    }
    None
}

// ---------------------------------------------------------------------------
// §4.2 Independence
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct IndependenceMetrics {
    /// External inputs summed over the run traces.
    pub external_inputs: BTreeMap<String, f64>,
    /// Critical external dependencies over all candidate recovery routes.
    pub critical_external_dependencies: Vec<String>,
    /// The critical-resource set (denominator m), derived automatically: every
    /// resource consumed on a candidate recovery route of a critical service,
    /// plus the critical external dependencies (VEC §4.2). Never hand-picked.
    pub critical_resources: Vec<String>,
    /// Critical resources with a closed local production route (numerator n).
    pub covered_resources: Vec<String>,
    /// For each covered resource, the external inputs its local route still
    /// draws from stock ("closed to the given stock level").
    pub coverage_external_inputs: BTreeMap<String, Vec<String>>,
    /// (n, m) = (covered critical resources, critical resources).
    pub local_coverage: (usize, usize),
}

/// Independence metrics (VEC §4.2).
///
/// A critical resource counts as locally covered only when, with its own stock
/// set to zero, the capability graph still finds a candidate route that
/// produces one unit at `min_verification` — route level including capability
/// levels (PO §6.3 rule 13) — with all inputs closed to the current stock. A
/// process whose consumable (e.g. a cutting insert) is out of stock does not
/// cover anything.
pub fn independence(model: &Model, services: &[String], min_verification: Verification) -> IndependenceMetrics {
    let mut dependencies: BTreeSet<String> = BTreeSet::new();
    let mut critical: BTreeSet<String> = BTreeSet::new();
    for service in services {
        let query = Query { goal: Goal::Service(service.clone()), min_verification, max_depth: DEFAULT_MAX_DEPTH };
        let report = graph::external_dependency_report(model, &query, 32);
        dependencies.extend(report.resources().into_iter().map(str::to_string));
        for route in graph::enumerate(model, &query, 32) {
            critical.extend(route.consumed.keys().cloned());
        }
    }
    critical.extend(dependencies.iter().cloned());
    let critical_external_dependencies: Vec<String> = dependencies.into_iter().collect();
    let critical_resources: Vec<String> = critical.into_iter().collect();
    let mut covered_resources = Vec::new();
    let mut coverage_external_inputs = BTreeMap::new();
    for resource in &critical_resources {
        if let Some(inputs) = local_route(model, resource, min_verification) {
            covered_resources.push(resource.clone());
            coverage_external_inputs.insert(resource.clone(), inputs);
        }
    }
    IndependenceMetrics {
        external_inputs: BTreeMap::new(),
        critical_external_dependencies,
        local_coverage: (covered_resources.len(), critical_resources.len()),
        critical_resources,
        covered_resources,
        coverage_external_inputs,
    }
}

/// External inputs of a closed local production route for one unit of
/// `resource`, or `None` when no such route exists.
fn local_route(model: &Model, resource: &str, min_verification: Verification) -> Option<Vec<String>> {
    let definition = model.resources.get(resource)?;
    if model.producers.get(resource).is_none_or(|producers| producers.is_empty()) {
        return None;
    }
    let mut patch = Patch::default();
    for lot in model.lots.iter().filter(|lot| lot.resource == resource) {
        patch.lot_quantity.insert(lot.id.clone(), format!("0 {}", definition.unit.symbol));
    }
    let zeroed = model.patched(&patch).ok()?;
    let goal = Goal::Obtain { resource: resource.to_string(), quantity: Quantity::new(1.0, definition.unit) };
    let query = Query { goal, min_verification, max_depth: DEFAULT_MAX_DEPTH };
    if graph::solve(&zeroed, &query).verdict != graph::Verdict::Candidate {
        return None;
    }
    let report = graph::external_dependency_report(&zeroed, &query, 8);
    Some(report.resources().into_iter().map(str::to_string).collect())
}

pub fn aggregate_external_inputs(traces: &[RunTrace]) -> BTreeMap<String, f64> {
    let mut totals: BTreeMap<String, f64> = BTreeMap::new();
    for trace in traces {
        for sample in &trace.samples {
            for (resource, amount) in &sample.external_inputs {
                *totals.entry(resource.clone()).or_insert(0.0) += amount;
            }
        }
    }
    totals
}

// ---------------------------------------------------------------------------
// §4.3 Resilience
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct ResilienceMetrics {
    /// ∫ max(0, minimum − actual) dt per service.
    pub service_loss: BTreeMap<String, f64>,
    /// First-violation to recovery time per service; `None` means not recovered.
    pub recovery_time_s: BTreeMap<String, Option<f64>>,
    pub not_recovered: Vec<String>,
    /// Deficit (minimum − level) still open at the horizon, from the last
    /// sample; `None` when the last reading is missing (unknown). Reported
    /// next to the loss integral so that "fail late, never recover" is visible
    /// (VEC §4.3, terminal state §2.1).
    pub gap_at_horizon: BTreeMap<String, Option<f64>>,
    /// Time without a reading per service: excluded from the loss integral and
    /// reported as unknown, never as full service.
    pub unobserved_s: BTreeMap<String, f64>,
    pub propagation_services: usize,
    pub propagation_assets: usize,
}

pub fn resilience(trace: &RunTrace) -> ResilienceMetrics {
    let mut service_loss: BTreeMap<String, f64> = BTreeMap::new();
    let mut recovery_time_s: BTreeMap<String, Option<f64>> = BTreeMap::new();
    let mut gap_at_horizon: BTreeMap<String, Option<f64>> = BTreeMap::new();
    let mut unobserved_s: BTreeMap<String, f64> = BTreeMap::new();
    let mut not_recovered = Vec::new();
    let mut first_violation_overall: Option<f64> = None;

    for (service, minimum) in &trace.minimum_levels {
        let mut loss = 0.0;
        let mut unobserved = 0.0;
        for (index, sample) in trace.samples.iter().enumerate() {
            let end = trace.samples.get(index + 1).map_or(trace.horizon_s, |next| next.at_s);
            let dt = (end - sample.at_s).max(0.0);
            match sample.service_levels.get(service) {
                Some(level) => loss += (minimum - level).max(0.0) * dt,
                None => unobserved += dt,
            }
        }
        service_loss.insert(service.clone(), loss);
        unobserved_s.insert(service.clone(), unobserved);
        gap_at_horizon
            .insert(service.clone(), trace.end().service_levels.get(service).map(|level| (minimum - level).max(0.0)));

        let below =
            |sample: &&TimeSample| sample.service_levels.get(service).is_some_and(|level| level + 1e-9 < *minimum);
        let first = trace.samples.iter().find(below);
        let recovery = first.and_then(|violation| {
            trace
                .samples
                .iter()
                .filter(|sample| sample.at_s > violation.at_s)
                .find(|sample| sample.service_levels.get(service).is_some_and(|level| level + 1e-9 >= *minimum))
        });
        match first {
            Some(violation) => {
                first_violation_overall =
                    Some(first_violation_overall.map_or(violation.at_s, |current: f64| current.min(violation.at_s)));
                match recovery {
                    Some(recovered) => {
                        recovery_time_s.insert(service.clone(), Some(recovered.at_s - violation.at_s));
                    }
                    None => {
                        recovery_time_s.insert(service.clone(), None);
                        not_recovered.push(service.clone());
                    }
                }
            }
            None => {
                recovery_time_s.insert(service.clone(), Some(0.0));
            }
        }
    }

    let (mut propagation_services, mut propagation_assets) = (BTreeSet::new(), BTreeSet::new());
    if let Some(first) = first_violation_overall {
        for sample in trace.samples.iter().filter(|sample| sample.at_s >= first) {
            propagation_services.extend(sample.affected_services.iter().cloned());
            propagation_assets.extend(sample.affected_assets.iter().cloned());
        }
    }
    ResilienceMetrics {
        service_loss,
        recovery_time_s,
        not_recovered,
        gap_at_horizon,
        unobserved_s,
        propagation_services: propagation_services.len(),
        propagation_assets: propagation_assets.len(),
    }
}

// ---------------------------------------------------------------------------
// §4.4 Sustainability
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct SustainabilityMetrics {
    /// End stock − start stock − external inputs, per resource.
    pub net_consumption: BTreeMap<String, f64>,
    /// Recycling inputs minus reusable outputs (kg).
    pub recycle_loss_kg: f64,
    /// Start and end health per asset.
    pub health_change: BTreeMap<String, (f64, f64)>,
    /// Cumulative good output per resource.
    pub output: BTreeMap<String, f64>,
}

pub fn sustainability(trace: &RunTrace) -> SustainabilityMetrics {
    let start = trace.start();
    let end = trace.end();
    let mut resources: BTreeSet<String> = BTreeSet::new();
    resources.extend(start.stock.keys().cloned());
    resources.extend(end.stock.keys().cloned());
    for sample in &trace.samples {
        resources.extend(sample.external_inputs.keys().cloned());
    }
    let mut net_consumption = BTreeMap::new();
    for resource in resources {
        let start_stock = start.stock.get(&resource).copied().unwrap_or(0.0);
        let end_stock = end.stock.get(&resource).copied().unwrap_or(0.0);
        let external: f64 =
            trace.samples.iter().map(|sample| sample.external_inputs.get(&resource).copied().unwrap_or(0.0)).sum();
        net_consumption.insert(resource, end_stock - start_stock - external);
    }
    let recycle_loss_kg = end
        .recycle_inputs_kg
        .iter()
        .map(|(stream, input)| input - end.recycle_reusable_kg.get(stream).copied().unwrap_or(0.0))
        .sum();
    let mut health_change = BTreeMap::new();
    for asset in start.health.keys() {
        let before = start.health[asset];
        let after = end.health.get(asset).copied().unwrap_or(before);
        health_change.insert(asset.clone(), (before, after));
    }
    SustainabilityMetrics { net_consumption, recycle_loss_kg, health_change, output: end.produced.clone() }
}

/// A sustainability claim that may only be shown together with its production
/// and service context (VEC §4.4 anti-cheat clause).
#[derive(Clone, Debug, PartialEq)]
pub struct SustainabilityClaim {
    pub sustainability: SustainabilityMetrics,
    pub production: BTreeMap<String, f64>,
    pub service: Vec<ServiceViability>,
}

impl SustainabilityClaim {
    pub fn new(
        sustainability: SustainabilityMetrics,
        production: BTreeMap<String, f64>,
        service: Vec<ServiceViability>,
    ) -> Result<SustainabilityClaim, MetricError> {
        if production.is_empty() {
            return Err(MetricError::AntiCheat(
                "production metrics are missing: stopping production is not sustainability".into(),
            ));
        }
        if service.is_empty() {
            return Err(MetricError::AntiCheat("service metrics are missing".into()));
        }
        Ok(SustainabilityClaim { sustainability, production, service })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample(at_s: f64, level: f64) -> TimeSample {
        TimeSample {
            at_s,
            service_levels: BTreeMap::from([("potable-water".to_string(), level)]),
            violations: Vec::new(),
            external_inputs: BTreeMap::new(),
            stock: BTreeMap::new(),
            produced: BTreeMap::new(),
            recycle_inputs_kg: BTreeMap::new(),
            recycle_reusable_kg: BTreeMap::new(),
            health: BTreeMap::new(),
            affected_services: Vec::new(),
            affected_assets: Vec::new(),
        }
    }

    fn trace(samples: Vec<TimeSample>) -> RunTrace {
        RunTrace {
            scenario_id: "s1".into(),
            horizon_s: 1000.0,
            minimum_levels: BTreeMap::from([("potable-water".to_string(), 1.0)]),
            samples,
            recovery_paths: BTreeMap::new(),
        }
    }

    #[test]
    fn viability_records_first_drop_and_censors_absence() {
        let dropped = trace(vec![sample(0.0, 1.0), sample(100.0, 0.5), sample(300.0, 1.0)]);
        let entry = &viability(&dropped)[0];
        assert_eq!(entry.maintained_s, Some(100.0));
        assert!(!entry.censored);

        let held = trace(vec![sample(0.0, 1.0), sample(500.0, 1.0)]);
        let entry = &viability(&held)[0];
        assert!(entry.censored);
        assert_eq!(entry.maintained_s, None);
    }

    #[test]
    fn censored_samples_suppress_the_mean() {
        let dropped = trace(vec![sample(0.0, 1.0), sample(100.0, 0.5)]);
        let held = trace(vec![sample(0.0, 1.0), sample(500.0, 1.0)]);
        let summary = summarize_viability(&[dropped, held]);
        assert_eq!(summary[0].censored, 1);
        assert_eq!(summary[0].mean_maintained_s, None);
        assert_eq!(summary[0].min_maintained_s, Some(100.0));
    }

    #[test]
    fn resilience_integrates_loss_and_recovery_time() {
        let first = sample(0.0, 1.0);
        let mut dropped = sample(100.0, 0.5);
        dropped.affected_services = vec!["potable-water".into()];
        dropped.affected_assets = vec!["pump-p1".into(), "bus-1".into()];
        let recovered = sample(300.0, 1.0);
        let metrics = resilience(&trace(vec![first, dropped, recovered]));
        assert_eq!(metrics.service_loss["potable-water"], 100.0);
        assert_eq!(metrics.recovery_time_s["potable-water"], Some(200.0));
        assert_eq!(metrics.propagation_services, 1);
        assert_eq!(metrics.propagation_assets, 2);
    }

    #[test]
    fn unrecovered_services_are_censored_not_zero() {
        let metrics = resilience(&trace(vec![sample(0.0, 1.0), sample(100.0, 0.5)]));
        assert_eq!(metrics.recovery_time_s["potable-water"], None);
        assert_eq!(metrics.not_recovered, vec!["potable-water"]);
    }

    #[test]
    fn independence_lists_external_dependencies_and_local_coverage() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01/model.toml");
        let model = Model::load(&path).unwrap();
        let metrics = independence(&model, &["potable-water".to_string()], Verification::Demonstrated);
        assert!(!metrics.critical_external_dependencies.is_empty());
        let (covered, total) = metrics.local_coverage;
        assert!(covered <= total);
        // VEC §4.2: the denominator is the derived critical-resource set,
        // which contains every external dependency.
        assert_eq!(total, metrics.critical_resources.len());
        for dependency in &metrics.critical_external_dependencies {
            assert!(metrics.critical_resources.contains(dependency));
        }
    }

    #[test]
    fn net_consumption_and_recycle_loss() {
        let mut first = sample(0.0, 1.0);
        first.stock = BTreeMap::from([("bronze-bar".to_string(), 10.0)]);
        first.health = BTreeMap::from([("pump-p1".to_string(), 0.9)]);
        let mut last = sample(1000.0, 1.0);
        last.stock = BTreeMap::from([("bronze-bar".to_string(), 12.0)]);
        last.produced = BTreeMap::from([("sleeve-p1".to_string(), 2.0)]);
        last.recycle_inputs_kg = BTreeMap::from([("bronze-swarf".to_string(), 4.0)]);
        last.recycle_reusable_kg = BTreeMap::from([("bronze-swarf".to_string(), 3.0)]);
        last.health = BTreeMap::from([("pump-p1".to_string(), 0.7)]);
        let mut second = sample(500.0, 1.0);
        second.external_inputs = BTreeMap::from([("bronze-bar".to_string(), 5.0)]);
        let metrics = sustainability(&trace(vec![first, second, last]));
        assert_eq!(metrics.net_consumption["bronze-bar"], -3.0);
        assert_eq!(metrics.recycle_loss_kg, 1.0);
        assert_eq!(metrics.health_change["pump-p1"], (0.9, 0.7));
        assert_eq!(metrics.output["sleeve-p1"], 2.0);
    }

    #[test]
    fn sustainability_cannot_be_claimed_without_production_and_service() {
        let metrics = sustainability(&trace(vec![sample(0.0, 1.0), sample(100.0, 1.0)]));
        let service = viability(&trace(vec![sample(0.0, 1.0), sample(100.0, 1.0)]));
        assert!(SustainabilityClaim::new(metrics.clone(), BTreeMap::new(), service.clone()).is_err());
        assert!(
            SustainabilityClaim::new(metrics, BTreeMap::from([("sleeve-p1".to_string(), 1.0)]), Vec::new()).is_err()
        );
        assert!(SustainabilityClaim::new(
            sustainability(&trace(vec![sample(0.0, 1.0), sample(100.0, 1.0)])),
            BTreeMap::from([("sleeve-p1".to_string(), 1.0)]),
            service
        )
        .is_ok());
    }

    #[test]
    fn invalid_traces_are_rejected() {
        let mut bad = trace(vec![sample(100.0, 1.0), sample(50.0, 1.0)]);
        assert!(bad.validate().is_err());
        bad = trace(vec![]);
        assert!(bad.validate().is_err());
        let out_of_range = trace(vec![sample(0.0, 1.2)]);
        assert!(out_of_range.validate().is_err());
    }
}
