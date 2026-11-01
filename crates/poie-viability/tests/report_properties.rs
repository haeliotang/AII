//! Randomized VEC report validation: valid reports round-trip, and every
//! single-field anti-cheat mutation is rejected with the expected reason.

use poie_viability::audit::DecisionRecord;
use poie_viability::method::Method;
use poie_viability::report::{
    ConfidenceInterval, InputHashes, MachineReport, MetricGroup, MetricRecord, ModelProvenance, RunRecord,
    ScenarioPartition, METRIC_DEFINITION_VERSION, REPORT_SCHEMA,
};
use poie_viability::verdict::Verdict;

const SEED: u64 = 0x706f_6965_7265_7031; // "poierep1"

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
}

fn verdict(method: Method) -> Verdict {
    Verdict::HeldOnTested { tested_scenarios: 1, method, uncovered: "declared unknowns listed".to_string() }
}

fn valid_report(rng: &mut SplitMix64) -> MachineReport {
    let run_count = 1 + rng.index(5);
    let method = if rng.index(2) == 0 { Method::Sampled } else { Method::Enumerated };
    let runs: Vec<RunRecord> = (0..run_count)
        .map(|index| RunRecord {
            scenario_id: format!("s{index}"),
            method,
            seed: if method == Method::Sampled { Some(index as u64) } else { None },
            budget_used_s: 1.0 + index as f64,
            verdict: verdict(method),
        })
        .collect();
    let mut metrics = vec![
        MetricRecord {
            group: MetricGroup::Terminal,
            name: "battery_kwh".into(),
            service: None,
            value: 1.5,
            unit: "kWh".into(),
            censored: false,
        },
        MetricRecord {
            group: MetricGroup::Viability,
            name: "maintained".into(),
            service: Some("water".into()),
            value: 100.0,
            unit: "s".into(),
            censored: false,
        },
        MetricRecord {
            group: MetricGroup::Production,
            name: "produced".into(),
            service: None,
            value: 5.0,
            unit: "kg".into(),
            censored: false,
        },
    ];
    if rng.index(2) == 0 {
        metrics.push(MetricRecord {
            group: MetricGroup::Sustainability,
            name: "net_consumption".into(),
            service: None,
            value: 3.0,
            unit: "kg".into(),
            censored: rng.index(2) == 0,
        });
    }
    let intervals =
        vec![ConfidenceInterval { metric: "maintained".into(), low: 10.0, high: 20.0, method: "bootstrap".into() }];
    MachineReport {
        schema: REPORT_SCHEMA.to_string(),
        contract_version: "viability-expansion/0.2-draft".into(),
        input_hashes: InputHashes {
            initial_state: "hash-state".into(),
            scenario_set: "hash-scenarios".into(),
            models: "hash-models".into(),
            policy: "hash-policy".into(),
            contract_version: "viability-expansion/0.2-draft".into(),
        },
        partition: ScenarioPartition { development: run_count, holdout: 0, holdout_viewed_in_development: false },
        method,
        horizon_s: 10_000.0,
        seeds: if method == Method::Sampled { vec![1, 2, 3] } else { Vec::new() },
        solver_version: "poie-viability 0.1.0".into(),
        solver_settings: "default".into(),
        actual_cost_s: 1.5,
        metric_definition_version: METRIC_DEFINITION_VERSION.into(),
        registered_in: "PREREG.md (scenario-set hash)".into(),
        seed_sets_drawn: usize::from(method == Method::Sampled),
        terminal_condition: "battery >= 1 kWh at H".into(),
        confidence_intervals: intervals,
        model_provenance: vec![ModelProvenance {
            id: "water-treatment".into(),
            version: "0.1.0".into(),
            synthetic: true,
            validity: "flow 5-15 m3/h".into(),
        }],
        decisions: runs
            .iter()
            .map(|run| DecisionRecord {
                scenario_id: run.scenario_id.clone(),
                decided_at_s: 0.0,
                observed_up_to_s: 0.0,
                policy_instance: "policy@v1".into(),
            })
            .collect(),
        runs,
        metrics,
    }
}

type Mutation = fn(&mut MachineReport);

fn mutations() -> Vec<(&'static str, Mutation)> {
    vec![
        ("schema", |report| report.schema = "poie.other/0.1".into()),
        ("empty contract", |report| report.contract_version.clear()),
        ("horizon", |report| report.horizon_s = 0.0),
        ("seeds", |report| {
            report.method = Method::Sampled;
            report.seeds.clear();
        }),
        ("actual cost", |report| report.actual_cost_s = -1.0),
        ("empty runs", |report| {
            report.runs.clear();
            report.partition.development = 0;
            report.partition.holdout = 0;
        }),
        ("partition", |report| report.partition.holdout += 1),
        ("scenario id", |report| report.runs[0].scenario_id.clear()),
        ("budget used", |report| report.runs[0].budget_used_s = -2.0),
        ("held_on_tested", |report| {
            let count = report.runs.len();
            report.runs[0].verdict = Verdict::HeldOnTested {
                tested_scenarios: count + 1,
                method: report.method,
                uncovered: "listed".into(),
            };
        }),
        ("confidence interval", |report| report.confidence_intervals[0].high = 1.0),
        ("metric value", |report| report.metrics[0].value = f64::NAN),
        ("metric unit", |report| report.metrics[0].unit.clear()),
        ("sustainability", |report| {
            report.metrics.retain(|metric| metric.group != MetricGroup::Production);
            if !report.metrics.iter().any(|metric| metric.group == MetricGroup::Sustainability) {
                report.metrics.push(MetricRecord {
                    group: MetricGroup::Sustainability,
                    name: "net_consumption".into(),
                    service: None,
                    value: 1.0,
                    unit: "kg".into(),
                    censored: false,
                });
            }
        }),
        ("provenance", |report| report.model_provenance[0].id.clear()),
        ("attachments", |report| {
            report.runs[0].verdict = Verdict::Counterexample {
                scenario_id: String::new(),
                first_violation_s: 1.0,
                constraint: "energy".into(),
                state: "state".into(),
            };
        }),
        // VEC §3.2: finite-scenario methods cannot prove infeasibility.
        ("method/verdict", |report| {
            report.runs[0].method = Method::Enumerated;
            report.runs[0].verdict =
                Verdict::InfeasibleInModel { certificate: "blocked".into(), checkable: "poie-graph".into() };
        }),
        // VEC §2.1: anticipating decisions and per-scenario policy instances.
        ("anticipation", |report| report.decisions[0].observed_up_to_s = report.decisions[0].decided_at_s + 1.0),
        ("policy instance", |report| {
            let mut extra = report.decisions[0].clone();
            extra.policy_instance = "tuned-for-this-scenario".into();
            report.decisions.push(extra);
        }),
        ("terminal state", |report| report.metrics.retain(|metric| metric.group != MetricGroup::Terminal)),
        // VEC §7: registration before running.
        ("registration", |report| report.registered_in.clear()),
    ]
}

#[test]
fn valid_reports_round_trip_and_every_mutation_is_rejected() {
    let mut rng = SplitMix64::new(SEED);
    for trial in 0..20 {
        let report = valid_report(&mut rng);
        report.validate().unwrap_or_else(|errors| panic!("trial {trial}: generated report is invalid: {errors:?}"));
        let toml = report.to_toml().expect("report serializes");
        let restored = MachineReport::from_toml(&toml).expect("report deserializes");
        assert_eq!(restored, report, "trial {trial}: TOML round-trip changed the report");
        assert!(report.human_summary().is_ok(), "trial {trial}: valid report has no human summary");

        for (name, mutation) in mutations() {
            let mut mutated = report.clone();
            mutation(&mut mutated);
            let errors = mutated.validate().expect_err(&format!("trial {trial}: mutation {name} was accepted"));
            let detail = format!("{errors:?}");
            assert!(!detail.is_empty(), "trial {trial}: mutation {name} produced no reason");
            assert!(mutated.human_summary().is_err(), "trial {trial}: mutation {name} produced a human summary");
        }
    }
}

#[test]
fn mutation_reasons_name_the_violated_rule() {
    // Spot-check the wording of the classification so a future refactor cannot
    // silently replace specific errors with a generic rejection.
    let mut rng = SplitMix64::new(SEED ^ 0x99);
    let report = valid_report(&mut rng);
    for (name, mutation, needle) in [
        ("sustainability", mutations()[13].1, "sustainability metrics"),
        ("partition", mutations()[6].1, "development + holdout"),
        ("attachments", mutations()[15].1, "lacks required attachments"),
        ("empty runs", mutations()[5].1, "list is empty"),
    ] {
        let mut mutated = report.clone();
        let _ = name;
        mutation(&mut mutated);
        let errors = mutated.validate().expect_err("mutation was accepted");
        let detail = format!("{errors:?}");
        assert!(detail.contains(needle), "mutation reason {detail:?} does not mention {needle:?}");
    }
}
