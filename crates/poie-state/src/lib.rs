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
//!
//! Not implemented yet: append-only persistence, projections with state
//! versions, dispatch review, crash recovery, adapters and fault injection
//! (SAP §4–§9). Facts and intentions remain separate records; nothing here
//! sends a command.

pub mod action;
pub mod record;
