//! Machine and human reports with the VEC §7 reproduction checklist.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::method::Method;
use crate::verdict::{check_summary, Verdict};

pub const REPORT_SCHEMA: &str = "poie.viability-report/0.1";
pub const METRIC_DEFINITION_VERSION: &str = "vec-metrics/0.1";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputHashes {
    pub initial_state: String,
    pub scenario_set: String,
    pub models: String,
    pub policy: String,
    pub contract_version: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioPartition {
    pub development: usize,
    pub holdout: usize,
    /// Must be honest: was the holdout looked at during development?
    pub holdout_viewed_in_development: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRecord {
    pub scenario_id: String,
    pub method: Method,
    pub seed: Option<u64>,
    pub budget_used_s: f64,
    pub verdict: Verdict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricGroup {
    Viability,
    Independence,
    Resilience,
    Sustainability,
    Production,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricRecord {
    pub group: MetricGroup,
    pub name: String,
    pub service: Option<String>,
    pub value: f64,
    pub unit: String,
    /// Censored values are not means and must stay marked as censored.
    pub censored: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfidenceInterval {
    pub metric: String,
    pub low: f64,
    pub high: f64,
    pub method: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MachineReport {
    pub schema: String,
    pub contract_version: String,
    pub input_hashes: InputHashes,
    pub partition: ScenarioPartition,
    pub method: Method,
    pub horizon_s: f64,
    pub seeds: Vec<u64>,
    pub solver_version: String,
    pub solver_settings: String,
    pub actual_cost_s: f64,
    pub metric_definition_version: String,
    pub confidence_intervals: Vec<ConfidenceInterval>,
    /// Every run, including failures, timeouts and infeasible/numerical outcomes.
    pub runs: Vec<RunRecord>,
    pub metrics: Vec<MetricRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReportError {
    Invalid(String),
    Toml(String),
    Wording(String),
}

impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReportError::Invalid(detail) => write!(f, "invalid report: {detail}"),
            ReportError::Toml(detail) => write!(f, "report TOML: {detail}"),
            ReportError::Wording(detail) => write!(f, "report wording: {detail}"),
        }
    }
}

impl std::error::Error for ReportError {}

impl MachineReport {
    pub fn validate(&self) -> Result<(), Vec<ReportError>> {
        let mut errors = Vec::new();
        if self.schema != REPORT_SCHEMA {
            errors.push(ReportError::Invalid(format!("unsupported schema {:?}", self.schema)));
        }
        for (field, value) in [
            ("contract_version", &self.contract_version),
            ("input_hashes.initial_state", &self.input_hashes.initial_state),
            ("input_hashes.scenario_set", &self.input_hashes.scenario_set),
            ("input_hashes.models", &self.input_hashes.models),
            ("input_hashes.policy", &self.input_hashes.policy),
            ("input_hashes.contract_version", &self.input_hashes.contract_version),
            ("solver_version", &self.solver_version),
            ("solver_settings", &self.solver_settings),
            ("metric_definition_version", &self.metric_definition_version),
        ] {
            if value.trim().is_empty() {
                errors.push(ReportError::Invalid(format!("{field} must not be empty")));
            }
        }
        if !self.horizon_s.is_finite() || self.horizon_s <= 0.0 {
            errors.push(ReportError::Invalid("horizon must be finite and positive".into()));
        }
        if self.method == Method::Sampled && self.seeds.is_empty() {
            errors.push(ReportError::Invalid("sampled reports need their seeds".into()));
        }
        if !self.actual_cost_s.is_finite() || self.actual_cost_s < 0.0 {
            errors.push(ReportError::Invalid("actual cost must be finite and non-negative".into()));
        }
        if self.runs.is_empty() {
            errors.push(ReportError::Invalid("all runs must be listed; the list is empty".into()));
        }
        if self.partition.development + self.partition.holdout != self.runs.len() {
            errors.push(ReportError::Invalid("development + holdout must equal the number of listed runs".into()));
        }
        for run in &self.runs {
            if run.scenario_id.trim().is_empty() {
                errors.push(ReportError::Invalid("scenario id must not be empty".into()));
            }
            if !run.verdict.attachments_complete() {
                errors
                    .push(ReportError::Invalid(format!("verdict for {} lacks required attachments", run.scenario_id)));
            }
            if !run.budget_used_s.is_finite() || run.budget_used_s < 0.0 {
                errors.push(ReportError::Invalid("budget used must be finite and non-negative".into()));
            }
            if let Verdict::HeldOnTested { tested_scenarios, .. } = &run.verdict {
                if *tested_scenarios > self.runs.len() {
                    errors.push(ReportError::Invalid("held_on_tested claims more scenarios than were run".into()));
                }
            }
            if let Err(error) = check_summary(&run.verdict, &run.verdict.canonical_summary()) {
                errors.push(ReportError::Wording(error.to_string()));
            }
        }
        for interval in &self.confidence_intervals {
            if interval.metric.trim().is_empty() || interval.method.trim().is_empty() {
                errors.push(ReportError::Invalid("confidence intervals need a metric and a method".into()));
            }
            if !interval.low.is_finite() || !interval.high.is_finite() || interval.low > interval.high {
                errors.push(ReportError::Invalid("confidence interval bounds are invalid".into()));
            }
        }
        for metric in &self.metrics {
            if metric.name.trim().is_empty() || metric.unit.trim().is_empty() {
                errors.push(ReportError::Invalid("metrics need a name and a unit".into()));
            }
            if !metric.value.is_finite() {
                errors.push(ReportError::Invalid("metric values must be finite".into()));
            }
        }
        // VEC §4.4 anti-cheat: sustainability only with production and viability.
        if self.metrics.iter().any(|metric| metric.group == MetricGroup::Sustainability) {
            let has_production = self.metrics.iter().any(|metric| metric.group == MetricGroup::Production);
            let has_viability = self.metrics.iter().any(|metric| metric.group == MetricGroup::Viability);
            if !has_production || !has_viability {
                errors.push(ReportError::Invalid(
                    "sustainability metrics must be reported together with production and viability metrics".into(),
                ));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Human-readable summary. Contains only wording that passes the verdict
    /// guards, and states censoring and the absence of robustness guarantees.
    pub fn human_summary(&self) -> Result<String, ReportError> {
        if let Err(errors) = self.validate() {
            return Err(errors[0].clone());
        }
        let mut out = String::new();
        out.push_str(&format!(
            "Viability report ({}, method {}, horizon {:.0}s)\n",
            self.contract_version, self.method, self.horizon_s
        ));
        out.push_str(&format!(
            "scenarios: development {}, holdout {} (holdout viewed in development: {})\n",
            self.partition.development, self.partition.holdout, self.partition.holdout_viewed_in_development
        ));
        out.push_str(&format!("solver: {} ({})\n", self.solver_version, self.solver_settings));
        let mut by_kind: BTreeMap<&str, usize> = BTreeMap::new();
        for run in &self.runs {
            *by_kind.entry(run.verdict.kind().as_str()).or_insert(0) += 1;
        }
        let kinds: Vec<String> = by_kind.iter().map(|(kind, count)| format!("{kind}: {count}")).collect();
        out.push_str(&format!("verdicts: {}\n", kinds.join(", ")));
        for run in &self.runs {
            out.push_str(&format!("- {}: {}\n", run.scenario_id, run.verdict.canonical_summary()));
        }
        if self.runs.iter().any(|run| matches!(run.verdict, Verdict::Timeout { .. } | Verdict::NumericalFailure { .. }))
        {
            out.push_str("note: undecided runs are reported as undecided, not as infeasible\n");
        }
        if self.metrics.iter().any(|metric| metric.censored) {
            out.push_str("note: censored survival times are marked censored; no mean is reported from them\n");
        }
        let groups: Vec<String> = {
            let mut groups: Vec<&str> = self
                .metrics
                .iter()
                .map(|metric| match metric.group {
                    MetricGroup::Viability => "viability",
                    MetricGroup::Independence => "independence",
                    MetricGroup::Resilience => "resilience",
                    MetricGroup::Sustainability => "sustainability",
                    MetricGroup::Production => "production",
                })
                .collect();
            groups.sort_unstable();
            groups.dedup();
            groups.into_iter().map(str::to_string).collect()
        };
        out.push_str(&format!("metric groups: {}\n", groups.join(", ")));
        if self.confidence_intervals.is_empty() {
            out.push_str("no confidence interval computed; finite samples are not a robustness guarantee\n");
        } else {
            for interval in &self.confidence_intervals {
                out.push_str(&format!(
                    "confidence interval for {}: [{:.3}, {:.3}] ({})\n",
                    interval.metric, interval.low, interval.high, interval.method
                ));
            }
        }
        Ok(out)
    }

    pub fn to_toml(&self) -> Result<String, ReportError> {
        self.validate().map_err(|errors| errors[0].clone())?;
        toml::to_string(self).map_err(|error| ReportError::Toml(error.to_string()))
    }

    pub fn from_toml(text: &str) -> Result<MachineReport, ReportError> {
        let report: MachineReport = toml::from_str(text).map_err(|error| ReportError::Toml(error.to_string()))?;
        report.validate().map_err(|errors| errors[0].clone())?;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verdict::Verdict;

    fn sample_report() -> MachineReport {
        MachineReport {
            schema: REPORT_SCHEMA.into(),
            contract_version: "viability-expansion/0.1".into(),
            input_hashes: InputHashes {
                initial_state: "state-hash".into(),
                scenario_set: "scenarios-hash".into(),
                models: "models-hash".into(),
                policy: "policy-hash".into(),
                contract_version: "vec-0.1".into(),
            },
            partition: ScenarioPartition { development: 2, holdout: 1, holdout_viewed_in_development: false },
            method: Method::Sampled,
            horizon_s: 604_800.0,
            seeds: vec![1, 2, 3],
            solver_version: "poie-viability 0.1.0".into(),
            solver_settings: "draws=4".into(),
            actual_cost_s: 1.5,
            metric_definition_version: METRIC_DEFINITION_VERSION.into(),
            confidence_intervals: vec![ConfidenceInterval {
                metric: "potable-water.maintained_s".into(),
                low: 3600.0,
                high: 7200.0,
                method: "percentile bootstrap".into(),
            }],
            runs: vec![
                RunRecord {
                    scenario_id: "sample-1".into(),
                    method: Method::Sampled,
                    seed: Some(1),
                    budget_used_s: 0.5,
                    verdict: Verdict::HeldOnTested {
                        tested_scenarios: 3,
                        method: Method::Sampled,
                        uncovered: "state-related failures above the tested range".into(),
                    },
                },
                RunRecord {
                    scenario_id: "sample-2".into(),
                    method: Method::Sampled,
                    seed: Some(2),
                    budget_used_s: 0.5,
                    verdict: Verdict::Timeout { completed_scenarios: 1, budget: "60 s".into() },
                },
                RunRecord {
                    scenario_id: "sample-3".into(),
                    method: Method::Sampled,
                    seed: Some(3),
                    budget_used_s: 0.5,
                    verdict: Verdict::Counterexample {
                        scenario_id: "sample-3".into(),
                        first_violation_s: 3600.0,
                        constraint: "pump head >= 30 m".into(),
                        state: "pump failed".into(),
                    },
                },
            ],
            metrics: vec![
                MetricRecord {
                    group: MetricGroup::Viability,
                    name: "maintained_s".into(),
                    service: Some("potable-water".into()),
                    value: 3600.0,
                    unit: "s".into(),
                    censored: true,
                },
                MetricRecord {
                    group: MetricGroup::Production,
                    name: "sleeve-p1".into(),
                    service: None,
                    value: 2.0,
                    unit: "item".into(),
                    censored: false,
                },
                MetricRecord {
                    group: MetricGroup::Sustainability,
                    name: "net_consumption.bronze-bar".into(),
                    service: None,
                    value: -3.0,
                    unit: "kg".into(),
                    censored: false,
                },
            ],
        }
    }

    #[test]
    fn a_complete_report_validates_and_round_trips() {
        let report = sample_report();
        assert!(report.validate().is_ok(), "{:?}", report.validate());
        let text = report.to_toml().unwrap();
        let parsed = MachineReport::from_toml(&text).unwrap();
        assert_eq!(parsed, report);
    }

    #[test]
    fn missing_inputs_and_partition_mismatches_are_rejected() {
        let mut report = sample_report();
        report.input_hashes.models.clear();
        assert!(report.validate().is_err());

        let mut report = sample_report();
        report.partition.holdout = 5;
        assert!(report.validate().is_err());

        let mut report = sample_report();
        report.runs.clear();
        assert!(report.validate().is_err());
    }

    #[test]
    fn sustainability_requires_production_and_viability() {
        let mut report = sample_report();
        report.metrics.retain(|metric| metric.group != MetricGroup::Production);
        let errors = report.validate().unwrap_err();
        assert!(errors.iter().any(|error| error.to_string().contains("sustainability")));
    }

    #[test]
    fn human_summary_is_safe_about_timeouts_and_censoring() {
        let summary = sample_report().human_summary().unwrap();
        assert!(summary.contains("undecided, not infeasible"));
        assert!(summary.contains("censored"));
        assert!(summary.contains("not a claim of safety or robustness"));
        assert!(!summary.to_lowercase().contains("is safe"));
    }

    #[test]
    fn sampled_reports_need_seeds() {
        let mut report = sample_report();
        report.seeds.clear();
        assert!(report.validate().is_err());
    }

    #[test]
    fn invalid_confidence_intervals_are_rejected() {
        let mut report = sample_report();
        report.confidence_intervals[0].low = 10.0;
        report.confidence_intervals[0].high = 5.0;
        assert!(report.validate().is_err());
    }
}
