//! Reference prototype of the Poieweave Physical Ontology contract
//! (`specifications/physical-ontology.zh-CN.md`, v0.2-draft).
//!
//! - [`units`]: dimensioned quantities and the v0.2-draft unit table (§2).
//! - [`model`]: entities, loading, validation and mass conservation (§3, §5).
//! - [`graph`]: capability-graph queries with a single-ledger solver (§6).
//! - [`scenario`]: executable scenario cases with expected outcomes.
//! - [`time`]: time-aware asset status and stock with failure predictions (M5.1).
//! - [`schedule`]: process instances and earliest-start scheduling feasibility
//!   under precedence and unit capacity (M5.2).
//!
//! This is a research prototype. A `candidate` route is structural only: it
//! does not check capacity, concurrency or physical execution; the scheduler
//! checks time/capacity separately and reports conflicts instead of hiding them.

pub mod graph;
pub mod model;
pub mod scenario;
pub mod schedule;
pub mod time;
pub mod units;
