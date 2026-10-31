//! Simulated executor for fault-injection tests (SAP §4, §9; M4.6).
//!
//! The real executor contract is in SAP §4: commands carry an idempotency key
//! and `valid_until`; the executor rejects duplicates inside its dedup window
//! and expired commands, and reports NACK when it chose not to execute. This
//! simulator models those behaviours, counts physical executions, and can drop
//! receipts or lose its dedup table on restart so the protocol layer can be
//! tested against the failure modes in SAP §9.

use std::collections::BTreeMap;

use crate::action::Action;

/// What the dispatcher sends to the executor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub action_id: String,
    pub idempotency_key: String,
    pub valid_until_ms: i64,
    pub payload: String,
}

impl Command {
    pub fn from_action(action: &Action) -> Command {
        Command {
            action_id: action.action_id.clone(),
            idempotency_key: action.idempotency_key.clone(),
            valid_until_ms: action.valid_until.0,
            payload: action.command.clone(),
        }
    }
}

/// What the executor effectively did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryOutcome {
    Executed,
    DuplicateRejected,
    Expired,
    Nack,
}

/// Result of one delivery, including whether the receipt reached the dispatcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeliveryReport {
    pub outcome: DeliveryOutcome,
    pub receipt_delivered: bool,
}

/// Faults the simulated executor can inject. Only tests should enable them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExecutorFaults {
    /// Execute, but lose the receipt (the dispatcher sees no ACK).
    pub drop_receipt: bool,
    /// Refuse the next command with a NACK (the executor declares it did not execute).
    pub nack_next: bool,
}

/// A deterministic in-process executor used by acceptance and fault tests.
#[derive(Debug)]
pub struct SimExecutor {
    dedup_window_ms: i64,
    faults: ExecutorFaults,
    last_execution_ms: BTreeMap<String, i64>,
    execution_counts: BTreeMap<String, u64>,
    physical_actions: Vec<String>,
}

impl SimExecutor {
    pub fn new(dedup_window_ms: i64) -> SimExecutor {
        SimExecutor {
            dedup_window_ms,
            faults: ExecutorFaults::default(),
            last_execution_ms: BTreeMap::new(),
            execution_counts: BTreeMap::new(),
            physical_actions: Vec::new(),
        }
    }

    pub fn set_faults(&mut self, faults: ExecutorFaults) {
        self.faults = faults;
    }

    /// Number of physical executions for one idempotency key.
    pub fn execution_count(&self, idempotency_key: &str) -> u64 {
        self.execution_counts.get(idempotency_key).copied().unwrap_or(0)
    }

    /// Every executed action id, in execution order (proof of physical effect).
    pub fn physical_actions(&self) -> &[String] {
        &self.physical_actions
    }

    /// Simulates a device restart that loses the dedup table (SAP-T9). The
    /// physical-effect history is kept because it really happened.
    pub fn restart_losing_dedup(&mut self) {
        self.last_execution_ms.clear();
    }

    /// Delivers one command at `now_ms`.
    pub fn deliver(&mut self, command: &Command, now_ms: i64) -> DeliveryReport {
        if now_ms > command.valid_until_ms {
            return DeliveryReport { outcome: DeliveryOutcome::Expired, receipt_delivered: true };
        }
        if self.faults.nack_next {
            self.faults.nack_next = false;
            return DeliveryReport { outcome: DeliveryOutcome::Nack, receipt_delivered: true };
        }
        if let Some(last) = self.last_execution_ms.get(&command.idempotency_key) {
            if now_ms.saturating_sub(*last) <= self.dedup_window_ms {
                return DeliveryReport { outcome: DeliveryOutcome::DuplicateRejected, receipt_delivered: true };
            }
        }
        self.last_execution_ms.insert(command.idempotency_key.clone(), now_ms);
        *self.execution_counts.entry(command.idempotency_key.clone()).or_insert(0) += 1;
        self.physical_actions.push(command.action_id.clone());
        DeliveryReport { outcome: DeliveryOutcome::Executed, receipt_delivered: !self.faults.drop_receipt }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(key: &str) -> Command {
        Command {
            action_id: "a1".to_string(),
            idempotency_key: key.to_string(),
            valid_until_ms: 10_000,
            payload: "replace-sleeve".to_string(),
        }
    }

    #[test]
    fn duplicate_delivery_executes_once() {
        let mut executor = SimExecutor::new(60_000);
        assert_eq!(executor.deliver(&command("k1"), 1_000).outcome, DeliveryOutcome::Executed);
        assert_eq!(executor.deliver(&command("k1"), 1_001).outcome, DeliveryOutcome::DuplicateRejected);
        assert_eq!(executor.execution_count("k1"), 1);
        assert_eq!(executor.physical_actions().len(), 1);
    }

    #[test]
    fn expired_commands_are_rejected() {
        let mut executor = SimExecutor::new(60_000);
        assert_eq!(executor.deliver(&command("k1"), 20_000).outcome, DeliveryOutcome::Expired);
        assert_eq!(executor.execution_count("k1"), 0);
    }

    #[test]
    fn dedup_window_expiry_allows_reexecution() {
        let mut executor = SimExecutor::new(1_000);
        executor.deliver(&command("k1"), 1_000);
        assert_eq!(executor.deliver(&command("k1"), 5_000).outcome, DeliveryOutcome::Executed);
        assert_eq!(executor.execution_count("k1"), 2);
    }

    #[test]
    fn a_lost_receipt_does_not_repeat_the_physical_action() {
        let mut executor = SimExecutor::new(60_000);
        executor.set_faults(ExecutorFaults { drop_receipt: true, nack_next: false });
        let report = executor.deliver(&command("k1"), 1_000);
        assert_eq!(report.outcome, DeliveryOutcome::Executed);
        assert!(!report.receipt_delivered);
        assert_eq!(executor.deliver(&command("k1"), 1_001).outcome, DeliveryOutcome::DuplicateRejected);
        assert_eq!(executor.execution_count("k1"), 1);
    }

    #[test]
    fn restart_losing_dedup_reintroduces_the_known_duplicate_risk() {
        let mut executor = SimExecutor::new(60_000);
        executor.deliver(&command("k1"), 1_000);
        executor.restart_losing_dedup();
        assert_eq!(executor.deliver(&command("k1"), 1_001).outcome, DeliveryOutcome::Executed);
        assert_eq!(executor.execution_count("k1"), 2);
    }

    #[test]
    fn nack_means_not_executed() {
        let mut executor = SimExecutor::new(60_000);
        executor.set_faults(ExecutorFaults { drop_receipt: false, nack_next: true });
        assert_eq!(executor.deliver(&command("k1"), 1_000).outcome, DeliveryOutcome::Nack);
        assert_eq!(executor.execution_count("k1"), 0);
        assert_eq!(executor.deliver(&command("k1"), 1_001).outcome, DeliveryOutcome::Executed);
    }
}
