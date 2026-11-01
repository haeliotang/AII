//! Generates the VEC §7 report for the 20 pre-registered timed Base-01 v0.2
//! scenarios, and checks a committed report for regressions.
//!
//! Usage:
//!   poie-vec-base01 <cases.toml> --write <report.toml> <human.md>
//!   poie-vec-base01 <cases.toml> --check <report.toml>
//!
//! Each timed case becomes one report run: its verdict maps onto the VEC
//! classes, and a coarse structural-availability trace supplies the metrics.
//! The trace records availability at the evaluation boundaries (start, first
//! violation, completion); it is not a full time-domain simulation, and the
//! report says so.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use poie_ontology::graph::Goal;
use poie_ontology::model::{Model, Verification};
use poie_ontology::units::Quantity;
use poie_process::{BridgeClient, BridgeError, BridgeErrorClass};
use poie_sim::timed::{CaseDoc, TimedCases, TimedOutcome, TimedRunner, TimedScenario, TimedVerdict};
use poie_viability::audit::DecisionRecord;
use poie_viability::bridge::recovery_path_for_goal;
use poie_viability::method::Method;
use poie_viability::metrics::{
    aggregate_external_inputs, independence, resilience, summarize_runs, RunTrace, TimeSample, UndecidedRun,
};
use poie_viability::report::{
    ConfidenceInterval, InputHashes, MachineReport, MetricGroup, MetricRecord, ModelProvenance, RunRecord,
    ScenarioPartition, METRIC_DEFINITION_VERSION, REPORT_SCHEMA,
};
use poie_viability::verdict::Verdict;

const CONTRACT_VERSION: &str = "viability-expansion/0.2";
const POLICY_INSTANCE: &str = "earliest-start scheduling at case start (poie-sim timed 0.1.0)";
const TERMINAL_CONDITION: &str = "none declared: the timed Base-01 cases carry no terminal stock or storage requirement; the open service gap at H is reported per case (terminal group)";
const PROCESS_CONFIG: &str = "experiments/vec-base01/process.toml";
const MINIMUM_SERVICE_LEVEL: f64 = 0.8;

struct TraceLevels {
    nominal: f64,
    minimum: f64,
}

#[derive(serde::Deserialize)]
struct ProcessConfig {
    model: String,
    version: String,
    deadline_ms: u64,
    inlet_flow_m3_h: f64,
    turbidity_ntu: f64,
    filter_loading: f64,
}

fn load_process_config(path: &Path) -> Result<ProcessConfig, String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    toml::from_str(&text).map_err(|error| format!("cannot parse {}: {error}", path.display()))
}

fn process_verdict(error: &BridgeError, model: &str, deadline_ms: u64) -> Verdict {
    match error.class {
        BridgeErrorClass::Timeout => Verdict::Timeout {
            completed_scenarios: 0,
            budget: format!("process model {model} deadline {deadline_ms} ms"),
        },
        BridgeErrorClass::NumericalFailure | BridgeErrorClass::NonConvergence => Verdict::NumericalFailure {
            solver: model.to_string(),
            settings: "water-treatment bridge".to_string(),
            message: format!("{}: {}", error.class.as_str(), error.detail),
        },
        _ => Verdict::UnknownInput {
            unknowns: vec![format!("process model {model}: {}: {}", error.class.as_str(), error.detail)],
        },
    }
}

fn validity_text(ranges: &std::collections::BTreeMap<String, (f64, f64)>) -> String {
    ranges.iter().map(|(field, (low, high))| format!("{field} {low}-{high}")).collect::<Vec<_>>().join("; ")
}

fn parse_seconds(text: &str) -> f64 {
    text.parse::<Quantity>().map(|quantity| quantity.si).unwrap_or(0.0)
}

fn fnv1a(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn goal_label(goal: &Goal) -> String {
    poie_viability::bridge::goal_label(goal)
}

fn map_verdict(case: &CaseDoc, outcome: &TimedOutcome, start_s: f64) -> Verdict {
    match outcome.verdict {
        TimedVerdict::Feasible => Verdict::HeldOnTested {
            tested_scenarios: 1,
            method: Method::Enumerated,
            uncovered: format!("only the fixed trajectory of case {}", case.id),
        },
        TimedVerdict::TooLate => {
            let violation = outcome.violation.as_ref();
            Verdict::Counterexample {
                scenario_id: case.id.clone(),
                first_violation_s: violation.map(|violation| violation.at_s).unwrap_or(0.0),
                constraint: violation
                    .map(|violation| violation.constraint.clone())
                    .unwrap_or_else(|| "deadline".into()),
                state: outcome.detail.clone(),
            }
        }
        TimedVerdict::InsufficientResource | TimedVerdict::CapacityConflict => {
            let violation = outcome.violation.as_ref();
            Verdict::Counterexample {
                scenario_id: case.id.clone(),
                first_violation_s: violation.map(|violation| violation.at_s).unwrap_or(0.0),
                constraint: violation
                    .map(|violation| violation.constraint.clone())
                    .unwrap_or_else(|| "resource".into()),
                state: outcome.detail.clone(),
            }
        }
        // VEC §3.2 (2026-11-01): an enumerated method cannot issue
        // `infeasible_in_model`. A structural block at case start is a
        // counterexample for the tested policy on this trajectory.
        TimedVerdict::Blocked => Verdict::Counterexample {
            scenario_id: case.id.clone(),
            first_violation_s: start_s,
            constraint: "structural recovery route (capability graph, current snapshot)".to_string(),
            state: format!("blocked: {}", outcome.detail),
        },
        TimedVerdict::SearchLimit => {
            Verdict::Timeout { completed_scenarios: 0, budget: "capability-graph max depth".to_string() }
        }
        TimedVerdict::Unknown => Verdict::UnknownInput { unknowns: vec![format!("{}: {}", case.id, outcome.detail)] },
    }
}

fn sample(at_s: f64, level: f64, label: &str) -> TimeSample {
    let mut levels = BTreeMap::new();
    levels.insert(label.to_string(), level);
    TimeSample {
        at_s,
        service_levels: levels,
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

fn build_trace(
    case: &CaseDoc,
    label: &str,
    start_s: f64,
    deadline_s: Option<f64>,
    outcome: &TimedOutcome,
    goal: &Goal,
    levels: TraceLevels,
) -> RunTrace {
    let nominal_level = levels.nominal;
    let minimum_level = levels.minimum;
    let horizon_s = start_s + 86_400.0 + deadline_s.unwrap_or(0.0);
    let initial_level = if matches!(outcome.verdict, TimedVerdict::Feasible) || outcome.violation.is_some() {
        nominal_level
    } else {
        0.0
    };
    let mut samples = vec![sample(start_s, initial_level, label)];
    if let Some(violation) = &outcome.violation {
        if violation.at_s > start_s + 1e-6 {
            let mut dropped = sample(violation.at_s, 0.0, label);
            dropped.affected_services = vec![label.to_string()];
            dropped.affected_assets = match goal {
                Goal::Restore(asset) => vec![asset.clone()],
                Goal::Service(_) | Goal::Obtain { .. } => Vec::new(),
            };
            samples.push(dropped);
        } else {
            samples[0].service_levels.insert(label.to_string(), 0.0);
            samples[0].affected_services = vec![label.to_string()];
        }
    }
    if let Some(completion) = outcome.completion_s {
        if matches!(outcome.verdict, TimedVerdict::Feasible) && completion > start_s + 1e-6 {
            samples.push(sample(completion, nominal_level, label));
        }
    }
    let mut minimum_levels = BTreeMap::new();
    minimum_levels.insert(label.to_string(), minimum_level);
    RunTrace { scenario_id: case.id.clone(), horizon_s, minimum_levels, samples, recovery_paths: BTreeMap::new() }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cases_path) = args.first() else {
        eprintln!("usage: poie-vec-base01 <cases.toml> --write <report.toml> <human.md> | --check <report.toml>");
        return ExitCode::from(2);
    };
    let cases_path = PathBuf::from(cases_path);
    let (cases, scenario_path) = match TimedCases::load(&cases_path) {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
    };
    let scenario = match TimedScenario::load(&scenario_path) {
        Ok(scenario) => scenario,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
    };
    let model: Model = scenario.model.clone();
    let runner = TimedRunner::new(scenario);
    let process_config = match load_process_config(Path::new(PROCESS_CONFIG)) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
    };
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../python/poie_models/bridge_server.py");
    let bridge = BridgeClient::new(script);

    let started = Instant::now();
    let mut runs = Vec::new();
    let mut traces = Vec::new();
    let mut provenance: Option<ModelProvenance> = None;
    let mut process_calls: usize = 0;
    let mut process_failures: Vec<String> = Vec::new();
    let mut decisions: Vec<DecisionRecord> = Vec::new();
    let mut undecided: Vec<UndecidedRun> = Vec::new();
    for case in &cases.cases {
        let outcome = match runner.run(case) {
            Ok(outcome) => outcome,
            Err(error) => {
                eprintln!("error: {error}");
                return ExitCode::from(2);
            }
        };
        let (patched_model, query) = match runner.prepared_at(case) {
            Ok(prepared) => prepared,
            Err(error) => {
                eprintln!("error: {error}");
                return ExitCode::from(2);
            }
        };
        let label = goal_label(&query.goal);
        let start_s = parse_seconds(&case.start);
        let deadline_s = case.deadline.as_deref().filter(|text| *text != "prediction").map(parse_seconds);
        let parameters = poie_process::Parameters {
            inlet_flow_m3_h: process_config.inlet_flow_m3_h,
            turbidity_ntu: process_config.turbidity_ntu,
            filter_loading: process_config.filter_loading,
        };
        let model_id =
            poie_process::ModelId { id: process_config.model.clone(), version: process_config.version.clone() };
        let bridge_request = poie_process::BridgeRequest::new(model_id, parameters, process_config.deadline_ms);
        let evaluated = bridge.call(&bridge_request);
        let (nominal_level, process_error) = match &evaluated {
            Ok((response, _validation)) => {
                let validity = response
                    .validity
                    .as_ref()
                    .map(|validity| validity_text(&validity.ranges))
                    .unwrap_or_else(|| "not declared".to_string());
                provenance = Some(ModelProvenance {
                    id: response.model.id.clone(),
                    version: response.model.version.clone(),
                    synthetic: response.synthetic,
                    validity,
                });
                (response.service_level.unwrap_or(0.0), None)
            }
            Err(error) => (0.0, Some(error.clone())),
        };
        let mut trace = build_trace(
            case,
            &label,
            start_s,
            deadline_s,
            &outcome,
            &query.goal,
            TraceLevels { nominal: nominal_level, minimum: MINIMUM_SERVICE_LEVEL },
        );
        if !matches!(outcome.verdict, TimedVerdict::Feasible) {
            let path = recovery_path_for_goal(&patched_model, &query.goal, case.min_verification);
            trace.recovery_paths.insert(label.clone(), path);
        }
        let mut verdict = map_verdict(case, &outcome, start_s);
        if let Some(error) = &process_error {
            verdict = process_verdict(error, &process_config.model, process_config.deadline_ms);
            process_failures.push(error.class.as_str().to_string());
        }
        process_calls += 1;
        decisions.push(DecisionRecord {
            scenario_id: case.id.clone(),
            decided_at_s: start_s,
            observed_up_to_s: start_s,
            policy_instance: POLICY_INSTANCE.to_string(),
        });
        if matches!(verdict, Verdict::Timeout { .. } | Verdict::NumericalFailure { .. } | Verdict::UnknownInput { .. })
        {
            undecided.push(UndecidedRun {
                scenario_id: case.id.clone(),
                stopped_at_s: start_s,
                class: verdict.kind().as_str().to_string(),
                services: vec![label.clone()],
            });
        }
        runs.push(RunRecord {
            scenario_id: case.id.clone(),
            method: Method::Enumerated,
            seed: None,
            budget_used_s: 0.0,
            verdict,
        });
        traces.push(trace);
    }

    let mut metrics = Vec::new();
    // VEC §4 (2026-11-01): undecided runs stay in the distribution, censored at
    // their stop time; their traces are not counted as observed failures.
    let undecided_ids: Vec<&str> = undecided.iter().map(|run| run.scenario_id.as_str()).collect();
    let decided_traces: Vec<RunTrace> =
        traces.iter().filter(|trace| !undecided_ids.contains(&trace.scenario_id.as_str())).cloned().collect();
    for summary in summarize_runs(&decided_traces, &undecided) {
        let service = Some(summary.service.clone());
        if let Some(value) = summary.min_maintained_s {
            metrics.push(metric(MetricGroup::Viability, "min_maintained_s", service.clone(), value, "s", false));
        }
        // Product-limit quantiles; a quantile beyond the follow-up is reported
        // as a censored value "> follow-up" (VEC §4.1).
        for (name, value) in
            [("median_maintained_s", summary.median_maintained_s), ("p90_maintained_s", summary.p90_maintained_s)]
        {
            match value {
                Some(value) => metrics.push(metric(MetricGroup::Viability, name, service.clone(), value, "s", false)),
                None => {
                    metrics.push(metric(MetricGroup::Viability, name, service.clone(), summary.follow_up_s, "s", true))
                }
            }
        }
        if summary.undecided > 0 {
            metrics.push(metric(
                MetricGroup::Viability,
                "undecided_count",
                service.clone(),
                summary.undecided as f64,
                "count",
                false,
            ));
            if let Some(value) = summary.worst_case_min_maintained_s {
                metrics.push(metric(
                    MetricGroup::Viability,
                    "worst_case_min_maintained_s",
                    service.clone(),
                    value,
                    "s",
                    false,
                ));
            }
        }
        if let Some(value) = summary.mean_maintained_s {
            metrics.push(metric(MetricGroup::Viability, "mean_maintained_s", service.clone(), value, "s", false));
        }
        metrics.push(metric(
            MetricGroup::Viability,
            "censored_count",
            service.clone(),
            summary.censored as f64,
            "count",
            false,
        ));
        if summary.censored > 0 {
            metrics.push(metric(MetricGroup::Viability, "mean_suppressed_by_censoring", service, 1.0, "flag", true));
        }
    }
    for trace in &traces {
        let result = resilience(trace);
        for (service, gap) in &result.gap_at_horizon {
            match gap {
                Some(value) => metrics.push(metric(
                    MetricGroup::Terminal,
                    "gap_at_horizon",
                    Some(service.clone()),
                    *value,
                    "1",
                    false,
                )),
                None => {
                    metrics.push(metric(MetricGroup::Terminal, "gap_at_horizon", Some(service.clone()), 0.0, "1", true))
                }
            }
        }
        for (service, loss) in &result.service_loss {
            metrics.push(metric(
                MetricGroup::Resilience,
                "service_loss",
                Some(service.clone()),
                *loss,
                "level·s",
                false,
            ));
        }
        for (service, recovery) in &result.recovery_time_s {
            match recovery {
                Some(value) => metrics.push(metric(
                    MetricGroup::Resilience,
                    "recovery_time_s",
                    Some(service.clone()),
                    *value,
                    "s",
                    false,
                )),
                None => metrics.push(metric(
                    MetricGroup::Resilience,
                    "recovery_time_s",
                    Some(service.clone()),
                    0.0,
                    "s",
                    true,
                )),
            }
        }
    }
    let external_inputs = aggregate_external_inputs(&traces);
    let independence_metrics = independence(&model, &["potable-water".to_string()], Verification::Demonstrated);
    for (resource, amount) in &external_inputs {
        metrics.push(metric(MetricGroup::Independence, "external_input", Some(resource.clone()), *amount, "SI", false));
    }
    metrics.push(metric(
        MetricGroup::Independence,
        "critical_external_dependencies",
        None,
        independence_metrics.critical_external_dependencies.len() as f64,
        "count",
        false,
    ));
    metrics.push(metric(
        MetricGroup::Independence,
        "local_coverage_covered",
        None,
        independence_metrics.local_coverage.0 as f64,
        "count",
        false,
    ));
    metrics.push(metric(
        MetricGroup::Independence,
        "local_coverage_critical_resources",
        None,
        independence_metrics.local_coverage.1 as f64,
        "count",
        false,
    ));
    for resource in &independence_metrics.covered_resources {
        metrics.push(metric(
            MetricGroup::Independence,
            "local_coverage_resource",
            Some(resource.clone()),
            1.0,
            "flag",
            false,
        ));
    }
    let model_service_level = traces
        .iter()
        .filter_map(|trace| trace.samples.first().and_then(|sample| sample.service_levels.values().next().copied()))
        .sum::<f64>()
        / traces.len().max(1) as f64;
    metrics.push(metric(MetricGroup::Process, "water_service_level", None, model_service_level, "1", false));
    metrics.push(metric(MetricGroup::Process, "calls", None, process_calls as f64, "count", false));
    metrics.push(metric(MetricGroup::Process, "failure_count", None, process_failures.len() as f64, "count", false));
    if let Some(provenance) = &provenance {
        metrics.push(metric(MetricGroup::Process, "synthetic", None, provenance.synthetic as u8 as f64, "flag", false));
    }
    for class in &process_failures {
        metrics.push(metric(MetricGroup::Process, &format!("failure.{class}"), None, 1.0, "count", false));
    }

    let file = |path: &Path| std::fs::read_to_string(path).unwrap_or_default();
    let report = MachineReport {
        schema: REPORT_SCHEMA.to_string(),
        contract_version: CONTRACT_VERSION.to_string(),
        input_hashes: InputHashes {
            initial_state: fnv1a(&format!("{}-timed", file(&scenario_path).len())),
            scenario_set: fnv1a(&file(&scenario_path)),
            models: fnv1a(&file(&cases_path.with_file_name("../../scenarios/base-01/model.toml"))),
            policy: fnv1a(&format!("case-start-detection:{}", cases_path.display())),
            contract_version: CONTRACT_VERSION.to_string(),
        },
        partition: ScenarioPartition {
            development: cases.cases.len(),
            holdout: 0,
            holdout_viewed_in_development: false,
        },
        method: Method::Enumerated,
        horizon_s: 86_400.0,
        seeds: Vec::new(),
        solver_version: "poie-viability 0.1.0 + poie-sim timed 0.1.0".to_string(),
        solver_settings: "finite fixed-trajectory set (20 pre-registered cases); no random sampling".to_string(),
        actual_cost_s: started.elapsed().as_secs_f64(),
        metric_definition_version: METRIC_DEFINITION_VERSION.to_string(),
        registered_in: format!(
            "scenarios/base-01-timed/cases.toml ({} pre-registered cases; fnv1a {})",
            cases.cases.len(),
            fnv1a(&file(&cases_path))
        ),
        seed_sets_drawn: 0,
        terminal_condition: TERMINAL_CONDITION.to_string(),
        confidence_intervals: Vec::<ConfidenceInterval>::new(),
        model_provenance: provenance.into_iter().collect(),
        runs,
        metrics,
        decisions,
    };
    if let Err(errors) = report.validate() {
        for error in errors {
            eprintln!("error: {error}");
        }
        return ExitCode::FAILURE;
    }

    let mode: Option<(&str, Vec<String>)> = if args.len() > 1 {
        match args[1].as_str() {
            "--write" => Some(("write", args[2..].to_vec())),
            "--check" => Some(("check", args[2..].to_vec())),
            other => {
                eprintln!("unexpected argument {other:?}");
                return ExitCode::from(2);
            }
        }
    } else {
        None
    };
    match mode {
        Some(("write", paths)) if paths.len() == 2 => {
            let toml_path = PathBuf::from(&paths[0]);
            let markdown_path = PathBuf::from(&paths[1]);
            let text = match report.to_toml() {
                Ok(text) => text,
                Err(error) => {
                    eprintln!("error: {error}");
                    return ExitCode::FAILURE;
                }
            };
            if let Err(error) = std::fs::write(&toml_path, text) {
                eprintln!("error: cannot write {}: {error}", toml_path.display());
                return ExitCode::FAILURE;
            }
            let summary = report.human_summary().expect("validated report has a summary");
            if let Err(error) = std::fs::write(&markdown_path, summary) {
                eprintln!("error: cannot write {}: {error}", markdown_path.display());
                return ExitCode::FAILURE;
            }
            println!("wrote {} runs to {} and {}", report.runs.len(), toml_path.display(), markdown_path.display());
            ExitCode::SUCCESS
        }
        Some(("check", paths)) if paths.len() == 1 => {
            let committed_path = PathBuf::from(&paths[0]);
            let committed = match std::fs::read_to_string(&committed_path)
                .map_err(|error| format!("cannot read {}: {error}", committed_path.display()))
                .and_then(|text| MachineReport::from_toml(&text).map_err(|error| error.to_string()))
            {
                Ok(report) => report,
                Err(error) => {
                    eprintln!("error: {error}");
                    return ExitCode::FAILURE;
                }
            };
            let mut regenerated = report.clone();
            regenerated.actual_cost_s = committed.actual_cost_s;
            if regenerated == committed {
                println!(
                    "report check: {} runs, {} metrics — replay matches",
                    committed.runs.len(),
                    committed.metrics.len()
                );
                ExitCode::SUCCESS
            } else {
                println!("report check: regenerated report differs from the committed report");
                ExitCode::FAILURE
            }
        }
        _ => {
            eprintln!("usage: poie-vec-base01 <cases.toml> --write <report.toml> <human.md> | --check <report.toml>");
            ExitCode::from(2)
        }
    }
}

fn metric(
    group: MetricGroup,
    name: &str,
    service: Option<String>,
    value: f64,
    unit: &str,
    censored: bool,
) -> MetricRecord {
    MetricRecord { group, name: name.to_string(), service, value, unit: unit.to_string(), censored }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(class: BridgeErrorClass) -> BridgeError {
        BridgeError { class, detail: "injected".into() }
    }

    #[test]
    fn process_failure_classes_map_to_distinct_report_verdicts() {
        assert!(matches!(
            process_verdict(&error(BridgeErrorClass::Timeout), "water-treatment", 5_000),
            Verdict::Timeout { .. }
        ));
        for class in [BridgeErrorClass::NumericalFailure, BridgeErrorClass::NonConvergence] {
            assert!(matches!(
                process_verdict(&error(class), "water-treatment", 5_000),
                Verdict::NumericalFailure { .. }
            ));
        }
        for class in [
            BridgeErrorClass::OutOfValidity,
            BridgeErrorClass::DimensionMismatch,
            BridgeErrorClass::Conservation,
            BridgeErrorClass::ModelMismatch,
            BridgeErrorClass::Protocol,
            BridgeErrorClass::Crashed,
        ] {
            assert!(matches!(process_verdict(&error(class), "water-treatment", 5_000), Verdict::UnknownInput { .. }));
        }
    }
}
