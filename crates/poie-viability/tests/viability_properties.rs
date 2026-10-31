//! Randomized VEC invariants: viability/recovery/resilience accounting,
//! censoring rules in summaries, sustainability formulas, and deterministic
//! reproducible methods.

use std::collections::BTreeMap;

use poie_viability::method::{enumerated_plan, sampled_plan, Method};
use poie_viability::metrics::{resilience, summarize_viability, sustainability, viability, RunTrace, TimeSample};
use poie_viability::query::FixedEvent;

const SEED: u64 = 0x706f_6965_7669_6162; // "poieviab"
const SERVICES: [&str; 3] = ["water", "heat", "inspection"];
const RESOURCES: [&str; 2] = ["bronze-bar", "electric-energy"];

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

fn trace(rng: &mut SplitMix64, id: usize) -> RunTrace {
    let horizon = 100.0 + rng.value_range(900.0);
    let count = 1 + rng.index(8);
    let mut services = Vec::new();
    for service in SERVICES {
        if rng.unit() < 0.8 {
            services.push(service);
        }
    }
    if services.is_empty() {
        services.push(SERVICES[0]);
    }
    let minimums: BTreeMap<String, f64> =
        services.iter().map(|service| (service.to_string(), 0.5 + rng.unit() * 0.5)).collect();
    let mut samples = Vec::new();
    let mut at = 0.0;
    for _ in 0..count {
        at += 1.0 + rng.value_range(horizon / count as f64);
        let levels = services
            .iter()
            .map(|service| {
                let roll = rng.unit();
                let level = if roll < 0.2 { 0.0 } else { rng.unit() };
                (service.to_string(), level)
            })
            .collect();
        let mut stock = BTreeMap::new();
        let mut produced = BTreeMap::new();
        let mut external = BTreeMap::new();
        for resource in RESOURCES {
            stock.insert(resource.to_string(), rng.value_range(10.0));
            produced.insert(resource.to_string(), rng.value_range(5.0));
            if rng.unit() < 0.5 {
                external.insert(resource.to_string(), rng.value_range(3.0));
            }
        }
        let mut recycle_inputs = BTreeMap::new();
        let mut recycle_reusable = BTreeMap::new();
        if rng.unit() < 0.5 {
            recycle_inputs.insert("swarf".to_string(), rng.value_range(4.0));
            recycle_reusable.insert("swarf".to_string(), rng.value_range(3.0));
        }
        samples.push(TimeSample {
            at_s: at.min(horizon - 0.5),
            service_levels: levels,
            violations: Vec::new(),
            external_inputs: external,
            stock,
            produced,
            recycle_inputs_kg: recycle_inputs,
            recycle_reusable_kg: recycle_reusable,
            health: BTreeMap::from([("pump-p1".to_string(), rng.unit())]),
            affected_services: services.iter().map(|service| service.to_string()).collect(),
            affected_assets: vec!["pump-p1".to_string()],
        });
    }
    RunTrace {
        scenario_id: format!("random-{id}"),
        horizon_s: horizon,
        minimum_levels: minimums,
        samples,
        recovery_paths: BTreeMap::new(),
    }
}

impl SplitMix64 {
    fn value_range(&mut self, range: f64) -> f64 {
        self.unit() * range
    }
}

#[test]
fn randomized_traces_follow_the_documented_metric_definitions() {
    let mut rng = SplitMix64::new(SEED);
    for trial in 0..60 {
        let trace = trace(&mut rng, trial);
        trace.validate().expect("generated trace is valid");

        // Viability: first drop below the minimum, censoring when it never drops.
        for entry in viability(&trace) {
            let minimum = trace.minimum_levels[&entry.service];
            let first = trace
                .samples
                .iter()
                .find(|sample| sample.service_levels.get(&entry.service).copied().unwrap_or(1.0) + 1e-9 < minimum);
            assert_eq!(entry.maintained_s, first.map(|sample| sample.at_s), "trial {trial}: wrong maintained time");
            assert_eq!(entry.censored, first.is_none(), "trial {trial}: wrong censoring flag");
        }

        // Resilience: loss integral is the step-function integral; recovery is
        // the first later sample at/above the minimum; unrecovered is censored.
        let resilience_metrics = resilience(&trace);
        for (service, minimum) in &trace.minimum_levels {
            let mut loss = 0.0;
            for (index, sample) in trace.samples.iter().enumerate() {
                let end = trace.samples.get(index + 1).map_or(trace.horizon_s, |next| next.at_s);
                let level = sample.service_levels.get(service).copied().unwrap_or(1.0);
                loss += (minimum - level).max(0.0) * (end - sample.at_s);
            }
            assert!((resilience_metrics.service_loss[service] - loss).abs() < 1e-6, "trial {trial}: loss integral");

            let violation = trace
                .samples
                .iter()
                .find(|sample| sample.service_levels.get(service).copied().unwrap_or(1.0) + 1e-9 < *minimum);
            match violation {
                Some(violation) => {
                    let recovery = trace.samples.iter().find(|sample| {
                        sample.at_s > violation.at_s
                            && sample.service_levels.get(service).copied().unwrap_or(1.0) + 1e-9 >= *minimum
                    });
                    let expected = recovery.map(|recovered| recovered.at_s - violation.at_s);
                    assert_eq!(resilience_metrics.recovery_time_s[service], expected, "trial {trial}: recovery time");
                    assert_eq!(
                        resilience_metrics.not_recovered.contains(service),
                        recovery.is_none(),
                        "trial {trial}: not-recovered set"
                    );
                }
                None => assert_eq!(resilience_metrics.recovery_time_s[service], Some(0.0)),
            }
        }

        // Sustainability formulas.
        let sustainability_metrics = sustainability(&trace);
        let start = &trace.samples[0];
        let end = trace.samples.last().unwrap();
        for resource in RESOURCES {
            let external: f64 =
                trace.samples.iter().map(|sample| sample.external_inputs.get(resource).copied().unwrap_or(0.0)).sum();
            let expected = end.stock.get(resource).copied().unwrap_or(0.0)
                - start.stock.get(resource).copied().unwrap_or(0.0)
                - external;
            assert!(
                (sustainability_metrics.net_consumption[resource] - expected).abs() < 1e-6,
                "trial {trial}: net consumption"
            );
        }
        let recycle_loss: f64 = end
            .recycle_inputs_kg
            .iter()
            .map(|(stream, input)| input - end.recycle_reusable_kg.get(stream).copied().unwrap_or(0.0))
            .sum();
        assert!((sustainability_metrics.recycle_loss_kg - recycle_loss).abs() < 1e-6, "trial {trial}: recycle loss");
    }
}

#[test]
fn summaries_respect_censoring_and_quantiles() {
    let mut rng = SplitMix64::new(SEED ^ 0xdead_beef);
    let traces: Vec<RunTrace> = (0..20).map(|index| trace(&mut rng, index)).collect();
    for summary in summarize_viability(&traces) {
        let mut times: Vec<f64> = Vec::new();
        let mut censored = 0;
        for trace in &traces {
            if let Some(entry) = viability(trace).into_iter().find(|entry| entry.service == summary.service) {
                match entry.maintained_s {
                    Some(time) => times.push(time),
                    None => censored += 1,
                }
            }
        }
        times.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
        assert_eq!(summary.censored, censored, "wrong censored count");
        if censored > 0 || times.is_empty() {
            assert_eq!(summary.mean_maintained_s, None, "censored mean must not be reported");
        }
        if let Some(minimum) = summary.min_maintained_s {
            assert!(times.first().is_some_and(|first| (first - minimum).abs() < 1e-9), "min mismatch");
        }
        if let (Some(median), Some(p90)) = (summary.median_maintained_s, summary.p90_maintained_s) {
            assert!(median <= p90 + 1e-9, "quantiles must be ordered");
        }
    }
}

#[test]
fn sampled_and_enumerated_methods_are_reproducible() {
    let first = sampled_plan(&[3, 7, 11], 4, 10).unwrap();
    let second = sampled_plan(&[3, 7, 11], 4, 10).unwrap();
    assert_eq!(first, second, "sampled plans are not deterministic");
    assert_eq!(first.digest(), second.digest(), "sampled digests differ");
    assert_eq!(first.method, Method::Sampled);
    let other = sampled_plan(&[1, 2, 5], 4, 10).unwrap();
    assert_ne!(first.digest(), other.digest(), "different seeds produced the same plan");

    let events = [
        FixedEvent { at_s: 10.0, description: "bus fails".into(), synthetic: true },
        FixedEvent { at_s: 20.0, description: "pump fails".into(), synthetic: true },
    ];
    let enumerated = enumerated_plan(&events, 100.0).unwrap();
    assert_eq!(enumerated.scenario_ids(), vec!["baseline", "fixed-0", "fixed-1", "fixed-all"]);
    assert_eq!(enumerated.method, Method::Enumerated);
    assert_eq!(enumerated, enumerated_plan(&events, 100.0).unwrap());
}
