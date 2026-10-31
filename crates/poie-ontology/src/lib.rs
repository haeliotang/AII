//! Reference prototype of the Poieweave Physical Ontology contract
//! (`specifications/physical-ontology.zh-CN.md`, v0.1-draft).
//!
//! - [`units`]: dimensioned quantities and the v0.1 unit table (§2).
//! - [`model`]: entities, loading, validation and mass conservation (§3, §5).
//! - [`graph`]: capability-graph queries with a single-ledger solver (§6).
//! - [`scenario`]: executable scenario cases with expected outcomes.
//!
//! This is a research prototype. A `candidate` route is structural only: it
//! does not check time, capacity, concurrency or physical execution.

pub mod graph;
pub mod model;
pub mod scenario;
pub mod units;
