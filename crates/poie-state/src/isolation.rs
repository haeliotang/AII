//! Simulation isolation (SAP §7; M4.8).
//!
//! A simulation branch may run its own lifecycle, but it must never dispatch a
//! field action, hold command ownership, a lease or an authorization. Records
//! from a simulation namespace are also excluded from live projections (see
//! `projection::project`). This module is the dispatch-side gate.

use crate::record::Namespace;

/// Why a dispatch was refused before any review or physical effect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IsolationError {
    /// Stable reason code, e.g. `simulation_cannot_dispatch`.
    pub reason: &'static str,
    pub detail: String,
}

/// Allows dispatch only from the live namespace (SAP §7.2).
pub fn ensure_live_dispatch(namespace: &Namespace, action_id: &str) -> Result<(), IsolationError> {
    match namespace {
        Namespace::Live => Ok(()),
        Namespace::Simulation(branch) => Err(IsolationError {
            reason: "simulation_cannot_dispatch",
            detail: format!("simulation branch {branch:?} must not dispatch field action {action_id:?}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_namespace_may_dispatch() {
        assert_eq!(ensure_live_dispatch(&Namespace::live(), "a1"), Ok(()));
    }

    #[test]
    fn simulation_namespace_may_never_dispatch() {
        let branch = Namespace::simulation("h1-preview").unwrap();
        let error = ensure_live_dispatch(&branch, "a1").unwrap_err();
        assert_eq!(error.reason, "simulation_cannot_dispatch");
        assert!(error.detail.contains("h1-preview"));
    }
}
