//! Scenario cases: a base model, per-case patches, a query, and the expected
//! outcome. Cases are the executable form of a scenario specification.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::graph::{
    self, BlockerKind, CandidateRoute, ExternalDependencyReport, Goal, Query, QueryResult, Verdict, DEFAULT_MAX_DEPTH,
};
use crate::model::{Model, ModelError, Patch, Verification};

pub const CASES_SCHEMA: &str = "poie.scenario-cases/0.2";

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CasesDoc {
    pub schema: String,
    /// Path of the model file, relative to the cases file.
    pub model: String,
    #[serde(rename = "case")]
    pub cases: Vec<Case>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum GoalDoc {
    Service(String),
    Restore(String),
    Obtain { resource: String, quantity: String },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedBlocker {
    pub kind: BlockerKind,
    pub subject: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expectation {
    pub verdict: Verdict,
    #[serde(default)]
    pub route_includes: Vec<String>,
    #[serde(default)]
    pub route_excludes: Vec<String>,
    /// Each must appear among the result's root causes.
    #[serde(default)]
    pub root_causes: Vec<ExpectedBlocker>,
    /// Minimum number of distinct structural candidate routes to enumerate.
    #[serde(default)]
    pub min_routes: Option<usize>,
    /// Maximum number of distinct structural candidate routes to enumerate.
    #[serde(default)]
    pub max_routes: Option<usize>,
    /// Each must appear in the query's external-dependency trace (PO §6.5).
    #[serde(default)]
    pub external_dependencies: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub goal: GoalDoc,
    pub min_verification: Verification,
    pub max_depth: Option<usize>,
    #[serde(default)]
    pub patch: Patch,
    pub expect: Expectation,
}

pub struct Scenario {
    pub path: PathBuf,
    pub model: Model,
    pub cases: Vec<Case>,
}

pub struct CaseRun<'a> {
    pub case: &'a Case,
    pub model: Model,
    pub result: QueryResult,
    /// Enumerated candidate routes, when the case asserts route counts.
    pub routes: Vec<CandidateRoute>,
    /// External-dependency trace, when the case asserts dependencies.
    pub external: Option<ExternalDependencyReport>,
    /// Empty when the result matches the expectation.
    pub mismatches: Vec<String>,
}

impl Scenario {
    pub fn load(cases_path: &Path) -> Result<Scenario, ModelError> {
        let text = std::fs::read_to_string(cases_path)
            .map_err(|e| ModelError(format!("cannot read {}: {e}", cases_path.display())))?;
        let doc: CasesDoc = toml::from_str(&text).map_err(|e| ModelError(format!("{}: {e}", cases_path.display())))?;
        if doc.schema != CASES_SCHEMA {
            return Err(ModelError(format!("unsupported schema {:?}; expected {CASES_SCHEMA:?}", doc.schema)));
        }
        let model_path = cases_path.parent().unwrap_or(Path::new(".")).join(&doc.model);
        let model = Model::load(&model_path)?;
        let mut ids = BTreeSet::new();
        for c in &doc.cases {
            if !ids.insert(c.id.as_str()) {
                return Err(ModelError(format!("duplicate case id {:?}", c.id)));
            }
        }
        Ok(Scenario { path: cases_path.to_path_buf(), model, cases: doc.cases })
    }

    pub fn run<'a>(&self, case: &'a Case) -> Result<CaseRun<'a>, ModelError> {
        let (model, query) = self.prepare(case)?;
        let result = graph::solve(&model, &query);
        let limit = case.expect.min_routes.unwrap_or(0).max(case.expect.max_routes.unwrap_or(0)).max(8);
        let routes = if case.expect.min_routes.is_some() || case.expect.max_routes.is_some() {
            if result.verdict == Verdict::Candidate {
                graph::enumerate(&model, &query, limit)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let external = if case.expect.external_dependencies.is_empty() {
            None
        } else {
            Some(graph::external_dependency_report(&model, &query, 8))
        };
        let mismatches = check(&case.expect, &result, &routes, external.as_ref());
        Ok(CaseRun { case, model, result, routes, external, mismatches })
    }

    /// Enumerates up to `limit` candidate routes for a case (PO §8.8).
    pub fn enumerate(&self, case: &Case, limit: usize) -> Result<Vec<CandidateRoute>, ModelError> {
        let (model, query) = self.prepare(case)?;
        Ok(graph::enumerate(&model, &query, limit))
    }

    /// Exposes the patched model and query for an experiment harness (M3.2).
    pub fn prepared(&self, case: &Case) -> Result<(Model, Query), ModelError> {
        self.prepare(case)
    }

    /// Traces the external dependencies of a case (PO §6.5, VEC §4.2).
    pub fn external_dependency_report(
        &self,
        case: &Case,
        limit: usize,
    ) -> Result<ExternalDependencyReport, ModelError> {
        let (model, query) = self.prepare(case)?;
        Ok(graph::external_dependency_report(&model, &query, limit))
    }

    fn prepare(&self, case: &Case) -> Result<(Model, Query), ModelError> {
        let ctx = |e: ModelError| ModelError(format!("case {}: {e}", case.id));
        let model = self.model.patched(&case.patch).map_err(ctx)?;
        let goal = resolve_goal(&model, &case.goal).map_err(ctx)?;
        for p in case.expect.route_includes.iter().chain(&case.expect.route_excludes) {
            if model.process(p).is_none() {
                return Err(ctx(ModelError(format!("expectation names unknown process {p:?}"))));
            }
        }
        let query = Query {
            goal,
            min_verification: case.min_verification,
            max_depth: case.max_depth.unwrap_or(DEFAULT_MAX_DEPTH),
        };
        Ok((model, query))
    }
}

fn resolve_goal(model: &Model, goal: &GoalDoc) -> Result<Goal, ModelError> {
    Ok(match goal {
        GoalDoc::Service(s) => {
            if !model.services.contains_key(s) {
                return Err(ModelError(format!("goal names unknown service {s:?}")));
            }
            Goal::Service(s.clone())
        }
        GoalDoc::Restore(a) => {
            if !model.assets.contains_key(a) {
                return Err(ModelError(format!("goal names unknown asset {a:?}")));
            }
            Goal::Restore(a.clone())
        }
        GoalDoc::Obtain { resource, quantity } => {
            let Some(r) = model.resources.get(resource) else {
                return Err(ModelError(format!("goal names unknown resource {resource:?}")));
            };
            let q: crate::units::Quantity = quantity.parse().map_err(|e| ModelError(format!("goal quantity: {e}")))?;
            if q.dim != r.unit.dim {
                return Err(ModelError(format!("goal quantity {quantity:?} does not match unit of {resource}")));
            }
            Goal::Obtain { resource: resource.clone(), quantity: q }
        }
    })
}

fn check(
    expect: &Expectation,
    result: &QueryResult,
    routes: &[CandidateRoute],
    external: Option<&ExternalDependencyReport>,
) -> Vec<String> {
    let mut out = Vec::new();
    if !expect.external_dependencies.is_empty() {
        let found: Vec<String> =
            external.map(|report| report.resources().iter().map(|s| s.to_string()).collect()).unwrap_or_default();
        for dependency in &expect.external_dependencies {
            if !found.contains(dependency) {
                out.push(format!("missing external dependency {dependency}; found {found:?}"));
            }
        }
    }
    if let Some(min) = expect.min_routes {
        if routes.len() < min {
            out.push(format!("expected at least {min} candidate routes, found {}", routes.len()));
        }
    }
    if let Some(max) = expect.max_routes {
        if routes.len() > max {
            out.push(format!("expected at most {max} candidate routes, found {}", routes.len()));
        }
    }
    if expect.verdict != result.verdict {
        out.push(format!("verdict: expected {}, got {}", expect.verdict.as_str(), result.verdict.as_str()));
    }
    let route: BTreeSet<&str> = result.steps.iter().map(|s| s.process.as_str()).collect();
    for p in &expect.route_includes {
        if !route.contains(p.as_str()) {
            out.push(format!("route should include {p}"));
        }
    }
    for p in &expect.route_excludes {
        if route.contains(p.as_str()) {
            out.push(format!("route should not include {p}"));
        }
    }
    for e in &expect.root_causes {
        if !result.root_causes().any(|b| b.kind == e.kind && b.subject == e.subject) {
            out.push(format!("missing root cause {} {}", e.kind.as_str(), e.subject));
        }
    }
    out
}
