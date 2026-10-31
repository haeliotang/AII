//! Comparison baselines for H2 (M8.3).
//!
//! Three deliberately simple policies, all producing the same `Plan` shape so
//! H2 can compare them fairly on identical scenarios and budgets:
//!
//! * `TaskPriority` - handles goals in the order given, repairs a failed asset
//!   only when the route is blocked, ignores deadlines.
//! * `SafetyStockRule` - like task priority but only repairs when the configured
//!   safety stock covers the repair route's inputs; otherwise it waits.
//! * `ShortTermOptimization` - repeatedly picks the goal with the earliest
//!   completion within the horizon.

use std::collections::BTreeMap;

use poie_ontology::graph::{self, Goal, Query, Step, Verdict, DEFAULT_MAX_DEPTH};
use poie_ontology::model::{LotQuality, Model, Process, Verification};
use poie_ontology::schedule::schedule_route;

use crate::plan::{GoalFailure, Plan, PlanError, PlannedAction, PlanningGoal};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaselineKind {
    TaskPriority,
    SafetyStockRule,
    ShortTermOptimization,
}

impl BaselineKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BaselineKind::TaskPriority => "task_priority",
            BaselineKind::SafetyStockRule => "safety_stock_rule",
            BaselineKind::ShortTermOptimization => "short_term_optimization",
        }
    }
}

#[derive(Clone, Debug)]
pub struct BaselineRequest<'a> {
    pub model: &'a Model,
    pub now_s: f64,
    pub horizon_s: f64,
    pub goals: Vec<PlanningGoal>,
    pub min_verification: Verification,
    pub max_actions: usize,
    /// Safety threshold per resource (SI) for `SafetyStockRule`.
    pub safety_stock: BTreeMap<String, f64>,
}

fn goal_label(goal: &Goal) -> String {
    match goal {
        Goal::Service(service) => service.clone(),
        Goal::Restore(asset) => format!("restore:{asset}"),
        Goal::Obtain { resource, .. } => format!("obtain:{resource}"),
    }
}

fn stock(model: &Model) -> BTreeMap<String, f64> {
    let mut stock: BTreeMap<String, f64> = BTreeMap::new();
    for lot in model.lots.iter().filter(|lot| lot.quality == LotQuality::Accepted) {
        *stock.entry(lot.resource.clone()).or_insert(0.0) += lot.si;
    }
    stock
}

fn safety_covered(actions: &[PlannedAction], stock: &BTreeMap<String, f64>, safety: &BTreeMap<String, f64>) -> bool {
    let mut needed: BTreeMap<&str, f64> = BTreeMap::new();
    for action in actions {
        for (resource, amount) in &action.allocations {
            *needed.entry(resource.as_str()).or_insert(0.0) += amount;
        }
    }
    needed.iter().all(|(resource, amount)| {
        let have = stock.get(*resource).copied().unwrap_or(0.0);
        let reserve = safety.get(*resource).copied().unwrap_or(0.0);
        have + 1e-9 >= reserve + amount
    })
}

fn actions_for_route(
    plan_id: &str,
    label: &str,
    model: &Model,
    steps: &[Step],
    instances: &[poie_ontology::schedule::ScheduledInstance],
    offset: usize,
) -> Result<Vec<PlannedAction>, PlanError> {
    let mut actions = Vec::new();
    for (position, step) in steps.iter().enumerate() {
        let id = format!("step{}-{}", position + 1, step.process);
        let instance = instances.iter().find(|instance| instance.id == id);
        let Some(instance) = instance else {
            return Err(PlanError::Schedule(format!("missing schedule entry {id}")));
        };
        let Some(process) = model.process(&step.process) else {
            return Err(PlanError::Model(format!("unknown process {:?}", step.process)));
        };
        actions.push(PlannedAction {
            action_id: format!("{label}-{}-{}-{}", plan_id, offset + position + 1, step.process),
            process: step.process.clone(),
            target: step.occupies.first().cloned().unwrap_or_else(|| "site".to_string()),
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
    Ok(actions)
}

fn preconditions(process: &Process, step: &Step) -> Vec<String> {
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

fn plan_goal(
    model: &Model,
    goal: &PlanningGoal,
    cursor: f64,
    min_verification: Verification,
    plan_id: &str,
    offset: usize,
) -> Result<(Vec<PlannedAction>, f64, Option<String>), PlanError> {
    let label = goal_label(&goal.goal);
    let query = Query { goal: goal.goal.clone(), min_verification, max_depth: DEFAULT_MAX_DEPTH };
    let result = graph::solve(model, &query);
    if result.verdict != Verdict::Candidate {
        return Ok((Vec::new(), cursor, Some(format!("blocked: {}", verdict_text(&result)))));
    }
    let report = schedule_route(model, &result, cursor).map_err(|error| PlanError::Schedule(error.to_string()))?;
    let actions = actions_for_route(plan_id, &label, model, &result.steps, &report.instances, offset)?;
    let completion = report.instances.iter().map(|instance| instance.end_s).fold(cursor, f64::max);
    Ok((actions, completion, None))
}

fn verdict_text(result: &graph::QueryResult) -> String {
    result
        .root_causes()
        .map(|blocker| format!("{} {}", blocker.kind.as_str(), blocker.subject))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Runs one baseline policy.
pub fn baseline(kind: BaselineKind, request: &BaselineRequest<'_>) -> Result<Plan, PlanError> {
    if !request.now_s.is_finite() || !request.horizon_s.is_finite() || request.horizon_s <= request.now_s {
        return Err(PlanError::InvalidRequest("horizon must be finite and greater than now".into()));
    }
    let plan_id = format!("baseline-{}-{:.0}", kind.as_str(), request.now_s);
    let mut actions = Vec::new();
    let mut cursor = request.now_s;
    let mut goals_met = Vec::new();
    let mut goals_unmet: Vec<GoalFailure> = Vec::new();
    let stock = stock(request.model);

    let mut pending: Vec<&PlanningGoal> = request.goals.iter().collect();
    match kind {
        BaselineKind::ShortTermOptimization => {
            // Pick the goal with the earliest completion first; repeat.
            while !pending.is_empty() && actions.len() < request.max_actions {
                let mut best: Option<(usize, f64, Vec<PlannedAction>, f64, String)> = None;
                for (index, goal) in pending.iter().enumerate() {
                    let query = Query {
                        goal: goal.goal.clone(),
                        min_verification: request.min_verification,
                        max_depth: DEFAULT_MAX_DEPTH,
                    };
                    let result = graph::solve(request.model, &query);
                    if result.verdict != Verdict::Candidate {
                        continue;
                    }
                    let Ok(report) = schedule_route(request.model, &result, cursor) else { continue };
                    let completion = report.instances.iter().map(|instance| instance.end_s).fold(cursor, f64::max);
                    let better = match &best {
                        None => true,
                        Some((_, best_completion, ..)) => completion < *best_completion - 1e-9,
                    };
                    if better {
                        let route_actions = actions_for_route(
                            &plan_id,
                            &goal_label(&goal.goal),
                            request.model,
                            &result.steps,
                            &report.instances,
                            actions.len(),
                        )?;
                        best = Some((index, completion, route_actions, completion, goal_label(&goal.goal)));
                    }
                }
                let Some((index, _, route_actions, completion, label)) = best else { break };
                actions.extend(route_actions);
                cursor = completion;
                goals_met.push(label);
                pending.remove(index);
            }
        }
        BaselineKind::TaskPriority | BaselineKind::SafetyStockRule => {
            for goal in &pending {
                let label = goal_label(&goal.goal);
                if actions.len() >= request.max_actions {
                    goals_unmet.push(GoalFailure { goal: label, reason: "action budget exhausted".into() });
                    continue;
                }
                let (route_actions, completion, failure) =
                    plan_goal(request.model, goal, cursor, request.min_verification, &plan_id, actions.len())?;
                if let Some(reason) = failure {
                    // Try a direct repair of the first failed asset the blocker names.
                    let query = Query {
                        goal: goal.goal.clone(),
                        min_verification: request.min_verification,
                        max_depth: DEFAULT_MAX_DEPTH,
                    };
                    let blocked = graph::solve(request.model, &query);
                    let asset = blocked
                        .root_causes()
                        .find(|blocker| blocker.kind == graph::BlockerKind::AssetUnavailable)
                        .map(|blocker| blocker.subject.clone());
                    let Some(asset) = asset else {
                        goals_unmet.push(GoalFailure { goal: label, reason });
                        continue;
                    };
                    let restore_query = Query {
                        goal: Goal::Restore(asset.clone()),
                        min_verification: request.min_verification,
                        max_depth: DEFAULT_MAX_DEPTH,
                    };
                    let restore = graph::solve(request.model, &restore_query);
                    if restore.verdict != Verdict::Candidate {
                        goals_unmet.push(GoalFailure { goal: label, reason });
                        continue;
                    }
                    if kind == BaselineKind::SafetyStockRule {
                        let covered = restore.steps.iter().flat_map(|step| step.allocations.iter()).all(|allocation| {
                            let have = stock.get(&allocation.resource).copied().unwrap_or(0.0);
                            let needed =
                                request.safety_stock.get(&allocation.resource).copied().unwrap_or(0.0) + allocation.si;
                            have + 1e-9 >= needed
                        });
                        if !covered {
                            goals_unmet
                                .push(GoalFailure { goal: label, reason: "safety stock below threshold".into() });
                            continue;
                        }
                    }
                    let report = schedule_route(request.model, &restore, cursor)
                        .map_err(|error| PlanError::Schedule(error.to_string()))?;
                    let repair = actions_for_route(
                        &plan_id,
                        &format!("restore:{asset}"),
                        request.model,
                        &restore.steps,
                        &report.instances,
                        actions.len(),
                    )?;
                    actions.extend(repair);
                    cursor = report.instances.iter().map(|instance| instance.end_s).fold(cursor, f64::max);
                    // Re-solve once after the repair.
                    let (route_actions, completion, failure) =
                        plan_goal(request.model, goal, cursor, request.min_verification, &plan_id, actions.len())?;
                    match failure {
                        None => {
                            actions.extend(route_actions);
                            cursor = completion;
                            goals_met.push(label);
                        }
                        Some(reason) => goals_unmet.push(GoalFailure { goal: label, reason }),
                    }
                    continue;
                }
                if kind == BaselineKind::SafetyStockRule
                    && !safety_covered(&route_actions, &stock, &request.safety_stock)
                {
                    goals_unmet.push(GoalFailure { goal: label, reason: "safety stock below threshold".into() });
                    continue;
                }
                actions.extend(route_actions);
                cursor = completion;
                goals_met.push(label);
            }
        }
    }
    for goal in pending.iter().filter(|goal| !goals_met.contains(&goal_label(&goal.goal))) {
        let label = goal_label(&goal.goal);
        if !goals_unmet.iter().any(|failure| failure.goal == label) {
            goals_unmet.push(GoalFailure { goal: label, reason: "not planned".into() });
        }
    }
    Ok(Plan {
        plan_id,
        start_s: request.now_s,
        horizon_s: request.horizon_s,
        actions,
        goals_met,
        goals_unmet,
        completion_s: cursor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use poie_ontology::model::{AssetStatus, Model, Patch};
    use std::path::PathBuf;

    fn base01() -> Model {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01/model.toml");
        Model::load(&path).unwrap()
    }

    fn request<'a>(model: &'a Model) -> BaselineRequest<'a> {
        BaselineRequest {
            model,
            now_s: 0.0,
            horizon_s: 86_400.0,
            goals: vec![PlanningGoal { goal: Goal::Service("potable-water".into()), deadline_s: None, priority: 10 }],
            min_verification: Verification::Demonstrated,
            max_actions: 20,
            safety_stock: BTreeMap::new(),
        }
    }

    #[test]
    fn task_priority_produces_a_plan() {
        let model = base01();
        let plan = baseline(BaselineKind::TaskPriority, &request(&model)).unwrap();
        assert!(!plan.actions.is_empty());
        assert!(plan.goals_unmet.is_empty());
    }

    #[test]
    fn safety_stock_rule_waits_when_the_repair_is_uncovered() {
        let model = base01();
        let mut patch = Patch::default();
        patch.asset_status.insert("pump-p1".into(), AssetStatus::Failed);
        let patched = model.patched(&patch).unwrap();
        let mut safety = request(&patched);
        // One sleeve is in stock but the policy wants two before it repairs.
        safety.safety_stock.insert("sleeve-p1".into(), 2.0);
        let plan = baseline(BaselineKind::SafetyStockRule, &safety).unwrap();
        assert!(
            plan.goals_unmet.iter().any(|failure| failure.reason == "safety stock below threshold"),
            "{:?}",
            plan.goals_unmet
        );
    }

    #[test]
    fn short_term_optimization_produces_a_plan() {
        let model = base01();
        let plan = baseline(BaselineKind::ShortTermOptimization, &request(&model)).unwrap();
        assert!(!plan.actions.is_empty());
        assert!(plan.goals_unmet.is_empty());
    }

    #[test]
    fn baselines_are_deterministic() {
        let model = base01();
        let first = baseline(BaselineKind::TaskPriority, &request(&model)).unwrap();
        let second = baseline(BaselineKind::TaskPriority, &request(&model)).unwrap();
        assert_eq!(first.digest(), second.digest());
    }
}
