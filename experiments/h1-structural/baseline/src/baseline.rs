//! The per-device baseline defined in `PREREG.md` §2.
//!
//! Deliberately shallow: for each required asset it looks at the asset's own
//! status and, if it must be restored, at the direct inputs, direct occupancy
//! and capability providers of its restoring processes. Nothing is recursive,
//! no ledger is kept, and stock is only summed over accepted lots.

use std::collections::BTreeSet;

use poie_ontology::graph::{Goal, Query, Verdict};
use poie_ontology::model::{AssetStatus, LotQuality, Model, Verification};
use poie_ontology::units;

#[derive(Clone, Debug, PartialEq)]
pub struct Blocker {
    pub kind: &'static str,
    pub subject: String,
}

#[derive(Clone, Debug)]
pub struct BaselineResult {
    pub verdict: Verdict,
    pub blockers: Vec<Blocker>,
}

impl BaselineResult {
    fn feasible() -> Self {
        BaselineResult { verdict: Verdict::Candidate, blockers: Vec::new() }
    }

    fn blocked(blockers: Vec<Blocker>) -> Self {
        BaselineResult { verdict: Verdict::Blocked, blockers }
    }

    fn unknown(subject: &str) -> Self {
        BaselineResult {
            verdict: Verdict::Unknown,
            blockers: vec![Blocker { kind: "unknown_state", subject: subject.to_string() }],
        }
    }
}

/// Runs the baseline against a query.
pub fn check(model: &Model, query: &Query) -> BaselineResult {
    match &query.goal {
        Goal::Service(service) => check_service(model, service, query.min_verification),
        Goal::Restore(asset) => check_asset(model, asset, query.min_verification),
        Goal::Obtain { resource, quantity } => {
            let available = accepted_stock(model, resource);
            if units::approx_le(quantity.si, available) {
                BaselineResult::feasible()
            } else {
                BaselineResult::blocked(vec![Blocker { kind: "missing_resource", subject: resource.clone() }])
            }
        }
    }
}

fn check_service(model: &Model, service: &str, min: Verification) -> BaselineResult {
    let Some(service) = model.services.get(service) else {
        return BaselineResult::blocked(vec![Blocker { kind: "unknown_service", subject: service.to_string() }]);
    };
    let mut blockers = Vec::new();
    let mut unknown = false;
    for asset in &service.requires {
        let result = check_asset(model, asset, min);
        match result.verdict {
            Verdict::Candidate => {}
            Verdict::Unknown => {
                unknown = true;
                blockers.extend(result.blockers);
            }
            _ => blockers.extend(result.blockers),
        }
    }
    if unknown {
        BaselineResult { verdict: Verdict::Unknown, blockers }
    } else if blockers.is_empty() {
        BaselineResult::feasible()
    } else {
        BaselineResult::blocked(blockers)
    }
}

fn check_asset(model: &Model, asset: &str, min: Verification) -> BaselineResult {
    let Some(a) = model.assets.get(asset) else {
        return BaselineResult::blocked(vec![Blocker { kind: "unknown_asset", subject: asset.to_string() }]);
    };
    match a.status {
        AssetStatus::Available => BaselineResult::feasible(),
        AssetStatus::Unknown => BaselineResult::unknown(asset),
        AssetStatus::Degraded | AssetStatus::Failed => {
            let restorers = model.restorers.get(asset).map(Vec::as_slice).unwrap_or(&[]);
            if restorers.is_empty() {
                return BaselineResult::blocked(vec![Blocker {
                    kind: "asset_unavailable",
                    subject: asset.to_string(),
                }]);
            }
            let mut blockers = Vec::new();
            for &index in restorers {
                let process = &model.processes[index];
                if process.verification < min {
                    blockers.push(Blocker { kind: "below_verification", subject: process.id.clone() });
                    continue;
                }
                let mut process_ok = true;
                for flow in &process.inputs {
                    let available = accepted_stock(model, &flow.resource);
                    if !units::approx_le(flow.si, available) {
                        blockers.push(Blocker { kind: "missing_resource", subject: flow.resource.clone() });
                        process_ok = false;
                    }
                }
                for occupied in &process.occupies {
                    if model.assets[occupied].status != AssetStatus::Available {
                        blockers.push(Blocker { kind: "asset_unavailable", subject: occupied.clone() });
                        process_ok = false;
                    }
                }
                for capability in &process.requires_capabilities {
                    let providers = model.providers.get(capability).map(Vec::as_slice).unwrap_or(&[]);
                    if !providers.iter().any(|provider| model.assets[provider].status == AssetStatus::Available) {
                        blockers.push(Blocker { kind: "no_capability_provider", subject: capability.clone() });
                        process_ok = false;
                    }
                }
                if process_ok {
                    return BaselineResult::feasible();
                }
            }
            let mut seen = BTreeSet::new();
            blockers.retain(|blocker| seen.insert((blocker.kind, blocker.subject.clone())));
            BaselineResult::blocked(blockers)
        }
    }
}

fn accepted_stock(model: &Model, resource: &str) -> f64 {
    model
        .lots
        .iter()
        .filter(|lot| lot.resource == resource && lot.quality == LotQuality::Accepted)
        .map(|lot| lot.si)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use poie_ontology::model::Model;

    fn model(body: &str) -> Model {
        let text = format!(
            r#"
schema = "poie.physical-ontology/0.2"
name = "baseline-test"
{body}
"#
        );
        poie_ontology::model::Model::from_doc(toml::from_str(&text).unwrap()).unwrap()
    }

    fn restore_query(asset: &str) -> Query {
        Query { goal: Goal::Restore(asset.to_string()), min_verification: Verification::Declared, max_depth: 32 }
    }

    const MACHINE: &str = r#"
[[resource]]
id = "part"
unit = "item"

[[asset]]
id = "machine"
kind = "pump"
status = "degraded"

[[asset]]
id = "robot"
kind = "robot"
status = "available"
provides = ["repair"]

[[process]]
id = "fix"
inputs = [{ resource = "part", quantity = "1 item" }]
restores = ["machine"]
requires_capabilities = ["repair"]
verification = "declared"
"#;

    #[test]
    fn available_asset_is_feasible() {
        let m = model(MACHINE);
        let result = check(&m, &restore_query("robot"));
        assert_eq!(result.verdict, Verdict::Candidate);
    }

    #[test]
    fn missing_direct_input_blocks() {
        let m = model(MACHINE);
        let result = check(&m, &restore_query("machine"));
        assert_eq!(result.verdict, Verdict::Blocked);
        assert!(result.blockers.iter().any(|b| b.kind == "missing_resource" && b.subject == "part"));
    }

    #[test]
    fn baseline_does_not_recurse_into_provider_supports() {
        // charger depends on the failed bus, but the baseline only looks at
        // the provider's direct status, so it reports the repair as feasible.
        let body = r#"
[[asset]]
id = "charger"
kind = "charger"
status = "available"
requires = ["bus"]

[[asset]]
id = "robot"
kind = "robot"
status = "available"
provides = ["repair"]
requires = ["charger"]

[[asset]]
id = "bus"
kind = "power-bus"
status = "failed"

[[process]]
id = "fix"
restores = ["bus"]
requires_capabilities = ["repair"]
verification = "declared"
"#;
        let m = model(body);
        let result = check(&m, &restore_query("bus"));
        assert_eq!(result.verdict, Verdict::Candidate, "baseline must not see the bus-cycle: {result:?}");
    }

    #[test]
    fn unknown_is_reported_as_unknown() {
        let body = r#"
[[asset]]
id = "sensor"
kind = "sensor"
status = "unknown"
"#;
        let m = model(body);
        let result = check(&m, &restore_query("sensor"));
        assert_eq!(result.verdict, Verdict::Unknown);
    }

    #[test]
    fn obtain_only_checks_stock() {
        let body = r#"
[[resource]]
id = "part"
unit = "item"

[[lot]]
id = "lot-part"
resource = "part"
quantity = "1 item"
quality = "accepted"

[[lot]]
id = "lot-quarantined"
resource = "part"
quantity = "5 item"
quality = "quarantined"
"#;
        let m = model(body);
        let query = Query {
            goal: Goal::Obtain { resource: "part".into(), quantity: "2 item".parse().unwrap() },
            min_verification: Verification::Declared,
            max_depth: 32,
        };
        let result = check(&m, &query);
        assert_eq!(result.verdict, Verdict::Blocked, "quarantined stock must not count");
    }
}
