//! Randomized graph-solver invariants: verdict classification matches the
//! reported blockers, candidates are independently checked (step indices,
//! provider/route verification), and search monotonicity holds for weaker
//! verification levels and larger depth limits.

use std::path::PathBuf;

use poie_ontology::graph::{self, BlockerKind, Goal, Query, Source, Verdict, DEFAULT_MAX_DEPTH};
use poie_ontology::model::{AssetStatus, LotQuality, Model, Patch, Verification};
use poie_ontology::units::Quantity;

const SEED: u64 = 0x706f_6965_6772_6170; // "poiegraph"

#[derive(Clone, Debug)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> SplitMix64 {
        SplitMix64 { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn index(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }

    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn base01() -> Model {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01/model.toml");
    Model::load(&path).expect("Base-01 loads")
}

fn random_model(rng: &mut SplitMix64) -> Model {
    let base = base01();
    let mut patch = Patch::default();
    for asset_id in base.assets.keys() {
        let roll = rng.unit();
        if roll < 0.2 {
            patch.asset_status.insert(asset_id.clone(), AssetStatus::Failed);
        } else if roll < 0.35 {
            patch.asset_status.insert(asset_id.clone(), AssetStatus::Degraded);
        } else if roll < 0.4 {
            patch.asset_status.insert(asset_id.clone(), AssetStatus::Unknown);
        }
    }
    for lot in base.lots.iter().filter(|lot| lot.quality == LotQuality::Accepted) {
        if rng.unit() < 0.25 {
            let unit = base.resources[&lot.resource].unit.symbol;
            patch.lot_quantity.insert(lot.id.clone(), format!("0 {unit}"));
        }
    }
    base.patched(&patch).expect("generated patch is valid")
}

fn goal_pool(model: &Model) -> Vec<Goal> {
    let mut goals = Vec::new();
    for service in model.services.keys() {
        goals.push(Goal::Service(service.clone()));
    }
    for asset in model.assets.keys() {
        goals.push(Goal::Restore(asset.clone()));
    }
    for resource in ["sleeve-p1", "bronze-bar", "electric-energy", "lathe-nut"] {
        for quantity in ["1 item", "2 item", "4 item", "2 kg", "20 kWh"] {
            if let Ok(quantity) = quantity.parse::<Quantity>() {
                goals.push(Goal::Obtain { resource: resource.to_string(), quantity });
            }
        }
    }
    goals
}

fn levels() -> [Verification; 4] {
    [Verification::Declared, Verification::Modeled, Verification::Demonstrated, Verification::Verified]
}

fn check_steps(trial: usize, result: &graph::QueryResult, min: Verification) {
    for (index, step) in result.steps.iter().enumerate() {
        assert!(!step.process.trim().is_empty(), "trial {trial}: empty process");
        assert!(step.runs >= 1, "trial {trial}: zero runs");
        for allocation in &step.allocations {
            assert!(allocation.si > 0.0, "trial {trial}: non-positive allocation");
            if let Source::Step(source) = allocation.source {
                assert!(source < index, "trial {trial}: step allocation refers forward to step {source}");
            }
        }
        for (_, _, provider_level) in &step.providers {
            assert!(*provider_level >= min, "trial {trial}: provider level below the query minimum");
        }
    }
    if let Some(weakest) = result.route_verification() {
        assert!(weakest >= min, "trial {trial}: route verification below the query minimum");
    }
}

fn check_classification(trial: usize, result: &graph::QueryResult) {
    let depth_blocked = result.blockers.iter().any(|blocker| blocker.kind == BlockerKind::DepthExceeded);
    let unknown = result.blockers.iter().any(|blocker| blocker.kind == BlockerKind::UnknownState);
    match result.verdict {
        Verdict::Candidate => {
            assert!(result.blockers.is_empty(), "trial {trial}: candidate has blockers");
        }
        Verdict::Blocked => {
            assert!(!result.blockers.is_empty(), "trial {trial}: blocked without blockers");
            let roots: Vec<_> = result.root_causes().collect();
            assert!(!roots.is_empty(), "trial {trial}: blocked without root causes");
            for root in roots {
                assert!(!root.subject.trim().is_empty(), "trial {trial}: root cause without subject");
                assert!(!root.detail.trim().is_empty(), "trial {trial}: root cause without detail");
            }
            assert!(!depth_blocked, "trial {trial}: blocked verdict with a depth-exceeded blocker");
        }
        Verdict::Unknown => {
            assert!(unknown, "trial {trial}: unknown verdict without an unknown-state blocker");
            assert!(!depth_blocked, "trial {trial}: unknown verdict with a depth-exceeded blocker");
        }
        Verdict::SearchLimit => {
            assert!(depth_blocked, "trial {trial}: search-limit verdict without a depth-exceeded blocker");
        }
    }
}

#[test]
fn randomized_queries_are_classified_and_candidates_are_consistent() {
    let mut rng = SplitMix64::new(SEED);
    for trial in 0..120 {
        let model = random_model(&mut rng);
        let pool = goal_pool(&model);
        let goal = pool[rng.index(pool.len())].clone();
        let min = levels()[rng.index(4)];
        let max_depth = 1 + rng.index(8);
        let query = Query { goal, min_verification: min, max_depth };
        let result = graph::solve(&model, &query);
        let again = graph::solve(&model, &query);

        // Determinism.
        assert_eq!(result.verdict, again.verdict, "trial {trial}: verdict not deterministic");
        assert_eq!(result.steps, again.steps, "trial {trial}: steps not deterministic");
        assert_eq!(result.blockers, again.blockers, "trial {trial}: blockers not deterministic");

        check_classification(trial, &result);
        if result.verdict == Verdict::Candidate {
            check_steps(trial, &result, min);
        }

        // Weaker verification requirements cannot remove a candidate.
        if result.verdict == Verdict::Candidate {
            let weaker = levels()[rng.index(levels().iter().position(|level| *level == min).unwrap_or(0) + 1)];
            let weaker_result =
                graph::solve(&model, &Query { goal: query.goal.clone(), min_verification: weaker, max_depth });
            assert_eq!(
                weaker_result.verdict,
                Verdict::Candidate,
                "trial {trial}: weakening {min:?} to {weaker:?} lost the candidate"
            );
        }

        // A strictly harder verification requirement cannot create a candidate.
        if result.verdict == Verdict::Blocked && min < Verification::Verified {
            let harder = levels()[levels().iter().position(|level| *level == min).unwrap() + 1];
            let harder_result =
                graph::solve(&model, &Query { goal: query.goal.clone(), min_verification: harder, max_depth });
            assert_eq!(
                harder_result.verdict,
                Verdict::Blocked,
                "trial {trial}: stricter {harder:?} created a candidate"
            );
        }

        // A larger depth limit cannot lose a candidate.
        if result.verdict == Verdict::Candidate {
            let deeper_result = graph::solve(
                &model,
                &Query { goal: query.goal.clone(), min_verification: min, max_depth: max_depth + 3 },
            );
            assert_eq!(deeper_result.verdict, Verdict::Candidate, "trial {trial}: larger depth lost the candidate");
        }
    }
}

#[test]
fn candidate_steps_remain_valid_under_the_default_depth() {
    // The default query depth must not change candidate semantics on Base-01.
    let model = base01();
    let goals = goal_pool(&model);
    for goal in goals {
        let query = Query { goal: goal.clone(), min_verification: Verification::Modeled, max_depth: DEFAULT_MAX_DEPTH };
        let result = graph::solve(&model, &query);
        if result.verdict == Verdict::Candidate {
            check_steps(0, &result, Verification::Modeled);
        }
    }
}
