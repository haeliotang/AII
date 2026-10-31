//! Independent per-device scheduling baseline for the H1 structural experiment.
//!
//! The baseline deliberately does **not** share the capability-graph solver's
//! semantics (see `experiments/h1-structural/PREREG.md` §2): it checks every
//! device on its own direct state, direct inputs and direct occupancy, with no
//! recursion, no shared ledger, no cycle detection, no production and no
//! capability-level filtering.

pub mod baseline;
