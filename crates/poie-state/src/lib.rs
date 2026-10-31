//! Reference implementation of the Poieweave State & Action Protocol
//! (`specifications/state-action-protocol.zh-CN.md`, v0.1-draft).
//!
//! Implemented so far (milestone M4):
//!
//! - [`record`]: the nine record kinds, the common envelope and schema
//!   validation (SAP §2.1–2.2).
//! - [`action`]: actions, the exhaustive lifecycle transition table, and a
//!   ledger that rejects out-of-table transitions and duplicate dispatch of one
//!   idempotency key (SAP §3).
//! - [`projection`]: versioned projections with state versions and evidence
//!   expiry (SAP §2.3, Physical Ontology §4).
//! - [`review`]: the six pre-dispatch checks with traceable rejection reasons
//!   (SAP §4).
//! - [`log`]: the append-only local log with per-site sequence numbers and
//!   crash-tail recovery (SAP §1, §5; ADR-0003).
//! - [`executor`]: a deterministic simulated executor with dedup, expiry and
//!   receipt-loss/NACK/restart fault injection (SAP §4, §9).
//! - [`recovery`]: restart replay of transition records and SAP §5 decisions
//!   (dispatched -> outcome_unknown, authorized -> re-review, ...).
//! - [`isolation`]: simulation branches can never dispatch a field action
//!   (SAP §7).
//!
//! Not implemented yet: command ownership leases, partitions and site-to-site
//! reconciliation (SAP §6), and the ontology bridge (§8). Facts and intentions
//! remain separate records; nothing here sends a command to real equipment.

pub mod action;
pub mod executor;
pub mod isolation;
pub mod log;
pub mod projection;
pub mod record;
pub mod recovery;
pub mod review;
