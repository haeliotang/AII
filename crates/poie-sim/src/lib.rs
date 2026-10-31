//! Discrete-event simulation for Poieweave (M5.3, M5.4).
//!
//! The simulator advances a deterministic event queue, applies disturbances
//! (fixed trajectories and state-dependent rules), integrates battery energy
//! with power/capacity limits, and checks mass conservation for every internal
//! transfer. The same input always yields the same event log and digest, which
//! is the basis for the replay requirement (M5.5).

pub mod battery;
pub mod event;
pub mod simulation;

pub use battery::{Battery, EnergyLimit, EnergyOutcome};
pub use event::{Event, EventKind, Flow, SimError};
pub use simulation::{ConservationViolation, ExternalFlow, LogEntry, RuleCondition, RuleEffect, Simulation, StateRule};
