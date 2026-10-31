//! Structure/temporal bridge (VEC §3.3, M6.5).
//!
//! When a timed trace shows a service violation, the recovery question is
//! answered by the structural capability graph: callers have the model patched
//! to the violation state (the simulator's asset-status patch) and this module
//! runs the capability-graph query for the violated service, recording the
//! verdict, the candidate steps or the blocked root causes in the trace's
//! [`RecoveryPath`](crate::metrics::RecoveryPath) slots.

use poie_ontology::graph::{self, Goal, Query, DEFAULT_MAX_DEPTH};
use poie_ontology::model::{Model, Verification};

use crate::metrics::{viability, RecoveryPath, RunTrace};

/// Runs the capability-graph recovery query for one service at a violation state.
pub fn recovery_path(model: &Model, service: &str, min_verification: Verification) -> RecoveryPath {
    let query = Query { goal: Goal::Service(service.to_string()), min_verification, max_depth: DEFAULT_MAX_DEPTH };
    let result = graph::solve(model, &query);
    let causes: Vec<String> =
        result.root_causes().map(|blocker| format!("{} {}", blocker.kind.as_str(), blocker.subject)).collect();
    let structural = match result.verdict {
        graph::Verdict::Candidate => {
            let steps: Vec<String> =
                result.steps.iter().map(|step| format!("{}(x{})", step.process, step.runs)).collect();
            let verification =
                result.route_verification().map(|level| level.to_string()).unwrap_or_else(|| "none".to_string());
            format!(
                "candidate: {} step(s) [{}]; route verification {verification}",
                result.steps.len(),
                steps.join(" -> ")
            )
        }
        graph::Verdict::Blocked => format!("blocked: root causes [{}]", causes.join(", ")),
        graph::Verdict::Unknown => format!("unknown: root causes [{}]", causes.join(", ")),
        graph::Verdict::SearchLimit => "search_limit: budget exhausted".to_string(),
    };
    RecoveryPath { target: service.to_string(), structural }
}

/// Attaches recovery paths for every service whose first violation appears in
/// the trace. Returns how many paths were attached. The caller must pass the
/// model patched to the violation state; for a single trace this is the state at
/// the first violation of each service (where the traces here assume one state
/// for all violations).
pub fn attach_recovery_paths(trace: &mut RunTrace, model: &Model, min_verification: Verification) -> usize {
    let mut attached = 0;
    for entry in viability(trace) {
        if entry.first_violation_s.is_some() {
            let path = recovery_path(model, &entry.service, min_verification);
            trace.recovery_paths.insert(entry.service.clone(), path);
            attached += 1;
        }
    }
    attached
}

#[cfg(test)]
mod tests {
    use super::*;
    use poie_ontology::model::{AssetStatus, Patch};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use crate::metrics::{RunTrace, TimeSample};

    fn base01() -> Model {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01/model.toml");
        Model::load(&path).unwrap()
    }

    fn trace_with_violation(url: &str) -> RunTrace {
        RunTrace {
            scenario_id: url.into(),
            horizon_s: 1000.0,
            minimum_levels: BTreeMap::from([("potable-water".to_string(), 1.0)]),
            samples: vec![
                TimeSample {
                    at_s: 0.0,
                    service_levels: BTreeMap::from([("potable-water".to_string(), 1.0)]),
                    violations: Vec::new(),
                    external_inputs: BTreeMap::new(),
                    stock: BTreeMap::new(),
                    produced: BTreeMap::new(),
                    recycle_inputs_kg: BTreeMap::new(),
                    recycle_reusable_kg: BTreeMap::new(),
                    health: BTreeMap::new(),
                    affected_services: Vec::new(),
                    affected_assets: Vec::new(),
                },
                TimeSample {
                    at_s: 100.0,
                    service_levels: BTreeMap::from([("potable-water".to_string(), 0.0)]),
                    violations: Vec::new(),
                    external_inputs: BTreeMap::new(),
                    stock: BTreeMap::new(),
                    produced: BTreeMap::new(),
                    recycle_inputs_kg: BTreeMap::new(),
                    recycle_reusable_kg: BTreeMap::new(),
                    health: BTreeMap::new(),
                    affected_services: vec!["potable-water".into()],
                    affected_assets: vec!["pump-p1".into()],
                },
            ],
            recovery_paths: BTreeMap::new(),
        }
    }

    #[test]
    fn a_violation_triggers_a_structural_recovery_query() {
        let model = base01();
        let mut trace = trace_with_violation("t0");
        let attached = attach_recovery_paths(&mut trace, &model, Verification::Demonstrated);
        assert_eq!(attached, 1);
        let path = &trace.recovery_paths["potable-water"];
        assert_eq!(path.target, "potable-water");
        assert!(path.structural.starts_with("candidate:"), "{}", path.structural);
        // The metrics view now carries the recovery path.
        assert_eq!(viability(&trace)[0].recovery_path.as_ref(), Some(path));
    }

    #[test]
    fn a_common_cause_failure_reports_the_blocked_root_cause() {
        let model = base01();
        let mut patch = Patch::default();
        patch.asset_status.insert("bus-1".into(), AssetStatus::Failed);
        patch.asset_status.insert("pump-p1".into(), AssetStatus::Failed);
        let patched = model.patched(&patch).unwrap();
        let mut trace = trace_with_violation("t1");
        let attached = attach_recovery_paths(&mut trace, &patched, Verification::Demonstrated);
        assert_eq!(attached, 1);
        let path = &trace.recovery_paths["potable-water"];
        assert!(path.structural.starts_with("blocked:"), "{}", path.structural);
        assert!(path.structural.contains("bus-1"), "{}", path.structural);
    }

    #[test]
    fn a_trace_without_violations_attaches_nothing() {
        let model = base01();
        let mut trace = trace_with_violation("t2");
        trace.samples[1].service_levels.insert("potable-water".into(), 1.0);
        let attached = attach_recovery_paths(&mut trace, &model, Verification::Demonstrated);
        assert_eq!(attached, 0);
        assert!(trace.recovery_paths.is_empty());
    }
}
