//! Viability & Expansion Contract reference implementation (M6.1, M6.2).
//!
//! This crate implements the *question* and *answer* formats of VEC §2–§3:
//! validated query fields, the six verdict classes with their required
//! attachments, reporting guards that refuse to render `held_on_tested` as
//! safety/robustness or `timeout` as infeasibility, and the reproducible
//! `sampled` / `enumerated` method plans with seed management.
//!
//! Metrics (VEC §4), machine/human reports (§7), the structural/temporal bridge
//! (§3.3) and the set-method exploration (§3.2) are the remaining M6 packages.

pub mod bridge;
pub mod method;
pub mod metrics;
pub mod query;
pub mod report;
pub mod verdict;

pub use bridge::{attach_recovery_paths, recovery_path};
pub use method::{enumerated_plan, sampled_plan, Method, MethodError, SamplePlan, Scenario, SplitMix64};
pub use metrics::{
    aggregate_external_inputs, independence, resilience, summarize_viability, sustainability, viability,
    ConstraintViolation, IndependenceMetrics, MetricError, RecoveryPath, ResilienceMetrics, RunTrace, ServiceViability,
    SustainabilityClaim, SustainabilityMetrics, TimeSample, ViabilitySummary,
};
pub use query::{
    Budget, DisturbanceSet, FixedEvent, InitialState, ModelRef, PolicyClass, Query, QueryError, ServiceLevel,
    StateRelatedRate,
};
pub use verdict::{check_summary, SummaryError, Verdict, VerdictKind};
