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
use poie_sim::timed::{CaseDoc, TimedCases, TimedOutcome, TimedRunner, TimedScenario, TimedVerdict};
use poie_viability::bridge::recovery_path_for_goal;
use poie_viability::method::Method;
use poie_viability::metrics::{
    aggregate_external_inputs, independence, resilience, summarize_viability, RunTrace, TimeSample,
};
use poie_viability::report::{
    ConfidenceInterval, InputHashes, MachineReport, MetricGroup, MetricRecord, RunRecord, ScenarioPartition,
    METRIC_DEFINITION_VERSION, REPORT_SCHEMA,
};
use poie_viability::verdict::Verdict;

const CONTRACT_VERSION: &str = "viability-expansion/0.2";
const CHECKABLE_CERTIFICATE: &str = "cargo run -p poie-ontology --bin poie-graph -- scenarios/base-01/cases.toml";

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

fn map_verdict(case: &CaseDoc, outcome: &TimedOutcome) -> Verdict {
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
        TimedVerdict::Blocked => Verdict::InfeasibleInModel {
            certificate: format!("capability-graph blocked: {}", outcome.detail),
            checkable: CHECKABLE_CERTIFICATE.to_string(),
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
) -> RunTrace {
    let horizon_s = start_s + 86_400.0 + deadline_s.unwrap_or(0.0);
    let initial_level =
        if matches!(outcome.verdict, TimedVerdict::Feasible) || outcome.violation.is_some() { 1.0 } else { 0.0 };
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
            samples.push(sample(completion, 1.0, label));
        }
    }
    let mut minimum_levels = BTreeMap::new();
    minimum_levels.insert(label.to_string(), 1.0);
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

    let started = Instant::now();
    let mut runs = Vec::new();
    let mut traces = Vec::new();
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
        let mut trace = build_trace(case, &label, start_s, deadline_s, &outcome, &query.goal);
        if !matches!(outcome.verdict, TimedVerdict::Feasible) {
            let path = recovery_path_for_goal(&patched_model, &query.goal, case.min_verification);
            trace.recovery_paths.insert(label.clone(), path);
        }
        let verdict = map_verdict(case, &outcome);
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
    for summary in summarize_viability(&traces) {
        let service = Some(summary.service.clone());
        if let Some(value) = summary.min_maintained_s {
            metrics.push(metric(MetricGroup::Viability, "min_maintained_s", service.clone(), value, "s", false));
        }
        if let Some(value) = summary.median_maintained_s {
            metrics.push(metric(MetricGroup::Viability, "median_maintained_s", service.clone(), value, "s", false));
        }
        if let Some(value) = summary.p90_maintained_s {
            metrics.push(metric(MetricGroup::Viability, "p90_maintained_s", service.clone(), value, "s", false));
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
        confidence_intervals: Vec::<ConfidenceInterval>::new(),
        runs,
        metrics,
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
