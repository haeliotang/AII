//! Simulated writable adapter with fault injection for the simulated H3 run.

use std::collections::BTreeSet;

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
}

#[derive(Debug)]
pub struct SimulatedWritableAdapter {
    declaration: WritableDeclaration,
    executed_keys: BTreeSet<String>,
    physical_executions: usize,
    duplicates: usize,
    pub fault: AdapterFault,
}

impl SimulatedWritableAdapter {
    pub fn new(declaration: WritableDeclaration) -> SimulatedWritableAdapter {
        SimulatedWritableAdapter {
            declaration,
            executed_keys: BTreeSet::new(),
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
        if self.declaration.supports_idempotency && self.executed_keys.contains(&command.idempotency_key) {
            return Ok(Receipt {
                action_id: command.action_id.clone(),
                accepted: true,
                executed_at_ms: command.valid_from_ms,
                detail: "deduplicated by idempotency key".to_string(),
            });
        }
        self.executed_keys.insert(command.idempotency_key.clone());
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
        }
    }

    #[test]
    fn idempotent_executor_deduplicates() {
        let mut adapter = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
        adapter.dispatch(&command()).unwrap();
        adapter.dispatch(&command()).unwrap();
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
}
