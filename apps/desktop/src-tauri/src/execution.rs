//! Execution snapshot: the plan turned into a real SAP action ledger.
//!
//! Builds one `poie-state` action per planned action, validates/authorizes it,
//! and runs all seven dispatch-review checks against a fresh dispatch context.
//! This is a host-side simulation: no command is sent to an executor.

use std::collections::{BTreeMap, BTreeSet};

use poie_state::action::{
    Action, ActionLedger, ActionState, Authorization, BudgetItem, Precondition, Reversibility, VerificationSpec,
};
use poie_state::record::Timestamp;
use poie_state::review::{review_dispatch, DispatchContext, ReviewCheck};
use serde::Serialize;

use crate::domain::DomainOverview;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionActionView {
    pub id: String,
    pub process: String,
    pub target: String,
    pub state: String,
    pub idempotency_key: String,
    pub verification: String,
    pub valid_from_ms: i64,
    pub valid_until_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckView {
    pub check: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionSnapshot {
    pub state: String,
    pub actions: Vec<ExecutionActionView>,
    pub checks: Vec<CheckView>,
    pub review_status: String,
    pub rejections: usize,
}

const CHECKS: [ReviewCheck; 7] = [
    ReviewCheck::TimeWindow,
    ReviewCheck::Preconditions,
    ReviewCheck::CommandOwnership,
    ReviewCheck::Reservations,
    ReviewCheck::IdempotencyKey,
    ReviewCheck::Authorization,
    ReviewCheck::DedupWindow,
];

fn build_action(source: &crate::domain::ActionView, plan_id: &str) -> Action {
    let budget = source
        .allocations
        .iter()
        .filter(|(_, amount)| *amount > 0.0)
        .map(|(resource, amount)| BudgetItem { resource: resource.clone(), amount_si: *amount })
        .collect();
    Action {
        action_id: source.id.clone(),
        idempotency_key: format!("idem:{}", source.id),
        target: source.target.clone(),
        command: source.process.clone(),
        plan_ref: Some(plan_id.to_string()),
        policy_version: "PO/SAP 2026-11-01".to_string(),
        precondition: Precondition {
            state_version: BTreeMap::from([("site-a".to_string(), 4)]),
            predicates: vec!["sleeve-p1 available".to_string(), "robot-m1 available".to_string()],
        },
        valid_from: Timestamp::from_millis(0),
        valid_until: Timestamp::from_millis(3_600_000),
        budget,
        reservations: Vec::new(),
        reversibility: Reversibility::Idempotent,
        verification_spec: VerificationSpec {
            observe: "pump-p1 status".to_string(),
            condition: "pump-p1 available".to_string(),
            window_ms: 600_000,
        },
        authorization: Some(Authorization {
            by: "maintainer".to_string(),
            basis: "issue #105 conservation fix".to_string(),
            scope: "dispatch".to_string(),
            valid_until: Timestamp::from_millis(7_200_000),
        }),
        redispatch_of: None,
    }
}

pub fn snapshot(overview: &DomainOverview) -> Result<ExecutionSnapshot, String> {
    let mut ledger = ActionLedger::new();
    let mut actions = Vec::new();
    let mut review_rejections = Vec::new();

    for source in &overview.plan.actions {
        let action = build_action(source, &overview.plan.plan_id);
        ledger.add(&action).map_err(|error| format!("action ledger add: {error}"))?;
        ledger.apply(&action.action_id, ActionState::Validated).map_err(|error| format!("validate action: {error}"))?;
        ledger
            .apply(&action.action_id, ActionState::Authorized)
            .map_err(|error| format!("authorize action: {error}"))?;

        let context = DispatchContext {
            now_ms: 1_000_000,
            current_state_version: BTreeMap::from([("site-a".to_string(), 4)]),
            holds_command_ownership: true,
            reservations_held: true,
            predicates_holding: BTreeSet::from(["sleeve-p1 available".to_string(), "robot-m1 available".to_string()]),
            authorization_revoked: false,
            ownership_dispatch_deadline_ms: None,
            executor_dedup_window_ms: None,
        };
        let result = review_dispatch(&action, &ledger, &context);
        if let Err(rejections) = &result {
            review_rejections.extend(rejections.iter().cloned());
        }
        let state = ledger.state(&action.action_id).unwrap_or(ActionState::Proposed);
        actions.push(ExecutionActionView {
            id: action.action_id,
            process: action.command,
            target: action.target,
            state: state.as_str().to_string(),
            idempotency_key: action.idempotency_key,
            verification: source.verification.clone(),
            valid_from_ms: action.valid_from.0,
            valid_until_ms: action.valid_until.0,
        });
    }

    let checks = CHECKS
        .iter()
        .map(|check| {
            let rejection = review_rejections.iter().find(|rejection| rejection.check == *check);
            CheckView {
                check: check.as_str().to_string(),
                passed: rejection.is_none(),
                detail: rejection.map(|rejection| rejection.detail.clone()).unwrap_or_else(|| "ok".to_string()),
            }
        })
        .collect();

    Ok(ExecutionSnapshot {
        state: if review_rejections.is_empty() { "ready".to_string() } else { "blocked".to_string() },
        actions,
        checks,
        review_status: if review_rejections.is_empty() { "ready" } else { "blocked" }.to_string(),
        rejections: review_rejections.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_actions_become_authorized_sap_actions() {
        let overview = crate::domain::overview().unwrap();
        let snapshot = snapshot(&overview).unwrap();
        assert!(!snapshot.actions.is_empty());
        assert_eq!(snapshot.review_status, "ready");
        assert_eq!(snapshot.rejections, 0);
        assert_eq!(snapshot.checks.len(), 7);
        assert!(snapshot.checks.iter().all(|check| check.passed));
        assert!(snapshot.actions.iter().all(|action| action.state == "authorized"));
        let json = serde_json::to_value(&snapshot).unwrap();
        assert!(json.get("reviewStatus").is_some());
        assert!(json.get("rejections").is_some());
        assert!(json["actions"][0].get("idempotencyKey").is_some());
    }
}
