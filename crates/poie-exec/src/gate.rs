//! The execution gate (M10.2, M10.5).

use std::fmt;

use poie_state::action::{receipt_transition, Action, ActionLedger, ActionState, LedgerError, ReceiptKind};
use poie_state::ownership::ASSUMED_CLOCK_SKEW_BOUND_MS;

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
    /// DUPLICATE receipt: the executor had already accepted this key (for
    /// `original`); nothing new was executed (SAP §3.3).
    Duplicate {
        original: String,
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
    lease: Option<GateLease>,
}

/// The lease this site holds for the gate's executor (SAP §6.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateLease {
    /// Fencing token every command carries (SAP §6.1 rule 5).
    pub epoch: u64,
    /// `lease.until - skew_bound`: no dispatch after it, and no command valid
    /// beyond it (SAP §4 item 3).
    pub dispatch_deadline_ms: i64,
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

    /// Site policy for re-dispatching an `idempotent` action while the previous
    /// attempt is `outcome_unknown` (SAP §3.2 rule 2). Off by default.
    pub fn set_unchecked_idempotent_redispatch(&mut self, allowed: bool) {
        self.ledger.set_unchecked_idempotent_redispatch(allowed);
    }

    /// The lease this site holds for the executor (SAP §6.1); `None` for local
    /// ownership without a lease.
    pub fn set_lease(&mut self, lease: Option<GateLease>) {
        self.lease = lease;
    }

    /// Records a verifier's conclusion for an action (SAP §3.3).
    pub fn record_outcome(&mut self, action_id: &str, to: ActionState) -> Result<(), GateError> {
        self.ledger.apply(action_id, to).map(|_| ()).map_err(|error| GateError::NotAuthorized(error.to_string()))
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
    /// authorized, the validity window has passed, the idempotency key rule of
    /// SAP §3.2 rule 2 / §4 item 5 is not met, the validity outlives the lease
    /// or the executor's dedup window, the operator has taken over, the runtime
    /// is degraded, or the adapter cannot honour an idempotency key.
    ///
    /// Receipts: an accepted receipt moves the action to `acknowledged`, a lost
    /// one to `outcome_unknown`; a DUPLICATE receipt (`ExecError::Duplicate`)
    /// to `acknowledged` — never `failed`; a NACK (`ExecError::Rejected`) to
    /// `failed` (SAP §3.3).
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
        if let Some(lease) = self.lease {
            if now_ms > lease.dispatch_deadline_ms {
                return Ok(DispatchOutcome::Rejected { reason: "lease dispatch deadline has passed".into() });
            }
            if action.valid_until.0 > lease.dispatch_deadline_ms {
                return Ok(DispatchOutcome::Rejected {
                    reason: "command validity outlives the lease dispatch deadline (SAP §4 item 3)".into(),
                });
            }
        }
        let needed = action.valid_until.0.saturating_sub(now_ms).saturating_add(ASSUMED_CLOCK_SKEW_BOUND_MS);
        if needed > i64::try_from(declaration.dedup_window_ms).unwrap_or(i64::MAX) {
            return Ok(DispatchOutcome::Rejected {
                reason: format!(
                    "remaining validity plus skew bound ({needed} ms) exceeds the executor's dedup window ({} ms) (SAP §4 item 7)",
                    declaration.dedup_window_ms
                ),
            });
        }
        match self.ledger.check_idempotency_key(action) {
            Ok(()) => {}
            Err(LedgerError::RedispatchNotAllowed { of, previous, .. }) => {
                return Ok(DispatchOutcome::Rejected {
                    reason: format!("re-dispatch of {of:?} is not allowed while it is {previous} (SAP §3.2 rule 2)"),
                });
            }
            Err(_) => return Ok(DispatchOutcome::Rejected { reason: "idempotency key already dispatched".into() }),
        }
        let command = Command {
            action_id: action.action_id.clone(),
            idempotency_key: action.idempotency_key.clone(),
            target: action.target.clone(),
            command: action.command.clone(),
            valid_from_ms: action.valid_from.0,
            valid_until_ms: action.valid_until.0,
            lease_epoch: self.lease.map(|lease| lease.epoch),
        };
        let apply = |ledger: &mut ActionLedger, to: ActionState| {
            ledger.apply(&action.action_id, to).map(|_| ()).map_err(|error| GateError::NotAuthorized(error.to_string()))
        };
        match adapter.dispatch(&command) {
            Ok(receipt) => {
                apply(&mut self.ledger, ActionState::Dispatched)?;
                if receipt.accepted {
                    apply(&mut self.ledger, receipt_transition(ReceiptKind::Ack))?;
                } else {
                    apply(&mut self.ledger, ActionState::OutcomeUnknown)?;
                }
                Ok(DispatchOutcome::Dispatched { receipt })
            }
            Err(ExecError::Duplicate(original)) => {
                apply(&mut self.ledger, ActionState::Dispatched)?;
                apply(&mut self.ledger, receipt_transition(ReceiptKind::Duplicate))?;
                Ok(DispatchOutcome::Duplicate { original })
            }
            Err(ExecError::Disconnected(detail)) => Ok(DispatchOutcome::Fallback {
                reason: format!("executor unreachable ({detail}); site controls take over"),
            }),
            Err(ExecError::Crashed(_)) => {
                apply(&mut self.ledger, ActionState::Dispatched)?;
                apply(&mut self.ledger, ActionState::OutcomeUnknown)?;
                Ok(DispatchOutcome::Rejected {
                    reason: "executor crashed after actuation; outcome unknown, no retry".into(),
                })
            }
            Err(ExecError::Rejected(detail)) => {
                // NACK: the command was sent and the executor declares it did
                // not execute it (SAP §3.3 `dispatched -> failed`).
                apply(&mut self.ledger, ActionState::Dispatched)?;
                apply(&mut self.ledger, receipt_transition(ReceiptKind::Nack))?;
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
            redispatch_of: None,
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

    /// Review A-04 / B-05 (issue #107): a policy-allowed re-dispatch after a
    /// lost ACK reaches an executor that already accepted the key; the DUPLICATE
    /// receipt must not be recorded as a NACK / `failed`.
    #[test]
    fn sap3_3_gate_redispatch_after_a_lost_ack_gets_a_duplicate_receipt_not_failed() {
        let first = action("a1", "key-1", 0, 10_000);
        let mut retry = action("a2", "key-1", 0, 10_000);
        retry.redispatch_of = Some("a1".to_string());
        let mut gate = ExecutionGate::new();
        gate.set_unchecked_idempotent_redispatch(true);
        gate.register_authorized(&first).unwrap();
        gate.register_authorized(&retry).unwrap();
        let mut adapter = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
        adapter.fault = crate::sim::AdapterFault::DropAck;
        gate.dispatch(&first, &mut adapter, 1_000).unwrap();
        assert_eq!(gate.ledger().state("a1"), Some(ActionState::OutcomeUnknown));

        adapter.fault = crate::sim::AdapterFault::None;
        let outcome = gate.dispatch(&retry, &mut adapter, 1_500).unwrap();
        assert!(matches!(&outcome, DispatchOutcome::Duplicate { original } if original == "a1"), "{outcome:?}");
        assert_eq!(gate.ledger().state("a2"), Some(ActionState::Acknowledged));
        assert_eq!(adapter.physical_executions(), 1);
    }

    /// Review B-03 (issue #107): the gate refuses a command whose remaining
    /// validity plus the skew bound exceeds the executor's dedup window.
    #[test]
    fn sap4_7_gate_refuses_validity_beyond_the_executor_dedup_window() {
        let (mut gate, action) = gate_with_action();
        let mut declaration = SimulatedWritableAdapter::declaration(true);
        declaration.dedup_window_ms = 5_000;
        let mut adapter = SimulatedWritableAdapter::new(declaration);
        let outcome = gate.dispatch(&action, &mut adapter, 1_000).unwrap();
        assert!(matches!(outcome, DispatchOutcome::Rejected { .. }), "{outcome:?}");
        assert_eq!(adapter.physical_executions(), 0);
        assert_eq!(gate.ledger().state("a1"), Some(ActionState::Authorized));
    }

    #[test]
    fn sap3_2_gate_irreversible_redispatch_waits_for_the_outcome_check() {
        let mut first = action("a1", "key-1", 0, 10_000);
        first.reversibility = Reversibility::Irreversible;
        let mut retry = action("a2", "key-1", 0, 10_000);
        retry.reversibility = Reversibility::Irreversible;
        retry.redispatch_of = Some("a1".to_string());
        let mut gate = ExecutionGate::new();
        gate.set_unchecked_idempotent_redispatch(true);
        gate.register_authorized(&first).unwrap();
        gate.register_authorized(&retry).unwrap();
        let mut adapter = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
        adapter.fault = crate::sim::AdapterFault::Unreachable;
        // Unreachable: nothing sent, state stays authorized.
        assert!(matches!(gate.dispatch(&first, &mut adapter, 1_000).unwrap(), DispatchOutcome::Fallback { .. }));
        adapter.fault = crate::sim::AdapterFault::CrashAfterExecute;
        gate.dispatch(&first, &mut adapter, 1_000).unwrap();
        assert_eq!(gate.ledger().state("a1"), Some(ActionState::OutcomeUnknown));

        adapter.fault = crate::sim::AdapterFault::None;
        let early = gate.dispatch(&retry, &mut adapter, 1_500).unwrap();
        assert!(matches!(early, DispatchOutcome::Rejected { .. }), "{early:?}");
        // The site check shows the valve did not move.
        gate.record_outcome("a1", ActionState::Failed).unwrap();
        let checked = gate.dispatch(&retry, &mut adapter, 1_600).unwrap();
        // The executor had recorded the key before crashing: DUPLICATE, no second actuation.
        assert!(matches!(checked, DispatchOutcome::Duplicate { .. }), "{checked:?}");
        assert_eq!(adapter.physical_executions(), 1);
    }

    #[test]
    fn a_nack_is_recorded_as_dispatched_then_failed() {
        let (mut gate, action) = gate_with_action();
        let mut adapter = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
        gate.set_lease(Some(GateLease { epoch: 1, dispatch_deadline_ms: 20_000 }));
        let newer = crate::writable::Command {
            action_id: "other".into(),
            idempotency_key: "other-key".into(),
            target: "pump-p1".into(),
            command: "noop".into(),
            valid_from_ms: 0,
            valid_until_ms: 10_000,
            lease_epoch: Some(2),
        };
        // Another owner with a newer epoch already reached the executor.
        adapter.dispatch(&newer).unwrap();
        let outcome = gate.dispatch(&action, &mut adapter, 1_000).unwrap();
        assert!(matches!(outcome, DispatchOutcome::Rejected { .. }), "{outcome:?}");
        assert_eq!(gate.ledger().state("a1"), Some(ActionState::Failed));
        assert_eq!(adapter.physical_executions(), 1);
    }

    #[test]
    fn sap4_3_gate_refuses_validity_beyond_the_lease_deadline() {
        let (mut gate, action) = gate_with_action();
        let mut adapter = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
        gate.set_lease(Some(GateLease { epoch: 1, dispatch_deadline_ms: 8_000 }));
        let outcome = gate.dispatch(&action, &mut adapter, 1_000).unwrap();
        assert!(matches!(outcome, DispatchOutcome::Rejected { .. }), "{outcome:?}");
        assert_eq!(adapter.physical_executions(), 0);
        gate.set_lease(Some(GateLease { epoch: 1, dispatch_deadline_ms: 10_000 }));
        assert!(gate.dispatch(&action, &mut adapter, 1_000).unwrap().dispatched());
    }
}
