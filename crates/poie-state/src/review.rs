//! Dispatch review: the six checks required before `authorized -> dispatched`
//! (SAP §4).
//!
//! Every failed check produces a `DispatchRejection` with a stable reason code
//! and a human-readable detail, so a rejection can be traced to its cause. The
//! caller turns any rejection into `authorized -> rejected`; nothing here sends
//! or changes a command.

use std::collections::{BTreeMap, BTreeSet};

use crate::action::{Action, ActionLedger};

/// The fresh state the dispatcher reviews against (SAP §4).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DispatchContext {
    /// Current time in milliseconds.
    pub now_ms: i64,
    /// Current state version vector: per-site maximum sequence number.
    pub current_state_version: BTreeMap<String, u64>,
    /// This site holds command ownership for the action's target (SAP §6.1).
    pub holds_command_ownership: bool,
    /// The reservations listed on the action are still held by it.
    pub reservations_held: bool,
    /// Precondition predicates that actually hold on the current state.
    pub predicates_holding: BTreeSet<String>,
    /// The authorization's policy version has been revoked.
    pub authorization_revoked: bool,
    /// When ownership is a lease: the last instant this site may dispatch,
    /// `lease.until - skew_bound` (SAP §4 item 3, §6.1). `None` for local
    /// ownership without a lease.
    pub ownership_dispatch_deadline_ms: Option<i64>,
    /// The executor's declared dedup-record retention (SAP §4 item 7). `None`
    /// when the executor itself refuses commands whose remaining validity
    /// exceeds its window.
    pub executor_dedup_window_ms: Option<i64>,
}

/// Which of the six checks failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewCheck {
    TimeWindow,
    Preconditions,
    CommandOwnership,
    Reservations,
    IdempotencyKey,
    Authorization,
}

impl ReviewCheck {
    pub fn as_str(self) -> &'static str {
        match self {
            ReviewCheck::TimeWindow => "time_window",
            ReviewCheck::Preconditions => "preconditions",
            ReviewCheck::CommandOwnership => "command_ownership",
            ReviewCheck::Reservations => "reservations",
            ReviewCheck::IdempotencyKey => "idempotency_key",
            ReviewCheck::Authorization => "authorization",
        }
    }
}

/// One traceable reason a dispatch must not proceed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispatchRejection {
    pub check: ReviewCheck,
    /// Stable reason code (usable in logs and tests).
    pub reason: &'static str,
    pub detail: String,
}

/// Runs all six checks. Returns every failure, not just the first, so the
/// rejection record can explain all stale conditions at once.
pub fn review_dispatch(
    action: &Action,
    ledger: &ActionLedger,
    context: &DispatchContext,
) -> Result<(), Vec<DispatchRejection>> {
    let mut rejections = Vec::new();

    if action.valid_from.0 > context.now_ms {
        rejections.push(DispatchRejection {
            check: ReviewCheck::TimeWindow,
            reason: "not_yet_valid",
            detail: format!("action valid from {} ms, current time {} ms", action.valid_from.0, context.now_ms),
        });
    }
    if action.valid_until.0 < context.now_ms {
        rejections.push(DispatchRejection {
            check: ReviewCheck::TimeWindow,
            reason: "expired",
            detail: format!("action expired at {} ms, current time {} ms", action.valid_until.0, context.now_ms),
        });
    }

    let mut behind = Vec::new();
    for (site, seq) in &action.precondition.state_version {
        match context.current_state_version.get(site) {
            Some(current) if current >= seq => {}
            Some(current) => behind.push(format!("{site}: referenced {seq}, current {current}")),
            None => behind.push(format!("{site}: referenced {seq}, current unknown")),
        }
    }
    if !behind.is_empty() {
        rejections.push(DispatchRejection {
            check: ReviewCheck::Preconditions,
            reason: "stale_precondition",
            detail: format!("state version is behind the preconditions: {}", behind.join(", ")),
        });
    }
    let missing: Vec<&str> = action
        .precondition
        .predicates
        .iter()
        .filter(|predicate| !context.predicates_holding.contains(*predicate))
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        rejections.push(DispatchRejection {
            check: ReviewCheck::Preconditions,
            reason: "stale_precondition",
            detail: format!("predicates no longer hold on the current state: {}", missing.join(", ")),
        });
    }

    if !context.holds_command_ownership {
        rejections.push(DispatchRejection {
            check: ReviewCheck::CommandOwnership,
            reason: "no_command_ownership",
            detail: format!("this site does not hold command ownership for {}", action.target),
        });
    }
    if !context.reservations_held {
        rejections.push(DispatchRejection {
            check: ReviewCheck::Reservations,
            reason: "reservations_not_held",
            detail: "the action's reservations are no longer held".to_string(),
        });
    }
    if let Some(existing) = ledger.dispatched_by(&action.idempotency_key) {
        rejections.push(DispatchRejection {
            check: ReviewCheck::IdempotencyKey,
            reason: "duplicate_idempotency_key",
            detail: format!("idempotency key already dispatched by {existing:?}"),
        });
    }
    match &action.authorization {
        None => rejections.push(DispatchRejection {
            check: ReviewCheck::Authorization,
            reason: "missing_authorization",
            detail: "the action has no authorization record".to_string(),
        }),
        Some(authorization) => {
            if authorization.valid_until.0 < context.now_ms {
                rejections.push(DispatchRejection {
                    check: ReviewCheck::Authorization,
                    reason: "authorization_expired",
                    detail: format!(
                        "authorization expired at {} ms, current time {} ms",
                        authorization.valid_until.0, context.now_ms
                    ),
                });
            }
            if context.authorization_revoked {
                rejections.push(DispatchRejection {
                    check: ReviewCheck::Authorization,
                    reason: "authorization_revoked",
                    detail: "the authorization's policy version is revoked".to_string(),
                });
            }
        }
    }

    if rejections.is_empty() {
        Ok(())
    } else {
        Err(rejections)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{ActionState, Authorization, BudgetItem, Precondition, Reversibility, VerificationSpec};
    use crate::record::Timestamp;

    fn action(id: &str, key: &str) -> Action {
        Action {
            action_id: id.to_string(),
            idempotency_key: key.to_string(),
            target: "pump-p1".to_string(),
            command: "replace-sleeve".to_string(),
            plan_ref: None,
            policy_version: "policy-3".to_string(),
            precondition: Precondition {
                state_version: BTreeMap::from([("site-a".to_string(), 10)]),
                predicates: vec!["pump-p1.status == degraded".to_string()],
            },
            valid_from: Timestamp(1_000),
            valid_until: Timestamp(10_000),
            budget: vec![BudgetItem { resource: "electric-energy".to_string(), amount_si: 1.8e6 }],
            reservations: vec!["lot-sleeve-01".to_string()],
            reversibility: Reversibility::Idempotent,
            verification_spec: VerificationSpec {
                observe: "pump-p1.status".to_string(),
                condition: "available".to_string(),
                window_ms: 600_000,
            },
            authorization: Some(Authorization {
                by: "operator-1".to_string(),
                basis: "policy-3".to_string(),
                scope: "pump-p1".to_string(),
                valid_until: Timestamp(9_000),
            }),
            redispatch_of: None,
        }
    }

    fn valid_context() -> DispatchContext {
        DispatchContext {
            now_ms: 5_000,
            current_state_version: BTreeMap::from([("site-a".to_string(), 12)]),
            holds_command_ownership: true,
            reservations_held: true,
            predicates_holding: BTreeSet::from(["pump-p1.status == degraded".to_string()]),
            authorization_revoked: false,
            ownership_dispatch_deadline_ms: None,
            executor_dedup_window_ms: None,
        }
    }

    #[test]
    fn a_fully_valid_dispatch_is_approved() {
        let ledger = ActionLedger::new();
        assert_eq!(review_dispatch(&action("a1", "k1"), &ledger, &valid_context()), Ok(()));
    }

    #[test]
    fn time_window_failures_are_reported() {
        let ledger = ActionLedger::new();
        let mut context = valid_context();
        context.now_ms = 500;
        let rejections = review_dispatch(&action("a1", "k1"), &ledger, &context).unwrap_err();
        assert!(rejections.iter().any(|r| r.reason == "not_yet_valid"));

        let mut context = valid_context();
        context.now_ms = 20_000;
        let rejections = review_dispatch(&action("a1", "k1"), &ledger, &context).unwrap_err();
        assert!(rejections.iter().any(|r| r.reason == "expired"));
    }

    #[test]
    fn stale_state_version_and_predicates_are_reported() {
        let ledger = ActionLedger::new();
        let mut context = valid_context();
        context.current_state_version.insert("site-a".to_string(), 3);
        let rejections = review_dispatch(&action("a1", "k1"), &ledger, &context).unwrap_err();
        assert!(rejections.iter().any(|r| r.reason == "stale_precondition" && r.detail.contains("state version")));

        let mut context = valid_context();
        context.predicates_holding.clear();
        let rejections = review_dispatch(&action("a1", "k1"), &ledger, &context).unwrap_err();
        assert!(rejections.iter().any(|r| r.reason == "stale_precondition" && r.detail.contains("predicates")));
    }

    #[test]
    fn ownership_and_reservation_failures_are_reported() {
        let ledger = ActionLedger::new();
        let mut context = valid_context();
        context.holds_command_ownership = false;
        context.reservations_held = false;
        let rejections = review_dispatch(&action("a1", "k1"), &ledger, &context).unwrap_err();
        assert!(rejections.iter().any(|r| r.reason == "no_command_ownership"));
        assert!(rejections.iter().any(|r| r.reason == "reservations_not_held"));
    }

    #[test]
    fn duplicate_idempotency_key_is_rejected_by_review() {
        let mut ledger = ActionLedger::new();
        ledger.add(&action("a1", "shared")).unwrap();
        ledger.add(&action("a2", "shared")).unwrap();
        for state in [ActionState::Validated, ActionState::Authorized, ActionState::Dispatched] {
            ledger.apply("a1", state).unwrap();
        }
        ledger.apply("a2", ActionState::Validated).unwrap();
        ledger.apply("a2", ActionState::Authorized).unwrap();
        let rejections = review_dispatch(&action("a2", "shared"), &ledger, &valid_context()).unwrap_err();
        assert!(rejections.iter().any(|r| r.reason == "duplicate_idempotency_key"));
    }

    #[test]
    fn authorization_failures_are_reported() {
        let ledger = ActionLedger::new();
        let mut missing = action("a1", "k1");
        missing.authorization = None;
        let rejections = review_dispatch(&missing, &ledger, &valid_context()).unwrap_err();
        assert!(rejections.iter().any(|r| r.reason == "missing_authorization"));

        let mut expired = action("a1", "k1");
        if let Some(authorization) = &mut expired.authorization {
            authorization.valid_until = Timestamp(4_000);
        }
        let rejections = review_dispatch(&expired, &ledger, &valid_context()).unwrap_err();
        assert!(rejections.iter().any(|r| r.reason == "authorization_expired"));

        let mut revoked = valid_context();
        revoked.authorization_revoked = true;
        let rejections = review_dispatch(&action("a1", "k1"), &ledger, &revoked).unwrap_err();
        assert!(rejections.iter().any(|r| r.reason == "authorization_revoked"));
    }

    #[test]
    fn every_failing_check_is_collected_for_traceability() {
        let ledger = ActionLedger::new();
        let mut context = valid_context();
        context.now_ms = 20_000;
        context.holds_command_ownership = false;
        context.reservations_held = false;
        context.predicates_holding.clear();
        context.authorization_revoked = true;
        let rejections = review_dispatch(&action("a1", "k1"), &ledger, &context).unwrap_err();
        assert!(rejections.len() >= 5, "{rejections:?}");
        let checks: BTreeSet<&str> = rejections.iter().map(|r| r.check.as_str()).collect();
        assert!(checks.contains("time_window"));
        assert!(checks.contains("preconditions"));
        assert!(checks.contains("command_ownership"));
        assert!(checks.contains("reservations"));
        assert!(checks.contains("authorization"));
    }
}
