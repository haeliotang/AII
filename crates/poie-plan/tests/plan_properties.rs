//! Randomized planner invariants: action ordering, non-overlap, uniqueness,
//! determinism, budget and SAP-validation.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use poie_ontology::graph::Goal;
use poie_ontology::model::{AssetStatus, Model, Patch, Verification};
use poie_ontology::units::Quantity;
use poie_plan::plan::{plan, PlanningGoal, PlanningRequest};
use poie_plan::sap::validate_plan_through_sap;

const SEED: u64 = 0x706f_6965_706c_616e; // "poieplan"

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

    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn index(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }
}

fn base01() -> Model {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01/model.toml");
    Model::load(&path).expect("Base-01 loads")
}

fn random_instance(rng: &mut SplitMix64) -> Model {
    let base = base01();
    let mut patch = Patch::default();
    for asset_id in base.assets.keys() {
        let roll = rng.unit();
        if roll < 0.15 {
            patch.asset_status.insert(asset_id.clone(), AssetStatus::Failed);
        } else if roll < 0.25 {
            patch.asset_status.insert(asset_id.clone(), AssetStatus::Degraded);
        }
    }
    for lot in base.lots.iter().filter(|lot| lot.quality == poie_ontology::model::LotQuality::Accepted) {
        if rng.unit() < 0.15 {
            let unit = base.resources[&lot.resource].unit.symbol;
            patch.lot_quantity.insert(lot.id.clone(), format!("0 {unit}"));
        }
    }
    base.patched(&patch).expect("random patch is valid")
}

fn goal_pool() -> Vec<Goal> {
    let mut goals = vec![
        Goal::Service("potable-water".into()),
        Goal::Restore("lathe-1".into()),
        Goal::Restore("pump-p1".into()),
        Goal::Restore("bus-1".into()),
        Goal::Restore("robot-m1".into()),
        Goal::Restore("charger-1".into()),
    ];
    for (resource, quantity) in [
        ("sleeve-p1", "1 item"),
        ("sleeve-p1", "2 item"),
        ("sleeve-p1", "4 item"),
        ("bronze-bar", "1 kg"),
        ("electric-energy", "5 kWh"),
        ("lathe-nut", "1 item"),
        ("lathe-nut", "2 item"),
    ] {
        goals.push(Goal::Obtain { resource: resource.into(), quantity: quantity.parse::<Quantity>().unwrap() });
    }
    goals
}

fn label(goal: &Goal) -> String {
    match goal {
        Goal::Service(service) => service.clone(),
        Goal::Restore(asset) => format!("restore:{asset}"),
        Goal::Obtain { resource, .. } => format!("obtain:{resource}"),
    }
}

fn verification(rng: &mut SplitMix64) -> Verification {
    match rng.index(4) {
        0 => Verification::Declared,
        1 => Verification::Modeled,
        2 => Verification::Demonstrated,
        _ => Verification::Verified,
    }
}

#[test]
fn randomized_plans_are_ordered_unique_deterministic_and_sap_valid() {
    let pool = goal_pool();
    let mut rng = SplitMix64::new(SEED);
    for trial in 0..80 {
        let model = random_instance(&mut rng);
        let now_s = rng.index(5) as f64 * 3_600.0;
        let horizon_s = now_s + (1 + rng.index(12)) as f64 * 3_600.0;
        let min_verification = verification(&mut rng);
        let max_actions = 1 + rng.index(20);
        let mut chosen = BTreeSet::new();
        let mut goals = Vec::new();
        for _ in 0..=rng.index(3) {
            let index = rng.index(pool.len());
            // Keep goal labels unique: the planner reports goals by label.
            if !chosen.insert(label(&pool[index])) {
                continue;
            }
            let deadline = if rng.unit() < 0.5 { Some(now_s + (1 + rng.index(12)) as f64 * 3_600.0) } else { None };
            goals.push(PlanningGoal {
                goal: pool[index].clone(),
                deadline_s: deadline,
                priority: rng.index(10) as u32,
            });
        }
        if goals.is_empty() {
            goals.push(PlanningGoal { goal: pool[0].clone(), deadline_s: None, priority: 5 });
        }
        let request =
            PlanningRequest { model: &model, now_s, horizon_s, goals: goals.clone(), min_verification, max_actions };
        let first_plan = plan(&request).expect("planner is total for a valid request");
        let second_plan = plan(&request).expect("planner is total for a valid request");
        assert_eq!(first_plan.digest(), second_plan.digest(), "trial {trial}: planner is not deterministic");

        assert!(first_plan.completion_s >= now_s - 1e-6, "trial {trial}: completion before now");
        assert!(first_plan.actions.len() <= max_actions, "trial {trial}: action budget exceeded");
        assert!(
            first_plan.actions.iter().all(|action| action.start_s + 1e-6 >= now_s),
            "trial {trial}: action before now"
        );
        assert!(
            first_plan.actions.iter().all(|action| action.end_s + 1e-6 >= action.start_s),
            "trial {trial}: negative duration"
        );
        for pair in first_plan.actions.windows(2) {
            assert!(pair[0].start_s <= pair[1].start_s + 1e-6, "trial {trial}: actions are not ordered");
        }
        let ids: BTreeSet<&str> = first_plan.actions.iter().map(|action| action.action_id.as_str()).collect();
        assert_eq!(ids.len(), first_plan.actions.len(), "trial {trial}: duplicate action id");
        for (index, first) in first_plan.actions.iter().enumerate() {
            for second in first_plan.actions.iter().skip(index + 1) {
                if first.start_s < second.end_s && second.start_s < first.end_s {
                    for asset in &first.occupies {
                        assert!(!second.occupies.contains(asset), "trial {trial}: overlapping occupations of {asset}");
                    }
                }
            }
        }

        let expected: BTreeSet<String> = goals.iter().map(|goal| label(&goal.goal)).collect();
        let reported: BTreeSet<String> = first_plan
            .goals_met
            .iter()
            .chain(first_plan.goals_unmet.iter().map(|failure| &failure.goal))
            .cloned()
            .collect();
        if reported != expected {
            eprintln!(
                "trial {trial}: goals={:?} met={:?} unmet={:?}",
                goals, first_plan.goals_met, first_plan.goals_unmet
            );
        }
        assert_eq!(reported, expected, "trial {trial}: a goal was silently dropped");
        assert_eq!(
            first_plan.goals_met.len() + first_plan.goals_unmet.len(),
            expected.len(),
            "trial {trial}: a goal was reported twice (met={:?}, unmet={:?})",
            first_plan.goals_met,
            first_plan.goals_unmet
        );

        // Every produced plan must pass the SAP lifecycle and dispatch checks.
        let state_version = BTreeMap::from([("site-a".to_string(), 100u64)]);
        validate_plan_through_sap(&first_plan, "site-a", &state_version)
            .unwrap_or_else(|error| panic!("trial {trial}: plan did not pass SAP validation: {error}"));
        if first_plan.goals_unmet.is_empty() {
            assert!(first_plan.completion_s <= horizon_s + 1e-6, "trial {trial}: met plan exceeds the horizon");
        }
    }
}
