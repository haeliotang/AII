//! Expansion accounting (M11.1, M11.2, M11.3).
//!
//! An expansion proposal is expressed as a capability-graph increment
//! (resources, assets, processes, lots). The structural gain compares a
//! human-given goal set before and after the increment and reports **both** the
//! gained and the lost goals. The temporal comparison compares the four VEC
//! metric groups on the same disturbance traces and checks that critical
//! services keep their minimum during construction.

pub mod proposal;
pub mod structural;
pub mod temporal;

pub use proposal::{Commissioning, Construction, ExpansionError, ExpansionProposal, ProposalAdds};
pub use structural::{structural_gain, GoalOutcome, StructuralGain};
pub use temporal::{temporal_gain, ConstructionCheck, TemporalGain};
