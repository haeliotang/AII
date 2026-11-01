//! Expansion accounting (M11.1, M11.2, M11.3).
//!
//! An expansion proposal is expressed as a capability-graph increment
//! (resources, assets, processes, lots). The structural gain compares a
//! human-given goal set on the base and on three stages of the expansion
//! (during construction, built, commissioned) with the construction and
//! bootstrap draw deducted, at the base's operating threshold, and reports
//! **both** the gained and the lost goals. The temporal comparison compares the
//! four VEC metric groups on a common exogenous source (the expanded branch
//! also carries the added assets' failure mechanisms) and checks that critical
//! services keep their minimum during construction.

pub mod proposal;
pub mod structural;
pub mod temporal;

pub use proposal::{Commissioning, Construction, ExpansionError, ExpansionProposal, FailureMode, ProposalAdds, Stage};
pub use structural::{classify, structural_gain, GoalOutcome, StageGain, StructuralGain, Transition};
pub use temporal::{temporal_gain, ConstructionCheck, TemporalGain};
