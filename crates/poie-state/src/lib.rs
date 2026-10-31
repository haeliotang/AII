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
//!
//! Not implemented yet: simulated executor, full crash-point coverage, command
//! ownership leases and fault injection (SAP §5–§9). Facts and intentions
//! remain separate records; nothing here sends a command.

pub mod action;
pub mod log;
pub mod projection;
pub mod record;
pub mod review;
