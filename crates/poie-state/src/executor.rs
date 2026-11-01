//! Simulated executor for fault-injection tests (SAP §4, §6.1, §9; M4.6).
//!
//! The executor contract is in SAP §4 and §6.1: commands carry an idempotency
//! key, `valid_until` and, under a lease, the owner's lease epoch. The executor
//! answers a repeated key inside its dedup window with a DUPLICATE receipt (not
//! a NACK), refuses expired commands, refuses commands whose remaining validity
//! its dedup window cannot cover, refuses commands from a superseded lease
//! epoch, and reports NACK whenever it chose not to execute. This simulator
//! models those behaviours, counts physical executions, and can drop receipts
//! or lose its dedup table on restart so the protocol layer can be tested
//! against the failure modes in SAP §9.

use std::collections::BTreeMap;

use crate::action::{Action, ReceiptKind};

/// What the dispatcher sends to the executor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub action_id: String,
    pub idempotency_key: String,
    pub valid_until_ms: i64,
    pub payload: String,
    /// Lease epoch (fencing token) of the dispatching owner (SAP §6.1).
    pub lease_epoch: Option<u64>,
}

impl Command {
    pub fn from_action(action: &Action) -> Command {
        Command {
            action_id: action.action_id.clone(),
            idempotency_key: action.idempotency_key.clone(),
            valid_until_ms: action.valid_until.0,
            payload: action.command.clone(),
            lease_epoch: None,
        }
    }

    /// Attaches the dispatching owner's lease epoch (SAP §6.1).
    pub fn with_lease_epoch(mut self, epoch: u64) -> Command {
        self.lease_epoch = Some(epoch);
        self
    }
}

/// What the executor effectively did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryOutcome {
    Executed,
    /// The key was already accepted inside the dedup window; nothing new was
    /// executed. Reported as a DUPLICATE receipt, not a NACK.
    Duplicate,
    Expired,
    /// The command's remaining validity exceeds the dedup window (SAP §4
    /// item 7); refused without executing.
    BeyondDedupWindow,
    /// The command carries a lease epoch lower than one already seen, or none
    /// after epochs were seen (SAP §6.1 rule 5); refused without executing.
    StaleEpoch,
    Nack,
}

impl DeliveryOutcome {
    /// The receipt the executor reports for this outcome (SAP §3.3, §4).
    pub fn receipt_kind(self) -> ReceiptKind {
        match self {
            DeliveryOutcome::Executed => ReceiptKind::Ack,
            DeliveryOutcome::Duplicate => ReceiptKind::Duplicate,
            DeliveryOutcome::Expired
            | DeliveryOutcome::BeyondDedupWindow
            | DeliveryOutcome::StaleEpoch
            | DeliveryOutcome::Nack => ReceiptKind::Nack,
        }
    }
}

/// Result of one delivery, including whether the receipt reached the dispatcher.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveryReport {
    pub outcome: DeliveryOutcome,
    pub receipt_delivered: bool,
    /// For a DUPLICATE receipt: the action that first delivered the key.
    pub duplicate_of: Option<String>,
}

impl DeliveryReport {
    fn new(outcome: DeliveryOutcome, receipt_delivered: bool) -> DeliveryReport {
        DeliveryReport { outcome, receipt_delivered, duplicate_of: None }
    }
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
    /// Per key: when it was first accepted and by which action.
    accepted: BTreeMap<String, (i64, String)>,
    highest_epoch: Option<u64>,
    execution_counts: BTreeMap<String, u64>,
    physical_actions: Vec<String>,
}

impl SimExecutor {
    pub fn new(dedup_window_ms: i64) -> SimExecutor {
        SimExecutor {
            dedup_window_ms,
            faults: ExecutorFaults::default(),
            accepted: BTreeMap::new(),
            highest_epoch: None,
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
    /// physical-effect history is kept because it really happened. The lease
    /// epoch is assumed to be kept in non-volatile memory.
    pub fn restart_losing_dedup(&mut self) {
        self.accepted.clear();
    }

    /// Delivers one command at `now_ms` (executor clock).
    pub fn deliver(&mut self, command: &Command, now_ms: i64) -> DeliveryReport {
        if now_ms > command.valid_until_ms {
            return DeliveryReport::new(DeliveryOutcome::Expired, true);
        }
        if self.faults.nack_next {
            self.faults.nack_next = false;
            return DeliveryReport::new(DeliveryOutcome::Nack, true);
        }
        match (command.lease_epoch, self.highest_epoch) {
            (Some(epoch), Some(highest)) if epoch < highest => {
                return DeliveryReport::new(DeliveryOutcome::StaleEpoch, true);
            }
            (None, Some(_)) => return DeliveryReport::new(DeliveryOutcome::StaleEpoch, true),
            (Some(epoch), _) => self.highest_epoch = Some(self.highest_epoch.map_or(epoch, |h| h.max(epoch))),
            (None, None) => {}
        }
        if let Some((first_ms, original)) = self.accepted.get(&command.idempotency_key) {
            if now_ms.saturating_sub(*first_ms) <= self.dedup_window_ms {
                let mut report = DeliveryReport::new(DeliveryOutcome::Duplicate, true);
                report.duplicate_of = Some(original.clone());
                return report;
            }
        }
        if command.valid_until_ms.saturating_sub(now_ms) > self.dedup_window_ms {
            return DeliveryReport::new(DeliveryOutcome::BeyondDedupWindow, true);
        }
        self.accepted.insert(command.idempotency_key.clone(), (now_ms, command.action_id.clone()));
        *self.execution_counts.entry(command.idempotency_key.clone()).or_insert(0) += 1;
        self.physical_actions.push(command.action_id.clone());
        DeliveryReport::new(DeliveryOutcome::Executed, !self.faults.drop_receipt)
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
            lease_epoch: None,
        }
    }

    #[test]
    fn duplicate_delivery_executes_once() {
        let mut executor = SimExecutor::new(60_000);
        assert_eq!(executor.deliver(&command("k1"), 1_000).outcome, DeliveryOutcome::Executed);
        let duplicate = executor.deliver(&command("k1"), 1_001);
        assert_eq!(duplicate.outcome, DeliveryOutcome::Duplicate);
        assert_eq!(duplicate.outcome.receipt_kind(), ReceiptKind::Duplicate);
        assert_eq!(duplicate.duplicate_of.as_deref(), Some("a1"));
        assert_eq!(executor.execution_count("k1"), 1);
        assert_eq!(executor.physical_actions().len(), 1);
    }

    #[test]
    fn expired_commands_are_rejected() {
        let mut executor = SimExecutor::new(60_000);
        assert_eq!(executor.deliver(&command("k1"), 20_000).outcome, DeliveryOutcome::Expired);
        assert_eq!(executor.execution_count("k1"), 0);
    }

    /// Issue #107 (review B-03): before the fix this test asserted that a
    /// 1 s dedup window lets a still-valid command execute twice. Now a command
    /// whose remaining validity the window cannot cover is refused up front.
    #[test]
    fn a_dedup_window_shorter_than_the_remaining_validity_refuses_the_command() {
        let mut executor = SimExecutor::new(1_000);
        let first = executor.deliver(&command("k1"), 1_000);
        assert_eq!(first.outcome, DeliveryOutcome::BeyondDedupWindow);
        assert_eq!(first.outcome.receipt_kind(), ReceiptKind::Nack);
        assert_eq!(executor.deliver(&command("k1"), 5_000).outcome, DeliveryOutcome::BeyondDedupWindow);
        assert_eq!(executor.execution_count("k1"), 0);
        // Once the remaining validity fits, it executes once and stays deduplicated.
        assert_eq!(executor.deliver(&command("k1"), 9_000).outcome, DeliveryOutcome::Executed);
        assert_eq!(executor.deliver(&command("k1"), 10_000).outcome, DeliveryOutcome::Duplicate);
        assert_eq!(executor.execution_count("k1"), 1);
    }

    #[test]
    fn a_lost_receipt_does_not_repeat_the_physical_action() {
        let mut executor = SimExecutor::new(60_000);
        executor.set_faults(ExecutorFaults { drop_receipt: true, nack_next: false });
        let report = executor.deliver(&command("k1"), 1_000);
        assert_eq!(report.outcome, DeliveryOutcome::Executed);
        assert!(!report.receipt_delivered);
        assert_eq!(executor.deliver(&command("k1"), 1_001).outcome, DeliveryOutcome::Duplicate);
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

    #[test]
    fn lease_epochs_fence_superseded_and_unfenced_commands() {
        let mut executor = SimExecutor::new(60_000);
        let mut newer = command("kb").with_lease_epoch(2);
        newer.action_id = "b1".to_string();
        assert_eq!(executor.deliver(&newer, 1_000).outcome, DeliveryOutcome::Executed);
        assert_eq!(executor.deliver(&command("ka").with_lease_epoch(1), 1_100).outcome, DeliveryOutcome::StaleEpoch);
        assert_eq!(executor.deliver(&command("kc"), 1_200).outcome, DeliveryOutcome::StaleEpoch);
        assert_eq!(executor.deliver(&command("kd").with_lease_epoch(3), 1_300).outcome, DeliveryOutcome::Executed);
        assert_eq!(executor.physical_actions(), ["b1".to_string(), "a1".to_string()]);
    }
}
