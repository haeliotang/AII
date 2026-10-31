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
    /// Sources that can independently confirm an action's effect.
    pub verification_sources: Vec<String>,
    pub compatibility: Option<String>,
}

impl WritableDeclaration {
    pub fn validate(&self) -> Result<(), ExecError> {
        if self.adapter_id.trim().is_empty() || self.version.trim().is_empty() || self.source.trim().is_empty() {
            return Err(ExecError::InvalidDeclaration("adapter id, version and source are required".into()));
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
}

impl fmt::Display for ExecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecError::InvalidDeclaration(detail) => write!(f, "invalid writable adapter declaration: {detail}"),
            ExecError::Unsupported(detail) => write!(f, "executor does not support: {detail}"),
            ExecError::Disconnected(detail) => write!(f, "executor disconnected: {detail}"),
            ExecError::Crashed(detail) => write!(f, "executor crashed: {detail}"),
            ExecError::Rejected(detail) => write!(f, "executor rejected the command: {detail}"),
        }
    }
}

impl std::error::Error for ExecError {}

pub trait WritableAdapter {
    fn name(&self) -> &str;
    fn declaration(&self) -> &WritableDeclaration;
    fn dispatch(&mut self, command: &Command) -> Result<Receipt, ExecError>;
}
