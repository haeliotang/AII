//! Structural gain and loss (VEC §6.2.1, M11.2).

use poie_ontology::graph::{self, Goal, Query, Verdict, DEFAULT_MAX_DEPTH};
use poie_ontology::model::{Model, Verification};

use crate::proposal::{ExpansionError, ExpansionProposal};

#[derive(Clone, Debug, PartialEq)]
pub struct GoalOutcome {
    pub label: String,
    pub before: Verdict,
    pub after: Verdict,
}

/// Both lists are always reported. `lost` is not optional: an expansion may
/// consume the last spare part or the only free asset and block a critical
/// recovery goal.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StructuralGain {
    pub gained: Vec<GoalOutcome>,
    pub lost: Vec<GoalOutcome>,
    pub unchanged_candidate: usize,
    pub unchanged_blocked: usize,
}

fn label(goal: &Goal) -> String {
    match goal {
        Goal::Service(service) => service.clone(),
        Goal::Restore(asset) => format!("restore:{asset}"),
        Goal::Obtain { resource, .. } => format!("obtain:{resource}"),
    }
}

/// Compares every goal in `goals` on the base and the expanded model.
pub fn structural_gain(
    base: &Model,
    proposal: &ExpansionProposal,
    goals: &[Goal],
    min_verification: Verification,
) -> Result<StructuralGain, ExpansionError> {
    let expanded = proposal.expand(base)?;
    let mut gain = StructuralGain::default();
    for goal in goals {
        let query = Query { goal: goal.clone(), min_verification, max_depth: DEFAULT_MAX_DEPTH };
        let before = graph::solve(base, &query).verdict;
        let after = graph::solve(&expanded, &query).verdict;
        let outcome = GoalOutcome { label: label(goal), before, after };
        match (before, after) {
            (Verdict::Candidate, Verdict::Candidate) => gain.unchanged_candidate += 1,
            (Verdict::Candidate, _) => gain.lost.push(outcome),
            (_, Verdict::Candidate) => gain.gained.push(outcome),
            _ => gain.unchanged_blocked += 1,
        }
    }
    Ok(gain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use poie_ontology::units::Quantity;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn base01() -> Model {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01/model.toml");
        Model::load(&path).unwrap()
    }

    fn proposal() -> ExpansionProposal {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01-expansion/proposal.toml");
        ExpansionProposal::load(&path).unwrap()
    }

    fn obtain(resource: &str, quantity: &str) -> Goal {
        Goal::Obtain { resource: resource.to_string(), quantity: quantity.parse::<Quantity>().unwrap() }
    }

    #[test]
    fn the_recycling_furnace_gains_a_previously_blocked_goal() {
        let base = base01();
        let proposal = proposal();
        let goals = vec![obtain("sleeve-p1", "4 item"), Goal::Service("potable-water".into())];
        let gain = structural_gain(&base, &proposal, &goals, Verification::Modeled).unwrap();
        assert_eq!(gain.gained.len(), 1, "{gain:?}");
        assert_eq!(gain.gained[0].label, "obtain:sleeve-p1");
        assert_eq!(gain.gained[0].before, Verdict::Blocked);
        assert_eq!(gain.gained[0].after, Verdict::Candidate);
        assert!(gain.lost.is_empty(), "{gain:?}");
    }

    #[test]
    fn an_expansion_without_effect_reports_no_gain() {
        let base = base01();
        let proposal = proposal();
        let goals = vec![Goal::Service("potable-water".into())];
        let gain = structural_gain(&base, &proposal, &goals, Verification::Demonstrated).unwrap();
        assert!(gain.gained.is_empty());
        assert!(gain.lost.is_empty());
        assert_eq!(gain.unchanged_candidate, 1);
    }

    #[test]
    fn labels_stay_stable() {
        let goals = BTreeMap::from([
            ("service", Goal::Service("potable-water".into())),
            ("restore", Goal::Restore("pump-p1".into())),
            ("obtain", obtain("sleeve-p1", "1 item")),
        ]);
        assert_eq!(label(&goals["service"]), "potable-water");
        assert_eq!(label(&goals["restore"]), "restore:pump-p1");
        assert_eq!(label(&goals["obtain"]), "obtain:sleeve-p1");
    }
}
