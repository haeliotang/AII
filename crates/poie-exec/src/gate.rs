//! The execution gate (M10.2, M10.5).

use std::collections::BTreeSet;
use std::fmt;

use poie_state::action::{Action, ActionLedger, ActionState};

use crate::writable::{Command, ExecError, Receipt, WritableAdapter};

#[derive(Clone, Debug, PartialEq)]
pub enum DispatchOutcome {
    Dispatched {
        receipt: Receipt,
    },
    /// Human takeover: the command is recorded but not sent.
    RecordedOnly {
        reason: String,
    },
    /// Poieweave is unhealthy/degraded: fall back to the site's own controls.
    Fallback {
        reason: String,
    },
    Rejected {
        reason: String,
    },
}

impl DispatchOutcome {
    pub fn dispatched(&self) -> bool {
        matches!(self, DispatchOutcome::Dispatched { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GateError {
    NotAuthorized(String),
    UnknownAction(String),
    Adapter(String),
}

impl fmt::Display for GateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GateError::NotAuthorized(detail) => write!(f, "action is not authorized: {detail}"),
            GateError::UnknownAction(id) => write!(f, "unknown action {id:?}"),
            GateError::Adapter(detail) => write!(f, "adapter error: {detail}"),
        }
    }
}

impl std::error::Error for GateError {}

/// The single path from an authorized SAP action to a writable adapter.
#[derive(Default)]
pub struct ExecutionGate {
    ledger: ActionLedger,
    takeover: bool,
    degraded: bool,
    dispatched_keys: BTreeSet<String>,
}

impl ExecutionGate {
    pub fn new() -> ExecutionGate {
        ExecutionGate::default()
    }

    /// Takes human control; all further commands are recorded, never dispatched.
    pub fn take_over(&mut self) {
        self.takeover = true;
    }

    pub fn release(&mut self) {
        self.takeover = false;
    }

    pub fn is_takeover(&self) -> bool {
        self.takeover
    }

    /// Marks the runtime degraded (Poieweave unhealthy): commands fall back to
    /// the site's own controls and are not dispatched.
    pub fn set_degraded(&mut self, degraded: bool) {
        self.degraded = degraded;
    }

    pub fn is_degraded(&self) -> bool {
        self.degraded
    }

    pub fn ledger(&self) -> &ActionLedger {
        &self.ledger
    }

    /// Registers an action that a real authorizer has authorized.
    pub fn register_authorized(&mut self, action: &Action) -> Result<(), GateError> {
        action.validate().map_err(|error| GateError::NotAuthorized(error.to_string()))?;
        self.ledger.add(action).map_err(|error| GateError::NotAuthorized(error.to_string()))?;
        self.ledger
            .apply(&action.action_id, ActionState::Validated)
            .map_err(|error| GateError::NotAuthorized(error.to_string()))?;
        self.ledger
            .apply(&action.action_id, ActionState::Authorized)
            .map_err(|error| GateError::NotAuthorized(error.to_string()))?;
        Ok(())
    }

    /// The only dispatch path. Refuses dispatch when the action is not
    /// authorized, the validity window has passed, the key was already used, the
    /// operator has taken over, the runtime is degraded, or the adapter cannot
    /// honour an idempotency key.
    pub fn dispatch(
        &mut self,
        action: &Action,
        adapter: &mut dyn WritableAdapter,
        now_ms: i64,
    ) -> Result<DispatchOutcome, GateError> {
        let declaration = adapter.declaration().clone();
        declaration.validate().map_err(|error| GateError::Adapter(error.to_string()))?;
        match self.ledger.state(&action.action_id) {
            Some(ActionState::Authorized) => {}
            Some(other) => return Err(GateError::NotAuthorized(format!("state is {other}"))),
            None => return Err(GateError::UnknownAction(action.action_id.clone())),
        }
        if self.takeover {
            return Ok(DispatchOutcome::RecordedOnly { reason: "human takeover is active".into() });
        }
        if self.degraded {
            return Ok(DispatchOutcome::Fallback { reason: "runtime is degraded; site controls take over".into() });
        }
        if !declaration.supports_idempotency {
            return Ok(DispatchOutcome::Fallback {
                reason: "executor cannot deduplicate by idempotency key; dispatch refused (ADR-0007)".into(),
            });
        }
        if now_ms < action.valid_from.0 {
            return Ok(DispatchOutcome::Rejected { reason: "command is not yet valid".into() });
        }
        if now_ms > action.valid_until.0 {
            return Ok(DispatchOutcome::Rejected { reason: "command validity has expired".into() });
        }
        if self.dispatched_keys.contains(&action.idempotency_key)
            || self.ledger.dispatched_by(&action.idempotency_key).is_some()
        {
            return Ok(DispatchOutcome::Rejected { reason: "idempotency key already dispatched".into() });
        }
        let command = Command {
            action_id: action.action_id.clone(),
            idempotency_key: action.idempotency_key.clone(),
            target: action.target.clone(),
            command: action.command.clone(),
            valid_from_ms: action.valid_from.0,
            valid_until_ms: action.valid_until.0,
        };
        match adapter.dispatch(&command) {
            Ok(receipt) => {
                self.dispatched_keys.insert(action.idempotency_key.clone());
                self.ledger
                    .apply(&action.action_id, ActionState::Dispatched)
                    .map_err(|error| GateError::NotAuthorized(error.to_string()))?;
                if receipt.accepted {
                    self.ledger
                        .apply(&action.action_id, ActionState::Acknowledged)
                        .map_err(|error| GateError::NotAuthorized(error.to_string()))?;
                } else {
                    self.ledger
                        .apply(&action.action_id, ActionState::OutcomeUnknown)
                        .map_err(|error| GateError::NotAuthorized(error.to_string()))?;
                }
                Ok(DispatchOutcome::Dispatched { receipt })
            }
            Err(ExecError::Crashed(_)) | Err(ExecError::Disconnected(_)) => {
                self.dispatched_keys.insert(action.idempotency_key.clone());
                self.ledger
                    .apply(&action.action_id, ActionState::OutcomeUnknown)
                    .map_err(|error| GateError::NotAuthorized(error.to_string()))?;
                Ok(DispatchOutcome::Rejected { reason: "executor failed; outcome unknown, no retry".into() })
            }
            Err(ExecError::Rejected(detail)) => {
                self.ledger
                    .apply(&action.action_id, ActionState::Failed)
                    .map_err(|error| GateError::NotAuthorized(error.to_string()))?;
                Ok(DispatchOutcome::Rejected { reason: detail })
            }
            Err(error) => Err(GateError::Adapter(error.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::SimulatedWritableAdapter;
    use poie_state::action::{Authorization, BudgetItem, Precondition, Reversibility, VerificationSpec};
    use poie_state::record::Timestamp;
    use std::collections::BTreeMap;

    fn action(id: &str, key: &str, from_ms: i64, until_ms: i64) -> Action {
        Action {
            action_id: id.to_string(),
            idempotency_key: key.to_string(),
            target: "pump-p1".to_string(),
            command: "replace-sleeve-p1".to_string(),
            plan_ref: Some("plan-1".to_string()),
            policy_version: "policy/0.1".to_string(),
            precondition: Precondition {
                state_version: BTreeMap::from([("site-a".to_string(), 42)]),
                predicates: vec!["pump-p1 available".to_string()],
            },
            valid_from: Timestamp(from_ms),
            valid_until: Timestamp(until_ms),
            budget: vec![BudgetItem { resource: "sleeve-p1".to_string(), amount_si: 1.0 }],
            reservations: Vec::new(),
            reversibility: Reversibility::Idempotent,
            verification_spec: VerificationSpec {
                observe: "pump status".to_string(),
                condition: "pump available".to_string(),
                window_ms: 60_000,
            },
            authorization: Some(Authorization {
                by: "operator-1".to_string(),
                basis: "maintenance window".to_string(),
                scope: "dispatch".to_string(),
                valid_until: Timestamp(until_ms + 60_000),
            }),
        }
    }

    fn gate_with_action() -> (ExecutionGate, Action) {
        let action = action("a1", "key-1", 0, 10_000);
        let mut gate = ExecutionGate::new();
        gate.register_authorized(&action).unwrap();
        (gate, action)
    }

    #[test]
    fn an_authorized_command_is_dispatched_once() {
        let (mut gate, action) = gate_with_action();
        let mut adapter = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
        let outcome = gate.dispatch(&action, &mut adapter, 1_000).unwrap();
        assert!(outcome.dispatched());
        assert_eq!(adapter.physical_executions(), 1);
        let second = gate.dispatch(&action, &mut adapter, 1_500);
        assert!(matches!(second, Err(GateError::NotAuthorized(_))), "{second:?}");
        assert_eq!(adapter.physical_executions(), 1);
    }

    #[test]
    fn human_takeover_records_but_never_dispatches() {
        let (mut gate, action) = gate_with_action();
        let mut adapter = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
        gate.take_over();
        let outcome = gate.dispatch(&action, &mut adapter, 1_000).unwrap();
        assert!(matches!(outcome, DispatchOutcome::RecordedOnly { .. }));
        assert_eq!(adapter.physical_executions(), 0);
        assert_eq!(gate.ledger().state("a1"), Some(ActionState::Authorized));
    }

    #[test]
    fn degradation_falls_back_to_site_controls() {
        let (mut gate, action) = gate_with_action();
        let mut adapter = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
        gate.set_degraded(true);
        let outcome = gate.dispatch(&action, &mut adapter, 1_000).unwrap();
        assert!(matches!(outcome, DispatchOutcome::Fallback { .. }));
        assert_eq!(adapter.physical_executions(), 0);
    }

    #[test]
    fn executors_without_idempotency_are_not_dispatched() {
        let (mut gate, action) = gate_with_action();
        let mut adapter = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(false));
        let outcome = gate.dispatch(&action, &mut adapter, 1_000).unwrap();
        assert!(matches!(outcome, DispatchOutcome::Fallback { .. }));
        assert_eq!(adapter.physical_executions(), 0);
    }

    #[test]
    fn expired_commands_are_rejected() {
        let (mut gate, action) = gate_with_action();
        let mut adapter = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
        let outcome = gate.dispatch(&action, &mut adapter, 20_000).unwrap();
        assert!(matches!(outcome, DispatchOutcome::Rejected { .. }));
        assert_eq!(adapter.physical_executions(), 0);
    }

    #[test]
    fn unregistered_actions_are_rejected() {
        let action = action("a1", "key-1", 0, 10_000);
        let mut gate = ExecutionGate::new();
        let mut adapter = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
        let error = gate.dispatch(&action, &mut adapter, 1_000).unwrap_err();
        assert!(matches!(error, GateError::UnknownAction(_)));
    }
}
