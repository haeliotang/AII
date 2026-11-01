//! Issue #110 (AI-preview review P12, P21, P22, P24, P25): statistics and
//! reporting rules of the Viability & Expansion Contract.
//!
//! Each test is named after the contract clause it checks and reproduces a
//! counterexample from docs/status/ai-preview/contract-review-c.zh-CN.md.

use std::collections::BTreeMap;
use std::path::PathBuf;

use poie_ontology::model::{Model, Patch, Verification};
use poie_viability::audit::DecisionRecord;
use poie_viability::method::Method;
use poie_viability::metrics::{
    independence, resilience, summarize_runs, summarize_viability, viability, RunTrace, TimeSample, UndecidedRun,
};
use poie_viability::report::{
    ConfidenceInterval, InputHashes, MachineReport, MetricGroup, MetricRecord, RunRecord, ScenarioPartition,
    METRIC_DEFINITION_VERSION, REPORT_SCHEMA,
};
use poie_viability::verdict::Verdict;

const HOUR: f64 = 3600.0;

fn sample(at_s: f64, level: f64) -> TimeSample {
    TimeSample {
        at_s,
        service_levels: BTreeMap::from([("water".to_string(), level)]),
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

/// A trace that fails at `fail_h` (or never, within `horizon_h`).
fn survival_trace(id: usize, horizon_h: f64, fail_h: Option<f64>) -> RunTrace {
    let mut samples = vec![sample(0.0, 1.0)];
    if let Some(fail) = fail_h {
        samples.push(sample(fail * HOUR, 0.0));
    }
    RunTrace {
        scenario_id: format!("s{id}"),
        horizon_s: horizon_h * HOUR,
        minimum_levels: BTreeMap::from([("water".to_string(), 1.0)]),
        samples,
        recovery_paths: BTreeMap::new(),
    }
}

// VEC §4.1 (review C-02, counterexample 1): 18 of 20 scenarios are censored at
// H = 72 h and two fail at 6 h and 8 h. The median of the censored empirical
// distribution lies beyond the horizon; it must not be reported as 6 h.
#[test]
fn vec4_1_median_with_heavy_censoring_is_beyond_the_horizon() {
    let mut traces: Vec<RunTrace> = (0..18).map(|index| survival_trace(index, 72.0, None)).collect();
    traces.push(survival_trace(18, 72.0, Some(6.0)));
    traces.push(survival_trace(19, 72.0, Some(8.0)));
    let summary = &summarize_viability(&traces)[0];
    assert_eq!(summary.censored, 18);
    assert_eq!(summary.min_maintained_s, Some(6.0 * HOUR));
    assert_eq!(summary.median_maintained_s, None, "median must be '> H', not the event-only median: {summary:?}");
    assert_eq!(summary.p90_maintained_s, None, "p90 must be '> H': {summary:?}");
}

// VEC §4.1 (review C-02): quantiles follow the product-limit (Kaplan–Meier)
// estimator over all runs. Ten scenarios, failures at 1..5 h and five censored
// at H = 10 h: the estimated survival reaches 0.5 at 5 h, so the median is 5 h
// (the event-only median would be 3 h).
#[test]
fn vec4_1_quantiles_follow_the_product_limit_estimator() {
    let mut traces: Vec<RunTrace> = (1..=5).map(|hour| survival_trace(hour, 10.0, Some(hour as f64))).collect();
    traces.extend((6..=10).map(|index| survival_trace(index, 10.0, None)));
    let summary = &summarize_viability(&traces)[0];
    assert_eq!(summary.median_maintained_s, Some(5.0 * HOUR), "{summary:?}");
    assert_eq!(summary.p90_maintained_s, None, "{summary:?}");
}

fn report(method: Method, runs: Vec<RunRecord>, intervals: Vec<ConfidenceInterval>) -> MachineReport {
    MachineReport {
        schema: REPORT_SCHEMA.to_string(),
        contract_version: "viability-expansion/0.2-draft".into(),
        input_hashes: InputHashes {
            initial_state: "hash-state".into(),
            scenario_set: "hash-scenarios".into(),
            models: "hash-models".into(),
            policy: "hash-policy".into(),
            contract_version: "viability-expansion/0.2-draft".into(),
        },
        partition: ScenarioPartition { development: runs.len(), holdout: 0, holdout_viewed_in_development: false },
        method,
        horizon_s: 10_000.0,
        seeds: if method == Method::Sampled { (1..=runs.len() as u64).collect() } else { Vec::new() },
        solver_version: "poie-viability 0.1.0".into(),
        solver_settings: "default".into(),
        actual_cost_s: 1.0,
        metric_definition_version: METRIC_DEFINITION_VERSION.into(),
        // Fields added by the fix (VEC §2.1, §7); the pre-registered
        // assertions below are unchanged.
        registered_in: "PREREG.md".into(),
        seed_sets_drawn: usize::from(method == Method::Sampled),
        terminal_condition: "none: no storage in scope".into(),
        confidence_intervals: intervals,
        model_provenance: Vec::new(),
        decisions: runs
            .iter()
            .map(|run| DecisionRecord {
                scenario_id: run.scenario_id.clone(),
                decided_at_s: 0.0,
                observed_up_to_s: 0.0,
                policy_instance: "policy@v1".into(),
            })
            .collect(),
        runs,
        metrics: vec![
            MetricRecord {
                group: MetricGroup::Viability,
                name: "censored_count".into(),
                service: Some("water".into()),
                value: 0.0,
                unit: "count".into(),
                censored: false,
            },
            MetricRecord {
                group: MetricGroup::Terminal,
                name: "gap_at_horizon".into(),
                service: Some("water".into()),
                value: 0.0,
                unit: "1".into(),
                censored: false,
            },
        ],
    }
}

fn held_run(id: &str, method: Method) -> RunRecord {
    RunRecord {
        scenario_id: id.into(),
        method,
        seed: if method == Method::Sampled { Some(1) } else { None },
        budget_used_s: 0.1,
        verdict: Verdict::HeldOnTested { tested_scenarios: 1, method, uncovered: "untested disturbances".into() },
    }
}

// VEC §3.2 (review C-07, counterexample 3): an `enumerated` method can at most
// support "holds on the finite W / counterexample found"; it cannot issue the
// proof-strength class `infeasible_in_model`.
#[test]
fn vec3_2_enumerated_method_cannot_issue_infeasible_in_model() {
    let runs = vec![RunRecord {
        scenario_id: "T13".into(),
        method: Method::Enumerated,
        seed: None,
        budget_used_s: 0.1,
        verdict: Verdict::InfeasibleInModel {
            certificate: "capability-graph blocked".into(),
            checkable: "poie-graph cases.toml".into(),
        },
    }];
    let errors = report(Method::Enumerated, runs, Vec::new()).validate().unwrap_err();
    assert!(errors.iter().any(|error| error.to_string().contains("infeasible_in_model")), "{errors:?}");
}

// VEC §3.1 (review C-06): a counterexample refutes the tested policy, not the
// existence of any policy (∃π); the wording must say so.
#[test]
fn vec3_1_counterexample_names_the_tested_policy() {
    let verdict = Verdict::Counterexample {
        scenario_id: "w1".into(),
        first_violation_s: 20.0 * HOUR,
        constraint: "potable-water >= 0.8".into(),
        state: "bus failed".into(),
    };
    assert!(verdict.canonical_summary().contains("tested policy"), "{}", verdict.canonical_summary());
}

// VEC §7 (review C-09, counterexample 3): with an empty (or viewed) holdout
// set the result may only be worded as a development-set result.
#[test]
fn vec7_empty_holdout_is_reported_as_development_only() {
    let runs = vec![held_run("a", Method::Enumerated), held_run("b", Method::Enumerated)];
    let summary = report(Method::Enumerated, runs, Vec::new()).human_summary().unwrap();
    assert!(summary.contains("development-set result"), "{summary}");
}

// VEC §7 (review C-09, counterexample 3): confidence intervals do not apply to
// an enumerated finite set; the report says "not applicable" instead of
// silently omitting a mandatory item.
#[test]
fn vec7_enumerated_reports_mark_confidence_intervals_not_applicable() {
    let runs = vec![held_run("a", Method::Enumerated)];
    let summary = report(Method::Enumerated, runs, Vec::new()).human_summary().unwrap();
    assert!(summary.contains("confidence interval: not applicable"), "{summary}");
}

// VEC §7: a sampled report without any confidence interval is incomplete.
#[test]
fn vec7_sampled_reports_need_a_confidence_interval() {
    let runs = vec![held_run("a", Method::Sampled), held_run("b", Method::Sampled)];
    let errors = report(Method::Sampled, runs, Vec::new()).validate().unwrap_err();
    assert!(errors.iter().any(|error| error.to_string().contains("confidence interval")), "{errors:?}");
}

// VEC §4.2 (review C-10, counterexample 3): the critical-resource denominator
// is derived from the recovery routes of the critical services, not chosen by
// hand and not restricted to purchased resources. In Base-01 the only
// candidate route for potable water consumes sleeve-p1, grease and
// electric-energy, so m = 3 (the old denominator listed only the two external
// resources, which by construction have no local producer, so n was always 0).
#[test]
fn vec4_2_coverage_denominator_is_derived_from_the_route_resources() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01/model.toml");
    let model = Model::load(&path).unwrap();
    let metrics = independence(&model, &["potable-water".to_string()], Verification::Demonstrated);
    assert_eq!(metrics.local_coverage.1, 3, "{metrics:?}");
    assert_eq!(metrics.local_coverage.0, 1, "sleeve-p1 has a closed local route: {metrics:?}");
}

// ---------------------------------------------------------------------------
// Added with the fix: these use interfaces introduced by it.
// ---------------------------------------------------------------------------

// VEC §4 (review C-02, counterexample 2): runs that stop undecided are not
// dropped. Five hard scenarios time out at 2 h; fifteen easy ones hold to H.
// Dropping the timeouts would report "0 failures"; instead they are censored
// at 2 h, counted, and the worst-case sensitivity treats them as failures.
#[test]
fn vec4_undecided_runs_stay_in_the_distribution() {
    let traces: Vec<RunTrace> = (0..15).map(|index| survival_trace(index, 72.0, None)).collect();
    let undecided: Vec<UndecidedRun> = (15..20)
        .map(|index| UndecidedRun {
            scenario_id: format!("s{index}"),
            stopped_at_s: 2.0 * HOUR,
            class: "timeout".into(),
            services: Vec::new(),
        })
        .collect();
    let summary = &summarize_runs(&traces, &undecided)[0];
    assert_eq!(summary.scenarios, 20);
    assert_eq!(summary.undecided, 5);
    assert_eq!(summary.censored, 20);
    assert_eq!(summary.mean_maintained_s, None);
    assert_eq!(summary.min_maintained_s, None);
    assert_eq!(summary.worst_case_min_maintained_s, Some(2.0 * HOUR));
    assert_eq!(summary.worst_case_median_maintained_s, None, "5/20 failures do not reach the median");
}

// VEC §4 (review C-02, counterexample 3): a missing reading censors the run at
// that time instead of extending survival to the horizon.
#[test]
fn vec4_missing_reading_censors_instead_of_counting_as_full_service() {
    let mut trace = survival_trace(0, 10.0, None);
    let mut gap = trace.samples[0].clone();
    gap.at_s = 3.0 * HOUR;
    gap.service_levels.clear();
    trace.samples.push(gap);
    let entry = &viability(&trace)[0];
    assert!(entry.censored);
    assert_eq!(entry.censored_at_s, Some(3.0 * HOUR));
    let summary = &summarize_viability(std::slice::from_ref(&trace))[0];
    assert_eq!(summary.follow_up_s, 3.0 * HOUR);
    let metrics = resilience(&trace);
    assert_eq!(metrics.unobserved_s["water"], 7.0 * HOUR);
    assert_eq!(metrics.gap_at_horizon["water"], None, "the state at H is unknown");
}

// VEC §4.3 / §2.1 (review C-03, counterexample 2): "fail late and never
// recover" must be visible next to the loss integral as an open gap at H.
#[test]
fn vec4_3_open_gap_at_the_horizon_is_reported() {
    let late = survival_trace(0, 100.0, Some(95.0));
    let metrics = resilience(&late);
    assert_eq!(metrics.gap_at_horizon["water"], Some(1.0));
    assert_eq!(metrics.not_recovered, vec!["water"]);
}

// VEC §4.2 (review C-10, counterexample 1): a local route that needs a cutting
// insert which is out of stock does not cover sleeve-p1.
#[test]
fn vec4_2_a_route_that_cannot_close_does_not_cover() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01/model.toml");
    let model = Model::load(&path).unwrap();
    let covered = independence(&model, &["potable-water".to_string()], Verification::Demonstrated);
    assert_eq!(covered.covered_resources, vec!["sleeve-p1".to_string()]);
    assert!(covered.coverage_external_inputs["sleeve-p1"].contains(&"cutting-insert".to_string()));

    let mut patch = Patch::default();
    patch.lot_quantity.insert("lot-insert".into(), "0 item".into());
    let no_inserts = model.patched(&patch).unwrap();
    let metrics = independence(&no_inserts, &["potable-water".to_string()], Verification::Demonstrated);
    assert!(metrics.covered_resources.is_empty(), "{metrics:?}");
    assert_eq!(metrics.local_coverage, (0, 3));
}
