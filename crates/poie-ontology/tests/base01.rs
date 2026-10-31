//! Runs the pre-registered Base-01 cases and checks accounting invariants
//! that the case expectations alone do not cover.

use std::path::PathBuf;

use poie_ontology::graph::{self, BlockerKind, Source, Verdict};
use poie_ontology::model::MassBalance;
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
