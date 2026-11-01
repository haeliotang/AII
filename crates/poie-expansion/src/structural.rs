//! Structural gain and loss (VEC §6.2.1, M11.2).
//!
//! Both lists are always reported, at three evaluation points (during
//! construction, built, commissioned), with the construction and bootstrap
//! draw deducted, at the operating threshold of the base (human policy, not
//! the proposal).
//!
//! - **gained**: `blocked` / `unknown` → `candidate` (at the commissioned point);
//! - **lost**: `candidate` → any non-candidate verdict, at any point;
//! - **undetermined**: changes from a `search_limit` base, which was never
//!   decided — never counted as a gain.

use poie_ontology::graph::{self, Goal, Query, Verdict, DEFAULT_MAX_DEPTH};
use poie_ontology::model::{Model, Verification};

use crate::proposal::{ExpansionError, ExpansionProposal, Stage};

#[derive(Clone, Debug, PartialEq)]
pub struct GoalOutcome {
    pub label: String,
    pub stage: Stage,
    pub before: Verdict,
    pub after: Verdict,
}

/// One evaluation point.
#[derive(Clone, Debug, PartialEq)]
pub struct StageGain {
    pub stage: Stage,
    pub gained: Vec<GoalOutcome>,
    pub lost: Vec<GoalOutcome>,
    pub undetermined: Vec<GoalOutcome>,
}

/// `lost` is not optional: an expansion may consume the last spare part or
/// occupy the only maintenance robot and block a critical recovery goal.
#[derive(Clone, Debug, PartialEq)]
pub struct StructuralGain {
    pub operating_threshold: Verification,
    /// Gains at the commissioned point (conditional on commissioning).
    pub gained: Vec<GoalOutcome>,
    /// Losses at every point, each tagged with its stage.
    pub lost: Vec<GoalOutcome>,
    /// `search_limit` involvement; reported separately, never a gain.
    pub undetermined: Vec<GoalOutcome>,
    pub stages: Vec<StageGain>,
    pub unchanged_candidate: usize,
    pub unchanged_blocked: usize,
}

/// How a goal moved between the base and one stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transition {
    Gained,
    Lost,
    Undetermined,
    UnchangedCandidate,
    UnchangedNonCandidate,
}

/// VEC §6.2.1 classification. `candidate → search_limit` is a loss (it can no
/// longer be shown); a `search_limit` base is undecided, so any change from it
/// is undetermined, not a gain.
pub fn classify(before: Verdict, after: Verdict) -> Transition {
    match (before, after) {
        (Verdict::Candidate, Verdict::Candidate) => Transition::UnchangedCandidate,
        (Verdict::Candidate, _) => Transition::Lost,
        (Verdict::SearchLimit, Verdict::Candidate) => Transition::Undetermined,
        (_, Verdict::SearchLimit) => Transition::Undetermined,
        (Verdict::Blocked | Verdict::Unknown, Verdict::Candidate) => Transition::Gained,
        _ => Transition::UnchangedNonCandidate,
    }
}

fn label(goal: &Goal) -> String {
    match goal {
        Goal::Service(service) => service.clone(),
        Goal::Restore(asset) => format!("restore:{asset}"),
        Goal::Obtain { resource, .. } => format!("obtain:{resource}"),
    }
}

/// Compares every goal in `goals` on the base and on the three stages of the
/// expanded model, at the base's operating threshold.
///
/// Errors: the commissioning level is below the operating threshold, or the
/// local stock cannot pay for the construction draw (`NotBuildable`).
pub fn structural_gain(
    base: &Model,
    proposal: &ExpansionProposal,
    goals: &[Goal],
    operating_threshold: Verification,
) -> Result<StructuralGain, ExpansionError> {
    proposal.check_threshold(operating_threshold)?;
    let staged: Vec<(Stage, Model)> = Stage::ALL
        .iter()
        .map(|stage| proposal.stage_model(base, *stage).map(|model| (*stage, model)))
        .collect::<Result<_, _>>()?;
    let mut gain = StructuralGain {
        operating_threshold,
        gained: Vec::new(),
        lost: Vec::new(),
        undetermined: Vec::new(),
        stages: Vec::new(),
        unchanged_candidate: 0,
        unchanged_blocked: 0,
    };
    let query =
        |goal: &Goal| Query { goal: goal.clone(), min_verification: operating_threshold, max_depth: DEFAULT_MAX_DEPTH };
    let before: Vec<Verdict> = goals.iter().map(|goal| graph::solve(base, &query(goal)).verdict).collect();
    for (stage, model) in &staged {
        let mut stage_gain =
            StageGain { stage: *stage, gained: Vec::new(), lost: Vec::new(), undetermined: Vec::new() };
        for (goal, before) in goals.iter().zip(&before) {
            let after = graph::solve(model, &query(goal)).verdict;
            let outcome = GoalOutcome { label: label(goal), stage: *stage, before: *before, after };
            match classify(*before, after) {
                Transition::Gained => stage_gain.gained.push(outcome),
                Transition::Lost => stage_gain.lost.push(outcome),
                Transition::Undetermined => stage_gain.undetermined.push(outcome),
                Transition::UnchangedCandidate | Transition::UnchangedNonCandidate => {}
            }
        }
        gain.lost.extend(stage_gain.lost.iter().cloned());
        gain.undetermined.extend(stage_gain.undetermined.iter().cloned());
        if *stage == Stage::Commissioned {
            gain.gained = stage_gain.gained.clone();
            for (goal, before) in goals.iter().zip(&before) {
                let after = graph::solve(model, &query(goal)).verdict;
                match classify(*before, after) {
                    Transition::UnchangedCandidate => gain.unchanged_candidate += 1,
                    Transition::UnchangedNonCandidate => gain.unchanged_blocked += 1,
                    _ => {}
                }
            }
        }
        gain.stages.push(stage_gain);
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

    // Revised for issue #109: the gain is judged at the operating threshold
    // (demonstrated) and only exists once commissioning reaches that level;
    // while the furnace is merely built (modeled) it adds nothing.
    #[test]
    fn the_recycling_furnace_gains_only_after_commissioning() {
        let base = base01();
        let proposal = proposal();
        let goals = vec![obtain("sleeve-p1", "4 item")];
        let gain = structural_gain(&base, &proposal, &goals, Verification::Demonstrated).unwrap();
        assert_eq!(gain.gained.len(), 1, "{gain:?}");
        assert_eq!(gain.gained[0].label, "obtain:sleeve-p1");
        assert_eq!(gain.gained[0].stage, Stage::Commissioned);
        assert_eq!(gain.gained[0].before, Verdict::Blocked);
        assert_eq!(gain.gained[0].after, Verdict::Candidate);
        let built = gain.stages.iter().find(|stage| stage.stage == Stage::Built).unwrap();
        assert!(built.gained.is_empty(), "a modeled furnace cannot count at a demonstrated threshold: {gain:?}");
        assert!(gain.lost.is_empty(), "{gain:?}");
    }

    // Revised for issue #109: construction occupies robot-m1, the only robot
    // that can replace the pump sleeve, so potable water loses its recovery
    // route while the furnace is being built. Before the fix this was invisible.
    #[test]
    fn occupying_the_only_robot_loses_potable_water_during_construction() {
        let base = base01();
        let proposal = proposal();
        let goals = vec![Goal::Service("potable-water".into())];
        let gain = structural_gain(&base, &proposal, &goals, Verification::Demonstrated).unwrap();
        assert!(gain.gained.is_empty());
        assert_eq!(gain.lost.len(), 1, "{gain:?}");
        assert_eq!(gain.lost[0].label, "potable-water");
        assert_eq!(gain.lost[0].stage, Stage::DuringConstruction);
        assert_eq!(gain.unchanged_candidate, 1, "potable water is a candidate again once construction ends");
    }

    #[test]
    fn vec6_2_classification_counts_every_candidate_loss_and_no_undecided_gain() {
        use Verdict::*;
        assert_eq!(classify(Candidate, Blocked), Transition::Lost);
        assert_eq!(classify(Candidate, Unknown), Transition::Lost);
        assert_eq!(classify(Candidate, SearchLimit), Transition::Lost);
        assert_eq!(classify(Blocked, Candidate), Transition::Gained);
        assert_eq!(classify(Unknown, Candidate), Transition::Gained);
        assert_eq!(classify(SearchLimit, Candidate), Transition::Undetermined);
        assert_eq!(classify(Blocked, SearchLimit), Transition::Undetermined);
        assert_eq!(classify(Blocked, Unknown), Transition::UnchangedNonCandidate);
        assert_eq!(classify(Candidate, Candidate), Transition::UnchangedCandidate);
    }

    #[test]
    fn a_draw_larger_than_the_stock_is_not_buildable() {
        let mut patch = poie_ontology::model::Patch::default();
        patch.lot_quantity.insert("lot-energy".into(), "2 kWh".into());
        let base = base01().patched(&patch).unwrap();
        let error = structural_gain(&base, &proposal(), &[], Verification::Demonstrated).unwrap_err();
        assert!(matches!(error, ExpansionError::NotBuildable(_)), "{error}");
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
