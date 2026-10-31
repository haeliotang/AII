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
use poie_ontology::model::{Model, Verification};

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
    /// censored (it never dropped within the horizon).
    pub maintained_s: Option<f64>,
    pub censored: bool,
    pub first_violation_s: Option<f64>,
    pub violation_count: usize,
    pub recovery_path: Option<RecoveryPath>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViabilitySummary {
    pub service: String,
    pub scenarios: usize,
    /// Smallest non-censored survival time; `None` when every scenario censored.
    pub min_maintained_s: Option<f64>,
    pub median_maintained_s: Option<f64>,
    pub p90_maintained_s: Option<f64>,
    pub censored: usize,
    /// Mean is only reported when no survival time is censored.
    pub mean_maintained_s: Option<f64>,
    pub first_violations: usize,
    pub violation_count: usize,
}

pub fn viability(trace: &RunTrace) -> Vec<ServiceViability> {
    trace
        .minimum_levels
        .iter()
        .map(|(service, minimum)| {
            let first = trace
                .samples
                .iter()
                .find(|sample| sample.service_levels.get(service).copied().unwrap_or(1.0) + 1e-9 < *minimum);
            let violation_count = trace
                .samples
                .iter()
                .filter(|sample| sample.violations.iter().any(|v| v.constraint == *service))
                .count();
            ServiceViability {
                service: service.clone(),
                maintained_s: first.map(|sample| sample.at_s),
                censored: first.is_none(),
                first_violation_s: first.map(|sample| sample.at_s),
                violation_count,
                recovery_path: trace.recovery_paths.get(service).cloned(),
            }
        })
        .collect()
}

pub fn summarize_viability(traces: &[RunTrace]) -> Vec<ViabilitySummary> {
    let mut per_service: BTreeMap<String, Vec<ServiceViability>> = BTreeMap::new();
    for trace in traces {
        for entry in viability(trace) {
            per_service.entry(entry.service.clone()).or_default().push(entry);
        }
    }
    per_service
        .into_iter()
        .map(|(service, entries)| {
            let mut times: Vec<f64> = entries.iter().filter_map(|entry| entry.maintained_s).collect();
            times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let censored = entries.iter().filter(|entry| entry.censored).count();
            let mean = if censored == 0 && !times.is_empty() {
                Some(times.iter().sum::<f64>() / times.len() as f64)
            } else {
                None
            };
            ViabilitySummary {
                service,
                scenarios: entries.len(),
                min_maintained_s: times.first().copied(),
                median_maintained_s: quantile(&times, 0.5),
                p90_maintained_s: quantile(&times, 0.9),
                censored,
                mean_maintained_s: mean,
                first_violations: entries.iter().filter(|entry| entry.first_violation_s.is_some()).count(),
                violation_count: entries.iter().map(|entry| entry.violation_count).sum(),
            }
        })
        .collect()
}

fn quantile(sorted: &[f64], fraction: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let index = ((fraction * sorted.len() as f64).ceil() as usize).saturating_sub(1).min(sorted.len() - 1);
    Some(sorted[index])
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
    /// (resources with a local producer at or above `min_verification`, critical resources).
    pub local_coverage: (usize, usize),
}

pub fn independence(model: &Model, services: &[String], min_verification: Verification) -> IndependenceMetrics {
    let mut dependencies: BTreeSet<String> = BTreeSet::new();
    for service in services {
        let query = Query { goal: Goal::Service(service.clone()), min_verification, max_depth: DEFAULT_MAX_DEPTH };
        let report = graph::external_dependency_report(model, &query, 32);
        dependencies.extend(report.resources().into_iter().map(str::to_string));
    }
    let critical_external_dependencies: Vec<String> = dependencies.into_iter().collect();
    let mut covered = 0usize;
    for resource in &critical_external_dependencies {
        let local = model.producers.get(resource).is_some_and(|indices| {
            indices.iter().any(|index| model.processes[*index].verification >= min_verification)
        });
        if local {
            covered += 1;
        }
    }
    IndependenceMetrics {
        external_inputs: BTreeMap::new(),
        critical_external_dependencies: critical_external_dependencies.clone(),
        local_coverage: (covered, critical_external_dependencies.len()),
    }
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
    pub propagation_services: usize,
    pub propagation_assets: usize,
}

pub fn resilience(trace: &RunTrace) -> ResilienceMetrics {
    let mut service_loss: BTreeMap<String, f64> = BTreeMap::new();
    let mut recovery_time_s: BTreeMap<String, Option<f64>> = BTreeMap::new();
    let mut not_recovered = Vec::new();
    let mut first_violation_overall: Option<f64> = None;

    for (service, minimum) in &trace.minimum_levels {
        let mut loss = 0.0;
        for (index, sample) in trace.samples.iter().enumerate() {
            let end = trace.samples.get(index + 1).map_or(trace.horizon_s, |next| next.at_s);
            let dt = (end - sample.at_s).max(0.0);
            let level = sample.service_levels.get(service).copied().unwrap_or(1.0);
            loss += (minimum - level).max(0.0) * dt;
        }
        service_loss.insert(service.clone(), loss);

        let first = trace
            .samples
            .iter()
            .find(|sample| sample.service_levels.get(service).copied().unwrap_or(1.0) + 1e-9 < *minimum);
        let recovery = first.and_then(|violation| {
            trace
                .samples
                .iter()
                .filter(|sample| sample.at_s > violation.at_s)
                .find(|sample| sample.service_levels.get(service).copied().unwrap_or(1.0) + 1e-9 >= *minimum)
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
        assert_eq!(total, metrics.critical_external_dependencies.len());
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
