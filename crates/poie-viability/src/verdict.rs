//! Verdict classes, required attachments and reporting guards (VEC §3.1).
//!
//! The six classes are deliberately distinct: `timeout` and `numerical_failure`
//! are *undecided*, not infeasible; `held_on_tested` is a statement about the
//! tested scenarios only and must never be rendered as "safe", "robust" or
//! "viable". [`check_summary`] enforces this on human-readable summaries.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::method::Method;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerdictKind {
    HeldOnTested,
    Counterexample,
    InfeasibleInModel,
    Timeout,
    NumericalFailure,
    UnknownInput,
}

impl VerdictKind {
    pub fn as_str(self) -> &'static str {
        match self {
            VerdictKind::HeldOnTested => "held_on_tested",
            VerdictKind::Counterexample => "counterexample",
            VerdictKind::InfeasibleInModel => "infeasible_in_model",
            VerdictKind::Timeout => "timeout",
            VerdictKind::NumericalFailure => "numerical_failure",
            VerdictKind::UnknownInput => "unknown_input",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    HeldOnTested { tested_scenarios: usize, method: Method, uncovered: String },
    Counterexample { scenario_id: String, first_violation_s: f64, constraint: String, state: String },
    InfeasibleInModel { certificate: String, checkable: String },
    Timeout { completed_scenarios: usize, budget: String },
    NumericalFailure { solver: String, settings: String, message: String },
    UnknownInput { unknowns: Vec<String> },
}

impl Verdict {
    pub fn kind(&self) -> VerdictKind {
        match self {
            Verdict::HeldOnTested { .. } => VerdictKind::HeldOnTested,
            Verdict::Counterexample { .. } => VerdictKind::Counterexample,
            Verdict::InfeasibleInModel { .. } => VerdictKind::InfeasibleInModel,
            Verdict::Timeout { .. } => VerdictKind::Timeout,
            Verdict::NumericalFailure { .. } => VerdictKind::NumericalFailure,
            Verdict::UnknownInput { .. } => VerdictKind::UnknownInput,
        }
    }

    /// VEC §3.1 required attachments must be present and non-empty.
    pub fn attachments_complete(&self) -> bool {
        match self {
            Verdict::HeldOnTested { tested_scenarios, uncovered, .. } => {
                *tested_scenarios > 0 && !uncovered.trim().is_empty()
            }
            Verdict::Counterexample { scenario_id, first_violation_s, constraint, state } => {
                !scenario_id.trim().is_empty()
                    && first_violation_s.is_finite()
                    && *first_violation_s >= 0.0
                    && !constraint.trim().is_empty()
                    && !state.trim().is_empty()
            }
            Verdict::InfeasibleInModel { certificate, checkable } => {
                !certificate.trim().is_empty() && !checkable.trim().is_empty()
            }
            Verdict::Timeout { budget, .. } => !budget.trim().is_empty(),
            Verdict::NumericalFailure { solver, settings, message } => {
                !solver.trim().is_empty() && !settings.trim().is_empty() && !message.trim().is_empty()
            }
            Verdict::UnknownInput { unknowns } => {
                !unknowns.is_empty() && unknowns.iter().all(|item| !item.trim().is_empty())
            }
        }
    }

    /// A canonical summary that obeys VEC §3.1's wording constraints.
    pub fn canonical_summary(&self) -> String {
        match self {
            Verdict::HeldOnTested { tested_scenarios, method, uncovered } => format!(
                "held on {tested_scenarios} tested scenario(s) with the {} method; {uncovered}; untested disturbances are not covered, and this is not a claim of safety or robustness",
                method.as_str()
            ),
            Verdict::Counterexample { scenario_id, first_violation_s, constraint, state } => format!(
                "counterexample for the tested policy in scenario {scenario_id}: constraint {constraint:?} first violated at {first_violation_s:.0}s (state: {state}); it refutes this policy, not every policy"
            ),
            Verdict::InfeasibleInModel { certificate, checkable } => {
                format!("infeasible in the model; certificate {certificate:?}, checked by {checkable:?}")
            }
            Verdict::Timeout { completed_scenarios, budget } => format!(
                "timeout after {completed_scenarios} scenario(s) ({budget}); the result is undecided, not infeasible"
            ),
            Verdict::NumericalFailure { solver, settings, message } => format!(
                "numerical failure in solver {solver:?} ({settings}): {message}; the result is undecided"
            ),
            Verdict::UnknownInput { unknowns } => {
                format!("cannot decide: unknown inputs {}", unknowns.join(", "))
            }
        }
    }
}

/// The query-level verdict aggregated from per-scenario verdicts (VEC §3.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryVerdict {
    pub kind: VerdictKind,
    /// Number of scenarios per verdict class; every class is reported.
    pub counts: Vec<(VerdictKind, usize)>,
}

/// Aggregation order (VEC §3.2): any `infeasible_in_model` wins; else any
/// `counterexample`; else the first undecided class present in the order
/// `unknown_input`, `numerical_failure`, `timeout`; `held_on_tested` only when
/// every scenario held. Majority votes and favourable subsets are not allowed.
pub fn aggregate<'a>(verdicts: impl IntoIterator<Item = &'a Verdict>) -> Option<QueryVerdict> {
    const ORDER: [VerdictKind; 6] = [
        VerdictKind::InfeasibleInModel,
        VerdictKind::Counterexample,
        VerdictKind::UnknownInput,
        VerdictKind::NumericalFailure,
        VerdictKind::Timeout,
        VerdictKind::HeldOnTested,
    ];
    let mut counts = [0usize; 6];
    let mut any = false;
    for verdict in verdicts {
        any = true;
        let position = ORDER.iter().position(|kind| *kind == verdict.kind()).expect("all kinds are ordered");
        counts[position] += 1;
    }
    if !any {
        return None;
    }
    let kind = ORDER.iter().zip(counts).find(|(_, count)| *count > 0).map(|(kind, _)| *kind)?;
    Some(QueryVerdict { kind, counts: ORDER.iter().copied().zip(counts).collect() })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SummaryError {
    Empty,
    ForbiddenClaim { claim: String, verdict: VerdictKind },
}

impl fmt::Display for SummaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SummaryError::Empty => f.write_str("summary must not be empty"),
            SummaryError::ForbiddenClaim { claim, verdict } => {
                write!(f, "claim {claim:?} is not allowed for a {} verdict", verdict.as_str())
            }
        }
    }
}

impl std::error::Error for SummaryError {}

const SAFETY_CLAIMS: [&str; 11] =
    ["safe", "safety", "secure", "robust", "robustness", "viable", "viability", "guarantee", "安全", "鲁棒", "可生存"];
const UNDECIDED_CLAIMS: [&str; 6] = ["infeasible", "not viable", "cannot be done", "不可行", "无法达到", "做不到"];
const COVERAGE_CLAIMS: [&str; 6] =
    ["all disturbances", "for all w", "in the model and w", "所有扰动", "在模型与 w 内", "覆盖全部扰动"];

/// Checks a human-readable summary against the verdict's wording constraints.
pub fn check_summary(verdict: &Verdict, summary: &str) -> Result<(), SummaryError> {
    let trimmed = summary.trim();
    if trimmed.is_empty() {
        return Err(SummaryError::Empty);
    }
    let lowered = trimmed.to_lowercase();
    let contains = |claims: &'static [&'static str]| -> Option<&'static str> {
        for claim in claims {
            for (index, _) in lowered.match_indices(claim) {
                let context = &lowered[index.saturating_sub(64)..index];
                let negated = context.contains("not ")
                    || context.contains("no ")
                    || context.contains("never ")
                    || context.contains("不是")
                    || context.contains("并非")
                    || context.contains('不');
                if !negated {
                    return Some(claim);
                }
            }
        }
        None
    };
    match verdict {
        Verdict::HeldOnTested { method, .. } => {
            if let Some(claim) = contains(&SAFETY_CLAIMS) {
                return Err(SummaryError::ForbiddenClaim { claim: claim.to_string(), verdict: verdict.kind() });
            }
            if !matches!(method, Method::Enumerated | Method::SetBased | Method::Certificate) {
                if let Some(claim) = contains(&COVERAGE_CLAIMS) {
                    return Err(SummaryError::ForbiddenClaim { claim: claim.to_string(), verdict: verdict.kind() });
                }
            }
        }
        Verdict::Timeout { .. } | Verdict::NumericalFailure { .. } => {
            if let Some(claim) = contains(&UNDECIDED_CLAIMS) {
                return Err(SummaryError::ForbiddenClaim { claim: claim.to_string(), verdict: verdict.kind() });
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held() -> Verdict {
        Verdict::HeldOnTested {
            tested_scenarios: 12,
            method: Method::Sampled,
            uncovered: "state-related failures above the tested vibration range".into(),
        }
    }

    #[test]
    fn all_six_classes_have_required_attachments() {
        let verdicts = [
            held(),
            Verdict::Counterexample {
                scenario_id: "sample-7".into(),
                first_violation_s: 30.0,
                constraint: "pump head >= 30 m".into(),
                state: "pump degraded".into(),
            },
            Verdict::InfeasibleInModel { certificate: "unsat-core".into(), checkable: "poie check cert.cert".into() },
            Verdict::Timeout { completed_scenarios: 3, budget: "60 s wall clock".into() },
            Verdict::NumericalFailure {
                solver: "vec-solver".into(),
                settings: "tol=1e-6".into(),
                message: "NaN".into(),
            },
            Verdict::UnknownInput { unknowns: vec!["bus breaker count".into()] },
        ];
        assert_eq!(verdicts.len(), 6);
        for verdict in &verdicts {
            assert!(verdict.attachments_complete(), "{verdict:?}");
            assert!(check_summary(verdict, &verdict.canonical_summary()).is_ok(), "{verdict:?}");
        }
    }

    #[test]
    fn held_on_tested_cannot_be_rendered_as_safety_or_robustness() {
        for claim in ["the plant is safe", "robust to failures", "安全", "鲁棒", "可生存", "viable"] {
            assert!(check_summary(&held(), claim).is_err(), "{claim}");
        }
        assert!(check_summary(&held(), "held on 12 tested scenarios; untested disturbances are not covered").is_ok());
    }

    #[test]
    fn sampled_held_cannot_claim_full_coverage() {
        assert!(check_summary(&held(), "holds for all disturbances").is_err());
        let enumerated = Verdict::HeldOnTested {
            tested_scenarios: 4,
            method: Method::Enumerated,
            uncovered: "state-related rates".into(),
        };
        assert!(check_summary(&enumerated, "holds for all fixed disturbances in W").is_ok());
    }

    #[test]
    fn timeout_and_numerical_failure_are_not_infeasibility() {
        let timeout = Verdict::Timeout { completed_scenarios: 2, budget: "60 s".into() };
        assert!(check_summary(&timeout, "infeasible within the budget").is_err());
        assert!(check_summary(&timeout, "不可行").is_err());
        let failure = Verdict::NumericalFailure { solver: "s".into(), settings: "t".into(), message: "m".into() };
        assert!(check_summary(&failure, "the plan is infeasible").is_err());
    }

    #[test]
    fn aggregation_follows_the_contract_order() {
        let counterexample = Verdict::Counterexample {
            scenario_id: "w".into(),
            first_violation_s: 1.0,
            constraint: "c".into(),
            state: "s".into(),
        };
        let timeout = Verdict::Timeout { completed_scenarios: 0, budget: "1 s".into() };
        let unknown = Verdict::UnknownInput { unknowns: vec!["x".into()] };
        assert_eq!(aggregate(&[held(), held()]).unwrap().kind, VerdictKind::HeldOnTested);
        assert_eq!(aggregate(&[held(), timeout.clone()]).unwrap().kind, VerdictKind::Timeout);
        assert_eq!(aggregate(&[timeout.clone(), unknown.clone()]).unwrap().kind, VerdictKind::UnknownInput);
        assert_eq!(aggregate(&[held(), unknown, counterexample]).unwrap().kind, VerdictKind::Counterexample);
        assert!(aggregate(&[]).is_none());
        let query = aggregate(&[held(), timeout]).unwrap();
        assert_eq!(query.counts.iter().map(|(_, count)| count).sum::<usize>(), 2);
    }

    #[test]
    fn canonical_summaries_state_their_limits() {
        assert!(held().canonical_summary().contains("not a claim of safety"));
        assert!(Verdict::Timeout { completed_scenarios: 1, budget: "60 s".into() }
            .canonical_summary()
            .contains("undecided, not infeasible"));
    }
}
