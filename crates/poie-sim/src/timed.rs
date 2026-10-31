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

pub const TIMED_SCENARIO_SCHEMA: &str = "poie.timed-scenario/0.1";
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

/// One evaluated timed case.
#[derive(Clone, Debug, PartialEq)]
pub struct TimedOutcome {
    pub verdict: TimedVerdict,
    pub structural: Verdict,
    pub completion_s: Option<f64>,
    pub detail: String,
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

        let mut asset_timelines = BTreeMap::new();
        for timeline in &doc.asset_timelines {
            if !model.assets.contains_key(&timeline.asset) {
                return error(format!("timeline names unknown asset {:?}", timeline.asset));
            }
            let initial = parse_status(&timeline.initial)?;
            let mut built = AssetTimeline::new(timeline.asset.clone(), initial);
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

        Ok(TimedScenario { model, asset_timelines, stock_timelines, description: doc.description })
    }

    pub fn from_parts(
        model: Model,
        asset_timelines: BTreeMap<String, AssetTimeline>,
        stock_timelines: BTreeMap<String, StockTimeline>,
    ) -> TimedScenario {
        TimedScenario { model, asset_timelines, stock_timelines, description: String::new() }
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
                });
            }
            Verdict::Unknown => {
                return Ok(TimedOutcome {
                    verdict: TimedVerdict::Unknown,
                    structural,
                    completion_s: None,
                    detail: structural_detail,
                });
            }
            Verdict::SearchLimit => {
                return Ok(TimedOutcome {
                    verdict: TimedVerdict::SearchLimit,
                    structural,
                    completion_s: None,
                    detail: structural_detail,
                });
            }
            Verdict::Candidate => {}
        }

        let report = schedule_route(&model, &result, start_s)
            .map_err(|error| TimedError(format!("case {}: {error}", case.id)))?;
        if !report.is_feasible() {
            let conflict = &report.conflicts[0];
            return Ok(TimedOutcome {
                verdict: TimedVerdict::CapacityConflict,
                structural,
                completion_s: None,
                detail: format!("{} {} vs {}", conflict.asset, conflict.first, conflict.second),
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
                    return Ok(TimedOutcome {
                        verdict: TimedVerdict::InsufficientResource,
                        structural,
                        completion_s: Some(completion_s),
                        detail: format!(
                            "{} needs {}, only {} available at {:.0}s",
                            allocation.resource,
                            fmt_quantity(&allocation.resource, &self.scenario.model, *total),
                            fmt_quantity(&allocation.resource, &self.scenario.model, available),
                            step_start
                        ),
                    });
                }
            }
        }

        if let Some(deadline) = deadline_s {
            if completion_s > deadline + 1e-6 {
                return Ok(TimedOutcome {
                    verdict: TimedVerdict::TooLate,
                    structural,
                    completion_s: Some(completion_s),
                    detail: format!("completion {:.0}s after deadline {:.0}s", completion_s, deadline),
                });
            }
        }

        Ok(TimedOutcome {
            verdict: TimedVerdict::Feasible,
            structural,
            completion_s: Some(completion_s),
            detail: String::new(),
        })
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
            let status = match self.scenario.asset_timelines.get(asset_id) {
                Some(timeline) => timeline.explicit_status_at(start_s),
                None => asset.status,
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

    #[test]
    fn prediction_deadlines_come_from_the_goal_assets() {
        let (runner, cases) = load();
        let case = cases.cases.iter().find(|case| case.id == "T0").unwrap();
        let outcome = runner.run(case).unwrap();
        assert_eq!(outcome.verdict, TimedVerdict::Feasible);
        assert!((outcome.completion_s.unwrap() - 3.0 * 3600.0).abs() < 1.0);
    }
}
