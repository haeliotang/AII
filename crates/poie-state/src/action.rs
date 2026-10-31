//! Actions and their lifecycle (SAP §3).
//!
//! The transition table is the normative one from SAP §3.3. Transitions that are
//! not in the table are rejected; terminal states never change; and one
//! idempotency key may be dispatched by at most one action on a site.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::record::Timestamp;

/// Action lifecycle states (SAP §3.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ActionState {
    Proposed,
    Validated,
    Authorized,
    Dispatched,
    Acknowledged,
    Verified,
    Rejected,
    Expired,
    Cancelled,
    Failed,
    OutcomeUnknown,
}

impl ActionState {
    pub fn as_str(self) -> &'static str {
        match self {
            ActionState::Proposed => "proposed",
            ActionState::Validated => "validated",
            ActionState::Authorized => "authorized",
            ActionState::Dispatched => "dispatched",
            ActionState::Acknowledged => "acknowledged",
            ActionState::Verified => "verified",
            ActionState::Rejected => "rejected",
            ActionState::Expired => "expired",
            ActionState::Cancelled => "cancelled",
            ActionState::Failed => "failed",
            ActionState::OutcomeUnknown => "outcome_unknown",
        }
    }

    pub fn parse(value: &str) -> Option<ActionState> {
        Some(match value {
            "proposed" => ActionState::Proposed,
            "validated" => ActionState::Validated,
            "authorized" => ActionState::Authorized,
            "dispatched" => ActionState::Dispatched,
            "acknowledged" => ActionState::Acknowledged,
            "verified" => ActionState::Verified,
            "rejected" => ActionState::Rejected,
            "expired" => ActionState::Expired,
            "cancelled" => ActionState::Cancelled,
            "failed" => ActionState::Failed,
            "outcome_unknown" => ActionState::OutcomeUnknown,
            _ => return None,
        })
    }

    /// Terminal states (SAP §3.2). `outcome_unknown` is explicitly not terminal.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            ActionState::Verified
                | ActionState::Rejected
                | ActionState::Expired
                | ActionState::Cancelled
                | ActionState::Failed
        )
    }

    pub fn is_pending_outcome(self) -> bool {
        self == ActionState::OutcomeUnknown
    }
}

impl fmt::Display for ActionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Who triggers a lifecycle transition (SAP §3.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trigger {
    Validator,
    Authorizer,
    Dispatcher,
    Runtime,
    ExecutorReceipt,
    Verifier,
    PlannerRevision,
}

impl Trigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Trigger::Validator => "validator",
            Trigger::Authorizer => "authorizer",
            Trigger::Dispatcher => "dispatcher",
            Trigger::Runtime => "runtime",
            Trigger::ExecutorReceipt => "executor_receipt",
            Trigger::Verifier => "verifier",
            Trigger::PlannerRevision => "planner_revision",
        }
    }
}

/// The exhaustive transition table from SAP §3.3.
pub const TRANSITIONS: [(ActionState, ActionState, Trigger); 18] = [
    (ActionState::Proposed, ActionState::Validated, Trigger::Validator),
    (ActionState::Proposed, ActionState::Rejected, Trigger::Validator),
    (ActionState::Proposed, ActionState::Cancelled, Trigger::PlannerRevision),
    (ActionState::Validated, ActionState::Authorized, Trigger::Authorizer),
    (ActionState::Validated, ActionState::Rejected, Trigger::Authorizer),
    (ActionState::Validated, ActionState::Cancelled, Trigger::PlannerRevision),
    (ActionState::Authorized, ActionState::Dispatched, Trigger::Dispatcher),
    (ActionState::Authorized, ActionState::Rejected, Trigger::Dispatcher),
    (ActionState::Authorized, ActionState::Expired, Trigger::Runtime),
    (ActionState::Authorized, ActionState::Cancelled, Trigger::PlannerRevision),
    (ActionState::Dispatched, ActionState::Acknowledged, Trigger::ExecutorReceipt),
    (ActionState::Dispatched, ActionState::Failed, Trigger::ExecutorReceipt),
    (ActionState::Dispatched, ActionState::OutcomeUnknown, Trigger::Runtime),
    (ActionState::Acknowledged, ActionState::Verified, Trigger::Verifier),
    (ActionState::Acknowledged, ActionState::Failed, Trigger::Verifier),
    (ActionState::Acknowledged, ActionState::OutcomeUnknown, Trigger::Runtime),
    (ActionState::OutcomeUnknown, ActionState::Verified, Trigger::Verifier),
    (ActionState::OutcomeUnknown, ActionState::Failed, Trigger::Verifier),
];

/// Whether `from -> to` is in the normative transition table.
pub fn is_allowed(from: ActionState, to: ActionState) -> bool {
    TRANSITIONS.iter().any(|&(f, t, _)| f == from && t == to)
}

/// The trigger declared for `from -> to`, if the transition is legal.
pub fn trigger_for(from: ActionState, to: ActionState) -> Option<Trigger> {
    TRANSITIONS.iter().find(|&&(f, t, _)| f == from && t == to).map(|&(_, _, trigger)| trigger)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reversibility {
    Idempotent,
    Reversible,
    Compensable,
    Irreversible,
}

impl Reversibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Reversibility::Idempotent => "idempotent",
            Reversibility::Reversible => "reversible",
            Reversibility::Compensable => "compensable",
            Reversibility::Irreversible => "irreversible",
        }
    }
}

/// One predicted resource consumption of an action (SAP §3.1, `budget`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BudgetItem {
    pub resource: String,
    pub amount_si: f64,
}

/// What must hold on a referenced state version before dispatch (SAP §3.1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Precondition {
    /// State version vector: per-site maximum sequence number.
    pub state_version: BTreeMap<String, u64>,
    pub predicates: Vec<String>,
}

/// How the effect is confirmed (SAP §3.1, `verification_spec`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VerificationSpec {
    pub observe: String,
    pub condition: String,
    pub window_ms: u64,
}

/// Authorization record attached to an action after `validated -> authorized`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Authorization {
    pub by: String,
    pub basis: String,
    pub scope: String,
    pub valid_until: Timestamp,
}

/// An action as defined in SAP §3.1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub action_id: String,
    pub idempotency_key: String,
    pub target: String,
    pub command: String,
    pub plan_ref: Option<String>,
    pub policy_version: String,
    pub precondition: Precondition,
    pub valid_from: Timestamp,
    pub valid_until: Timestamp,
    pub budget: Vec<BudgetItem>,
    pub reservations: Vec<String>,
    pub reversibility: Reversibility,
    pub verification_spec: VerificationSpec,
    pub authorization: Option<Authorization>,
}

impl Action {
    /// Structural validation of the action fields (SAP §3.1). Guard evaluation
    /// against a state version happens at validation and dispatch time.
    pub fn validate(&self) -> Result<(), ActionError> {
        for (field, value) in [
            ("action_id", self.action_id.as_str()),
            ("idempotency_key", self.idempotency_key.as_str()),
            ("target", self.target.as_str()),
            ("command", self.command.as_str()),
            ("policy_version", self.policy_version.as_str()),
        ] {
            if value.is_empty() {
                return Err(ActionError::EmptyField(field));
            }
        }
        if self.valid_from > self.valid_until {
            return Err(ActionError::InvalidValidity);
        }
        if self.precondition.state_version.is_empty() {
            return Err(ActionError::EmptyStateVersion);
        }
        if self.verification_spec.observe.is_empty() || self.verification_spec.condition.is_empty() {
            return Err(ActionError::EmptyField("verification_spec"));
        }
        if self.verification_spec.window_ms == 0 {
            return Err(ActionError::InvalidVerificationWindow);
        }
        for item in &self.budget {
            if !item.amount_si.is_finite() || item.amount_si <= 0.0 || item.resource.is_empty() {
                return Err(ActionError::InvalidBudget);
            }
        }
        if let Some(auth) = &self.authorization {
            if auth.by.is_empty() || auth.basis.is_empty() || auth.scope.is_empty() {
                return Err(ActionError::EmptyField("authorization"));
            }
            if auth.valid_until < self.valid_from {
                return Err(ActionError::ExpiredAuthorization);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ActionError {
    EmptyField(&'static str),
    InvalidValidity,
    EmptyStateVersion,
    InvalidVerificationWindow,
    InvalidBudget,
    ExpiredAuthorization,
}

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActionError::EmptyField(field) => write!(f, "field {field:?} must not be empty"),
            ActionError::InvalidValidity => f.write_str("valid_from must not be after valid_until"),
            ActionError::EmptyStateVersion => f.write_str("precondition must reference a state version"),
            ActionError::InvalidVerificationWindow => f.write_str("verification window must be positive"),
            ActionError::InvalidBudget => f.write_str("budget items need a resource and a positive finite amount"),
            ActionError::ExpiredAuthorization => f.write_str("authorization expires before the action is valid"),
        }
    }
}

impl std::error::Error for ActionError {}

/// A site-local set of actions and their states.
///
/// This is the M4.4 state-machine core: it enforces the transition table and
/// the idempotency-key dispatch invariant. It does not persist, dispatch or
/// review yet.
#[derive(Debug, Default)]
pub struct ActionLedger {
    states: BTreeMap<String, ActionState>,
    keys: BTreeMap<String, String>,
    dispatched_keys: BTreeMap<String, String>,
}

impl ActionLedger {
    pub fn new() -> ActionLedger {
        ActionLedger::default()
    }

    /// Adds a proposed action.
    pub fn add(&mut self, action: &Action) -> Result<(), LedgerError> {
        action.validate().map_err(LedgerError::InvalidAction)?;
        if self.states.contains_key(&action.action_id) {
            return Err(LedgerError::DuplicateAction(action.action_id.clone()));
        }
        self.keys.insert(action.action_id.clone(), action.idempotency_key.clone());
        self.states.insert(action.action_id.clone(), ActionState::Proposed);
        Ok(())
    }

    pub fn state(&self, action_id: &str) -> Option<ActionState> {
        self.states.get(action_id).copied()
    }

    /// The action that dispatched `idempotency_key`, if any.
    pub fn dispatched_by(&self, idempotency_key: &str) -> Option<&str> {
        self.dispatched_keys.get(idempotency_key).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.states.len()
    }

    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    /// Applies a lifecycle transition. Rejects unknown actions, transitions not in
    /// the normative table, and a second action dispatching an already-dispatched
    /// idempotency key.
    pub fn apply(&mut self, action_id: &str, to: ActionState) -> Result<ActionState, LedgerError> {
        let Some(from) = self.states.get(action_id).copied() else {
            return Err(LedgerError::UnknownAction(action_id.to_string()));
        };
        if !is_allowed(from, to) {
            return Err(LedgerError::IllegalTransition { action_id: action_id.to_string(), from, to });
        }
        if to == ActionState::Dispatched {
            let key =
                self.keys.get(action_id).cloned().ok_or_else(|| LedgerError::UnknownAction(action_id.to_string()))?;
            if let Some(existing) = self.dispatched_keys.get(&key) {
                if existing != action_id {
                    return Err(LedgerError::DuplicateDispatch {
                        idempotency_key: key,
                        existing: existing.clone(),
                        action_id: action_id.to_string(),
                    });
                }
            }
            self.dispatched_keys.insert(key, action_id.to_string());
        }
        self.states.insert(action_id.to_string(), to);
        Ok(to)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LedgerError {
    InvalidAction(ActionError),
    DuplicateAction(String),
    UnknownAction(String),
    IllegalTransition { action_id: String, from: ActionState, to: ActionState },
    DuplicateDispatch { idempotency_key: String, existing: String, action_id: String },
}

impl fmt::Display for LedgerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LedgerError::InvalidAction(error) => write!(f, "invalid action: {error}"),
            LedgerError::DuplicateAction(id) => write!(f, "action {id:?} already exists"),
            LedgerError::UnknownAction(id) => write!(f, "unknown action {id:?}"),
            LedgerError::IllegalTransition { action_id, from, to } => {
                write!(f, "action {action_id:?}: transition {from} -> {to} is not in the table")
            }
            LedgerError::DuplicateDispatch { idempotency_key, existing, action_id } => write!(
                f,
                "idempotency key {idempotency_key:?} already dispatched by {existing:?}; {action_id:?} must not dispatch it again"
            ),
        }
    }
}

impl std::error::Error for LedgerError {}

#[cfg(test)]
mod tests {
    use super::*;

    const STATES: [ActionState; 11] = [
        ActionState::Proposed,
        ActionState::Validated,
        ActionState::Authorized,
        ActionState::Dispatched,
        ActionState::Acknowledged,
        ActionState::Verified,
        ActionState::Rejected,
        ActionState::Expired,
        ActionState::Cancelled,
        ActionState::Failed,
        ActionState::OutcomeUnknown,
    ];

    fn action(id: &str, key: &str) -> Action {
        Action {
            action_id: id.to_string(),
            idempotency_key: key.to_string(),
            target: "pump-p1".to_string(),
            command: "replace-sleeve".to_string(),
            plan_ref: Some("plan-7".to_string()),
            policy_version: "policy-3".to_string(),
            precondition: Precondition {
                state_version: BTreeMap::from([("site-a".to_string(), 41)]),
                predicates: vec!["pump-p1.status == degraded".to_string()],
            },
            valid_from: Timestamp::from_millis(1_000),
            valid_until: Timestamp::from_millis(2_000),
            budget: vec![BudgetItem { resource: "electric-energy".to_string(), amount_si: 1.8e6 }],
            reservations: vec!["lot-sleeve-01".to_string()],
            reversibility: Reversibility::Idempotent,
            verification_spec: VerificationSpec {
                observe: "pump-p1.status".to_string(),
                condition: "available".to_string(),
                window_ms: 600_000,
            },
            authorization: None,
        }
    }

    #[test]
    fn the_table_has_exactly_the_eighteen_normative_transitions() {
        assert_eq!(TRANSITIONS.len(), 18);
        let unique: BTreeSet<(ActionState, ActionState)> = TRANSITIONS.iter().map(|&(f, t, _)| (f, t)).collect();
        assert_eq!(unique.len(), 18, "no duplicate rows");
        for (from, to, _) in TRANSITIONS {
            assert!(is_allowed(from, to));
            assert!(!from.is_terminal(), "terminal states must not have outgoing transitions");
        }
    }

    #[test]
    fn terminal_states_are_final_and_allowed_elsewhere() {
        for state in STATES {
            let outgoing = TRANSITIONS.iter().filter(|&&(from, _, _)| from == state).count();
            if state.is_terminal() {
                assert_eq!(outgoing, 0, "{state} must be terminal");
            } else {
                assert!(outgoing > 0, "{state} must have an outgoing transition");
            }
        }
    }

    #[test]
    fn illegal_transitions_and_unknown_actions_are_rejected() {
        let mut ledger = ActionLedger::new();
        ledger.add(&action("a1", "k1")).unwrap();
        assert!(matches!(ledger.apply("a1", ActionState::Verified), Err(LedgerError::IllegalTransition { .. })));
        assert!(matches!(ledger.apply("missing", ActionState::Validated), Err(LedgerError::UnknownAction(_))));
        ledger.apply("a1", ActionState::Cancelled).unwrap();
        assert!(matches!(ledger.apply("a1", ActionState::Validated), Err(LedgerError::IllegalTransition { .. })));
    }

    #[test]
    fn duplicate_idempotency_key_dispatch_is_rejected() {
        let mut ledger = ActionLedger::new();
        ledger.add(&action("a1", "shared-key")).unwrap();
        ledger.add(&action("a2", "shared-key")).unwrap();
        for state in [ActionState::Validated, ActionState::Authorized, ActionState::Dispatched] {
            ledger.apply("a1", state).unwrap();
        }
        ledger.apply("a2", ActionState::Validated).unwrap();
        ledger.apply("a2", ActionState::Authorized).unwrap();
        let error = ledger.apply("a2", ActionState::Dispatched).unwrap_err();
        assert!(matches!(error, LedgerError::DuplicateDispatch { .. }), "{error}");
        assert_eq!(ledger.dispatched_by("shared-key"), Some("a1"));
        assert_eq!(ledger.state("a2"), Some(ActionState::Authorized));
    }

    #[test]
    fn action_validation_rejects_bad_fields() {
        let mut bad = action("a1", "k1");
        bad.valid_until = Timestamp::from_millis(500);
        assert_eq!(bad.validate(), Err(ActionError::InvalidValidity));

        let mut bad = action("a1", "k1");
        bad.precondition.state_version.clear();
        assert_eq!(bad.validate(), Err(ActionError::EmptyStateVersion));

        let mut bad = action("a1", "k1");
        bad.verification_spec.window_ms = 0;
        assert_eq!(bad.validate(), Err(ActionError::InvalidVerificationWindow));

        let mut bad = action("a1", "k1");
        bad.budget[0].amount_si = 0.0;
        assert_eq!(bad.validate(), Err(ActionError::InvalidBudget));

        let mut bad = action("a1", "k1");
        bad.authorization = Some(Authorization {
            by: "operator-1".to_string(),
            basis: "policy-3".to_string(),
            scope: "pump-p1".to_string(),
            valid_until: Timestamp::from_millis(900),
        });
        assert_eq!(bad.validate(), Err(ActionError::ExpiredAuthorization));
    }

    use std::collections::BTreeSet;

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }

        fn below(&mut self, bound: usize) -> usize {
            (self.next() % bound as u64) as usize
        }
    }

    fn allowed_targets(state: ActionState) -> Vec<ActionState> {
        TRANSITIONS.iter().filter(|&&(from, _, _)| from == state).map(|&(_, to, _)| to).collect()
    }

    /// The forward step of the nominal lifecycle, used to bias the walk.
    fn preferred(state: ActionState) -> Option<ActionState> {
        Some(match state {
            ActionState::Proposed => ActionState::Validated,
            ActionState::Validated => ActionState::Authorized,
            ActionState::Authorized => ActionState::Dispatched,
            ActionState::Dispatched => ActionState::Acknowledged,
            ActionState::Acknowledged | ActionState::OutcomeUnknown => ActionState::Verified,
            _ => return None,
        })
    }

    #[test]
    fn random_event_sequences_never_leave_the_table_or_dispatch_a_key_twice() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let mut ledger = ActionLedger::new();
        for index in 0..8 {
            ledger.add(&action(&format!("a{index}"), &format!("k{}", index % 3))).unwrap();
        }
        // Shadow of which action dispatched each key, for an independent check.
        let mut shadow: BTreeMap<String, String> = BTreeMap::new();
        let mut illegal_rejections = 0usize;
        let mut duplicate_rejections = 0usize;
        let mut successful_transitions = 0usize;
        let mut dispatches = 0usize;

        for _ in 0..100_000 {
            let action_index = rng.below(8);
            let action_id = format!("a{action_index}");
            let before = ledger.state(&action_id).unwrap();
            // Mostly follow the nominal lifecycle (so actions reach dispatch),
            // sometimes take any legal transition, sometimes attempt an
            // arbitrary state to exercise rejection.
            let roll = rng.below(100);
            let to = if !before.is_terminal() && roll < 70 {
                preferred(before).unwrap_or_else(|| allowed_targets(before)[0])
            } else if !before.is_terminal() && roll < 85 {
                let targets = allowed_targets(before);
                targets[rng.below(targets.len())]
            } else {
                STATES[rng.below(STATES.len())]
            };
            match ledger.apply(&action_id, to) {
                Ok(actual) => {
                    assert_eq!(actual, to);
                    assert!(is_allowed(before, to), "out-of-table transition succeeded: {before} -> {to}");
                    successful_transitions += 1;
                    if to == ActionState::Dispatched {
                        dispatches += 1;
                        let key = format!("k{}", action_index % 3);
                        if let Some(existing) = shadow.get(&key) {
                            panic!("key {key} dispatched twice: {existing} and {action_id}");
                        }
                        shadow.insert(key, action_id.clone());
                    }
                }
                Err(error) => {
                    assert_eq!(ledger.state(&action_id), Some(before), "rejected transition must not change state");
                    match error {
                        LedgerError::IllegalTransition { .. } => illegal_rejections += 1,
                        LedgerError::DuplicateDispatch { .. } => duplicate_rejections += 1,
                        other => panic!("unexpected rejection: {other}"),
                    }
                }
            }
        }

        assert!(illegal_rejections > 0);
        assert!(duplicate_rejections > 0, "the generated sequences must exercise a duplicate key dispatch");
        assert!(successful_transitions > 0 && dispatches > 0);
        for (key, action_id) in &shadow {
            assert_eq!(ledger.dispatched_by(key), Some(action_id.as_str()));
        }
        assert_eq!(ledger.len(), 8);
    }
}
