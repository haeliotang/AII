//! Domain snapshot for the Python-free desktop shell.
//!
//! The host embeds the Base-01 model and evaluates it with the real
//! `poie-ontology` / `poie-plan` crates. The frontend receives a small,
//! serializable snapshot; it never gets the model or solver internals.

use poie_ontology::graph::{self, Goal, Query, DEFAULT_MAX_DEPTH};
use poie_ontology::model::{AssetStatus, Model, ModelDoc, Patch, Verification};
use poie_plan::plan::{plan, PlanningGoal, PlanningRequest};
use serde::Serialize;

const BASE_01_MODEL: &str = include_str!("../../../../scenarios/base-01/model.toml");

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub services: usize,
    pub assets: usize,
    pub resources: usize,
    pub processes: usize,
    pub lots: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityView {
    pub id: String,
    pub kind: String,
    pub status: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceView {
    pub id: String,
    pub label: String,
    pub verdict: String,
    pub steps: usize,
    pub blockers: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyView {
    pub resource: String,
    pub max_si: f64,
    pub in_root_causes: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionView {
    pub id: String,
    pub process: String,
    pub target: String,
    pub start_s: f64,
    pub end_s: f64,
    pub runs: u32,
    pub verification: String,
    pub preconditions: Vec<String>,
    pub allocations: Vec<(String, f64)>,
    pub occupies: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanView {
    pub plan_id: String,
    pub goal: String,
    pub horizon_s: f64,
    pub completion_s: f64,
    pub digest: String,
    pub actions: Vec<ActionView>,
    pub goals_met: Vec<String>,
    pub goals_unmet: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainOverview {
    pub scenario: String,
    pub model_name: String,
    pub counts: Counts,
    pub assets: Vec<EntityView>,
    pub resources: Vec<EntityView>,
    pub processes: Vec<EntityView>,
    pub services: Vec<ServiceView>,
    pub plan: PlanView,
    pub external_dependencies: Vec<DependencyView>,
}

fn base_model() -> Result<Model, String> {
    let doc: ModelDoc = toml::from_str(BASE_01_MODEL).map_err(|error| format!("Base-01 model: {error}"))?;
    Model::from_doc(doc).map_err(|error| format!("Base-01 model: {error}"))
}

/// A small, deliberate demo state: the pump is degraded, so the planner has a
/// real repair route to produce.
fn degraded_model(model: &Model) -> Result<Model, String> {
    let mut patch = Patch::default();
    patch.asset_status.insert("pump-p1".to_string(), AssetStatus::Degraded);
    model.patched(&patch).map_err(|error| format!("Base-01 patch: {error}"))
}

fn verification(value: Verification) -> String {
    value.to_string()
}

pub fn overview() -> Result<DomainOverview, String> {
    let base = base_model()?;
    let model = degraded_model(&base)?;
    let min_verification = Verification::Modeled;

    let mut services = Vec::new();
    for (id, service) in &model.services {
        let query = Query { goal: Goal::Service(id.clone()), min_verification, max_depth: DEFAULT_MAX_DEPTH };
        let result = graph::solve(&model, &query);
        let blockers =
            result.root_causes().map(|blocker| format!("{} {}", blocker.kind.as_str(), blocker.subject)).collect();
        services.push(ServiceView {
            id: id.clone(),
            label: service.id.clone(),
            verdict: result.verdict.as_str().to_string(),
            steps: result.steps.len(),
            blockers,
        });
    }

    let water_goal = Goal::Service("potable-water".to_string());
    let water_query = Query { goal: water_goal.clone(), min_verification, max_depth: DEFAULT_MAX_DEPTH };
    let dependencies = graph::external_dependency_report(&model, &water_query, 8)
        .dependencies
        .iter()
        .map(|dependency| DependencyView {
            resource: dependency.resource.clone(),
            max_si: dependency.max_si,
            in_root_causes: dependency.in_root_causes,
        })
        .collect();

    let planned = plan(&PlanningRequest {
        model: &model,
        now_s: 0.0,
        horizon_s: 86_400.0,
        goals: vec![PlanningGoal { goal: water_goal.clone(), deadline_s: Some(4.0 * 3600.0), priority: 10 }],
        min_verification: Verification::Demonstrated,
        max_actions: 12,
    })
    .map_err(|error| format!("Base-01 planning: {error}"))?;

    let actions = planned
        .actions
        .iter()
        .map(|action| ActionView {
            id: action.action_id.clone(),
            process: action.process.clone(),
            target: action.target.clone(),
            start_s: action.start_s,
            end_s: action.end_s,
            runs: action.runs,
            verification: verification(action.verification),
            preconditions: action.preconditions.clone(),
            allocations: action.allocations.clone(),
            occupies: action.occupies.clone(),
        })
        .collect();

    let assets = model
        .assets
        .values()
        .map(|asset| EntityView {
            id: asset.id.clone(),
            kind: "asset".to_string(),
            status: match asset.status {
                AssetStatus::Available => "available",
                AssetStatus::Degraded => "degraded",
                AssetStatus::Failed => "failed",
                AssetStatus::Unknown => "unknown",
            }
            .to_string(),
        })
        .collect();
    let resources = model
        .resources
        .keys()
        .map(|id| EntityView { id: id.clone(), kind: "resource".to_string(), status: "declared".to_string() })
        .collect();
    let processes = model
        .processes
        .iter()
        .map(|process| EntityView {
            id: process.id.clone(),
            kind: "process".to_string(),
            status: "declared".to_string(),
        })
        .collect();

    Ok(DomainOverview {
        scenario: "Base-01 · pump degraded".to_string(),
        model_name: base.name.clone(),
        counts: Counts {
            services: base.services.len(),
            assets: base.assets.len(),
            resources: base.resources.len(),
            processes: base.processes.len(),
            lots: base.lots.len(),
        },
        assets,
        resources,
        processes,
        services,
        plan: PlanView {
            plan_id: planned.plan_id.clone(),
            goal: "potable-water".to_string(),
            horizon_s: planned.horizon_s,
            completion_s: planned.completion_s,
            digest: planned.digest(),
            actions,
            goals_met: planned.goals_met.clone(),
            goals_unmet: planned.goals_unmet.iter().map(|failure| failure.goal.clone()).collect(),
        },
        external_dependencies: dependencies,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overview_uses_the_embedded_domain_model() {
        let overview = overview().unwrap();
        assert!(!overview.services.is_empty());
        assert_eq!(overview.services.iter().filter(|service| service.id == "potable-water").count(), 1);
        assert!(overview.counts.assets >= 9);
        assert!(!overview.plan.digest.is_empty());
    }

    #[test]
    fn degraded_pump_produces_a_plan() {
        let overview = overview().unwrap();
        assert!(!overview.plan.actions.is_empty(), "pump degraded should require work");
        assert!(overview
            .plan
            .actions
            .iter()
            .any(|action| action.target == "pump-p1" || action.process.contains("sleeve")));
    }
}
