//! Replanning after new observations or plan failure (M8.2).
//!
//! Replanning starts from the model patched to the *current* state and only uses
//! information available at `now_s`; future disturbances are not inputs, so the
//! non-anticipating requirement is structural. The previous plan is diffed into
//! completed/kept, cancelled (future) and newly added actions.

use std::fmt;

use poie_ontology::model::{Model, Verification};

use crate::plan::{plan, Plan, PlanError, PlannedAction, PlanningGoal, PlanningRequest};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplanTrigger {
    Observation,
    PlanInvalid,
    DeadlineRisk,
}

impl ReplanTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            ReplanTrigger::Observation => "observation",
            ReplanTrigger::PlanInvalid => "plan_invalid",
            ReplanTrigger::DeadlineRisk => "deadline_risk",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReplanRequest<'a> {
    pub previous: &'a Plan,
    /// The model patched to the current observed state.
    pub model: &'a Model,
    pub now_s: f64,
    pub horizon_s: f64,
    pub goals: Vec<PlanningGoal>,
    pub min_verification: Verification,
    pub max_actions: usize,
    pub trigger: ReplanTrigger,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReplanResult {
    pub plan: Plan,
    pub trigger: ReplanTrigger,
    /// Previous actions that had already finished before `now_s`.
    pub kept_actions: Vec<String>,
    /// Previous future actions that the new plan replaces.
    pub cancelled_actions: Vec<String>,
    /// Actions in the new plan that were not in the cancelled set.
    pub added_actions: Vec<String>,
}

impl ReplanResult {
    pub fn digest(&self) -> String {
        format!(
            "{}|{}|kept={:?}|cancelled={:?}|added={:?}",
            self.trigger.as_str(),
            self.plan.digest(),
            self.kept_actions,
            self.cancelled_actions,
            self.added_actions
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ReplanError {
    Invalid(String),
    Plan(PlanError),
}

impl fmt::Display for ReplanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReplanError::Invalid(detail) => write!(f, "invalid replanning request: {detail}"),
            ReplanError::Plan(error) => write!(f, "replanning failed: {error}"),
        }
    }
}

impl std::error::Error for ReplanError {}

/// Replans from the current state. Actions are only kept if they had already
/// finished before `now_s`; everything still in the future is replaced.
pub fn replan(request: &ReplanRequest<'_>) -> Result<ReplanResult, ReplanError> {
    if !request.now_s.is_finite() || !request.horizon_s.is_finite() || request.horizon_s <= request.now_s {
        return Err(ReplanError::Invalid("horizon must be finite and greater than now".into()));
    }
    if request.now_s + 1e-9 < request.previous.start_s {
        return Err(ReplanError::Invalid("cannot replan before the previous plan started".into()));
    }
    let planning = PlanningRequest {
        model: request.model,
        now_s: request.now_s,
        horizon_s: request.horizon_s,
        goals: request.goals.clone(),
        min_verification: request.min_verification,
        max_actions: request.max_actions,
    };
    let plan = plan(&planning).map_err(ReplanError::Plan)?;
    if plan.actions.iter().any(|action| action.start_s + 1e-9 < request.now_s) {
        return Err(ReplanError::Invalid("new plan schedules work before now".into()));
    }

    let kept_actions: Vec<String> = request
        .previous
        .actions
        .iter()
        .filter(|action| action.end_s <= request.now_s + 1e-9)
        .map(|action| action.action_id.clone())
        .collect();
    let cancelled_actions: Vec<String> = request
        .previous
        .actions
        .iter()
        .filter(|action| action.end_s > request.now_s + 1e-9)
        .map(|action| action.action_id.clone())
        .collect();
    let same = |left: &PlannedAction, right: &PlannedAction| {
        left.process == right.process
            && left.target == right.target
            && (left.start_s - right.start_s).abs() < 1e-6
            && (left.end_s - right.end_s).abs() < 1e-6
    };
    let previous_future: Vec<&PlannedAction> =
        request.previous.actions.iter().filter(|action| action.end_s > request.now_s + 1e-9).collect();
    let added_actions: Vec<String> = plan
        .actions
        .iter()
        .filter(|action| !previous_future.iter().any(|previous| same(previous, action)))
        .map(|action| action.action_id.clone())
        .collect();
    Ok(ReplanResult { plan, trigger: request.trigger, kept_actions, cancelled_actions, added_actions })
}

#[cfg(test)]
mod tests {
    use super::*;
    use poie_ontology::graph::Goal;
    use poie_ontology::model::{AssetStatus, Model, Patch};
    use std::path::PathBuf;

    use crate::plan::{plan, PlanningGoal, PlanningRequest};

    fn base01() -> Model {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01/model.toml");
        Model::load(&path).unwrap()
    }

    fn goals() -> Vec<PlanningGoal> {
        vec![PlanningGoal { goal: Goal::Service("potable-water".into()), deadline_s: None, priority: 10 }]
    }

    fn initial_plan(model: &Model) -> Plan {
        plan(&PlanningRequest {
            model,
            now_s: 0.0,
            horizon_s: 86_400.0,
            goals: goals(),
            min_verification: Verification::Demonstrated,
            max_actions: 20,
        })
        .unwrap()
    }

    #[test]
    fn replanning_uses_only_the_current_state() {
        let model = base01();
        let previous = initial_plan(&model);
        let mut patch = Patch::default();
        patch.asset_status.insert("pump-p1".into(), AssetStatus::Failed);
        let observed = model.patched(&patch).unwrap();
        let result = replan(&ReplanRequest {
            previous: &previous,
            model: &observed,
            now_s: 900.0,
            horizon_s: 86_400.0,
            goals: goals(),
            min_verification: Verification::Demonstrated,
            max_actions: 20,
            trigger: ReplanTrigger::Observation,
        })
        .unwrap();
        assert_eq!(result.trigger, ReplanTrigger::Observation);
        assert!(result.plan.actions.iter().all(|action| action.start_s + 1e-9 >= 900.0));
        assert!(!result.plan.actions.is_empty());
    }

    #[test]
    fn replanning_diffs_previous_plan_into_kept_cancelled_and_added() {
        let model = base01();
        let previous = initial_plan(&model);
        let result = replan(&ReplanRequest {
            previous: &previous,
            model: &model,
            now_s: 20_000.0,
            horizon_s: 86_400.0,
            goals: goals(),
            min_verification: Verification::Demonstrated,
            max_actions: 20,
            trigger: ReplanTrigger::PlanInvalid,
        })
        .unwrap();
        // The 3 h repair finished before 20 000 s, so it is kept.
        assert_eq!(result.kept_actions.len(), previous.actions.len());
        assert!(result.cancelled_actions.is_empty());
        assert!(!result.plan.actions.is_empty());
    }

    #[test]
    fn replanning_before_the_previous_start_is_rejected() {
        let model = base01();
        let previous = initial_plan(&model);
        let error = replan(&ReplanRequest {
            previous: &previous,
            model: &model,
            now_s: -1.0,
            horizon_s: 86_400.0,
            goals: goals(),
            min_verification: Verification::Demonstrated,
            max_actions: 20,
            trigger: ReplanTrigger::Observation,
        })
        .unwrap_err();
        assert!(matches!(error, ReplanError::Invalid(_)));
    }

    #[test]
    fn replanning_digest_is_deterministic() {
        let model = base01();
        let previous = initial_plan(&model);
        let request = ReplanRequest {
            previous: &previous,
            model: &model,
            now_s: 900.0,
            horizon_s: 86_400.0,
            goals: goals(),
            min_verification: Verification::Demonstrated,
            max_actions: 20,
            trigger: ReplanTrigger::Observation,
        };
        assert_eq!(replan(&request).unwrap().digest(), replan(&request).unwrap().digest());
    }
}
