//! Query fields and validation (VEC §2.2).

use std::fmt;

use poie_ontology::model::Verification;

use crate::method::Method;

/// The starting state, identified by a state-version hash or scenario initial-state hash.
#[derive(Clone, Debug, PartialEq)]
pub struct InitialState {
    pub state_version_hash: String,
}

/// A critical service and its minimum level (fraction of nominal, 0..=1).
#[derive(Clone, Debug, PartialEq)]
pub struct ServiceLevel {
    pub service: String,
    pub minimum: f64,
}

/// A fixed-trajectory disturbance ("bus fails at hour 30").
#[derive(Clone, Debug, PartialEq)]
pub struct FixedEvent {
    pub at_s: f64,
    pub description: String,
    /// `true` when the parameters are synthetic assumptions.
    pub synthetic: bool,
}

/// A state-related disturbance whose rate depends on state and usage history.
#[derive(Clone, Debug, PartialEq)]
pub struct StateRelatedRate {
    pub driver: String,
    pub failure_rate_per_hour: f64,
    pub synthetic: bool,
}

/// The disturbance set W, split into fixed and state-related parts (VEC §2.3).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DisturbanceSet {
    pub fixed: Vec<FixedEvent>,
    pub state_related: Vec<StateRelatedRate>,
}

impl DisturbanceSet {
    pub fn is_empty(&self) -> bool {
        self.fixed.is_empty() && self.state_related.is_empty()
    }

    pub fn is_synthetic(&self) -> bool {
        self.fixed.iter().any(|event| event.synthetic) || self.state_related.iter().any(|rate| rate.synthetic)
    }
}

/// The non-anticipating policy class (rules, planner, replanning, observations).
#[derive(Clone, Debug, PartialEq)]
pub struct PolicyClass {
    pub rules: String,
    pub replanning_frequency_s: f64,
    pub observations: String,
}

/// A model identifier, version and scope.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelRef {
    pub id: String,
    pub version: String,
    pub scope: String,
}

/// Compute budget: wall-clock and scenario limits.
#[derive(Clone, Debug, PartialEq)]
pub struct Budget {
    pub wall_clock_s: f64,
    pub max_scenarios: usize,
}

/// A feasibility query (VEC §2.2). All required fields must be present.
#[derive(Clone, Debug, PartialEq)]
pub struct Query {
    pub initial_state: InitialState,
    pub horizon_s: f64,
    pub services: Vec<ServiceLevel>,
    pub hard_constraints: Vec<String>,
    pub disturbances: DisturbanceSet,
    pub policy_class: PolicyClass,
    pub models: Vec<ModelRef>,
    pub min_verification: Verification,
    pub budget: Budget,
    pub method: Method,
    /// Required for `sampled`; must be non-empty and duplicate-free.
    pub seeds: Vec<u64>,
}

impl Query {
    pub fn validate(&self) -> Result<(), Vec<QueryError>> {
        let mut errors = Vec::new();
        if self.initial_state.state_version_hash.trim().is_empty() {
            errors.push(QueryError::Missing("initial_state.state_version_hash"));
        }
        if !self.horizon_s.is_finite() || self.horizon_s <= 0.0 {
            errors.push(QueryError::Invalid("horizon must be finite and positive"));
        }
        if self.services.is_empty() {
            errors.push(QueryError::Missing("services"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for service in &self.services {
            if service.service.trim().is_empty() {
                errors.push(QueryError::Invalid("service name must not be empty"));
            }
            if !seen.insert(service.service.as_str()) {
                errors.push(QueryError::Duplicate(format!("service {:?}", service.service)));
            }
            if !service.minimum.is_finite() || !(0.0..=1.0).contains(&service.minimum) {
                errors.push(QueryError::Invalid("service minimum must be a fraction in 0..=1"));
            }
        }
        if self.hard_constraints.is_empty() {
            errors.push(QueryError::Missing("hard_constraints"));
        }
        if self.disturbances.is_empty() {
            errors.push(QueryError::Missing("disturbances"));
        }
        for event in &self.disturbances.fixed {
            if !event.at_s.is_finite() || event.at_s < 0.0 || event.at_s > self.horizon_s {
                errors.push(QueryError::Invalid("fixed event time must lie within the horizon"));
            }
            if event.description.trim().is_empty() {
                errors.push(QueryError::Invalid("fixed event description must not be empty"));
            }
        }
        for rate in &self.disturbances.state_related {
            if rate.driver.trim().is_empty() {
                errors.push(QueryError::Invalid("state-related driver must not be empty"));
            }
            if !rate.failure_rate_per_hour.is_finite() || rate.failure_rate_per_hour <= 0.0 {
                errors.push(QueryError::Invalid("state-related failure rate must be finite and positive"));
            }
        }
        if self.policy_class.rules.trim().is_empty() {
            errors.push(QueryError::Missing("policy_class.rules"));
        }
        if self.policy_class.observations.trim().is_empty() {
            errors.push(QueryError::Missing("policy_class.observations"));
        }
        if !self.policy_class.replanning_frequency_s.is_finite() || self.policy_class.replanning_frequency_s <= 0.0 {
            errors.push(QueryError::Invalid("replanning frequency must be finite and positive"));
        }
        if self.models.is_empty() {
            errors.push(QueryError::Missing("models"));
        }
        for model in &self.models {
            if model.id.trim().is_empty() || model.version.trim().is_empty() || model.scope.trim().is_empty() {
                errors.push(QueryError::Invalid("model id, version and scope must not be empty"));
            }
        }
        if !self.budget.wall_clock_s.is_finite() || self.budget.wall_clock_s <= 0.0 {
            errors.push(QueryError::Invalid("wall-clock budget must be finite and positive"));
        }
        if self.budget.max_scenarios == 0 {
            errors.push(QueryError::Invalid("max_scenarios must be positive"));
        }
        if self.seeds.len() > self.budget.max_scenarios {
            errors.push(QueryError::Invalid("more seeds than the scenario budget allows"));
        }
        if self.method == Method::Sampled {
            if self.seeds.is_empty() {
                errors.push(QueryError::Missing("seeds (required for the sampled method)"));
            }
            let mut seen_seeds = std::collections::BTreeSet::new();
            for seed in &self.seeds {
                if !seen_seeds.insert(*seed) {
                    errors.push(QueryError::Duplicate(format!("seed {seed}")));
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn horizon_s(&self) -> f64 {
        self.horizon_s
    }

    pub fn seeds(&self) -> &[u64] {
        &self.seeds
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryError {
    Missing(&'static str),
    Invalid(&'static str),
    Duplicate(String),
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueryError::Missing(field) => write!(f, "missing required field: {field}"),
            QueryError::Invalid(detail) => write!(f, "invalid field: {detail}"),
            QueryError::Duplicate(item) => write!(f, "duplicate {item}"),
        }
    }
}

impl std::error::Error for QueryError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_query() -> Query {
        Query {
            initial_state: InitialState { state_version_hash: "state-v3".into() },
            horizon_s: 7.0 * 86_400.0,
            services: vec![ServiceLevel { service: "potable-water".into(), minimum: 1.0 }],
            hard_constraints: vec!["pump head >= 30 m".into()],
            disturbances: DisturbanceSet {
                fixed: vec![FixedEvent { at_s: 30.0 * 3_600.0, description: "bus fails".into(), synthetic: true }],
                state_related: vec![StateRelatedRate {
                    driver: "pump vibration".into(),
                    failure_rate_per_hour: 0.002,
                    synthetic: true,
                }],
            },
            policy_class: PolicyClass {
                rules: "restore-first".into(),
                replanning_frequency_s: 900.0,
                observations: "asset status, stock and service level".into(),
            },
            models: vec![ModelRef { id: "base-01".into(), version: "0.3".into(), scope: "synthetic plant".into() }],
            min_verification: Verification::Modeled,
            budget: Budget { wall_clock_s: 60.0, max_scenarios: 100 },
            method: Method::Sampled,
            seeds: vec![1, 2, 3],
        }
    }

    #[test]
    fn a_complete_query_validates() {
        assert!(base_query().validate().is_ok());
    }

    #[test]
    fn sampled_requires_seeds_and_missing_fields_are_reported() {
        let mut query = base_query();
        query.seeds.clear();
        let errors = query.validate().unwrap_err();
        assert!(errors.iter().any(|error| matches!(error, QueryError::Missing(field) if field.contains("seeds"))));

        let mut query = base_query();
        query.horizon_s = 0.0;
        query.services.clear();
        query.hard_constraints.clear();
        assert!(query.validate().is_err());
    }

    #[test]
    fn duplicate_services_and_seeds_are_rejected() {
        let mut query = base_query();
        query.services.push(ServiceLevel { service: "potable-water".into(), minimum: 0.5 });
        assert!(query.validate().unwrap_err().iter().any(|error| matches!(error, QueryError::Duplicate(_))));

        let mut query = base_query();
        query.seeds = vec![7, 7];
        assert!(query.validate().unwrap_err().iter().any(|error| matches!(error, QueryError::Duplicate(_))));
    }

    #[test]
    fn events_outside_the_horizon_are_rejected() {
        let mut query = base_query();
        query.disturbances.fixed[0].at_s = query.horizon_s + 1.0;
        assert!(query.validate().is_err());
    }
}
