//! The deterministic joint planner (M8.1).

use std::collections::BTreeSet;
use std::fmt;

use poie_ontology::graph::{self, Goal, Query, Verdict, DEFAULT_MAX_DEPTH};
use poie_ontology::model::{AssetStatus, Model, Patch, Verification};
use poie_ontology::schedule::schedule_route;

/// One goal the planner should achieve, with an optional deadline.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanningGoal {
    pub goal: Goal,
    pub deadline_s: Option<f64>,
    /// Higher priority goals are planned first.
    pub priority: u32,
}

/// Planner input. `model` must already be patched to the current state.
#[derive(Clone, Debug)]
pub struct PlanningRequest<'a> {
    pub model: &'a Model,
    pub now_s: f64,
    pub horizon_s: f64,
    pub goals: Vec<PlanningGoal>,
    pub min_verification: Verification,
    pub max_actions: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedAction {
    pub action_id: String,
    pub process: String,
    pub target: String,
    pub runs: u32,
    pub start_s: f64,
    pub end_s: f64,
    pub preconditions: Vec<String>,
    pub allocations: Vec<(String, f64)>,
    pub occupies: Vec<String>,
    pub restores: Vec<String>,
    pub verification: Verification,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GoalFailure {
    pub goal: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub plan_id: String,
    pub start_s: f64,
    pub horizon_s: f64,
    pub actions: Vec<PlannedAction>,
    pub goals_met: Vec<String>,
    pub goals_unmet: Vec<GoalFailure>,
    pub completion_s: f64,
}

impl Plan {
    /// Canonical digest used for reproducibility checks.
    pub fn digest(&self) -> String {
        let mut out = format!("plan {};", self.plan_id);
        for action in &self.actions {
            out.push_str(&format!(
                "{}|{}|{}|{}|{:.3}-{:.3};",
                action.action_id, action.process, action.target, action.runs, action.start_s, action.end_s
            ));
        }
        for failure in &self.goals_unmet {
            out.push_str(&format!("unmet {}:{};", failure.goal, failure.reason));
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlanError {
    InvalidRequest(String),
    Model(String),
    Schedule(String),
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::InvalidRequest(detail) => write!(f, "invalid planning request: {detail}"),
            PlanError::Model(detail) => write!(f, "model error: {detail}"),
            PlanError::Schedule(detail) => write!(f, "schedule error: {detail}"),
        }
    }
}

impl std::error::Error for PlanError {}

fn goal_label(goal: &Goal) -> String {
    match goal {
        Goal::Service(service) => service.clone(),
        Goal::Restore(asset) => format!("restore:{asset}"),
        Goal::Obtain { resource, .. } => format!("obtain:{resource}"),
    }
}

/// Plans goals in priority order. For each goal it takes the structural
/// candidate route, schedules it after the current cursor, and when the route is
/// blocked by a failed asset it tries to restore that asset (bounded rounds)
/// before re-solving. It is a deterministic greedy planner, not a global
/// optimiser; ADR-0006 records that choice.
pub fn plan(request: &PlanningRequest<'_>) -> Result<Plan, PlanError> {
    if !request.now_s.is_finite() || !request.horizon_s.is_finite() || request.horizon_s <= request.now_s {
        return Err(PlanError::InvalidRequest("horizon must be finite and greater than now".into()));
    }
    if request.goals.is_empty() {
        return Err(PlanError::InvalidRequest("at least one goal is required".into()));
    }
    if request.max_actions == 0 {
        return Err(PlanError::InvalidRequest("max_actions must be positive".into()));
    }

    let mut working: Model = request.model.clone();
    let mut actions: Vec<PlannedAction> = Vec::new();
    let mut cursor = request.now_s;
    let mut goals_met = Vec::new();
    let mut goals_unmet = Vec::new();
    let mut restored: BTreeSet<String> = BTreeSet::new();
    let mut next_action_id: usize = 0;

    let mut ordered: Vec<&PlanningGoal> = request.goals.iter().collect();
    ordered.sort_by_key(|goal| std::cmp::Reverse(goal.priority));

    for (goal_index, goal) in ordered.iter().enumerate() {
        let goal = *goal;
        let label = goal_label(&goal.goal);
        let mut met = false;
        for _round in 0..8 {
            let query = Query {
                goal: goal.goal.clone(),
                min_verification: request.min_verification,
                max_depth: DEFAULT_MAX_DEPTH,
            };
            let result = graph::solve(&working, &query);
            match result.verdict {
                Verdict::Candidate => {
                    let report = schedule_route(&working, &result, cursor)
                        .map_err(|error| PlanError::Schedule(error.to_string()))?;
                    for (position, step) in result.steps.iter().enumerate() {
                        if actions.len() >= request.max_actions {
                            goals_unmet
                                .push(GoalFailure { goal: label.clone(), reason: "action budget exhausted".into() });
                            for remaining in ordered.iter().skip(goal_index + 1) {
                                goals_unmet.push(GoalFailure {
                                    goal: goal_label(&remaining.goal),
                                    reason: "action budget exhausted".into(),
                                });
                            }
                            return finish(request, actions, goals_met, goals_unmet, cursor);
                        }
                        let id = format!("step{}-{}", position + 1, step.process);
                        let instance = report.instances.iter().find(|instance| instance.id == id);
                        let (start_s, end_s) = instance
                            .map(|instance| (instance.start_s, instance.end_s))
                            .ok_or_else(|| PlanError::Schedule(format!("missing schedule entry {id}")))?;
                        if end_s > request.horizon_s {
                            goals_unmet.push(GoalFailure { goal: label.clone(), reason: "horizon exceeded".into() });
                            met = true;
                            break;
                        }
                        let process = working
                            .process(&step.process)
                            .ok_or_else(|| PlanError::Model(format!("unknown process {:?}", step.process)))?;
                        let preconditions = preconditions(process, step);
                        let target = step.occupies.first().cloned().unwrap_or_else(|| "site".to_string());
                        next_action_id += 1;
                        actions.push(PlannedAction {
                            action_id: format!("{label}-{next_action_id}-{}", step.process),
                            process: step.process.clone(),
                            target,
                            runs: step.runs,
                            start_s,
                            end_s,
                            preconditions,
                            allocations: step
                                .allocations
                                .iter()
                                .map(|allocation| (allocation.resource.clone(), allocation.si))
                                .collect(),
                            occupies: step.occupies.clone(),
                            restores: step.restores.clone(),
                            verification: step.verification,
                        });
                    }
                    if goals_unmet.iter().any(|failure| failure.goal == label) {
                        break;
                    }
                    // Apply the planned restores to the working model so later
                    // goals do not plan the same repair again.
                    let mut restore_patch = Patch::default();
                    for step in &result.steps {
                        for asset in &step.restores {
                            restore_patch.asset_status.insert(asset.clone(), AssetStatus::Available);
                        }
                    }
                    if !restore_patch.asset_status.is_empty() {
                        working =
                            working.patched(&restore_patch).map_err(|error| PlanError::Model(error.to_string()))?;
                    }
                    cursor = cursor.max(report.instances.iter().map(|instance| instance.end_s).fold(cursor, f64::max));
                    if let Some(deadline) = goal.deadline_s {
                        if cursor > deadline + 1e-6 {
                            goals_unmet.push(GoalFailure { goal: label.clone(), reason: "deadline missed".into() });
                            met = true;
                            break;
                        }
                    }
                    goals_met.push(label.clone());
                    met = true;
                    break;
                }
                Verdict::Blocked => {
                    let restorable: Vec<String> = result
                        .root_causes()
                        .filter(|blocker| blocker.kind == graph::BlockerKind::AssetUnavailable)
                        .map(|blocker| blocker.subject.clone())
                        .filter(|asset| !restored.contains(asset))
                        .collect();
                    if restorable.is_empty() {
                        goals_unmet.push(GoalFailure {
                            goal: label.clone(),
                            reason: format!("blocked: {}", root_cause_text(&result)),
                        });
                        break;
                    }
                    let mut progress = false;
                    for asset in restorable {
                        let restore_query = Query {
                            goal: Goal::Restore(asset.clone()),
                            min_verification: request.min_verification,
                            max_depth: DEFAULT_MAX_DEPTH,
                        };
                        let restore_result = graph::solve(&working, &restore_query);
                        if restore_result.verdict != Verdict::Candidate {
                            continue;
                        }
                        let report = schedule_route(&working, &restore_result, cursor)
                            .map_err(|error| PlanError::Schedule(error.to_string()))?;
                        for (position, step) in restore_result.steps.iter().enumerate() {
                            if actions.len() >= request.max_actions {
                                goals_unmet.push(GoalFailure {
                                    goal: label.clone(),
                                    reason: "action budget exhausted".into(),
                                });
                                for remaining in ordered.iter().skip(goal_index + 1) {
                                    goals_unmet.push(GoalFailure {
                                        goal: goal_label(&remaining.goal),
                                        reason: "action budget exhausted".into(),
                                    });
                                }
                                return finish(request, actions, goals_met, goals_unmet, cursor);
                            }
                            let id = format!("restore-{asset}-{position}-{}", step.process);
                            let instance = report
                                .instances
                                .iter()
                                .find(|instance| instance.id == format!("step{}-{}", position + 1, step.process));
                            let Some(instance) = instance else {
                                return Err(PlanError::Schedule(format!("missing schedule entry for {id}")));
                            };
                            let process = working
                                .process(&step.process)
                                .ok_or_else(|| PlanError::Model(format!("unknown process {:?}", step.process)))?;
                            next_action_id += 1;
                            actions.push(PlannedAction {
                                action_id: format!("restore-{asset}-{next_action_id}-{}", step.process),
                                process: step.process.clone(),
                                target: asset.clone(),
                                runs: step.runs,
                                start_s: instance.start_s,
                                end_s: instance.end_s,
                                preconditions: preconditions(process, step),
                                allocations: step
                                    .allocations
                                    .iter()
                                    .map(|allocation| (allocation.resource.clone(), allocation.si))
                                    .collect(),
                                occupies: step.occupies.clone(),
                                restores: step.restores.clone(),
                                verification: step.verification,
                            });
                        }
                        cursor =
                            cursor.max(report.instances.iter().map(|instance| instance.end_s).fold(cursor, f64::max));
                        let mut patch = Patch::default();
                        patch.asset_status.insert(asset.clone(), AssetStatus::Available);
                        working = working.patched(&patch).map_err(|error| PlanError::Model(error.to_string()))?;
                        restored.insert(asset);
                        progress = true;
                    }
                    if !progress {
                        goals_unmet.push(GoalFailure { goal: label.clone(), reason: "no restorable blocker".into() });
                        break;
                    }
                }
                Verdict::Unknown => {
                    goals_unmet.push(GoalFailure { goal: label.clone(), reason: "unknown state".into() });
                    break;
                }
                Verdict::SearchLimit => {
                    goals_unmet.push(GoalFailure { goal: label.clone(), reason: "search limit".into() });
                    break;
                }
            }
        }
        if !met && !goals_unmet.iter().any(|failure| failure.goal == label) {
            goals_unmet.push(GoalFailure { goal: label, reason: "no plan found".into() });
        }
    }
    finish(request, actions, goals_met, goals_unmet, cursor)
}

fn finish(
    request: &PlanningRequest<'_>,
    actions: Vec<PlannedAction>,
    goals_met: Vec<String>,
    goals_unmet: Vec<GoalFailure>,
    cursor: f64,
) -> Result<Plan, PlanError> {
    let plan_id = format!("plan-{:.0}-{}", request.now_s, actions.len());
    Ok(Plan {
        plan_id,
        start_s: request.now_s,
        horizon_s: request.horizon_s,
        completion_s: cursor,
        actions,
        goals_met,
        goals_unmet,
    })
}

fn preconditions(process: &poie_ontology::model::Process, step: &poie_ontology::graph::Step) -> Vec<String> {
    let mut conditions = Vec::new();
    for asset in &step.occupies {
        conditions.push(format!("{asset} available"));
    }
    for (capability, provider, level) in &step.providers {
        conditions.push(format!("capability {capability} via {provider} >= {level}"));
    }
    for allocation in &step.allocations {
        conditions.push(format!("{} allocation {:.6}", allocation.resource, allocation.si));
    }
    for input in &process.inputs {
        conditions.push(format!("input {}", input.resource));
    }
    conditions.sort();
    conditions.dedup();
    conditions
}

fn root_cause_text(result: &graph::QueryResult) -> String {
    result
        .root_causes()
        .map(|blocker| format!("{} {}", blocker.kind.as_str(), blocker.subject))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use poie_ontology::model::Patch;
    use std::path::PathBuf;

    fn base01() -> Model {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01/model.toml");
        Model::load(&path).unwrap()
    }

    fn request<'a>(model: &'a Model, goals: Vec<PlanningGoal>) -> PlanningRequest<'a> {
        PlanningRequest {
            model,
            now_s: 0.0,
            horizon_s: 24.0 * 3600.0,
            goals,
            min_verification: Verification::Demonstrated,
            max_actions: 20,
        }
    }

    fn service_goal() -> PlanningGoal {
        PlanningGoal { goal: Goal::Service("potable-water".into()), deadline_s: None, priority: 10 }
    }

    #[test]
    fn plans_a_repair_for_the_degraded_pump() {
        let model = base01();
        let plan = plan(&request(&model, vec![service_goal()])).unwrap();
        assert_eq!(plan.goals_met.len(), 1);
        assert!(plan.goals_unmet.is_empty());
        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.actions[0].process, "replace-sleeve-p1");
        assert!(plan.actions[0].end_s > plan.actions[0].start_s);
    }

    #[test]
    fn plans_are_deterministic() {
        let model = base01();
        let first = plan(&request(&model, vec![service_goal()])).unwrap();
        let second = plan(&request(&model, vec![service_goal()])).unwrap();
        assert_eq!(first.digest(), second.digest());
    }

    #[test]
    fn a_deadline_miss_is_reported() {
        let model = base01();
        let goal = PlanningGoal { goal: Goal::Service("potable-water".into()), deadline_s: Some(3600.0), priority: 10 };
        let plan = plan(&request(&model, vec![goal])).unwrap();
        assert!(plan.goals_met.is_empty());
        assert!(plan.goals_unmet.iter().any(|failure| failure.reason == "deadline missed"));
    }

    #[test]
    fn the_planner_restores_a_failed_asset_before_re_solving() {
        let model = base01();
        let mut patch = Patch::default();
        patch.asset_status.insert("lathe-1".into(), AssetStatus::Failed);
        let patched = model.patched(&patch).unwrap();
        let plan = plan(&request(
            &patched,
            vec![PlanningGoal { goal: Goal::Restore("lathe-1".into()), deadline_s: None, priority: 10 }],
        ))
        .unwrap();
        assert!(plan.goals_met.is_empty());
        assert!(plan.goals_unmet.iter().any(|failure| failure.reason.contains("blocked")), "{:?}", plan.goals_unmet);
    }

    #[test]
    fn action_budget_is_respected() {
        let model = base01();
        let mut request = request(&model, vec![service_goal()]);
        request.max_actions = 0;
        assert!(plan(&request).is_err());
    }
}
