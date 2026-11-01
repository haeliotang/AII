//! Inventory reservations at the physical site (SAP §6.2; SAP-T6).
//!
//! The site that physically holds a batch is the only one that may reserve it.
//! A remote site can send a request, but it has no effect on the reservation
//! until the holding site applies it; the total reserved quantity never exceeds
//! the batch's available quantity. This is the partition-safe behaviour SAP-T6
//! requires.
//!
//! Reservations follow the action lifecycle (SAP §6.2 rules 3–6; issue #111):
//!
//! | action event | reservation |
//! | --- | --- |
//! | up to `acknowledged` | `held`: counts against the batch |
//! | `verified` | `consumed`: deducted from the batch, hold ends |
//! | `rejected` / `expired` / `cancelled` | `released`: never dispatched |
//! | `dispatched → failed` (NACK: the executor declares it did not execute) | `released` |
//! | `acknowledged → failed`, `outcome_unknown → failed` | `in_doubt`: a failed effect does not prove nothing was consumed |
//! | `outcome_unknown` | `in_doubt` |
//!
//! An `in_doubt` hold still counts against the batch but is reported
//! separately; holds in doubt longer than the policy's limit are escalated
//! ([`ReservationBook::overdue_in_doubt`]). It ends when the action is
//! verified (consumed) or when a recount of the batch is taken after the
//! command's `valid_until` (the executor rejects expired commands, so the count
//! is final).

use std::collections::BTreeMap;
use std::fmt;

use crate::action::ActionState;

#[derive(Clone, Debug, PartialEq)]
pub enum ReservationError {
    UnknownBatch(String),
    Insufficient {
        batch: String,
        remaining_si: f64,
        requested_si: f64,
    },
    /// A recount cannot settle an in-doubt hold while its command could still
    /// execute.
    RecountTooEarly {
        batch: String,
        action_id: String,
        command_valid_until_ms: i64,
    },
}

impl fmt::Display for ReservationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReservationError::UnknownBatch(batch) => write!(f, "unknown batch {batch:?}"),
            ReservationError::Insufficient { batch, remaining_si, requested_si } => {
                write!(f, "batch {batch:?}: {requested_si} requested, only {remaining_si} unreserved")
            }
            ReservationError::RecountTooEarly { batch, action_id, command_valid_until_ms } => write!(
                f,
                "batch {batch:?}: action {action_id:?} is in doubt and its command is valid until \
                 {command_valid_until_ms} ms; a recount before then cannot settle it"
            ),
        }
    }
}

impl std::error::Error for ReservationError {}

/// A request from another site; it only takes effect when the holding site
/// applies it after reconnection.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteRequest {
    pub from_site: String,
    pub batch: String,
    pub action_id: String,
    pub amount_si: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum HoldState {
    Held,
    InDoubt { since_ms: i64, command_valid_until_ms: i64 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Hold {
    amount_si: f64,
    state: HoldState,
}

#[derive(Clone, Debug, Default)]
struct Batch {
    available_si: f64,
    reserved: BTreeMap<String, Hold>,
}

impl Batch {
    fn reserved_si(&self) -> f64 {
        self.reserved.values().map(|hold| hold.amount_si).sum()
    }
}

/// One lifecycle transition of an action that holds a reservation.
#[derive(Clone, Debug, PartialEq)]
pub struct LifecycleEvent {
    pub action_id: String,
    pub from: ActionState,
    pub to: ActionState,
    pub at_ms: i64,
    /// The dispatched command's `valid_until`; after it the executor rejects
    /// the command, so a recount is final.
    pub command_valid_until_ms: i64,
}

/// What a lifecycle event did to the reservation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Settlement {
    Held,
    Consumed,
    Released,
    InDoubt,
    /// The action holds no reservation on this batch.
    NotReserved,
}

/// Site-local reservation book. Only the site holding the batch owns one of these
/// for that batch.
#[derive(Debug, Default)]
pub struct ReservationBook {
    batches: BTreeMap<String, Batch>,
}

impl ReservationBook {
    pub fn new() -> ReservationBook {
        ReservationBook::default()
    }

    pub fn add_batch(&mut self, batch: &str, available_si: f64) {
        self.batches.entry(batch.to_string()).or_default().available_si += available_si;
    }

    pub fn available(&self, batch: &str) -> Option<f64> {
        self.batches.get(batch).map(|entry| entry.available_si)
    }

    /// Total held quantity, including holds in doubt.
    pub fn reserved(&self, batch: &str) -> f64 {
        self.batches.get(batch).map(Batch::reserved_si).unwrap_or(0.0)
    }

    /// Quantity held by actions whose outcome is in doubt (reported separately).
    pub fn in_doubt(&self, batch: &str) -> f64 {
        self.batches
            .get(batch)
            .map(|entry| {
                entry
                    .reserved
                    .values()
                    .filter(|hold| matches!(hold.state, HoldState::InDoubt { .. }))
                    .map(|hold| hold.amount_si)
                    .sum()
            })
            .unwrap_or(0.0)
    }

    pub fn remaining(&self, batch: &str) -> f64 {
        self.available(batch).map(|available| available - self.reserved(batch)).unwrap_or(0.0)
    }

    /// Reserves locally (the caller must be the site holding the batch).
    pub fn reserve(&mut self, batch: &str, action_id: &str, amount_si: f64) -> Result<(), ReservationError> {
        let Some(entry) = self.batches.get_mut(batch) else {
            return Err(ReservationError::UnknownBatch(batch.to_string()));
        };
        let remaining = entry.available_si - entry.reserved_si();
        if amount_si > remaining {
            return Err(ReservationError::Insufficient {
                batch: batch.to_string(),
                remaining_si: remaining,
                requested_si: amount_si,
            });
        }
        entry.reserved.insert(action_id.to_string(), Hold { amount_si, state: HoldState::Held });
        Ok(())
    }

    /// Applies one lifecycle transition of `event.action_id` to its hold on
    /// `batch`, following the table in the module docs (SAP §6.2).
    pub fn settle(&mut self, batch: &str, event: &LifecycleEvent) -> Result<Settlement, ReservationError> {
        let Some(entry) = self.batches.get_mut(batch) else {
            return Err(ReservationError::UnknownBatch(batch.to_string()));
        };
        let Some(hold) = entry.reserved.get(&event.action_id).copied() else {
            return Ok(Settlement::NotReserved);
        };
        let in_doubt = |hold: Hold| match hold.state {
            HoldState::InDoubt { .. } => hold.state,
            HoldState::Held => {
                HoldState::InDoubt { since_ms: event.at_ms, command_valid_until_ms: event.command_valid_until_ms }
            }
        };
        let settlement = match event.to {
            ActionState::Verified => {
                entry.available_si -= hold.amount_si;
                entry.reserved.remove(&event.action_id);
                Settlement::Consumed
            }
            ActionState::Rejected | ActionState::Expired | ActionState::Cancelled => {
                entry.reserved.remove(&event.action_id);
                Settlement::Released
            }
            ActionState::Failed if event.from == ActionState::Dispatched => {
                entry.reserved.remove(&event.action_id);
                Settlement::Released
            }
            ActionState::Failed | ActionState::OutcomeUnknown => {
                entry
                    .reserved
                    .insert(event.action_id.clone(), Hold { amount_si: hold.amount_si, state: in_doubt(hold) });
                Settlement::InDoubt
            }
            ActionState::Proposed
            | ActionState::Validated
            | ActionState::Authorized
            | ActionState::Dispatched
            | ActionState::Acknowledged => match hold.state {
                HoldState::Held => Settlement::Held,
                HoldState::InDoubt { .. } => Settlement::InDoubt,
            },
        };
        Ok(settlement)
    }

    /// Holds that have been in doubt longer than `max_in_doubt_ms` at `now_ms`:
    /// `(batch, action_id)` pairs that policy must escalate to a person.
    pub fn overdue_in_doubt(&self, now_ms: i64, max_in_doubt_ms: i64) -> Vec<(String, String)> {
        let mut overdue = Vec::new();
        for (batch, entry) in &self.batches {
            for (action_id, hold) in &entry.reserved {
                if let HoldState::InDoubt { since_ms, .. } = hold.state {
                    if now_ms.saturating_sub(since_ms) > max_in_doubt_ms {
                        overdue.push((batch.clone(), action_id.clone()));
                    }
                }
            }
        }
        overdue
    }

    /// A physical recount of `batch` at `now_ms`. The count replaces the
    /// available quantity and settles every in-doubt hold, whose consumption
    /// (if any) the count already reflects — but only once each in-doubt
    /// command is past its `valid_until`; otherwise it could still execute.
    pub fn recount(&mut self, batch: &str, observed_si: f64, now_ms: i64) -> Result<(), ReservationError> {
        let Some(entry) = self.batches.get_mut(batch) else {
            return Err(ReservationError::UnknownBatch(batch.to_string()));
        };
        for (action_id, hold) in &entry.reserved {
            if let HoldState::InDoubt { command_valid_until_ms, .. } = hold.state {
                if now_ms <= command_valid_until_ms {
                    return Err(ReservationError::RecountTooEarly {
                        batch: batch.to_string(),
                        action_id: action_id.clone(),
                        command_valid_until_ms,
                    });
                }
            }
        }
        entry.reserved.retain(|_, hold| hold.state == HoldState::Held);
        entry.available_si = observed_si;
        Ok(())
    }

    pub fn release(&mut self, batch: &str, action_id: &str) {
        if let Some(entry) = self.batches.get_mut(batch) {
            entry.reserved.remove(action_id);
        }
    }

    /// Applies a remote request on reconnection, subject to the same cap.
    pub fn apply_remote(&mut self, request: &RemoteRequest) -> Result<(), ReservationError> {
        self.reserve(&request.batch, &request.action_id, request.amount_si)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_requests_have_no_effect_until_applied() {
        let mut book = ReservationBook::new();
        book.add_batch("lot-bronze", 5.0);
        let request = RemoteRequest {
            from_site: "site-b".into(),
            batch: "lot-bronze".into(),
            action_id: "a1".into(),
            amount_si: 4.0,
        };
        // While partitioned the holding site has reserved nothing for the remote action.
        assert_eq!(book.reserved("lot-bronze"), 0.0);
        assert_eq!(book.remaining("lot-bronze"), 5.0);

        book.apply_remote(&request).unwrap();
        assert_eq!(book.reserved("lot-bronze"), 4.0);
        assert_eq!(book.remaining("lot-bronze"), 1.0);
    }

    #[test]
    fn reservations_never_exceed_the_batch() {
        let mut book = ReservationBook::new();
        book.add_batch("lot-bronze", 5.0);
        book.reserve("lot-bronze", "a1", 4.0).unwrap();
        let over = RemoteRequest {
            from_site: "site-b".into(),
            batch: "lot-bronze".into(),
            action_id: "a2".into(),
            amount_si: 3.0,
        };
        assert!(matches!(book.apply_remote(&over), Err(ReservationError::Insufficient { .. })));
        assert_eq!(book.reserved("lot-bronze"), 4.0);
    }

    #[test]
    fn releasing_makes_capacity_available_again() {
        let mut book = ReservationBook::new();
        book.add_batch("lot-bronze", 5.0);
        book.reserve("lot-bronze", "a1", 5.0).unwrap();
        assert_eq!(book.remaining("lot-bronze"), 0.0);
        book.release("lot-bronze", "a1");
        assert_eq!(book.remaining("lot-bronze"), 5.0);
    }

    // Issue #111 (P17, review B-09): reservations follow the action lifecycle
    // (SAP §6.2). Releasing on `outcome_unknown` would let A2 over-allocate a
    // sleeve that A1 may already have installed; holding forever would leak.

    use crate::action::ActionState;

    fn settle(book: &mut ReservationBook, action: &str, from: ActionState, to: ActionState, at_ms: i64) -> Settlement {
        book.settle(
            "lot-sleeve",
            &LifecycleEvent { action_id: action.into(), from, to, at_ms, command_valid_until_ms: 10_000 },
        )
        .unwrap()
    }

    fn sleeves() -> ReservationBook {
        let mut book = ReservationBook::new();
        book.add_batch("lot-sleeve", 5.0);
        book.reserve("lot-sleeve", "a1", 1.0).unwrap();
        book
    }

    #[test]
    fn sap6_2_verified_consumes_the_reservation() {
        let mut book = sleeves();
        assert_eq!(settle(&mut book, "a1", ActionState::Dispatched, ActionState::Acknowledged, 100), Settlement::Held);
        assert_eq!(
            settle(&mut book, "a1", ActionState::Acknowledged, ActionState::Verified, 200),
            Settlement::Consumed
        );
        assert_eq!(book.available("lot-sleeve"), Some(4.0));
        assert_eq!(book.reserved("lot-sleeve"), 0.0);
        assert_eq!(book.remaining("lot-sleeve"), 4.0);
    }

    #[test]
    fn sap6_2_never_dispatched_actions_release_the_reservation() {
        for to in [ActionState::Rejected, ActionState::Expired, ActionState::Cancelled] {
            let mut book = sleeves();
            assert_eq!(settle(&mut book, "a1", ActionState::Authorized, to, 100), Settlement::Released);
            assert_eq!(book.remaining("lot-sleeve"), 5.0);
        }
        // A NACK (the executor declares it did not execute) also releases.
        let mut book = sleeves();
        assert_eq!(settle(&mut book, "a1", ActionState::Dispatched, ActionState::Failed, 100), Settlement::Released);
        assert_eq!(book.remaining("lot-sleeve"), 5.0);
    }

    #[test]
    fn sap6_2_outcome_unknown_keeps_the_quantity_in_doubt() {
        let mut book = sleeves();
        assert_eq!(
            settle(&mut book, "a1", ActionState::Dispatched, ActionState::OutcomeUnknown, 1_000),
            Settlement::InDoubt
        );
        // Still deducted from what others may reserve, but reported separately.
        assert_eq!(book.remaining("lot-sleeve"), 4.0);
        assert_eq!(book.in_doubt("lot-sleeve"), 1.0);
        assert!(matches!(book.reserve("lot-sleeve", "a2", 4.5), Err(ReservationError::Insufficient { .. })));
        // A failure after an ACK does not prove that nothing was consumed.
        let mut other = sleeves();
        assert_eq!(settle(&mut other, "a1", ActionState::Acknowledged, ActionState::Failed, 100), Settlement::InDoubt);
        // Resolution: a later verification consumes it ...
        assert_eq!(
            settle(&mut book, "a1", ActionState::OutcomeUnknown, ActionState::Verified, 2_000),
            Settlement::Consumed
        );
        assert_eq!(book.in_doubt("lot-sleeve"), 0.0);
        assert_eq!(book.available("lot-sleeve"), Some(4.0));
    }

    #[test]
    fn sap6_2_in_doubt_reservations_escalate_and_settle_by_recount() {
        let mut book = sleeves();
        settle(&mut book, "a1", ActionState::Dispatched, ActionState::OutcomeUnknown, 1_000);
        assert!(book.overdue_in_doubt(5_000, 10_000).is_empty());
        assert_eq!(book.overdue_in_doubt(20_000, 10_000), vec![("lot-sleeve".to_string(), "a1".to_string())]);
        // A recount before the command could still execute must not settle it.
        assert!(book.recount("lot-sleeve", 4.0, 9_000).is_err());
        // After the command's valid_until the executor rejects it, so the count is final.
        book.recount("lot-sleeve", 4.0, 10_001).unwrap();
        assert_eq!(book.in_doubt("lot-sleeve"), 0.0);
        assert_eq!(book.available("lot-sleeve"), Some(4.0));
        assert_eq!(book.remaining("lot-sleeve"), 4.0);
    }
}
