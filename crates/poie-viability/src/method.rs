//! Reproducible `sampled` and `enumerated` methods (VEC §3.2).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::query::FixedEvent;
use crate::verdict::VerdictKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Sampled,
    Enumerated,
    SetBased,
    Certificate,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Sampled => "sampled",
            Method::Enumerated => "enumerated",
            Method::SetBased => "set_based",
            Method::Certificate => "certificate",
        }
    }

    /// The strongest claim this method can support (VEC §3.2).
    pub fn strongest_claim(self) -> &'static str {
        match self {
            Method::Sampled => "holds on the tested scenarios / a counterexample was found",
            Method::Enumerated => "holds on the finite disturbance set W / a counterexample was found",
            Method::SetBased => "holds in the model and W (state the conservative direction)",
            Method::Certificate => "infeasible or holds in the model, with an independently checkable certificate",
        }
    }
}

impl Method {
    /// Method × verdict compatibility (VEC §3.2). `sampled` and `enumerated`
    /// test finitely many scenarios and can never prove infeasibility in the
    /// model; only `set_based` and `certificate` may issue `infeasible_in_model`.
    pub fn allows(self, verdict: VerdictKind) -> bool {
        match verdict {
            VerdictKind::InfeasibleInModel => matches!(self, Method::SetBased | Method::Certificate),
            VerdictKind::HeldOnTested
            | VerdictKind::Counterexample
            | VerdictKind::Timeout
            | VerdictKind::NumericalFailure
            | VerdictKind::UnknownInput => true,
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// SplitMix64: a small deterministic PRNG so sampled runs need no external crate
/// and replay from the seeds alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> SplitMix64 {
        SplitMix64 { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)` with 53 bits of precision.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform index in `0..bound`; `bound == 0` returns 0.
    pub fn range(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next_u64() % bound as u64) as usize
        }
    }
}

/// One sampled or enumerated scenario.
#[derive(Clone, Debug, PartialEq)]
pub struct Scenario {
    pub id: String,
    pub seed: u64,
    pub draws: Vec<f64>,
}

/// A reproducible method plan: method, scenarios and a canonical digest.
#[derive(Clone, Debug, PartialEq)]
pub struct SamplePlan {
    pub method: Method,
    pub scenarios: Vec<Scenario>,
}

impl SamplePlan {
    pub fn len(&self) -> usize {
        self.scenarios.len()
    }

    pub fn is_empty(&self) -> bool {
        self.scenarios.is_empty()
    }

    pub fn scenario_ids(&self) -> Vec<&str> {
        self.scenarios.iter().map(|scenario| scenario.id.as_str()).collect()
    }

    /// Canonical digest of ids and draws; two plans with the same seeds and
    /// parameters must produce the same digest (replay evidence).
    pub fn digest(&self) -> String {
        let mut out = format!("method={};", self.method.as_str());
        for scenario in &self.scenarios {
            out.push_str(&format!("{}|{}|", scenario.id, scenario.seed));
            for draw in &scenario.draws {
                out.push_str(&format!("{draw:.12},"));
            }
            out.push(';');
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MethodError {
    EmptySeeds,
    DuplicateSeed(u64),
    TooManySeeds { seeds: usize, max: usize },
    ZeroDraws,
    NoScenarios,
    InvalidEvent(i64),
}

impl fmt::Display for MethodError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MethodError::EmptySeeds => f.write_str("the sampled method needs at least one seed"),
            MethodError::DuplicateSeed(seed) => write!(f, "duplicate seed {seed}"),
            MethodError::TooManySeeds { seeds, max } => {
                write!(f, "{seeds} seeds exceed the scenario budget of {max}")
            }
            MethodError::ZeroDraws => f.write_str("draws per scenario must be positive"),
            MethodError::NoScenarios => f.write_str("the enumerated method needs at least one scenario"),
            MethodError::InvalidEvent(index) => write!(f, "fixed event {index} is invalid"),
        }
    }
}

impl std::error::Error for MethodError {}

/// Samples scenarios from seeds; each scenario draws `draws_per_scenario`
/// uniform values in `[0, 1)`. The same seeds always produce the same plan.
pub fn sampled_plan(seeds: &[u64], draws_per_scenario: usize, max_scenarios: usize) -> Result<SamplePlan, MethodError> {
    if seeds.is_empty() {
        return Err(MethodError::EmptySeeds);
    }
    if draws_per_scenario == 0 {
        return Err(MethodError::ZeroDraws);
    }
    if seeds.len() > max_scenarios {
        return Err(MethodError::TooManySeeds { seeds: seeds.len(), max: max_scenarios });
    }
    let mut seen = std::collections::BTreeSet::new();
    for seed in seeds {
        if !seen.insert(*seed) {
            return Err(MethodError::DuplicateSeed(*seed));
        }
    }
    let scenarios = seeds
        .iter()
        .map(|seed| {
            let mut rng = SplitMix64::new(*seed);
            let draws = (0..draws_per_scenario).map(|_| rng.next_f64()).collect();
            Scenario { id: format!("sample-{seed}"), seed: *seed, draws }
        })
        .collect();
    Ok(SamplePlan { method: Method::Sampled, scenarios })
}

/// Enumerates a finite disturbance set: baseline (no fixed event), each fixed
/// event alone, and (when there is more than one) all fixed events together.
/// State-related rates stay outside this finite enumeration and are sampled.
pub fn enumerated_plan(fixed_events: &[FixedEvent], horizon_s: f64) -> Result<SamplePlan, MethodError> {
    if fixed_events.is_empty() {
        return Err(MethodError::NoScenarios);
    }
    for (index, event) in fixed_events.iter().enumerate() {
        if !event.at_s.is_finite() || event.at_s < 0.0 || event.at_s > horizon_s {
            return Err(MethodError::InvalidEvent(index as i64));
        }
    }
    let mut scenarios = Vec::with_capacity(fixed_events.len() + 2);
    scenarios.push(Scenario { id: "baseline".into(), seed: 0, draws: Vec::new() });
    for (index, event) in fixed_events.iter().enumerate() {
        scenarios.push(Scenario { id: format!("fixed-{index}"), seed: 0, draws: vec![event.at_s] });
    }
    if fixed_events.len() > 1 {
        scenarios.push(Scenario {
            id: "fixed-all".into(),
            seed: 0,
            draws: fixed_events.iter().map(|event| event.at_s).collect(),
        });
    }
    Ok(SamplePlan { method: Method::Enumerated, scenarios })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(at_s: f64) -> FixedEvent {
        FixedEvent { at_s, description: "bus fails".into(), synthetic: true }
    }

    #[test]
    fn splitmix64_is_deterministic() {
        let mut left = SplitMix64::new(42);
        let mut right = SplitMix64::new(42);
        for _ in 0..8 {
            assert_eq!(left.next_u64(), right.next_u64());
        }
        let mut rng = SplitMix64::new(7);
        for _ in 0..100 {
            let value = rng.next_f64();
            assert!((0.0..1.0).contains(&value));
        }
    }

    #[test]
    fn sampled_plans_replay_from_the_seeds() {
        let first = sampled_plan(&[1, 2, 3], 4, 100).unwrap();
        let second = sampled_plan(&[1, 2, 3], 4, 100).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.digest(), second.digest());
        assert_eq!(first.scenario_ids(), vec!["sample-1", "sample-2", "sample-3"]);

        let other = sampled_plan(&[4, 5, 6], 4, 100).unwrap();
        assert_ne!(first.digest(), other.digest());
    }

    #[test]
    fn sampled_plan_rejects_bad_seeds_and_budgets() {
        assert_eq!(sampled_plan(&[], 4, 10), Err(MethodError::EmptySeeds));
        assert_eq!(sampled_plan(&[1, 1], 4, 10), Err(MethodError::DuplicateSeed(1)));
        assert_eq!(sampled_plan(&[1, 2], 0, 10), Err(MethodError::ZeroDraws));
        assert!(matches!(sampled_plan(&[1, 2, 3], 1, 2), Err(MethodError::TooManySeeds { .. })));
    }

    #[test]
    fn enumerated_plans_cover_baseline_each_event_and_all_events() {
        let plan = enumerated_plan(&[event(10.0), event(20.0)], 100.0).unwrap();
        assert_eq!(plan.method, Method::Enumerated);
        assert_eq!(plan.scenario_ids(), vec!["baseline", "fixed-0", "fixed-1", "fixed-all"]);
        let single = enumerated_plan(&[event(10.0)], 100.0).unwrap();
        assert_eq!(single.scenario_ids(), vec!["baseline", "fixed-0"]);
        assert!(matches!(enumerated_plan(&[event(200.0)], 100.0), Err(MethodError::InvalidEvent(0))));
        assert_eq!(enumerated_plan(&[], 100.0), Err(MethodError::NoScenarios));
    }

    #[test]
    fn only_proof_methods_may_issue_infeasible_in_model() {
        assert!(!Method::Sampled.allows(VerdictKind::InfeasibleInModel));
        assert!(!Method::Enumerated.allows(VerdictKind::InfeasibleInModel));
        assert!(Method::SetBased.allows(VerdictKind::InfeasibleInModel));
        assert!(Method::Certificate.allows(VerdictKind::InfeasibleInModel));
        assert!(Method::Enumerated.allows(VerdictKind::Counterexample));
    }

    #[test]
    fn methods_state_their_strongest_claims() {
        assert!(Method::Sampled.strongest_claim().contains("tested scenarios"));
        assert!(Method::Enumerated.strongest_claim().contains("finite disturbance set"));
        assert!(Method::SetBased.strongest_claim().contains("model and W"));
    }
}
