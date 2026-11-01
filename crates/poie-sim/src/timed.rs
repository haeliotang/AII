//! Timed scenario: timelines, pre-registered timed cases and deterministic replay
//! digests (M5.5, M5.6).
//!
//! A timed scenario layers time onto the structural Base-01 model: per-asset
//! status timelines with failure predictions, per-resource stock timelines, and
//! a `cases.toml` with pre-registered expectations. A case asks whether a goal is
//! feasible from a start time, within an optional deadline, with the resources
//! that are actually available at each step's start.
//!
//! Verdicts extend the structural ones with `too_late`, `insufficient_resource`
//! and `capacity_conflict`. Every evaluation produces a canonical digest; a
//! committed digest file lets CI verify that any run replays bit-for-bit (M5.5).

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use poie_ontology::graph::{self, Goal, Query, Verdict, DEFAULT_MAX_DEPTH};
use poie_ontology::model::{AssetStatus, Model, Patch, Verification};
use poie_ontology::scenario::GoalDoc;
use poie_ontology::time::{AssetTimeline, StockTimeline, TimeWindow};
use poie_ontology::units::{Dimension, Quantity};

use crate::schedule_route;

pub const TIMED_SCENARIO_SCHEMA: &str = "poie.timed-scenario/0.2";
pub const TIMED_CASES_SCHEMA: &str = "poie.timed-cases/0.1";
pub const TIMED_DIGESTS_SCHEMA: &str = "poie.timed-digests/0.1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimedError(pub String);

impl fmt::Display for TimedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TimedError {}

fn error<T>(message: impl Into<String>) -> Result<T, TimedError> {
    Err(TimedError(message.into()))
}

// ---------------------------------------------------------------------------
// Parsed data
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioDoc {
    schema: String,
    model: String,
    #[serde(default)]
    description: String,
    /// How long status evidence supports a status (PO §4 rule 4). Required
    /// since schema 0.2 (issue #111): a timed scenario must say when its
    /// evidence expires instead of keeping the last value forever.
    evidence_validity: String,
    #[serde(default, rename = "asset_timeline")]
    asset_timelines: Vec<AssetTimelineDoc>,
    #[serde(default, rename = "stock_timeline")]
    stock_timelines: Vec<StockTimelineDoc>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssetTimelineDoc {
    asset: String,
    initial: String,
    #[serde(default, rename = "change")]
    changes: Vec<StatusChangeDoc>,
    prediction: Option<WindowDoc>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusChangeDoc {
    at: String,
    status: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowDoc {
    from: String,
    to: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StockTimelineDoc {
    resource: String,
    initial: String,
    #[serde(default, rename = "change")]
    changes: Vec<StockChangeDoc>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StockChangeDoc {
    at: String,
    delta: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CasesDoc {
    schema: String,
    scenario: String,
    #[serde(rename = "case")]
    cases: Vec<CaseDoc>,
}

/// One pre-registered timed case.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseDoc {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub goal: GoalDoc,
    pub min_verification: Verification,
    pub start: String,
    pub deadline: Option<String>,
    pub expect: ExpectDoc,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectDoc {
    pub verdict: TimedVerdict,
    pub completion_le: Option<String>,
}

/// Verdict classes of a timed case.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimedVerdict {
    Feasible,
    Blocked,
    Unknown,
    SearchLimit,
    TooLate,
    InsufficientResource,
    CapacityConflict,
}

impl TimedVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            TimedVerdict::Feasible => "feasible",
            TimedVerdict::Blocked => "blocked",
            TimedVerdict::Unknown => "unknown",
            TimedVerdict::SearchLimit => "search_limit",
            TimedVerdict::TooLate => "too_late",
            TimedVerdict::InsufficientResource => "insufficient_resource",
            TimedVerdict::CapacityConflict => "capacity_conflict",
        }
    }
}

/// Where and why a timed case first violated its requirement; used by the VEC
/// report runner to fill `first_violation_s` and the constraint name (M6).
#[derive(Clone, Debug, PartialEq)]
pub struct TimedViolation {
    pub at_s: f64,
    pub constraint: String,
    pub detail: String,
}

/// One evaluated timed case.
#[derive(Clone, Debug, PartialEq)]
pub struct TimedOutcome {
    pub verdict: TimedVerdict,
    pub structural: Verdict,
    pub completion_s: Option<f64>,
    pub detail: String,
    pub violation: Option<TimedViolation>,
    /// Failure predictions that overlap a step's assets (PO §3.8 rule 2): the
    /// assets may be used, but the report must flag them. Not part of the
    /// replay digest.
    pub warnings: Vec<String>,
}

impl TimedOutcome {
    /// Canonical digest used by the replay check (M5.5).
    pub fn digest(&self) -> String {
        format!(
            "{}|{}|{:.3}|{}",
            self.verdict.as_str(),
            self.structural.as_str(),
            self.completion_s.unwrap_or(-1.0),
            self.detail
        )
    }
}

// ---------------------------------------------------------------------------
// Scenario
// ---------------------------------------------------------------------------

pub struct TimedScenario {
    pub model: Model,
    pub asset_timelines: BTreeMap<String, AssetTimeline>,
    pub stock_timelines: BTreeMap<String, StockTimeline>,
    pub description: String,
    /// Evidence validity in seconds; `None` only for programmatic scenarios
    /// built with [`TimedScenario::from_parts`], whose evidence never expires.
    pub evidence_validity_s: Option<f64>,
}

impl TimedScenario {
    pub fn load(path: &Path) -> Result<TimedScenario, TimedError> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| TimedError(format!("cannot read {}: {error}", path.display())))?;
        let doc: ScenarioDoc =
            toml::from_str(&text).map_err(|error| TimedError(format!("{}: {error}", path.display())))?;
        if doc.schema != TIMED_SCENARIO_SCHEMA {
            return error(format!("unsupported timed scenario schema {:?}", doc.schema));
        }
        let model_path = path.parent().unwrap_or(Path::new(".")).join(&doc.model);
        let model =
            Model::load(&model_path).map_err(|error| TimedError(format!("{}: {error}", model_path.display())))?;

        let evidence_validity_s = parse_duration(&doc.evidence_validity)?;
        if evidence_validity_s < 0.0 {
            return error("evidence_validity must not be negative");
        }

        let mut asset_timelines = BTreeMap::new();
        for timeline in &doc.asset_timelines {
            if !model.assets.contains_key(&timeline.asset) {
                return error(format!("timeline names unknown asset {:?}", timeline.asset));
            }
            let initial = parse_status(&timeline.initial)?;
            let mut built = AssetTimeline::new(timeline.asset.clone(), initial);
            built
                .set_evidence_validity(Some(evidence_validity_s))
                .map_err(|error| TimedError(format!("{}: {error}", timeline.asset)))?;
            for change in &timeline.changes {
                let at_s = parse_duration(&change.at)?;
                built
                    .schedule_change(at_s, parse_status(&change.status)?)
                    .map_err(|error| TimedError(format!("{}: {error}", timeline.asset)))?;
            }
            if let Some(prediction) = &timeline.prediction {
                let window = TimeWindow::new(parse_duration(&prediction.from)?, parse_duration(&prediction.to)?)
                    .map_err(|error| TimedError(format!("{}: {error}", timeline.asset)))?;
                built.predict_failure(window).map_err(|error| TimedError(format!("{}: {error}", timeline.asset)))?;
            }
            if asset_timelines.insert(timeline.asset.clone(), built).is_some() {
                return error(format!("duplicate timeline for asset {:?}", timeline.asset));
            }
        }

        let mut stock_timelines = BTreeMap::new();
        for timeline in &doc.stock_timelines {
            let Some(resource) = model.resources.get(&timeline.resource) else {
                return error(format!("timeline names unknown resource {:?}", timeline.resource));
            };
            let initial = parse_quantity(&timeline.initial)?;
            if initial.dim != resource.unit.dim {
                return error(format!("stock timeline for {:?} has the wrong dimension", timeline.resource));
            }
            let mut built = StockTimeline::new(timeline.resource.clone(), initial.si)
                .map_err(|error| TimedError(format!("{}: {error}", timeline.resource)))?;
            for change in &timeline.changes {
                let at_s = parse_duration(&change.at)?;
                let delta = parse_quantity(&change.delta)?;
                if delta.dim != resource.unit.dim {
                    return error(format!("stock change for {:?} has the wrong dimension", timeline.resource));
                }
                built
                    .schedule_change(at_s, delta.si)
                    .map_err(|error| TimedError(format!("{}: {error}", timeline.resource)))?;
            }
            if stock_timelines.insert(timeline.resource.clone(), built).is_some() {
                return error(format!("duplicate stock timeline for {:?}", timeline.resource));
            }
        }

        Ok(TimedScenario {
            model,
            asset_timelines,
            stock_timelines,
            description: doc.description,
            evidence_validity_s: Some(evidence_validity_s),
        })
    }

    pub fn from_parts(
        model: Model,
        asset_timelines: BTreeMap<String, AssetTimeline>,
        stock_timelines: BTreeMap<String, StockTimeline>,
    ) -> TimedScenario {
        TimedScenario { model, asset_timelines, stock_timelines, description: String::new(), evidence_validity_s: None }
    }
}

// ---------------------------------------------------------------------------
// Cases
// ---------------------------------------------------------------------------

pub struct TimedCases {
    pub cases: Vec<CaseDoc>,
}

impl TimedCases {
    pub fn load(path: &Path) -> Result<(TimedCases, PathBuf), TimedError> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| TimedError(format!("cannot read {}: {error}", path.display())))?;
        let doc: CasesDoc =
            toml::from_str(&text).map_err(|error| TimedError(format!("{}: {error}", path.display())))?;
        if doc.schema != TIMED_CASES_SCHEMA {
            return error(format!("unsupported timed cases schema {:?}", doc.schema));
        }
        let scenario_path = path.parent().unwrap_or(Path::new(".")).join(&doc.scenario);
        Ok((TimedCases { cases: doc.cases }, scenario_path))
    }
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

pub struct TimedRunner {
    scenario: TimedScenario,
}

impl TimedRunner {
    pub fn new(scenario: TimedScenario) -> TimedRunner {
        TimedRunner { scenario }
    }

    pub fn scenario(&self) -> &TimedScenario {
        &self.scenario
    }

    pub fn run(&self, case: &CaseDoc) -> Result<TimedOutcome, TimedError> {
        let start_s = parse_duration(&case.start)?;
        let model = self.patched_model_at(start_s)?;
        let goal = resolve_goal(&model, &case.goal)?;
        let deadline_s = match case.deadline.as_deref() {
            Some("prediction") => self.prediction_deadline(&goal, &model)?,
            Some(text) => Some(parse_duration(text)?),
            None => None,
        };
        let query = Query { goal, min_verification: case.min_verification, max_depth: DEFAULT_MAX_DEPTH };
        let result = graph::solve(&model, &query);
        let structural = result.verdict;
        let structural_detail = result
            .root_causes()
            .next()
            .map(|blocker| format!("{} {}", blocker.kind.as_str(), blocker.subject))
            .unwrap_or_default();

        match structural {
            Verdict::Blocked => {
                return Ok(TimedOutcome {
                    verdict: TimedVerdict::Blocked,
                    structural,
                    completion_s: None,
                    detail: structural_detail,
                    violation: None,
                    warnings: Vec::new(),
                });
            }
            Verdict::Unknown => {
                return Ok(TimedOutcome {
                    verdict: TimedVerdict::Unknown,
                    structural,
                    completion_s: None,
                    detail: structural_detail,
                    violation: None,
                    warnings: Vec::new(),
                });
            }
            Verdict::SearchLimit => {
                return Ok(TimedOutcome {
                    verdict: TimedVerdict::SearchLimit,
                    structural,
                    completion_s: None,
                    detail: structural_detail,
                    violation: None,
                    warnings: Vec::new(),
                });
            }
            Verdict::Candidate => {}
        }

        let report = schedule_route(&model, &result, start_s)
            .map_err(|error| TimedError(format!("case {}: {error}", case.id)))?;
        let warnings = self.prediction_warnings(&result.steps, &report.instances)?;
        if !report.is_feasible() {
            let conflict = &report.conflicts[0];
            let detail = format!("{} {} vs {}", conflict.asset, conflict.first, conflict.second);
            return Ok(TimedOutcome {
                verdict: TimedVerdict::CapacityConflict,
                structural,
                completion_s: None,
                detail: detail.clone(),
                violation: Some(TimedViolation {
                    at_s: conflict.overlap.from_s,
                    constraint: conflict.asset.clone(),
                    detail,
                }),
                warnings,
            });
        }
        let completion_s = report.instances.iter().map(|instance| instance.end_s).fold(start_s, f64::max);

        let starts: BTreeMap<&str, f64> =
            report.instances.iter().map(|instance| (instance.id.as_str(), instance.start_s)).collect();
        let mut consumed: BTreeMap<&str, f64> = BTreeMap::new();
        for (position, step) in result.steps.iter().enumerate() {
            let id = format!("step{}-{}", position + 1, step.process);
            let step_start = starts.get(id.as_str()).copied().unwrap_or(start_s);
            for allocation in &step.allocations {
                let Some(timeline) = self.scenario.stock_timelines.get(&allocation.resource) else {
                    continue;
                };
                let total = consumed.entry(allocation.resource.as_str()).or_insert(0.0);
                *total += allocation.si;
                let available = timeline.available_at(step_start);
                if *total > available + 1e-6 {
                    let detail = format!(
                        "{} needs {}, only {} available at {:.0}s",
                        allocation.resource,
                        fmt_quantity(&allocation.resource, &self.scenario.model, *total),
                        fmt_quantity(&allocation.resource, &self.scenario.model, available),
                        step_start
                    );
                    return Ok(TimedOutcome {
                        verdict: TimedVerdict::InsufficientResource,
                        structural,
                        completion_s: Some(completion_s),
                        detail: detail.clone(),
                        violation: Some(TimedViolation {
                            at_s: step_start,
                            constraint: allocation.resource.clone(),
                            detail,
                        }),
                        warnings,
                    });
                }
            }
        }

        if let Some(deadline) = deadline_s {
            if completion_s > deadline + 1e-6 {
                let detail = format!("completion {:.0}s after deadline {:.0}s", completion_s, deadline);
                return Ok(TimedOutcome {
                    verdict: TimedVerdict::TooLate,
                    structural,
                    completion_s: Some(completion_s),
                    detail: detail.clone(),
                    violation: Some(TimedViolation { at_s: deadline, constraint: "deadline".into(), detail }),
                    warnings,
                });
            }
        }

        Ok(TimedOutcome {
            verdict: TimedVerdict::Feasible,
            structural,
            completion_s: Some(completion_s),
            detail: String::new(),
            violation: None,
            warnings,
        })
    }

    /// Flags steps whose occupied or restored assets have a failure prediction
    /// overlapping the step (PO §3.8 rule 2): the step may use them, but the
    /// report must say so.
    fn prediction_warnings(
        &self,
        steps: &[graph::Step],
        instances: &[poie_ontology::schedule::ScheduledInstance],
    ) -> Result<Vec<String>, TimedError> {
        let mut warnings = Vec::new();
        for (position, step) in steps.iter().enumerate() {
            let id = format!("step{}-{}", position + 1, step.process);
            let Some(instance) = instances.iter().find(|instance| instance.id == id) else {
                continue;
            };
            let span = TimeWindow::new(instance.start_s, instance.end_s)
                .map_err(|error| TimedError(format!("{id}: {error}")))?;
            for asset in step.occupies.iter().chain(&step.restores) {
                let Some(window) =
                    self.scenario.asset_timelines.get(asset).and_then(|t| t.prediction_overlapping(span))
                else {
                    continue;
                };
                warnings.push(format!(
                    "predicted_failure {asset} [{:.0}s, {:.0}s] overlaps {id} [{:.0}s, {:.0}s]",
                    window.from_s, window.to_s, span.from_s, span.to_s
                ));
            }
        }
        Ok(warnings)
    }

    /// The patched model and query for a case, for experiment harnesses (M5.7).
    pub fn prepared_at(&self, case: &CaseDoc) -> Result<(Model, Query), TimedError> {
        let start_s = parse_duration(&case.start)?;
        let model = self.patched_model_at(start_s)?;
        let goal = resolve_goal(&model, &case.goal)?;
        Ok((model, Query { goal, min_verification: case.min_verification, max_depth: DEFAULT_MAX_DEPTH }))
    }

    /// Compares an outcome against the pre-registered expectation.
    pub fn check(&self, case: &CaseDoc) -> Result<(TimedOutcome, Vec<String>), TimedError> {
        let outcome = self.run(case)?;
        let mut mismatches = Vec::new();
        if outcome.verdict != case.expect.verdict {
            mismatches.push(format!(
                "verdict: expected {}, got {} ({})",
                case.expect.verdict.as_str(),
                outcome.verdict.as_str(),
                outcome.detail
            ));
        }
        if let Some(limit) = case.expect.completion_le.as_deref() {
            let limit_s = parse_duration(limit)?;
            match outcome.completion_s {
                Some(completion) if completion <= limit_s + 1e-6 => {}
                Some(completion) => {
                    mismatches.push(format!("completion: expected <= {limit_s:.0}s, got {completion:.0}s"))
                }
                None => mismatches.push("completion: expected a completion time, got none".to_string()),
            }
        }
        Ok((outcome, mismatches))
    }

    /// Deadline from failure predictions: the earliest predicted failure of an
    /// asset the goal depends on. Used when a case says `deadline = "prediction"`.
    fn prediction_deadline(&self, goal: &Goal, model: &Model) -> Result<Option<f64>, TimedError> {
        let assets: Vec<&str> = match goal {
            Goal::Service(service) => {
                let Some(service) = model.services.get(service) else {
                    return error(format!("unknown service {service:?}"));
                };
                service.requires.iter().map(String::as_str).collect()
            }
            Goal::Restore(asset) => vec![asset.as_str()],
            Goal::Obtain { .. } => Vec::new(),
        };
        let mut deadline: Option<f64> = None;
        for asset in assets {
            if let Some(window) = self.scenario.asset_timelines.get(asset).and_then(AssetTimeline::predicted_failure) {
                deadline = Some(deadline.map_or(window.from_s, |current| current.min(window.from_s)));
            }
        }
        Ok(deadline)
    }

    fn patched_model_at(&self, start_s: f64) -> Result<Model, TimedError> {
        let mut patch = Patch::default();
        for (asset_id, asset) in &self.scenario.model.assets {
            // One semantics for every consumer (PO §3.8, §4; issue #111): the
            // timeline's evidence-based status, with expiry. An asset without a
            // timeline keeps its model status as evidence at t = 0.
            let status = match self.scenario.asset_timelines.get(asset_id) {
                Some(timeline) => timeline.status_at(start_s).status,
                None => {
                    let mut implicit = AssetTimeline::new(asset_id.clone(), asset.status);
                    implicit
                        .set_evidence_validity(self.scenario.evidence_validity_s)
                        .map_err(|error| TimedError(format!("{asset_id}: {error}")))?;
                    implicit.status_at(start_s).status
                }
            };
            patch.asset_status.insert(asset_id.clone(), status);
        }
        self.scenario.model.patched(&patch).map_err(|error| TimedError(format!("cannot patch model: {error}")))
    }
}

fn resolve_goal(model: &Model, goal: &GoalDoc) -> Result<Goal, TimedError> {
    Ok(match goal {
        GoalDoc::Service(service) => {
            if !model.services.contains_key(service) {
                return error(format!("unknown service {service:?}"));
            }
            Goal::Service(service.clone())
        }
        GoalDoc::Restore(asset) => {
            if !model.assets.contains_key(asset) {
                return error(format!("unknown asset {asset:?}"));
            }
            Goal::Restore(asset.clone())
        }
        GoalDoc::Obtain { resource, quantity } => {
            let Some(definition) = model.resources.get(resource) else {
                return error(format!("unknown resource {resource:?}"));
            };
            let parsed = parse_quantity(quantity)?;
            if parsed.dim != definition.unit.dim {
                return error(format!("goal quantity {quantity:?} does not match the unit of {resource:?}"));
            }
            Goal::Obtain { resource: resource.clone(), quantity: parsed }
        }
    })
}

// ---------------------------------------------------------------------------
// Replay digests
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DigestsDoc {
    schema: String,
    #[serde(rename = "digest")]
    digests: Vec<DigestEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DigestEntry {
    pub id: String,
    pub digest: String,
}

pub fn read_digests(path: &Path) -> Result<Vec<DigestEntry>, TimedError> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| TimedError(format!("cannot read {}: {error}", path.display())))?;
    let doc: DigestsDoc = toml::from_str(&text).map_err(|error| TimedError(format!("{}: {error}", path.display())))?;
    if doc.schema != TIMED_DIGESTS_SCHEMA {
        return error(format!("unsupported digests schema {:?}", doc.schema));
    }
    Ok(doc.digests)
}

pub fn write_digests(path: &Path, entries: Vec<DigestEntry>) -> Result<(), TimedError> {
    let doc = DigestsDoc { schema: TIMED_DIGESTS_SCHEMA.to_string(), digests: entries };
    let text = toml::to_string(&doc).map_err(|error| TimedError(format!("cannot serialize digests: {error}")))?;
    std::fs::write(path, text).map_err(|error| TimedError(format!("cannot write {}: {error}", path.display())))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_duration(text: &str) -> Result<f64, TimedError> {
    let quantity: Quantity = text.parse().map_err(|error| TimedError(format!("invalid time {text:?}: {error}")))?;
    if quantity.dim != Dimension::TIME {
        return error(format!("expected a time, got {text:?}"));
    }
    Ok(quantity.si)
}

fn parse_quantity(text: &str) -> Result<Quantity, TimedError> {
    text.parse().map_err(|error| TimedError(format!("invalid quantity {text:?}: {error}")))
}

fn parse_status(text: &str) -> Result<AssetStatus, TimedError> {
    Ok(match text {
        "available" => AssetStatus::Available,
        "degraded" => AssetStatus::Degraded,
        "failed" => AssetStatus::Failed,
        "unknown" => AssetStatus::Unknown,
        other => return error(format!("unknown asset status {other:?}")),
    })
}

fn fmt_quantity(resource: &str, model: &Model, amount_si: f64) -> String {
    model
        .resources
        .get(resource)
        .map(|definition| definition.fmt_amount(amount_si))
        .unwrap_or_else(|| format!("{amount_si} SI"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_path(relative: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
    }

    fn load() -> (TimedRunner, TimedCases) {
        let (cases, scenario_path) = TimedCases::load(&repo_path("../../scenarios/base-01-timed/cases.toml")).unwrap();
        let scenario = TimedScenario::load(&scenario_path).unwrap();
        (TimedRunner::new(scenario), cases)
    }

    #[test]
    fn timed_evaluation_is_deterministic() {
        let (runner, cases) = load();
        let first: Vec<String> = cases.cases.iter().map(|case| runner.run(case).unwrap().digest()).collect();
        let second: Vec<String> = cases.cases.iter().map(|case| runner.run(case).unwrap().digest()).collect();
        assert_eq!(first, second);
        assert!(first.iter().all(|digest| !digest.is_empty()));
    }

    #[test]
    fn digest_files_round_trip() {
        let path = std::env::temp_dir().join(format!(
            "poie-timed-digests-{}-{}.toml",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let entries = vec![DigestEntry { id: "T0".into(), digest: "feasible|candidate|0.000|".into() }];
        write_digests(&path, entries.clone()).unwrap();
        let read = read_digests(&path).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].id, entries[0].id);
        assert_eq!(read[0].digest, entries[0].digest);
        let _ = std::fs::remove_file(&path);
    }

    // Issue #111 (P14, reviews A-09 / B-10): the timed evaluation must use the
    // same status semantics as the timeline (PO §3.8, §4): expired evidence is
    // `unknown`, and a predicted failure is a flagged warning.

    fn case(id: &str, goal: &str, start: &str) -> CaseDoc {
        toml::from_str(&format!(
            r#"
id = "{id}"
title = "issue 111"
goal = {goal}
min_verification = "demonstrated"
start = "{start}"
[expect]
verdict = "feasible"
"#
        ))
        .unwrap()
    }

    #[test]
    fn po4_4_timed_evaluation_projects_expired_evidence_to_unknown() {
        let (runner, _) = load();
        // Every piece of evidence in the scenario is older than its validity at 5 d.
        let outcome = runner.run(&case("X1", r#"{ service = "potable-water" }"#, "120 h")).unwrap();
        assert_eq!(outcome.verdict, TimedVerdict::Unknown, "{outcome:?}");
    }

    #[test]
    fn po3_8_predicted_failure_overlapping_a_step_is_flagged() {
        let (runner, cases) = load();
        // T8 repairs the pump from 3 h to 6 h; the pump's failure is predicted for 4-8 h.
        let t8 = cases.cases.iter().find(|case| case.id == "T8").unwrap();
        let outcome = runner.run(t8).unwrap();
        assert_eq!(outcome.verdict, TimedVerdict::Feasible);
        assert!(outcome.warnings.iter().any(|warning| warning.contains("pump-p1")), "{outcome:?}");
        // T0 finishes at 3 h, before any prediction window: no warning.
        let t0 = cases.cases.iter().find(|case| case.id == "T0").unwrap();
        assert!(runner.run(t0).unwrap().warnings.is_empty());
    }

    #[test]
    fn prediction_deadlines_come_from_the_goal_assets() {
        let (runner, cases) = load();
        let case = cases.cases.iter().find(|case| case.id == "T0").unwrap();
        let outcome = runner.run(case).unwrap();
        assert_eq!(outcome.verdict, TimedVerdict::Feasible);
        assert!((outcome.completion_s.unwrap() - 3.0 * 3600.0).abs() < 1.0);
    }
}
