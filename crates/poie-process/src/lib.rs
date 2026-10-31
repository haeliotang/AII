//! Versioned bridge to external process models (M7.1, M7.3, M7.4).
//!
//! The Rust core calls a Python model as a separate process over a JSON-line
//! protocol, with an explicit schema, model identifier/version, units, a
//! deadline and classified failures. Responses are validated (units, finiteness,
//! validity range, mass conservation) before they are used; a failing call never
//! turns into an assumption of success.

pub mod client;
pub mod protocol;
pub mod validate;

pub use client::{BridgeClient, BridgeError, BridgeErrorClass};
pub use protocol::{BridgeRequest, BridgeResponse, ModelId, Parameters, Validity, BRIDGE_SCHEMA};
pub use validate::{validate_response, ValidationReport};
