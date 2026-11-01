//! Temporal gain and construction-period check (VEC §6.2.2, M11.3).

use std::collections::BTreeMap;

use poie_ontology::time::TimeWindow;
use poie_viability::metrics::{summarize_viability, RunTrace, ViabilitySummary};

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ConstructionCheck {
    pub within_window: bool,
    pub violations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TemporalGain {
    pub base: Vec<ViabilitySummary>,
    pub expanded: Vec<ViabilitySummary>,
    /// expanded.min_maintained_s − base.min_maintained_s per service (None when
    /// either side is fully censored).
    pub min_maintained_delta_s: BTreeMap<String, Option<f64>>,
    pub construction: ConstructionCheck,
}

/// Compares the viability metric on the same disturbance traces and checks that
/// critical services keep their minimum during construction.
pub fn temporal_gain(
    base_traces: &[RunTrace],
    expanded_traces: &[RunTrace],
    minimum_levels: &BTreeMap<String, f64>,
    construction_window: TimeWindow,
) -> TemporalGain {
    let base = summarize_viability(base_traces);
    let expanded = summarize_viability(expanded_traces);
    let mut deltas: BTreeMap<String, Option<f64>> = BTreeMap::new();
    for summary in &expanded {
        let base_value =
            base.iter().find(|entry| entry.service == summary.service).and_then(|entry| entry.min_maintained_s);
        deltas.insert(
            summary.service.clone(),
            match (summary.min_maintained_s, base_value) {
                (Some(after), Some(before)) => Some(after - before),
                _ => None,
            },
        );
    }
    let mut violations = Vec::new();
    for trace in expanded_traces {
        for sample in &trace.samples {
            if !construction_window.contains(sample.at_s) {
                continue;
            }
            for (service, minimum) in minimum_levels {
                // VEC §4: a missing reading is unknown, never full service.
                match sample.service_levels.get(service).copied() {
                    Some(level) if level + 1e-9 < *minimum => {
                        violations.push(format!("{} at {:.0}s: {:.3} < {:.3}", service, sample.at_s, level, minimum))
                    }
                    Some(_) => {}
                    None => violations.push(format!("{} at {:.0}s: no reading (unknown)", service, sample.at_s)),
                }
            }
        }
    }
    TemporalGain {
        base,
        expanded,
        min_maintained_delta_s: deltas,
        construction: ConstructionCheck { within_window: violations.is_empty(), violations },
    }
}

impl TemporalGain {
    /// Non-inferiority on one critical service (VEC §6.3 condition 3): the
    /// expanded branch's smallest observed survival time may fall short of the
    /// base by at most the pre-registered margin. A fully censored side counts
    /// as "> follow-up". Undecided expanded runs fail the check. Returns
    /// `None` when the service is not summarized.
    pub fn non_inferior(&self, service: &str, margin_s: f64) -> Option<bool> {
        let base = self.base.iter().find(|summary| summary.service == service)?;
        let expanded = self.expanded.iter().find(|summary| summary.service == service)?;
        if expanded.undecided > 0 || expanded.scenarios != base.scenarios {
            return Some(false);
        }
        Some(match (base.min_maintained_s, expanded.min_maintained_s) {
            (_, None) => true,
            (None, Some(_)) => false,
            (Some(before), Some(after)) => after + margin_s >= before,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use poie_viability::metrics::{RunTrace, TimeSample};

    fn sample(at_s: f64, level: f64) -> TimeSample {
        TimeSample {
            at_s,
            service_levels: BTreeMap::from([("potable-water".to_string(), level)]),
            violations: Vec::new(),
            external_inputs: BTreeMap::new(),
            stock: BTreeMap::new(),
            produced: BTreeMap::new(),
            recycle_inputs_kg: BTreeMap::new(),
            recycle_reusable_kg: BTreeMap::new(),
            health: BTreeMap::new(),
            affected_services: Vec::new(),
            affected_assets: Vec::new(),
        }
    }

    fn trace(id: &str, samples: Vec<TimeSample>) -> RunTrace {
        RunTrace {
            scenario_id: id.to_string(),
            horizon_s: 10_000.0,
            minimum_levels: BTreeMap::from([("potable-water".to_string(), 1.0)]),
            samples,
            recovery_paths: BTreeMap::new(),
        }
    }

    fn construction_window() -> TimeWindow {
        TimeWindow::new(0.0, 2_000.0).unwrap()
    }

    #[test]
    fn temporal_gain_reports_deltas_and_construction_ok() {
        let base = vec![trace("b", vec![sample(0.0, 1.0), sample(1_000.0, 0.5)])];
        let expanded = vec![trace("e", vec![sample(0.0, 1.0), sample(1_500.0, 1.0), sample(3_000.0, 0.5)])];
        let levels = BTreeMap::from([("potable-water".to_string(), 1.0)]);
        let gain = temporal_gain(&base, &expanded, &levels, construction_window());
        assert_eq!(gain.min_maintained_delta_s["potable-water"], Some(2_000.0));
        assert!(gain.construction.within_window);
    }

    #[test]
    fn a_construction_violation_blocks_the_claim() {
        let base = vec![trace("b", vec![sample(0.0, 1.0)])];
        let expanded = vec![trace("e", vec![sample(0.0, 1.0), sample(1_000.0, 0.4)])];
        let levels = BTreeMap::from([("potable-water".to_string(), 0.8)]);
        let gain = temporal_gain(&base, &expanded, &levels, construction_window());
        assert!(!gain.construction.within_window);
        assert_eq!(gain.construction.violations.len(), 1);
    }

    #[test]
    fn vec6_3_non_inferiority_uses_the_preregistered_margin() {
        let base = vec![trace("b", vec![sample(0.0, 1.0), sample(5_000.0, 0.5)])];
        let worse = vec![trace("e", vec![sample(0.0, 1.0), sample(4_000.0, 0.5)])];
        let levels = BTreeMap::from([("potable-water".to_string(), 1.0)]);
        let gain = temporal_gain(&base, &worse, &levels, TimeWindow::new(0.0, 1_000.0).unwrap());
        assert_eq!(gain.non_inferior("potable-water", 500.0), Some(false));
        assert_eq!(gain.non_inferior("potable-water", 1_000.0), Some(true));
        assert_eq!(gain.non_inferior("heat", 0.0), None);
        // Fewer expanded runs than base runs (e.g. not buildable) is not non-inferior.
        let gain = temporal_gain(&base, &[], &levels, TimeWindow::new(0.0, 1_000.0).unwrap());
        assert_eq!(gain.non_inferior("potable-water", 1e9), None);
    }
}
