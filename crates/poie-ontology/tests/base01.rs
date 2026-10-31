//! Runs the pre-registered Base-01 cases and checks accounting invariants
//! that the case expectations alone do not cover.

use std::path::PathBuf;

use poie_ontology::graph::{self, BlockerKind, Source, Verdict};
use poie_ontology::model::{MassBalance, Verification};
use poie_ontology::scenario::{CaseRun, Scenario};

fn scenario() -> Scenario {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01/cases.toml");
    Scenario::load(&path).expect("Base-01 loads")
}

fn run<'a>(s: &'a Scenario, id: &str) -> CaseRun<'a> {
    let case = s.cases.iter().find(|c| c.id == id).unwrap_or_else(|| panic!("case {id}"));
    s.run(case).unwrap()
}

fn consumed(r: &CaseRun, resource: &str) -> f64 {
    r.result.steps.iter().flat_map(|s| &s.allocations).filter(|a| a.resource == resource).map(|a| a.si).sum()
}

#[test]
fn every_case_matches_its_preregistered_expectation() {
    let s = scenario();
    let mut failures = Vec::new();
    for case in &s.cases {
        let r = s.run(case).unwrap();
        if !r.mismatches.is_empty() {
            failures.push(format!("{}: {:?}\n{}", case.id, r.mismatches, graph::render(&r.model, &r.result)));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn base_model_is_fully_mass_balanced() {
    let s = scenario();
    for (p, b) in &s.model.mass_balance {
        assert!(matches!(b, MassBalance::Balanced { .. }), "{p}: {b:?}");
    }
}

#[test]
fn quarantined_lot_is_never_allocated() {
    let s = scenario();
    let r = run(&s, "S1");
    assert_eq!(r.result.verdict, Verdict::Candidate);
    let lots: Vec<&Source> = r.result.steps.iter().flat_map(|s| &s.allocations).map(|a| &a.source).collect();
    assert!(!lots.contains(&&Source::Lot("lot-sleeve-02".into())), "{lots:?}");
}

#[test]
fn partial_stock_is_used_before_manufacturing() {
    let s = scenario();
    let r = run(&s, "S8");
    // One sleeve from stock, exactly one machined: 1.2 kg bronze, not 2.4 kg.
    assert!((consumed(&r, "bronze-bar") - 1.2).abs() < 1e-9);
    let turn = r.result.steps.iter().find(|s| s.process == "turn-sleeve-p1").unwrap();
    assert_eq!(turn.runs, 1);
}

#[test]
fn energy_is_accounted_across_the_whole_route() {
    let s = scenario();
    let r = run(&s, "S1");
    assert!((consumed(&r, "electric-energy") - 2.6 * 3.6e6).abs() < 1e-3);
}

#[test]
fn common_cause_is_reported_at_the_root() {
    let s = scenario();
    let r = run(&s, "S3");
    for b in r.result.root_causes() {
        assert_eq!(b.subject, "bus-1", "{b:?}");
        assert!(b.path.iter().any(|f| f == "restore:charger-1"), "{:?}", b.path);
    }
}

#[test]
fn results_are_deterministic() {
    let s = scenario();
    for case in &s.cases {
        let a = s.run(case).unwrap();
        let b = s.run(case).unwrap();
        assert_eq!(graph::render(&a.model, &a.result), graph::render(&b.model, &b.result), "{}", case.id);
    }
}

#[test]
fn depth_limit_is_search_limit_not_infeasible() {
    let s = scenario();
    let mut case = s.cases.iter().find(|c| c.id == "S1").unwrap().clone();
    case.max_depth = Some(3);
    let r = s.run(&case).unwrap();
    assert_eq!(r.result.verdict, Verdict::SearchLimit);
}

#[test]
fn or_support_breaks_the_common_cause_cycle() {
    let s = scenario();
    let r = run(&s, "S11");
    assert_eq!(r.result.verdict, Verdict::Candidate);
    let route: Vec<&str> = r.result.steps.iter().map(|s| s.process.as_str()).collect();
    assert!(route.contains(&"replace-bus-breaker"), "{route:?}");
    assert!(route.contains(&"replace-sleeve-p1"), "{route:?}");
}

#[test]
fn unusable_alternatives_are_not_treated_as_support() {
    let s = scenario();
    let r = run(&s, "S12");
    assert_eq!(r.result.verdict, Verdict::Blocked);
    assert!(
        r.result.root_causes().any(|b| b.kind == BlockerKind::AssetUnavailable && b.subject == "emergency-bus"),
        "{:?}",
        r.result.root_causes().collect::<Vec<_>>()
    );
}

#[test]
fn multi_source_route_stays_within_each_source_stock() {
    let s = scenario();
    let r = run(&s, "S13");
    assert_eq!(r.result.verdict, Verdict::Candidate);
    // Exactly one machined raw sleeve (1.2 kg bronze) and one blank are consumed.
    assert!((consumed(&r, "bronze-bar") - 1.2).abs() < 1e-9);
    assert!((consumed(&r, "sleeve-blank") - 1.0).abs() < 1e-9);
    let sleeve_steps: Vec<&str> = r
        .result
        .steps
        .iter()
        .filter(|s| s.outputs.iter().any(|(r, _)| r == "sleeve-p1"))
        .map(|s| s.process.as_str())
        .collect();
    assert_eq!(sleeve_steps, ["inspect-sleeve-p1", "inspect-blank"]);
}

#[test]
fn degraded_policy_marks_the_step_and_caps_occupations() {
    let s = scenario();
    let r = run(&s, "S15");
    assert_eq!(r.result.verdict, Verdict::Candidate);
    assert_eq!(r.result.steps[0].degraded, vec!["lathe-1".to_string()]);
    let r = run(&s, "S16");
    assert_eq!(r.result.verdict, Verdict::Blocked);
    assert!(
        r.result.root_causes().any(|b| b.kind == BlockerKind::DegradedRestriction && b.subject == "lathe-1"),
        "{:?}",
        r.result.root_causes().collect::<Vec<_>>()
    );
}

#[test]
fn yield_and_rework_are_accounted() {
    let s = scenario();
    let r = run(&s, "S18");
    assert_eq!(r.result.verdict, Verdict::Candidate);
    let route: Vec<&str> = r.result.steps.iter().map(|s| s.process.as_str()).collect();
    assert_eq!(route, ["rework-sleeve-p1", "inspect-sleeve-yield"]);
    // Two rejects reworked (2 x 1 kWh), then two inspection runs (2 x 0.1 kWh).
    assert!((consumed(&r, "sleeve-p1-reject") - 2.0).abs() < 1e-9);
    assert!((consumed(&r, "electric-energy") - 1.2 * 3.6e6).abs() < 1e-3);
    let inspect = r.result.steps.iter().find(|s| s.process == "inspect-sleeve-yield").unwrap();
    assert!(
        inspect.outputs.iter().any(|(res, q)| res == "sleeve-p1-reject" && (*q - 1.0).abs() < 1e-9),
        "{:?}",
        inspect.outputs
    );
}

#[test]
fn capability_level_limits_the_route_verification() {
    let s = scenario();
    let r = run(&s, "S19");
    assert_eq!(r.result.verdict, Verdict::Blocked);
    assert!(
        r.result
            .root_causes()
            .any(|b| b.kind == BlockerKind::CapabilityBelowVerification && b.subject == "sleeve-replacement"),
        "{:?}",
        r.result.root_causes().collect::<Vec<_>>()
    );
    let r = run(&s, "S20");
    assert_eq!(r.result.verdict, Verdict::Candidate);
    assert_eq!(r.result.route_verification(), Some(Verification::Modeled));
}

#[test]
fn enumeration_returns_distinct_routes_with_summaries() {
    let s = scenario();
    let case = s.cases.iter().find(|c| c.id == "S21").unwrap();
    let routes = s.enumerate(case, 8).unwrap();
    assert_eq!(routes.len(), 2, "{routes:?}");
    assert!(routes.iter().any(|r| r.external_dependencies.iter().any(|d| d == "bronze-bar")));
    assert!(routes.iter().any(|r| r.external_dependencies.iter().any(|d| d == "sleeve-blank")));
    assert!(routes.iter().all(|r| r.verification.is_some()));
    let r = run(&s, "S22");
    assert_eq!(r.routes.len(), 1, "baseline has exactly one structural route");
}

#[test]
fn external_dependencies_are_traced_across_routes_and_root_causes() {
    let s = scenario();
    let case = s.cases.iter().find(|c| c.id == "S23").unwrap();
    let report = s.external_dependency_report(case, 8).unwrap();
    assert_eq!(report.candidate_routes, 2);
    for dependency in ["bronze-bar", "cutting-insert", "sleeve-blank", "grease", "electric-energy"] {
        assert!(report.resources().contains(&dependency), "missing {dependency}: {:?}", report.resources());
    }
    let case = s.cases.iter().find(|c| c.id == "S24").unwrap();
    let report = s.external_dependency_report(case, 8).unwrap();
    assert_eq!(report.candidate_routes, 0);
    assert!(report.dependencies.iter().any(|d| d.resource == "cutting-insert" && d.in_root_causes));
}
