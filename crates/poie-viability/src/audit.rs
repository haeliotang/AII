//! Checkable non-anticipation records (VEC §2.1, review C-06).
//!
//! The feasible set is defined with ∃π∀w over *non-anticipating* policies. An
//! evaluation that re-optimizes each scenario with the whole disturbance
//! trajectory in view answers ∀w∃π instead. Reports therefore carry one record
//! per decision: when it was taken, up to which time the state or observations
//! it used extend, and which policy instance took it. A report is valid only if
//! no decision used information from after its own time and a single policy
//! instance ran on every scenario.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionRecord {
    pub scenario_id: String,
    /// Time of the decision.
    pub decided_at_s: f64,
    /// Latest time of any state version or observation the decision used.
    pub observed_up_to_s: f64,
    /// The policy instance (same parameters and configuration on every scenario).
    pub policy_instance: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuditError {
    Empty,
    Anticipating { scenario_id: String, decided_at_s: String, observed_up_to_s: String },
    SeveralPolicyInstances(Vec<String>),
    Invalid(String),
}

impl fmt::Display for AuditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuditError::Empty => f.write_str("decision records are required (VEC §2.1 non-anticipation)"),
            AuditError::Anticipating { scenario_id, decided_at_s, observed_up_to_s } => write!(
                f,
                "decision in {scenario_id} at {decided_at_s}s used observations up to {observed_up_to_s}s (anticipating)"
            ),
            AuditError::SeveralPolicyInstances(instances) => write!(
                f,
                "one policy instance must run on every scenario; found {} ({})",
                instances.len(),
                instances.join(", ")
            ),
            AuditError::Invalid(detail) => write!(f, "invalid decision record: {detail}"),
        }
    }
}

impl std::error::Error for AuditError {}

/// Checks the decision records of one evaluation.
pub fn check_non_anticipation(records: &[DecisionRecord]) -> Result<(), Vec<AuditError>> {
    if records.is_empty() {
        return Err(vec![AuditError::Empty]);
    }
    let mut errors = Vec::new();
    let mut instances = BTreeSet::new();
    for record in records {
        if record.scenario_id.trim().is_empty() || record.policy_instance.trim().is_empty() {
            errors.push(AuditError::Invalid("scenario id and policy instance must not be empty".into()));
        }
        if !record.decided_at_s.is_finite() || !record.observed_up_to_s.is_finite() {
            errors.push(AuditError::Invalid("decision and observation times must be finite".into()));
            continue;
        }
        if record.observed_up_to_s > record.decided_at_s + 1e-9 {
            errors.push(AuditError::Anticipating {
                scenario_id: record.scenario_id.clone(),
                decided_at_s: format!("{:.0}", record.decided_at_s),
                observed_up_to_s: format!("{:.0}", record.observed_up_to_s),
            });
        }
        instances.insert(record.policy_instance.clone());
    }
    if instances.len() > 1 {
        errors.push(AuditError::SeveralPolicyInstances(instances.into_iter().collect()));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(scenario: &str, decided: f64, observed: f64, instance: &str) -> DecisionRecord {
        DecisionRecord {
            scenario_id: scenario.into(),
            decided_at_s: decided,
            observed_up_to_s: observed,
            policy_instance: instance.into(),
        }
    }

    #[test]
    fn causal_records_of_one_instance_pass() {
        let records = [record("w1", 0.0, 0.0, "p"), record("w1", 7200.0, 7200.0, "p"), record("w2", 0.0, 0.0, "p")];
        assert!(check_non_anticipation(&records).is_ok());
    }

    #[test]
    fn empty_records_are_rejected() {
        assert_eq!(check_non_anticipation(&[]), Err(vec![AuditError::Empty]));
    }
}
