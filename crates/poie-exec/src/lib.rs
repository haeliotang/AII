//! Bounded execution (M10).
//!
//! [`writable`] defines the writable-adapter contract (idempotency key, command
//! validity, declared verification sources). [`gate`] is the only path from an
//! authorized SAP action to a writable adapter: it enforces the validity
//! window, the idempotency key, human takeover and safe degradation, and it
//! refuses dispatch when the adapter cannot honour an idempotency key.
//! [`verify`] requires at least one verification source independent of the
//! executor. [`sim`] provides a simulated writable adapter with fault injection
//! for the simulated H3 run.

pub mod gate;
pub mod sim;
pub mod verify;
pub mod writable;

pub use gate::{DispatchOutcome, ExecutionGate, GateError, GateLease};
pub use sim::{AdapterFault, SimulatedWritableAdapter};
pub use verify::{verify_action, VerificationError, VerificationOutcome, VerificationSignal};
pub use writable::{Command, ExecError, Receipt, WritableAdapter, WritableDeclaration};
