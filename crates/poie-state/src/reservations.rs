//! Inventory reservations at the physical site (SAP §6.2; SAP-T6).
//!
//! The site that physically holds a batch is the only one that may reserve it.
//! A remote site can send a request, but it has no effect on the reservation
//! until the holding site applies it; the total reserved quantity never exceeds
//! the batch's available quantity. This is the partition-safe behaviour SAP-T6
//! requires.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum ReservationError {
    UnknownBatch(String),
    Insufficient { batch: String, remaining_si: f64, requested_si: f64 },
}

impl fmt::Display for ReservationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReservationError::UnknownBatch(batch) => write!(f, "unknown batch {batch:?}"),
            ReservationError::Insufficient { batch, remaining_si, requested_si } => {
                write!(f, "batch {batch:?}: {requested_si} requested, only {remaining_si} unreserved")
            }
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

#[derive(Clone, Debug, Default)]
struct Batch {
    available_si: f64,
    reserved: BTreeMap<String, f64>,
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

    pub fn reserved(&self, batch: &str) -> f64 {
        self.batches.get(batch).map(|entry| entry.reserved.values().sum()).unwrap_or(0.0)
    }

    pub fn remaining(&self, batch: &str) -> f64 {
        self.available(batch).map(|available| available - self.reserved(batch)).unwrap_or(0.0)
    }

    /// Reserves locally (the caller must be the site holding the batch).
    pub fn reserve(&mut self, batch: &str, action_id: &str, amount_si: f64) -> Result<(), ReservationError> {
        let Some(entry) = self.batches.get_mut(batch) else {
            return Err(ReservationError::UnknownBatch(batch.to_string()));
        };
        let remaining = entry.available_si - entry.reserved.values().sum::<f64>();
        if amount_si > remaining {
            return Err(ReservationError::Insufficient {
                batch: batch.to_string(),
                remaining_si: remaining,
                requested_si: amount_si,
            });
        }
        entry.reserved.insert(action_id.to_string(), amount_si);
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
}
