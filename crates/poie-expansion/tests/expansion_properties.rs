//! Expansion accounting invariants: structural gains/losses are a partition of
//! the queried goals, temporal deltas follow the summary definition, and the
//! construction check is exactly the windowed minimum-level violations.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use poie_expansion::proposal::ExpansionProposal;
use poie_expansion::structural::structural_gain;
use poie_expansion::temporal::temporal_gain;
use poie_ontology::graph::{self, Goal, Query, Verdict, DEFAULT_MAX_DEPTH};
use poie_ontology::model::{Model, Verification};
use poie_ontology::time::TimeWindow;
use poie_ontology::units::Quantity;
use poie_viability::metrics::{RunTrace, TimeSample};

const SEED: u64 = 0x706f_6965_6578_7031; // "poieexp1"
const SERVICES: [&str; 2] = ["potable-water", "heat-treatment"];

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

fn proposal() -> ExpansionProposal {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01-expansion/proposal.toml");
    ExpansionProposal::load(&path).expect("proposal loads")
}

fn goal_pool() -> Vec<Goal> {
    vec![
        Goal::Service("potable-water".into()),
        Goal::Restore("pump-p1".into()),
        Goal::Restore("lathe-1".into()),
        Goal::Obtain { resource: "sleeve-p1".into(), quantity: "4 item".parse::<Quantity>().unwrap() },
        Goal::Obtain { resource: "sleeve-p1".into(), quantity: "1 item".parse::<Quantity>().unwrap() },
        Goal::Obtain { resource: "bronze-bar".into(), quantity: "2 kg".parse::<Quantity>().unwrap() },
    ]
}

fn label(goal: &Goal) -> String {
    match goal {
        Goal::Service(service) => service.clone(),
        Goal::Restore(asset) => format!("restore:{asset}"),
        Goal::Obtain { resource, .. } => format!("obtain:{resource}"),
    }
}

#[test]
fn structural_gains_and_losses_partition_the_queried_goals() {
    let base = base01();
    let proposal = proposal();
    let expanded = proposal.expand(&base).expect("proposal expands");
    let patched = base.patched(&proposal.to_patch()).expect("patch applies");
    assert_eq!(
        format!("{:?}", expanded.assets),
        format!("{:?}", patched.assets),
        "expand and to_patch disagree on assets"
    );
    assert_eq!(
        format!("{:?}", expanded.resources),
        format!("{:?}", patched.resources),
        "expand and to_patch disagree on resources"
    );
    assert_eq!(
        format!("{:?}", expanded.processes),
        format!("{:?}", patched.processes),
        "expand and to_patch disagree on processes"
    );
    assert_eq!(format!("{:?}", expanded.lots), format!("{:?}", patched.lots), "expand and to_patch disagree on lots");

    let pool = goal_pool();
    let mut rng = SplitMix64::new(SEED);
    for trial in 0..40 {
        let mut goals = Vec::new();
        for goal in &pool {
            if rng.unit() < 0.6 {
                goals.push(goal.clone());
            }
        }
        let min_verification = match rng.index(3) {
            0 => Verification::Declared,
            1 => Verification::Modeled,
            _ => Verification::Demonstrated,
        };
        let gain = structural_gain(&base, &proposal, &goals, min_verification).expect("gain computes");

        // Independent verdicts on base and expanded model.
        let mut expected_gained = 0;
        let mut expected_lost = 0;
        let mut expected_unchanged_candidate = 0;
        let mut expected_unchanged_blocked = 0;
        for goal in &goals {
            let query = Query { goal: goal.clone(), min_verification, max_depth: DEFAULT_MAX_DEPTH };
            let before = graph::solve(&base, &query).verdict;
            let after = graph::solve(&expanded, &query).verdict;
            match (before, after) {
                (Verdict::Candidate, Verdict::Candidate) => expected_unchanged_candidate += 1,
                (Verdict::Candidate, _) => expected_lost += 1,
                (_, Verdict::Candidate) => expected_gained += 1,
                _ => expected_unchanged_blocked += 1,
            }
        }
        assert_eq!(gain.gained.len(), expected_gained, "trial {trial}: gained count");
        assert_eq!(gain.lost.len(), expected_lost, "trial {trial}: lost count");
        assert_eq!(gain.unchanged_candidate, expected_unchanged_candidate, "trial {trial}: unchanged candidate count");
        assert_eq!(gain.unchanged_blocked, expected_unchanged_blocked, "trial {trial}: unchanged blocked count");
        assert_eq!(
            gain.gained.len() + gain.lost.len() + gain.unchanged_candidate + gain.unchanged_blocked,
            goals.len(),
            "trial {trial}: outcomes do not partition the goals"
        );

        // Every reported outcome carries the independently computed verdicts.
        for outcome in gain.gained.iter().chain(gain.lost.iter()) {
            let goal = goals.iter().find(|goal| label(goal) == outcome.label).expect("outcome has a goal");
            let query = Query { goal: goal.clone(), min_verification, max_depth: DEFAULT_MAX_DEPTH };
            assert_eq!(outcome.before, graph::solve(&base, &query).verdict, "trial {trial}: before verdict");
            assert_eq!(outcome.after, graph::solve(&expanded, &query).verdict, "trial {trial}: after verdict");
        }
        let labels: BTreeSet<String> =
            gain.gained.iter().chain(gain.lost.iter()).map(|outcome| outcome.label.clone()).collect();
        assert_eq!(labels.len(), gain.gained.len() + gain.lost.len(), "trial {trial}: duplicate outcome labels");
    }
}

fn trace(id: &str, rng: &mut SplitMix64, services: &[String], samples: usize) -> RunTrace {
    let horizon = 5_000.0;
    let mut levels = Vec::new();
    for index in 0..samples {
        let at = (index as f64 + 1.0) * (horizon / (samples as f64 + 1.0));
        let service_levels: BTreeMap<String, f64> =
            services.iter().map(|service| (service.clone(), rng.unit())).collect();
        levels.push(TimeSample {
            at_s: at,
            service_levels,
            violations: Vec::new(),
            external_inputs: BTreeMap::new(),
            stock: BTreeMap::new(),
            produced: BTreeMap::new(),
            recycle_inputs_kg: BTreeMap::new(),
            recycle_reusable_kg: BTreeMap::new(),
            health: BTreeMap::new(),
            affected_services: services.to_vec(),
            affected_assets: Vec::new(),
        });
    }
    RunTrace {
        scenario_id: id.to_string(),
        horizon_s: horizon,
        minimum_levels: BTreeMap::new(),
        samples: levels,
        recovery_paths: BTreeMap::new(),
    }
}

#[test]
fn temporal_deltas_and_construction_check_follow_the_summary_definition() {
    let mut rng = SplitMix64::new(SEED ^ 0x5157);
    for trial in 0..40 {
        // Base and expanded traces may cover different service subsets.
        let base_services: Vec<String> =
            SERVICES.iter().filter(|_| rng.unit() < 0.7).map(|service| service.to_string()).collect();
        let expanded_services: Vec<String> = SERVICES
            .iter()
            .filter(|service| !base_services.contains(&service.to_string()) || rng.unit() < 0.8)
            .map(|service| service.to_string())
            .collect();
        if expanded_services.is_empty() {
            continue;
        }
        let count = 1 + rng.index(4);
        let base_samples = 1 + rng.index(4);
        let base_traces: Vec<RunTrace> =
            (0..count).map(|index| trace(&format!("base-{index}"), &mut rng, &base_services, base_samples)).collect();
        let expanded_samples = 1 + rng.index(4);
        let expanded_traces: Vec<RunTrace> = (0..count)
            .map(|index| trace(&format!("exp-{index}"), &mut rng, &expanded_services, expanded_samples))
            .collect();
        let minimum_levels: BTreeMap<String, f64> =
            expanded_services.iter().map(|service| (service.clone(), 0.5 + rng.unit() * 0.5)).collect();
        let window = TimeWindow::new(rng.unit() * 2_000.0, 2_000.0 + rng.unit() * 3_000.0).expect("window");

        let gain = temporal_gain(&base_traces, &expanded_traces, &minimum_levels, window);

        // Deltas: expanded minus base minimum maintained time, None unless both exist.
        for (service, delta) in &gain.min_maintained_delta_s {
            let expanded = gain.expanded.iter().find(|summary| summary.service == *service).unwrap();
            let base = gain.base.iter().find(|summary| summary.service == *service);
            let expected = match (expanded.min_maintained_s, base.and_then(|summary| summary.min_maintained_s)) {
                (Some(after), Some(before)) => Some(after - before),
                _ => None,
            };
            assert_eq!(*delta, expected, "trial {trial}: delta for {service}");
        }
        assert_eq!(gain.min_maintained_delta_s.len(), gain.expanded.len(), "trial {trial}: a summary has no delta");

        // Construction check: exactly the samples inside the window below minimum.
        let mut expected_violations = 0;
        for trace in &expanded_traces {
            for sample in &trace.samples {
                if !window.contains(sample.at_s) {
                    continue;
                }
                for (service, minimum) in &minimum_levels {
                    let level = sample.service_levels.get(service).copied().unwrap_or(1.0);
                    if level + 1e-9 < *minimum {
                        expected_violations += 1;
                    }
                }
            }
        }
        assert_eq!(gain.construction.violations.len(), expected_violations, "trial {trial}: construction violations");
        assert_eq!(gain.construction.within_window, expected_violations == 0, "trial {trial}: construction verdict");
    }
}
