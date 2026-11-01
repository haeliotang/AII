//! Simulated writable adapter with fault injection for the simulated H3 run.

use std::collections::BTreeMap;

use crate::writable::{ExecError, Receipt, WritableAdapter, WritableDeclaration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterFault {
    None,
    /// The actuator moves but the acknowledgement is lost (outcome unknown).
    DropAck,
    /// The actuator executes twice for one command (a physical duplicate).
    DuplicateDelivery,
    /// The actuator moves and the process then crashes.
    CrashAfterExecute,
    /// The executor cannot be reached (no physical action).
    Unreachable,
}

#[derive(Debug)]
pub struct SimulatedWritableAdapter {
    declaration: WritableDeclaration,
    /// Accepted idempotency keys and the action that first delivered each.
    executed_keys: BTreeMap<String, String>,
    highest_epoch: Option<u64>,
    physical_executions: usize,
    duplicates: usize,
    pub fault: AdapterFault,
}

impl SimulatedWritableAdapter {
    pub fn new(declaration: WritableDeclaration) -> SimulatedWritableAdapter {
        SimulatedWritableAdapter {
            declaration,
            executed_keys: BTreeMap::new(),
            highest_epoch: None,
            physical_executions: 0,
            duplicates: 0,
            fault: AdapterFault::None,
        }
    }

    pub fn declaration(supports_idempotency: bool) -> WritableDeclaration {
        WritableDeclaration {
            adapter_id: "sim-executor".to_string(),
            version: "0.1.0".to_string(),
            source: "simulated-testbed".to_string(),
            supports_idempotency,
            max_command_validity_ms: 60_000,
            // The simulated executor never forgets an accepted key.
            dedup_window_ms: u64::MAX,
            supports_lease_epoch: true,
            verification_sources: vec!["sim-sensor-pump-status".to_string(), "sim-pressure-sensor".to_string()],
            compatibility: Some("none; simulated".to_string()),
        }
    }

    pub fn physical_executions(&self) -> usize {
        self.physical_executions
    }

    pub fn duplicate_physical_actions(&self) -> usize {
        self.duplicates
    }
}

impl WritableAdapter for SimulatedWritableAdapter {
    fn name(&self) -> &str {
        &self.declaration.adapter_id
    }

    fn declaration(&self) -> &WritableDeclaration {
        &self.declaration
    }

    fn dispatch(&mut self, command: &crate::writable::Command) -> Result<Receipt, ExecError> {
        if self.fault == AdapterFault::Unreachable {
            return Err(ExecError::Disconnected("simulated network loss".into()));
        }
        if self.declaration.supports_lease_epoch {
            match (command.lease_epoch, self.highest_epoch) {
                (Some(epoch), Some(highest)) if epoch < highest => {
                    return Err(ExecError::Rejected(format!("stale lease epoch {epoch} < {highest}")));
                }
                (None, Some(highest)) => {
                    return Err(ExecError::Rejected(format!("command without lease epoch after epoch {highest}")));
                }
                (Some(epoch), _) => self.highest_epoch = Some(self.highest_epoch.map_or(epoch, |h| h.max(epoch))),
                (None, None) => {}
            }
        }
        if self.declaration.supports_idempotency {
            if let Some(original) = self.executed_keys.get(&command.idempotency_key) {
                return Err(ExecError::Duplicate(original.clone()));
            }
        }
        self.executed_keys.insert(command.idempotency_key.clone(), command.action_id.clone());
        self.physical_executions += 1;
        if self.fault == AdapterFault::DuplicateDelivery {
            self.physical_executions += 1;
            self.duplicates += 1;
        }
        match self.fault {
            AdapterFault::CrashAfterExecute => {
                Err(ExecError::Crashed("simulated process crash after actuation".into()))
            }
            AdapterFault::DropAck => Ok(Receipt {
                action_id: command.action_id.clone(),
                accepted: false,
                executed_at_ms: command.valid_from_ms,
                detail: "acknowledgement dropped".to_string(),
            }),
            _ => Ok(Receipt {
                action_id: command.action_id.clone(),
                accepted: true,
                executed_at_ms: command.valid_from_ms,
                detail: "executed".to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writable::Command;

    fn command() -> Command {
        Command {
            action_id: "a1".into(),
            idempotency_key: "key-1".into(),
            target: "pump-p1".into(),
            command: "replace-sleeve-p1".into(),
            valid_from_ms: 0,
            valid_until_ms: 10_000,
            lease_epoch: None,
        }
    }

    #[test]
    fn idempotent_executor_deduplicates() {
        let mut adapter = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
        adapter.dispatch(&command()).unwrap();
        // Issue #107: a repeated key is a DUPLICATE receipt naming the original
        // action, not an ACK and not a NACK.
        let mut again = command();
        again.action_id = "a2".into();
        assert_eq!(adapter.dispatch(&again), Err(ExecError::Duplicate("a1".into())));
        assert_eq!(adapter.physical_executions(), 1);
        assert_eq!(adapter.duplicate_physical_actions(), 0);
    }

    #[test]
    fn a_duplicate_delivery_shows_up_as_a_physical_duplicate() {
        let mut adapter = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
        adapter.fault = AdapterFault::DuplicateDelivery;
        adapter.dispatch(&command()).unwrap();
        assert_eq!(adapter.duplicate_physical_actions(), 1);
    }

    #[test]
    fn dropped_ack_and_crash_are_reported() {
        let mut adapter = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
        adapter.fault = AdapterFault::DropAck;
        let receipt = adapter.dispatch(&command()).unwrap();
        assert!(!receipt.accepted);

        let mut adapter = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
        adapter.fault = AdapterFault::CrashAfterExecute;
        assert!(matches!(adapter.dispatch(&command()), Err(ExecError::Crashed(_))));
        assert_eq!(adapter.physical_executions(), 1);
    }

    /// Review B-01 (issue #108): the executor remembers the highest lease epoch
    /// and refuses a command from a superseded owner.
    #[test]
    fn sap6_1_adapter_fences_a_superseded_lease_epoch() {
        let mut adapter = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
        let mut newer = command();
        newer.action_id = "b1".into();
        newer.idempotency_key = "key-b".into();
        newer.lease_epoch = Some(2);
        adapter.dispatch(&newer).unwrap();
        let mut stale = command();
        stale.lease_epoch = Some(1);
        assert!(matches!(adapter.dispatch(&stale), Err(ExecError::Rejected(_))));
        assert_eq!(adapter.physical_executions(), 1);
    }
}
