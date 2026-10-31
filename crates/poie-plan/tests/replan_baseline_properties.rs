//! Randomized replanning and baseline invariants: the kept/cancelled partition,
//! no work scheduled before `now`, added-action accounting, determinism, and
//! comparable deterministic baseline plans.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use poie_ontology::graph::Goal;
use poie_ontology::model::{AssetStatus, LotQuality, Model, Patch, Verification};
use poie_ontology::units::Quantity;
use poie_plan::baselines::{baseline, BaselineKind, BaselineRequest};
use poie_plan::plan::{plan, PlanningGoal, PlanningRequest};
use poie_plan::replan::{replan, ReplanRequest, ReplanTrigger};

const SEED: u64 = 0x706f_6965_7265_706c; // "poierepl"

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
    for asset in base.assets.keys() {
        if rng.unit() < 0.2 {
            patch.asset_status.insert(asset.clone(), AssetStatus::Failed);
        } else if rng.unit() < 0.2 {
            patch.asset_status.insert(asset.clone(), AssetStatus::Degraded);
        }
    }
    for lot in base.lots.iter().filter(|lot| lot.quality == LotQuality::Accepted) {
        if rng.unit() < 0.2 {
            let unit = base.resources[&lot.resource].unit.symbol;
            patch.lot_quantity.insert(lot.id.clone(), format!("0 {unit}"));
        }
    }
    base.patched(&patch).expect("generated patch is valid")
}

fn pool() -> Vec<Goal> {
    let mut goals =
        vec![Goal::Service("potable-water".into()), Goal::Restore("lathe-1".into()), Goal::Restore("pump-p1".into())];
    for (resource, quantity) in [("sleeve-p1", "1 item"), ("sleeve-p1", "4 item"), ("bronze-bar", "2 kg")] {
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

fn random_goals(rng: &mut SplitMix64) -> Vec<PlanningGoal> {
    let pool = pool();
    let mut chosen = BTreeSet::new();
    let mut goals = Vec::new();
    for _ in 0..=(rng.index(3)) {
        let index = rng.index(pool.len());
        if !chosen.insert(label(&pool[index])) {
            continue;
        }
        goals.push(PlanningGoal {
            goal: pool[index].clone(),
            deadline_s: if rng.unit() < 0.5 { Some((1 + rng.index(12)) as f64 * 3_600.0) } else { None },
            priority: rng.index(10) as u32,
        });
    }
    if goals.is_empty() {
        goals.push(PlanningGoal { goal: pool[0].clone(), deadline_s: None, priority: 5 });
    }
    goals
}

#[test]
fn replanning_partitions_and_never_schedules_before_now() {
    let mut rng = SplitMix64::new(SEED);
    for trial in 0..80 {
        let model = random_model(&mut rng);
        let goals = random_goals(&mut rng);
        let min_verification = match rng.index(3) {
            0 => Verification::Declared,
            1 => Verification::Modeled,
            _ => Verification::Demonstrated,
        };
        let max_actions = 1 + rng.index(12);
        let previous = plan(&PlanningRequest {
            model: &model,
            now_s: 0.0,
            horizon_s: 86_400.0,
            goals: goals.clone(),
            min_verification,
            max_actions,
        })
        .expect("initial plan");

        let now_s = rng.unit() * previous.completion_s.max(1.0);
        let result = replan(&ReplanRequest {
            previous: &previous,
            model: &model,
            now_s,
            horizon_s: now_s + 86_400.0,
            goals: goals.clone(),
            min_verification,
            max_actions,
            trigger: ReplanTrigger::Observation,
        })
        .expect("replan");
        let again = replan(&ReplanRequest {
            previous: &previous,
            model: &model,
            now_s,
            horizon_s: now_s + 86_400.0,
            goals: goals.clone(),
            min_verification,
            max_actions,
            trigger: ReplanTrigger::Observation,
        })
        .expect("replan");
        assert_eq!(result.digest(), again.digest(), "trial {trial}: replanning is not deterministic");
        assert_eq!(result.plan.digest(), again.plan.digest(), "trial {trial}: plan digest changed");

        // Every new action is scheduled at or after now.
        for action in &result.plan.actions {
            assert!(action.start_s + 1e-9 >= now_s, "trial {trial}: replan scheduled work before now");
        }

        // Kept and cancelled partition the previous plan's actions.
        let previous_ids: BTreeSet<&str> = previous.actions.iter().map(|action| action.action_id.as_str()).collect();
        let mut kept: BTreeSet<&str> = BTreeSet::new();
        let mut cancelled: BTreeSet<&str> = BTreeSet::new();
        for action in &previous.actions {
            let id = action.action_id.as_str();
            if action.end_s <= now_s + 1e-9 {
                assert!(result.kept_actions.iter().any(|kept| kept == id), "trial {trial}: finished action not kept");
                kept.insert(id);
            } else {
                assert!(
                    result.cancelled_actions.iter().any(|cancelled| cancelled == id),
                    "trial {trial}: future action not cancelled"
                );
                cancelled.insert(id);
            }
        }
        assert_eq!(kept.len() + cancelled.len(), previous_ids.len(), "trial {trial}: previous actions lost");
        assert!(kept.is_disjoint(&cancelled), "trial {trial}: an action is both kept and cancelled");
        assert_eq!(
            result.kept_actions.iter().cloned().collect::<BTreeSet<_>>(),
            kept.iter().map(|id| id.to_string()).collect(),
            "trial {trial}: kept list mismatch"
        );
        assert_eq!(
            result.cancelled_actions.iter().cloned().collect::<BTreeSet<_>>(),
            cancelled.iter().map(|id| id.to_string()).collect(),
            "trial {trial}: cancelled list mismatch"
        );

        // Added actions are the new ones that do not match a cancelled previous
        // action (same process, target and timing); a replacement with the same
        // schedule is not counted as new.
        let previous_future: Vec<_> = previous.actions.iter().filter(|action| action.end_s > now_s + 1e-9).collect();
        let same = |left: &poie_plan::plan::PlannedAction, right: &poie_plan::plan::PlannedAction| {
            left.process == right.process
                && left.target == right.target
                && (left.start_s - right.start_s).abs() < 1e-6
                && (left.end_s - right.end_s).abs() < 1e-6
        };
        let added: BTreeSet<String> = result
            .plan
            .actions
            .iter()
            .filter(|action| !previous_future.iter().any(|previous| same(previous, action)))
            .map(|action| action.action_id.clone())
            .collect();
        assert_eq!(
            added,
            result.added_actions.iter().cloned().collect::<BTreeSet<_>>(),
            "trial {trial}: added list mismatch"
        );
    }
}

#[test]
fn replan_at_the_start_is_the_same_plan_and_invalid_requests_are_rejected() {
    let mut rng = SplitMix64::new(SEED ^ 0x5151);
    for trial in 0..20 {
        let model = random_model(&mut rng);
        let goals = random_goals(&mut rng);
        let max_actions = 1 + rng.index(8);
        let previous = plan(&PlanningRequest {
            model: &model,
            now_s: 0.0,
            horizon_s: 86_400.0,
            goals: goals.clone(),
            min_verification: Verification::Modeled,
            max_actions,
        })
        .expect("initial plan");
        let replanned = replan(&ReplanRequest {
            previous: &previous,
            model: &model,
            now_s: 0.0,
            horizon_s: 86_400.0,
            goals: goals.clone(),
            min_verification: Verification::Modeled,
            max_actions,
            trigger: ReplanTrigger::Observation,
        })
        .expect("replan at start");
        assert_eq!(replanned.plan.digest(), previous.digest(), "trial {trial}: replan at the start changed the plan");
        assert!(replanned.kept_actions.is_empty(), "trial {trial}: an action finished before the start?");
        assert_eq!(
            replanned.cancelled_actions.len(),
            previous.actions.len(),
            "trial {trial}: all actions must be replaced"
        );

        assert!(
            replan(&ReplanRequest {
                previous: &previous,
                model: &model,
                now_s: 10.0,
                horizon_s: 10.0,
                goals: goals.clone(),
                min_verification: Verification::Modeled,
                max_actions,
                trigger: ReplanTrigger::Observation,
            })
            .is_err(),
            "trial {trial}: zero-length horizon accepted"
        );
        assert!(
            replan(&ReplanRequest {
                previous: &previous,
                model: &model,
                now_s: -1.0,
                horizon_s: 86_400.0,
                goals,
                min_verification: Verification::Modeled,
                max_actions,
                trigger: ReplanTrigger::Observation,
            })
            .is_err(),
            "trial {trial}: replan before the previous start accepted"
        );
    }
}

#[test]
fn baselines_are_deterministic_and_never_schedule_before_now() {
    let mut rng = SplitMix64::new(SEED ^ 0xba5e);
    for trial in 0..60 {
        let model = random_model(&mut rng);
        let goals = random_goals(&mut rng);
        let now_s = rng.index(5) as f64 * 3_600.0;
        let horizon_s = now_s + (1 + rng.index(12)) as f64 * 3_600.0;
        let max_actions = 1 + rng.index(10);
        let safety_stock = BTreeMap::from([
            ("sleeve-p1".to_string(), rng.index(3) as f64),
            ("bronze-bar".to_string(), rng.index(3) as f64),
        ]);
        for kind in [BaselineKind::TaskPriority, BaselineKind::SafetyStockRule, BaselineKind::ShortTermOptimization] {
            let request = BaselineRequest {
                model: &model,
                now_s,
                horizon_s,
                goals: goals.clone(),
                min_verification: Verification::Modeled,
                max_actions,
                safety_stock: safety_stock.clone(),
            };
            let first = baseline(kind, &request).expect("baseline");
            let second = baseline(kind, &request).expect("baseline");
            assert_eq!(first.digest(), second.digest(), "trial {trial}/{kind:?}: baseline is not deterministic");
            assert!(first.actions.len() <= max_actions, "trial {trial}/{kind:?}: budget exceeded");
            for action in &first.actions {
                assert!(action.start_s + 1e-9 >= now_s, "trial {trial}/{kind:?}: action before now");
                assert!(action.end_s + 1e-9 >= action.start_s, "trial {trial}/{kind:?}: negative duration");
            }
            for pair in first.actions.windows(2) {
                assert!(pair[0].start_s <= pair[1].start_s + 1e-9, "trial {trial}/{kind:?}: actions not ordered");
            }
        }
    }
}
