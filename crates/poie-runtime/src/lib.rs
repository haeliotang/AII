//! Local shadow runtime for M9.
//!
//! The runtime only accepts **read-only** adapters, converts their observations
//! into SAP observation records, projects them with an evidence-expiry policy,
//! and derives the ontology state. Shadow mode runs the planner on that state
//! and records what it would do without dispatching anything. Predictions are
//! reconciled against observations separately.

pub mod adapter;
pub mod prediction;
pub mod runtime;
pub mod shadow;

pub use adapter::{
    AdapterDeclaration, AdapterError, AdapterKind, DisconnectBehavior, Observation, ObservationQuality,
    ReadOnlyAdapter, SimulatedAdapter, TimestampSemantics, UnitDecl,
};
pub use prediction::{check_predictions, Prediction, PredictionCheck, PredictionSummary};
pub use runtime::{HealthReport, LocalRuntime, PollReport, RuntimeError};
pub use shadow::{shadow_plan, OperatorAction, ShadowDivergence, ShadowReport};
