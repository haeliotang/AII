//! Writable adapter contract (M10.1).

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub struct WritableDeclaration {
    pub adapter_id: String,
    pub version: String,
    pub source: String,
    /// The executor can deduplicate by idempotency key. Without this the gate
    /// must not dispatch (ADR-0007).
    pub supports_idempotency: bool,
    /// Maximum command validity the executor honours.
    pub max_command_validity_ms: u64,
    /// How long the executor keeps a dedup record after first accepting a key.
    /// A command's remaining validity plus the skew bound must fit inside it
    /// (SAP §4 item 7).
    pub dedup_window_ms: u64,
    /// The executor remembers the highest lease epoch it has seen and refuses
    /// commands from a lower one (SAP §6.1 rule 5).
    pub supports_lease_epoch: bool,
    /// Sources that can independently confirm an action's effect.
    pub verification_sources: Vec<String>,
    pub compatibility: Option<String>,
}

impl WritableDeclaration {
    pub fn validate(&self) -> Result<(), ExecError> {
        if self.adapter_id.trim().is_empty() || self.version.trim().is_empty() || self.source.trim().is_empty() {
            return Err(ExecError::InvalidDeclaration("adapter id, version and source are required".into()));
        }
        if self.dedup_window_ms == 0 {
            return Err(ExecError::InvalidDeclaration("dedup window must be positive".into()));
        }
        if self.max_command_validity_ms == 0 {
            return Err(ExecError::InvalidDeclaration("max command validity must be positive".into()));
        }
        if self.verification_sources.is_empty() {
            return Err(ExecError::InvalidDeclaration("at least one verification source is required".into()));
        }
        Ok(())
    }
}

/// A command as handed to the writable adapter.
#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub action_id: String,
    pub idempotency_key: String,
    pub target: String,
    pub command: String,
    pub valid_from_ms: i64,
    pub valid_until_ms: i64,
    /// Lease epoch (fencing token) of the dispatching owner, if leased.
    pub lease_epoch: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Receipt {
    pub action_id: String,
    pub accepted: bool,
    pub executed_at_ms: i64,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecError {
    InvalidDeclaration(String),
    Unsupported(String),
    Disconnected(String),
    Crashed(String),
    Rejected(String),
    /// DUPLICATE receipt: the key was already accepted earlier (by the named
    /// action); nothing new was executed. Not a NACK (SAP §3.3).
    Duplicate(String),
}

impl fmt::Display for ExecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecError::InvalidDeclaration(detail) => write!(f, "invalid writable adapter declaration: {detail}"),
            ExecError::Unsupported(detail) => write!(f, "executor does not support: {detail}"),
            ExecError::Disconnected(detail) => write!(f, "executor disconnected: {detail}"),
            ExecError::Crashed(detail) => write!(f, "executor crashed: {detail}"),
            ExecError::Rejected(detail) => write!(f, "executor rejected the command: {detail}"),
            ExecError::Duplicate(original) => write!(f, "idempotency key already accepted for {original}"),
        }
    }
}

impl std::error::Error for ExecError {}

pub trait WritableAdapter {
    fn name(&self) -> &str;
    fn declaration(&self) -> &WritableDeclaration;
    fn dispatch(&mut self, command: &Command) -> Result<Receipt, ExecError>;
}
