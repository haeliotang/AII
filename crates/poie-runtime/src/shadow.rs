//! Shadow planning (M9.4): the planner runs on the projected field state, its
//! actions are only recorded, and the "would do" plan is compared with the
//! operator's observed actions.

use poie_ontology::model::{Model, Verification};
use poie_plan::plan::{plan, Plan, PlanError, PlanningGoal, PlanningRequest};

/// One action an operator actually performed (from a work log or observation).
#[derive(Clone, Debug, PartialEq)]
pub struct OperatorAction {
    pub at_s: f64,
    pub process: String,
    pub target: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShadowDivergence {
    /// `would_do_not_observed` or `observed_not_planned`.
    pub kind: String,
    pub planned: Option<(String, String)>,
    pub operator: Option<(String, String)>,
    pub at_s: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShadowReport {
    pub plan: Plan,
    /// Always false: the shadow runtime has no dispatch path.
    pub dispatched: bool,
    pub divergences: Vec<ShadowDivergence>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ShadowError {
    Plan(PlanError),
}

impl std::fmt::Display for ShadowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShadowError::Plan(error) => write!(f, "shadow plan error: {error}"),
        }
    }
}

impl std::error::Error for ShadowError {}

/// Plans on the current (projected) model and compares with the operator log.
/// No executor, no adapter and no dispatch path is involved.
pub fn shadow_plan(
    model: &Model,
    now_s: f64,
    horizon_s: f64,
    goals: Vec<PlanningGoal>,
    min_verification: Verification,
    operator_actions: &[OperatorAction],
) -> Result<ShadowReport, ShadowError> {
    let plan = plan(&PlanningRequest { model, now_s, horizon_s, goals, min_verification, max_actions: 50 })
        .map_err(ShadowError::Plan)?;
    let planned: Vec<(String, String)> =
        plan.actions.iter().map(|action| (action.process.clone(), action.target.clone())).collect();
    let observed: Vec<(String, String)> =
        operator_actions.iter().map(|action| (action.process.clone(), action.target.clone())).collect();
    let mut divergences = Vec::new();
    for action in &plan.actions {
        let key = (action.process.clone(), action.target.clone());
        if !observed.contains(&key) {
            divergences.push(ShadowDivergence {
                kind: "would_do_not_observed".to_string(),
                planned: Some(key),
                operator: None,
                at_s: Some(action.start_s),
            });
        }
    }
    for action in operator_actions {
        let key = (action.process.clone(), action.target.clone());
        if !planned.contains(&key) {
            divergences.push(ShadowDivergence {
                kind: "observed_not_planned".to_string(),
                planned: None,
                operator: Some(key),
                at_s: Some(action.at_s),
            });
        }
    }
    Ok(ShadowReport { plan, dispatched: false, divergences })
}

/// An explicit guard used by runtime integrations: shadow mode must never
/// dispatch, regardless of what the caller asks for.
pub fn ensure_no_dispatch(dispatch_requested: bool) -> Result<(), ShadowError> {
    if dispatch_requested {
        return Err(ShadowError::Plan(PlanError::InvalidRequest(
            "shadow mode cannot dispatch commands; use the SAP execution path with a real authorizer".into(),
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use poie_ontology::graph::Goal;
    use std::path::PathBuf;

    fn base01() -> Model {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01/model.toml");
        Model::load(&path).unwrap()
    }

    fn goals() -> Vec<PlanningGoal> {
        vec![PlanningGoal { goal: Goal::Service("potable-water".into()), deadline_s: None, priority: 10 }]
    }

    #[test]
    fn shadow_plan_records_actions_without_dispatching() {
        let model = base01();
        let report = shadow_plan(&model, 0.0, 86_400.0, goals(), Verification::Demonstrated, &[]).unwrap();
        assert!(!report.plan.actions.is_empty());
        assert!(!report.dispatched);
        assert!(report.divergences.iter().any(|divergence| divergence.kind == "would_do_not_observed"));
    }

    #[test]
    fn matching_operator_actions_remove_divergences() {
        let model = base01();
        let planned = shadow_plan(&model, 0.0, 86_400.0, goals(), Verification::Demonstrated, &[]).unwrap();
        let operator: Vec<OperatorAction> = planned
            .plan
            .actions
            .iter()
            .map(|action| OperatorAction {
                at_s: action.start_s,
                process: action.process.clone(),
                target: action.target.clone(),
            })
            .collect();
        let report = shadow_plan(&model, 0.0, 86_400.0, goals(), Verification::Demonstrated, &operator).unwrap();
        assert!(report.divergences.is_empty(), "{:?}", report.divergences);
    }

    #[test]
    fn an_operator_action_outside_the_plan_is_reported() {
        let model = base01();
        let operator = vec![OperatorAction { at_s: 10.0, process: "turn-sleeve-p1".into(), target: "lathe-1".into() }];
        let report = shadow_plan(&model, 0.0, 86_400.0, goals(), Verification::Demonstrated, &operator).unwrap();
        assert!(report.divergences.iter().any(|divergence| divergence.kind == "observed_not_planned"));
    }

    #[test]
    fn shadow_mode_refuses_dispatch_requests() {
        assert!(ensure_no_dispatch(false).is_ok());
        assert!(ensure_no_dispatch(true).is_err());
    }
}
