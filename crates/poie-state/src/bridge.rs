//! Bridge from SAP projections to the Physical Ontology model (SAP §8; M4.9).
//!
//! The ontology's asset `status` is derived from observation/estimate projections
//! (SAP §2.3). This module turns a `ProjectedState` into a validated asset-status
//! patch for a model, so capability-graph queries read projected state instead
//! of hand-set statuses. Assets without valid evidence become `unknown` — missing
//! or expired evidence is never treated as healthy (Physical Ontology §4).

use poie_ontology::model::{AssetStatus, Model, ModelError, Patch};

use crate::projection::{Projected, ProjectedState};

/// Builds an asset-status patch from a projection.
pub fn asset_status_patch(model: &Model, state: &ProjectedState) -> Patch {
    let mut patch = Patch::default();
    for asset_id in model.assets.keys() {
        let status = match state.assets.get(asset_id) {
            Some(Projected::Known(status)) => *status,
            Some(Projected::Unknown { .. }) | None => AssetStatus::Unknown,
        };
        patch.asset_status.insert(asset_id.clone(), status);
    }
    patch
}

/// Applies projected asset statuses to a copy of the model.
pub fn patched_model(model: &Model, state: &ProjectedState) -> Result<Model, ModelError> {
    model.patched(&asset_status_patch(model, state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::{project, EvidencePolicy};
    use crate::record::{
        Envelope, Hlc, Namespace, ObservationValue, Payload, Quality, Record, RecordId, RecordKind, Source, Timestamp,
    };
    use poie_ontology::graph::{self, Goal, Query, Verdict, DEFAULT_MAX_DEPTH};
    use poie_ontology::model::Verification;
    use std::path::PathBuf;

    fn base01() -> Model {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01/model.toml");
        Model::load(&path).expect("Base-01 model loads")
    }

    fn status_record(seq: u64, asset: &str, status: &str) -> Record {
        let envelope = Envelope::new(
            RecordId::new("site-a", seq),
            RecordKind::Observation,
            asset,
            Source::Sensor("sensor-1".into()),
            Timestamp(1_000 + seq as i64),
            Timestamp(1_100 + seq as i64),
            Hlc::new(1_000 + seq as i64, 0),
            Namespace::live(),
        );
        let payload = Payload::Observation {
            value: ObservationValue::Status(status.to_string()),
            quality: Quality::declared(None, Some("synthetic")).unwrap(),
        };
        Record::new(envelope, payload).unwrap()
    }

    fn observed_model(model: &Model, override_asset: Option<(&str, &str)>, omit: Option<&str>) -> Model {
        let mut records = Vec::new();
        let mut seq = 1u64;
        for (asset_id, asset) in &model.assets {
            if omit == Some(asset_id.as_str()) {
                continue;
            }
            let status = if override_asset.map(|(id, _)| id) == Some(asset_id.as_str()) {
                override_asset.map(|(_, status)| status).unwrap_or("unknown")
            } else {
                match asset.status {
                    AssetStatus::Available => "available",
                    AssetStatus::Degraded => "degraded",
                    AssetStatus::Failed => "failed",
                    AssetStatus::Unknown => "unknown",
                }
            };
            records.push(status_record(seq, asset_id, status));
            seq += 1;
        }
        let state = project(&records, EvidencePolicy::new(600_000), 2_000);
        patched_model(model, &state).expect("projected patch applies")
    }

    #[test]
    fn projected_failure_of_the_bus_blocks_the_water_service() {
        let model = base01();
        let patched = observed_model(&model, Some(("bus-1", "failed")), None);
        assert_eq!(patched.assets["bus-1"].status, AssetStatus::Failed);

        let query = Query {
            goal: Goal::Service("potable-water".into()),
            min_verification: Verification::Demonstrated,
            max_depth: DEFAULT_MAX_DEPTH,
        };
        let result = graph::solve(&patched, &query);
        assert_eq!(result.verdict, Verdict::Blocked, "{}", graph::render(&patched, &result));
        assert!(result.root_causes().any(|blocker| blocker.subject == "bus-1"));
    }

    #[test]
    fn missing_evidence_becomes_unknown_not_available() {
        let model = base01();
        let patched = observed_model(&model, None, Some("pump-p1"));
        assert_eq!(patched.assets["pump-p1"].status, AssetStatus::Unknown);

        let query = Query {
            goal: Goal::Service("potable-water".into()),
            min_verification: Verification::Demonstrated,
            max_depth: DEFAULT_MAX_DEPTH,
        };
        let result = graph::solve(&patched, &query);
        assert_eq!(result.verdict, Verdict::Unknown, "{}", graph::render(&patched, &result));
        assert!(result.root_causes().any(|blocker| blocker.subject == "pump-p1"));
    }
}
