//! Process instances and scheduling feasibility (M5.2; Physical Ontology §3.8 draft).
//!
//! A process instance has a start time, a duration and the assets it occupies.
//! The first scheduler is the earliest-start schedule under precedence edges and
//! unit capacity per asset: two overlapping occupations of the same asset are a
//! `capacity_conflict`, not a feasible plan. Durations come from the model, so a
//! route produced by the structural solver can be turned into a timed plan (or a
//! list of conflicts) with [`schedule_route`].

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::graph::QueryResult;
use crate::model::Model;
use crate::time::TimeWindow;

/// One execution of a process.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcessInstance {
    pub id: String,
    pub process: String,
    pub occupies: Vec<String>,
    pub duration_s: f64,
}

impl ProcessInstance {
    pub fn new(
        id: impl Into<String>,
        process: impl Into<String>,
        occupies: Vec<String>,
        duration_s: f64,
    ) -> Result<ProcessInstance, ScheduleError> {
        let id = id.into();
        if id.is_empty() {
            return Err(ScheduleError::InvalidInstance("instance id must not be empty".into()));
        }
        if !duration_s.is_finite() || duration_s < 0.0 {
            return Err(ScheduleError::InvalidInstance(format!("{id}: duration must be finite and non-negative")));
        }
        Ok(ProcessInstance { id, process: process.into(), occupies, duration_s })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScheduledInstance {
    pub id: String,
    pub start_s: f64,
    pub end_s: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapacityConflict {
    pub asset: String,
    pub first: String,
    pub second: String,
    pub overlap: TimeWindow,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ScheduleReport {
    pub instances: Vec<ScheduledInstance>,
    pub conflicts: Vec<CapacityConflict>,
}

impl ScheduleReport {
    pub fn is_feasible(&self) -> bool {
        self.conflicts.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScheduleError {
    DuplicateInstance(String),
    UnknownInstance(String),
    PrecedenceCycle(Vec<String>),
    MissingDuration(String),
    InvalidInstance(String),
}

impl fmt::Display for ScheduleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScheduleError::DuplicateInstance(id) => write!(f, "duplicate process instance {id:?}"),
            ScheduleError::UnknownInstance(id) => write!(f, "precedence names unknown instance {id:?}"),
            ScheduleError::PrecedenceCycle(ids) => write!(f, "precedence cycle among {ids:?}"),
            ScheduleError::MissingDuration(process) => {
                write!(f, "process {process:?} has no duration; a timed plan needs one")
            }
            ScheduleError::InvalidInstance(detail) => write!(f, "invalid process instance: {detail}"),
        }
    }
}

impl std::error::Error for ScheduleError {}

/// Earliest-start schedule with precedence edges and unit capacity per asset.
///
/// Predecessors must finish before a successor starts. The schedule itself may
/// contain capacity conflicts when independent instances share an asset; those
/// are reported rather than hidden by serialising everything.
pub fn earliest_start_schedule(
    instances: &[ProcessInstance],
    precedence: &[(String, String)],
) -> Result<ScheduleReport, ScheduleError> {
    let mut index: BTreeMap<&str, usize> = BTreeMap::new();
    for (position, instance) in instances.iter().enumerate() {
        if index.insert(instance.id.as_str(), position).is_some() {
            return Err(ScheduleError::DuplicateInstance(instance.id.clone()));
        }
    }
    let mut successors: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut predecessors: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut indegree: BTreeMap<&str, usize> = BTreeMap::new();
    for instance in instances {
        indegree.entry(instance.id.as_str()).or_insert(0);
    }
    for (before, after) in precedence {
        if !index.contains_key(before.as_str()) {
            return Err(ScheduleError::UnknownInstance(before.clone()));
        }
        if !index.contains_key(after.as_str()) {
            return Err(ScheduleError::UnknownInstance(after.clone()));
        }
        successors.entry(before.as_str()).or_default().push(after.as_str());
        predecessors.entry(after.as_str()).or_default().push(before.as_str());
        *indegree.entry(after.as_str()).or_insert(0) += 1;
    }

    let mut ready: BTreeSet<&str> = indegree.iter().filter(|(_, degree)| **degree == 0).map(|(id, _)| *id).collect();
    let mut scheduled: BTreeMap<&str, ScheduledInstance> = BTreeMap::new();
    while let Some(id) = ready.pop_first() {
        let instance = &instances[index[id]];
        let start = predecessors
            .get(id)
            .map_or(0.0_f64, |before| before.iter().map(|pred| scheduled[pred].end_s).fold(0.0_f64, f64::max));
        scheduled
            .insert(id, ScheduledInstance { id: id.to_string(), start_s: start, end_s: start + instance.duration_s });
        for after in successors.get(id).into_iter().flatten() {
            let degree = indegree.get_mut(after).expect("successor has an indegree");
            *degree -= 1;
            if *degree == 0 {
                ready.insert(after);
            }
        }
    }
    if scheduled.len() != instances.len() {
        let remaining: Vec<String> = instances
            .iter()
            .filter(|instance| !scheduled.contains_key(instance.id.as_str()))
            .map(|i| i.id.clone())
            .collect();
        return Err(ScheduleError::PrecedenceCycle(remaining));
    }

    let mut sorted: Vec<ScheduledInstance> = scheduled.values().cloned().collect();
    sorted.sort_by(|a, b| {
        a.start_s.partial_cmp(&b.start_s).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.id.cmp(&b.id))
    });

    let mut by_asset: BTreeMap<&str, Vec<(&str, f64, f64)>> = BTreeMap::new();
    for instance in instances {
        let window = &scheduled[instance.id.as_str()];
        for asset in &instance.occupies {
            by_asset.entry(asset.as_str()).or_default().push((instance.id.as_str(), window.start_s, window.end_s));
        }
    }
    let mut conflicts = Vec::new();
    for (asset, mut intervals) in by_asset {
        intervals.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.0.cmp(b.0)));
        for (first_index, first) in intervals.iter().enumerate() {
            for second in intervals.iter().skip(first_index + 1) {
                let start = first.1.max(second.1);
                let end = first.2.min(second.2);
                if start < end {
                    conflicts.push(CapacityConflict {
                        asset: asset.to_string(),
                        first: first.0.to_string(),
                        second: second.0.to_string(),
                        overlap: TimeWindow::new(start, end).expect("overlap window is valid"),
                    });
                }
            }
        }
    }

    Ok(ScheduleReport { instances: sorted, conflicts })
}

/// Turns a structural candidate route into instances and schedules it.
///
/// Steps are chained by precedence (the structural solver emits them in
/// dependency order); each instance lasts `duration × runs`. The whole schedule
/// is shifted by `start_s`.
pub fn schedule_route(model: &Model, result: &QueryResult, start_s: f64) -> Result<ScheduleReport, ScheduleError> {
    if !start_s.is_finite() {
        return Err(ScheduleError::InvalidInstance("start time must be finite".into()));
    }
    let mut instances = Vec::new();
    for (position, step) in result.steps.iter().enumerate() {
        let process =
            model.process(&step.process).ok_or_else(|| ScheduleError::UnknownInstance(step.process.clone()))?;
        let duration = process.duration_s.ok_or_else(|| ScheduleError::MissingDuration(step.process.clone()))?;
        let id = format!("step{}-{}", position + 1, step.process);
        instances.push(ProcessInstance::new(
            id,
            step.process.clone(),
            step.occupies.clone(),
            duration * step.runs as f64,
        )?);
    }
    let precedence: Vec<(String, String)> =
        instances.windows(2).map(|pair| (pair[0].id.clone(), pair[1].id.clone())).collect();
    let mut report = earliest_start_schedule(&instances, &precedence)?;
    for instance in &mut report.instances {
        instance.start_s += start_s;
        instance.end_s += start_s;
    }
    for conflict in &mut report.conflicts {
        conflict.overlap = TimeWindow::new(conflict.overlap.from_s + start_s, conflict.overlap.to_s + start_s)
            .expect("shifted overlap window is valid");
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{self, Goal, Query, Step, Verdict, DEFAULT_MAX_DEPTH};
    use crate::model::Verification;
    use crate::scenario::Scenario;
    use std::path::PathBuf;

    fn instance(id: &str, asset: &str, duration_s: f64) -> ProcessInstance {
        ProcessInstance::new(id, "process", vec![asset.to_string()], duration_s).unwrap()
    }

    #[test]
    fn overlapping_occupations_of_one_asset_are_conflicts() {
        let instances = vec![instance("a", "lathe-1", 60.0), instance("b", "lathe-1", 60.0)];
        let report = earliest_start_schedule(&instances, &[]).unwrap();
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].asset, "lathe-1");
        assert_eq!(report.conflicts[0].overlap, TimeWindow::new(0.0, 60.0).unwrap());
    }

    #[test]
    fn precedence_serialises_the_plan() {
        let instances = vec![instance("a", "lathe-1", 60.0), instance("b", "lathe-1", 60.0)];
        let report = earliest_start_schedule(&instances, &[("a".to_string(), "b".to_string())]).unwrap();
        assert!(report.is_feasible());
        assert_eq!(report.instances.iter().find(|i| i.id == "b").unwrap().start_s, 60.0);
    }

    #[test]
    fn cycles_and_unknown_instances_are_rejected() {
        let instances = vec![instance("a", "lathe-1", 60.0), instance("b", "lathe-1", 60.0)];
        let cycle = vec![("a".to_string(), "b".to_string()), ("b".to_string(), "a".to_string())];
        assert!(matches!(earliest_start_schedule(&instances, &cycle), Err(ScheduleError::PrecedenceCycle(_))));
        let unknown = vec![("a".to_string(), "missing".to_string())];
        assert!(matches!(earliest_start_schedule(&instances, &unknown), Err(ScheduleError::UnknownInstance(_))));
    }

    fn scenario() -> Scenario {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01/cases.toml");
        Scenario::load(&path).expect("Base-01 loads")
    }

    fn solve_case(s: &Scenario, id: &str) -> (Model, QueryResult) {
        let case = s.cases.iter().find(|case| case.id == id).unwrap();
        let run = s.run(case).unwrap();
        (run.model, run.result)
    }

    #[test]
    fn a_route_becomes_a_timed_plan() {
        let s = scenario();
        let (model, result) = solve_case(&s, "S0");
        let report = schedule_route(&model, &result, 100.0).unwrap();
        assert_eq!(report.instances.len(), 1);
        assert_eq!(report.instances[0].start_s, 100.0);
        // replace-sleeve-p1 lasts 3 h.
        assert_eq!(report.instances[0].end_s, 100.0 + 3.0 * 3600.0);
        assert!(report.is_feasible());
    }

    #[test]
    fn a_multi_step_route_is_chained_by_durations() {
        let s = scenario();
        let (model, result) = solve_case(&s, "S1");
        let report = schedule_route(&model, &result, 0.0).unwrap();
        assert_eq!(report.instances.len(), 3);
        let turn = report.instances.iter().find(|i| i.id.ends_with("turn-sleeve-p1")).unwrap();
        let inspect = report.instances.iter().find(|i| i.id.ends_with("inspect-sleeve-p1")).unwrap();
        let replace = report.instances.iter().find(|i| i.id.ends_with("replace-sleeve-p1")).unwrap();
        assert_eq!(turn.start_s, 0.0);
        assert_eq!(inspect.start_s, turn.end_s);
        assert_eq!(replace.start_s, inspect.end_s);
        assert!(report.is_feasible());
    }

    #[test]
    fn a_process_without_duration_cannot_be_scheduled() {
        let text = r#"
schema = "poie.physical-ontology/0.3"
name = "no-duration"

[[asset]]
id = "machine"
kind = "pump"
status = "degraded"

[[process]]
id = "fix"
restores = ["machine"]
verification = "declared"
"#;
        let doc: crate::model::ModelDoc = toml::from_str(text).unwrap();
        let model = Model::from_doc(doc).unwrap();
        let step = Step {
            process: "fix".to_string(),
            runs: 1,
            verification: Verification::Declared,
            allocations: Vec::new(),
            occupies: Vec::new(),
            providers: Vec::new(),
            outputs: Vec::new(),
            restores: vec!["machine".to_string()],
            degraded: Vec::new(),
        };
        let result = QueryResult { verdict: Verdict::Candidate, steps: vec![step], blockers: Vec::new() };
        assert!(matches!(schedule_route(&model, &result, 0.0), Err(ScheduleError::MissingDuration(_))));
    }

    #[test]
    fn graph_and_schedule_agree_on_the_route() {
        let s = scenario();
        let (model, result) = solve_case(&s, "S2");
        let query = Query {
            goal: Goal::Service("potable-water".to_string()),
            min_verification: Verification::Modeled,
            max_depth: DEFAULT_MAX_DEPTH,
        };
        assert_eq!(graph::solve(&model, &query).verdict, Verdict::Blocked);
        assert!(result.steps.is_empty(), "a blocked route has nothing to schedule");
    }
}
