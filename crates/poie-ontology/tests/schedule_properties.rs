//! Randomized scheduling invariants: precedence, duration preservation,
//! capacity-conflict completeness and soundness, determinism and cycle detection.

use std::collections::{BTreeMap, BTreeSet};

use poie_ontology::schedule::{earliest_start_schedule, ProcessInstance, ScheduleError};
use poie_ontology::time::TimeWindow;

const SEED: u64 = 0x706f_6965_7363_6864; // "poieschd"

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

const ASSETS: [&str; 3] = ["asset-a", "asset-b", "asset-c"];

fn instances(rng: &mut SplitMix64) -> Vec<ProcessInstance> {
    let count = 1 + rng.index(8);
    let mut instances = Vec::new();
    for index in 0..count {
        let duration = rng.index(500) as f64;
        let mut occupies = Vec::new();
        for asset in ASSETS {
            if rng.unit() < 0.5 {
                occupies.push(asset.to_string());
            }
        }
        instances.push(ProcessInstance::new(format!("i{index}"), "process", occupies, duration).unwrap());
    }
    instances
}

/// A DAG by construction: edges always point from a lower index to a higher one.
fn precedence(rng: &mut SplitMix64, count: usize) -> Vec<(String, String)> {
    let mut edges = Vec::new();
    for before in 0..count {
        for after in (before + 1)..count {
            if rng.unit() < 0.35 {
                edges.push((format!("i{before}"), format!("i{after}")));
            }
        }
    }
    edges
}

fn normalized_conflict(asset: &str, first: &str, second: &str) -> (String, String, String) {
    let (left, right) = if first <= second { (first, second) } else { (second, first) };
    (asset.to_string(), left.to_string(), right.to_string())
}

#[test]
fn randomized_schedules_respect_precedence_and_capacity_accounting() {
    let mut rng = SplitMix64::new(SEED);
    for trial in 0..120 {
        let instances = instances(&mut rng);
        let edges = precedence(&mut rng, instances.len());
        let report = earliest_start_schedule(&instances, &edges).expect("a DAG never fails");
        let again = earliest_start_schedule(&instances, &edges).expect("a DAG never fails");
        assert_eq!(report, again, "trial {trial}: scheduling is not deterministic");

        let durations: BTreeMap<&str, f64> =
            instances.iter().map(|instance| (instance.id.as_str(), instance.duration_s)).collect();
        let windows: BTreeMap<&str, (f64, f64)> = report
            .instances
            .iter()
            .map(|scheduled| (scheduled.id.as_str(), (scheduled.start_s, scheduled.end_s)))
            .collect();
        assert_eq!(windows.len(), instances.len(), "trial {trial}: an instance disappeared");
        for scheduled in &report.instances {
            assert_eq!(
                scheduled.end_s - scheduled.start_s,
                durations[scheduled.id.as_str()],
                "trial {trial}: duration changed for {}",
                scheduled.id
            );
            assert!(scheduled.start_s >= -1e-9, "trial {trial}: negative start time");
        }
        for pair in report.instances.windows(2) {
            let left = (pair[0].start_s, pair[0].id.as_str());
            let right = (pair[1].start_s, pair[1].id.as_str());
            assert!(left <= right, "trial {trial}: instances are not sorted by start then id");
        }

        // Precedence: every predecessor finishes before its successor starts.
        for (before, after) in &edges {
            let (_, before_end) = windows[before.as_str()];
            let (after_start, _) = windows[after.as_str()];
            assert!(before_end <= after_start + 1e-9, "trial {trial}: precedence {before}->{after} violated");
        }

        // Conflict soundness: every reported conflict is a real overlap.
        for conflict in &report.conflicts {
            let (first_start, first_end) = windows[conflict.first.as_str()];
            let (second_start, second_end) = windows[conflict.second.as_str()];
            assert!(
                first_start < second_end && second_start < first_end,
                "trial {trial}: reported conflict without overlap on {}",
                conflict.asset
            );
            assert_eq!(
                conflict.overlap,
                TimeWindow::new(first_start.max(second_start), first_end.min(second_end)).unwrap(),
                "trial {trial}: wrong overlap window"
            );
        }

        // Conflict completeness: every overlapping occupation is reported.
        let occupations: BTreeMap<&str, Vec<&str>> = instances
            .iter()
            .map(|instance| (instance.id.as_str(), instance.occupies.iter().map(String::as_str).collect()))
            .collect();
        let mut expected: BTreeSet<(String, String, String)> = BTreeSet::new();
        let ids: Vec<&str> = windows.keys().copied().collect();
        for (position, left) in ids.iter().enumerate() {
            for right in ids.iter().skip(position + 1) {
                let (left_start, left_end) = windows[left];
                let (right_start, right_end) = windows[right];
                if left_start < right_end && right_start < left_end {
                    for asset in &occupations[left] {
                        if occupations[right].contains(asset) {
                            expected.insert(normalized_conflict(asset, left, right));
                        }
                    }
                }
            }
        }
        let reported: BTreeSet<(String, String, String)> = report
            .conflicts
            .iter()
            .map(|conflict| normalized_conflict(&conflict.asset, &conflict.first, &conflict.second))
            .collect();
        assert_eq!(reported, expected, "trial {trial}: capacity accounting is incomplete");
    }
}

#[test]
fn cycles_are_detected_and_unknown_instances_rejected() {
    let instances = vec![
        ProcessInstance::new("a", "process", vec!["asset-a".to_string()], 10.0).unwrap(),
        ProcessInstance::new("b", "process", vec!["asset-a".to_string()], 20.0).unwrap(),
    ];
    let cycle = vec![("a".to_string(), "b".to_string()), ("b".to_string(), "a".to_string())];
    let error = earliest_start_schedule(&instances, &cycle).unwrap_err();
    assert!(matches!(error, ScheduleError::PrecedenceCycle(_)), "{error}");

    let unknown = vec![("a".to_string(), "missing".to_string())];
    let error = earliest_start_schedule(&instances, &unknown).unwrap_err();
    assert!(matches!(error, ScheduleError::UnknownInstance(_)), "{error}");
}
