//! Multi-site federation, library-level (M12.1–M12.4).
//!
//! Two or more simulated sites each keep an append-only record log and a hybrid
//! logical clock. Sites exchange logs on reconnection; records are deduplicated
//! by `RecordId` and causally ordered by HLC. Reconciliation reports conflicts
//! instead of applying last-write-wins: duplicate idempotency keys, double
//! dispatch, overlapping command ownership, over-reservation and reused
//! sequence numbers with different content (`sequence_collision`) are all
//! surfaced for human resolution, and reservations/leases are never silently
//! dropped.

pub mod federation;
pub mod hlc;
pub mod site;

pub use federation::{Conflict, ConflictKind, ExchangeReport, Federation, ReconciliationReport};
pub use hlc::HlcClock;
pub use site::{FederationError, Site};
