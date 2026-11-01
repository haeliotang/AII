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

pub mod audit;
pub mod bridge;
pub mod method;
pub mod metrics;
pub mod query;
pub mod report;
pub mod verdict;

pub use audit::{check_non_anticipation, AuditError, DecisionRecord};
pub use bridge::{attach_recovery_paths, goal_label, recovery_path, recovery_path_for_goal};
pub use method::{enumerated_plan, sampled_plan, Method, MethodError, SamplePlan, Scenario, SplitMix64};
pub use metrics::{
    aggregate_external_inputs, independence, km_quantile, resilience, summarize_runs, summarize_viability,
    sustainability, viability, ConstraintViolation, IndependenceMetrics, MetricError, RecoveryPath, ResilienceMetrics,
    RunTrace, ServiceViability, SustainabilityClaim, SustainabilityMetrics, TimeSample, UndecidedRun, ViabilitySummary,
};
pub use query::{
    Budget, DisturbanceSet, FixedEvent, InitialState, ModelRef, PolicyClass, Query, QueryError, ServiceLevel,
    StateRelatedRate, TerminalCondition,
};
pub use verdict::{aggregate, check_summary, QueryVerdict, SummaryError, Verdict, VerdictKind};
