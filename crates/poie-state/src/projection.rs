//! Versioned projections and state versions (SAP §2.3).
//!
//! A projection is a deterministic function of a record sequence, a projection
//! version and an evidence policy: given the same inputs it must produce the
//! same state. The projection result carries a state-version vector (per-site
//! maximum sequence number) that actions reference in their preconditions.
//! Evidence older than the policy's maximum age projects to `unknown` instead of
//! silently keeping the last value (Physical Ontology §4).
//!
//! Because expiry depends on the evaluation time, the state version alone does
//! not determine the state: the state identity is `(state version, projection
//! version, evidence policy, as_of)` ([`StateId`]; SAP §2.3, issue #111).

use std::collections::{BTreeMap, BTreeSet};

use poie_ontology::model::AssetStatus;

use crate::record::{ObservationValue, Payload, Record, RecordId};

/// Projection rules are versioned (SAP §2.3). Bump this whenever the rules change.
pub const PROJECTION_VERSION: &str = "poie.projection/0.1";

/// How long evidence may support a projected state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvidencePolicy {
    pub max_age_ms: u64,
}

impl EvidencePolicy {
    pub fn new(max_age_ms: u64) -> EvidencePolicy {
        EvidencePolicy { max_age_ms }
    }
}

/// A projected value, or an explicit unknown. `expired` distinguishes "evidence
/// exists but is too old" from "no evidence at all".
#[derive(Clone, Debug, PartialEq)]
pub enum Projected<T> {
    Known(T),
    Unknown { expired: bool },
}

/// The projected state plus the state version the projection was computed on.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectedState {
    pub assets: BTreeMap<String, Projected<AssetStatus>>,
    pub lots: BTreeMap<String, Projected<f64>>,
    pub state_version: BTreeMap<String, u64>,
    pub projection_version: String,
    /// Evaluation time of the projection (evidence age is measured against it).
    pub as_of_ms: i64,
    /// The evidence policy the projection applied.
    pub evidence_policy: EvidencePolicy,
}

/// Everything that determines a projected state (SAP §2.3 rule 2): the same
/// identity over the same records always yields the same state. References to
/// "the state" — action preconditions, VEC `initial_state` — must use the full
/// identity, not the state version alone.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StateId {
    pub state_version: BTreeMap<String, u64>,
    pub projection_version: String,
    pub evidence_max_age_ms: u64,
    pub as_of_ms: i64,
}

impl ProjectedState {
    pub fn state_id(&self) -> StateId {
        StateId {
            state_version: self.state_version.clone(),
            projection_version: self.projection_version.clone(),
            evidence_max_age_ms: self.evidence_policy.max_age_ms,
            as_of_ms: self.as_of_ms,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Evidence {
    Status(AssetStatus),
    Quantity(f64),
}

/// Projects a record sequence into asset statuses and lot quantities.
///
/// - only `live` namespace records participate (simulation branches are isolated, SAP §7);
/// - a record that is referenced by a `correction` no longer contributes state;
/// - the latest contributing observation/estimate per subject wins (log order);
/// - evidence older than `policy.max_age_ms` projects to `Unknown { expired: true }`;
/// - the state version is the per-site maximum sequence number over the live
///   records read, including superseded ones;
/// - `now_ms` is recorded as the projection's `as_of_ms` and is part of the
///   state identity ([`StateId`]).
pub fn project(records: &[Record], policy: EvidencePolicy, now_ms: i64) -> ProjectedState {
    let corrected: BTreeSet<RecordId> = records
        .iter()
        .filter_map(|record| match &record.payload {
            Payload::Correction { corrected, .. } => Some(corrected.clone()),
            _ => None,
        })
        .collect();

    let mut state_version: BTreeMap<String, u64> = BTreeMap::new();
    let mut evidence: BTreeMap<String, (Evidence, i64)> = BTreeMap::new();

    for record in records {
        if !record.envelope.namespace.is_live() {
            continue;
        }
        let site = record.envelope.record_id.site.clone();
        let seq = record.envelope.record_id.seq;
        state_version.entry(site).and_modify(|current| *current = (*current).max(seq)).or_insert(seq);

        if corrected.contains(&record.envelope.record_id) {
            continue;
        }
        let event_time_ms = record.envelope.event_time.0;
        let subject = record.envelope.subject.clone();
        let value = match &record.payload {
            Payload::Observation { value, .. } => match value {
                ObservationValue::Status(status) => match status.as_str() {
                    "available" => Some(Evidence::Status(AssetStatus::Available)),
                    "degraded" => Some(Evidence::Status(AssetStatus::Degraded)),
                    "failed" => Some(Evidence::Status(AssetStatus::Failed)),
                    "unknown" => Some(Evidence::Status(AssetStatus::Unknown)),
                    _ => None,
                },
                ObservationValue::Quantity(quantity) => finite(quantity.value_si).map(Evidence::Quantity),
            },
            Payload::Estimate { value, .. } => finite(value.value_si).map(Evidence::Quantity),
            _ => None,
        };
        if let Some(value) = value {
            evidence.insert(subject, (value, event_time_ms));
        }
    }

    let mut assets: BTreeMap<String, Projected<AssetStatus>> = BTreeMap::new();
    let mut lots: BTreeMap<String, Projected<f64>> = BTreeMap::new();
    for (subject, (value, event_time_ms)) in evidence {
        let expired = now_ms.saturating_sub(event_time_ms) > policy.max_age_ms as i64;
        match value {
            Evidence::Status(status) => {
                assets.insert(subject, if expired { Projected::Unknown { expired } } else { Projected::Known(status) });
            }
            Evidence::Quantity(quantity) => {
                lots.insert(subject, if expired { Projected::Unknown { expired } } else { Projected::Known(quantity) });
            }
        }
    }
    ProjectedState {
        assets,
        lots,
        state_version,
        projection_version: PROJECTION_VERSION.to_string(),
        as_of_ms: now_ms,
        evidence_policy: policy,
    }
}

fn finite(value: f64) -> Option<f64> {
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{Envelope, Hlc, Namespace, RecordKind, Source, Timestamp};

    fn envelope(id: (&str, u64), subject: &str, event_time_ms: i64, recorded_time_ms: i64) -> Envelope {
        Envelope::new(
            RecordId::new(id.0, id.1),
            RecordKind::Observation,
            subject,
            Source::Sensor("sensor-1".into()),
            Timestamp(event_time_ms),
            Timestamp(recorded_time_ms),
            Hlc::new(event_time_ms, 0),
            Namespace::live(),
        )
    }

    fn status_record(id: (&str, u64), subject: &str, status: &str, event_time_ms: i64) -> Record {
        let payload = Payload::Observation {
            value: ObservationValue::Status(status.to_string()),
            quality: crate::record::Quality::declared(None, Some("calibrated")).unwrap(),
        };
        let mut envelope = envelope(id, subject, event_time_ms, event_time_ms + 5);
        envelope.kind = RecordKind::Observation;
        Record::new(envelope, payload).unwrap()
    }

    #[test]
    fn latest_observation_wins_and_state_version_is_per_site() {
        let records = vec![
            status_record(("site-a", 1), "pump-p1", "failed", 1_000),
            status_record(("site-a", 2), "pump-p1", "degraded", 1_100),
        ];
        let state = project(&records, EvidencePolicy::new(100_000), 2_000);
        assert_eq!(state.assets["pump-p1"], Projected::Known(AssetStatus::Degraded));
        assert_eq!(state.state_version["site-a"], 2);
        assert_eq!(state.projection_version, PROJECTION_VERSION);
    }

    #[test]
    fn expired_evidence_projects_to_unknown() {
        let records = vec![status_record(("site-a", 1), "pump-p1", "available", 1_000)];
        let state = project(&records, EvidencePolicy::new(500), 10_000);
        assert_eq!(state.assets["pump-p1"], Projected::Unknown { expired: true });
    }

    #[test]
    fn missing_evidence_is_not_listed_and_consumers_treat_it_as_unknown() {
        let state = project(&[], EvidencePolicy::new(500), 1_000);
        assert!(state.assets.is_empty() && state.lots.is_empty() && state.state_version.is_empty());
    }

    #[test]
    fn a_correction_removes_the_corrected_record() {
        let mut records = vec![status_record(("site-a", 1), "pump-p1", "failed", 1_000)];
        let correction = Payload::Correction {
            corrected: RecordId::new("site-a", 1),
            reason: "sensor was miscalibrated".to_string(),
        };
        let mut envelope = envelope(("site-a", 2), "pump-p1", 1_050, 1_055);
        envelope.kind = RecordKind::Correction;
        records.push(Record::new(envelope, correction).unwrap());
        let state = project(&records, EvidencePolicy::new(100_000), 2_000);
        assert!(!state.assets.contains_key("pump-p1"));
        assert_eq!(state.state_version["site-a"], 2);
    }

    #[test]
    fn simulation_records_are_isolated_from_live_state() {
        let mut record = status_record(("site-a", 1), "pump-p1", "available", 1_000);
        record.envelope.namespace = Namespace::simulation("branch-1").unwrap();
        let state = project(&[record], EvidencePolicy::new(100_000), 2_000);
        assert!(state.assets.is_empty());
        assert!(state.state_version.is_empty());
    }

    #[test]
    fn projections_are_deterministic() {
        let records = vec![
            status_record(("site-a", 1), "pump-p1", "failed", 1_000),
            status_record(("site-b", 1), "pump-p1", "degraded", 1_010),
        ];
        let first = project(&records, EvidencePolicy::new(100_000), 2_000);
        let second = project(&records, EvidencePolicy::new(100_000), 2_000);
        assert_eq!(first, second);
    }

    #[test]
    fn quantity_observations_project_lot_quantities() {
        let payload = Payload::Observation {
            value: ObservationValue::Quantity(crate::record::Quantity { value_si: 2.5, unit: "kg".into() }),
            quality: crate::record::Quality::declared(Some(0.01), None).unwrap(),
        };
        let mut envelope = envelope(("site-a", 5), "lot-bronze", 1_000, 1_002);
        envelope.kind = RecordKind::Observation;
        let record = Record::new(envelope, payload).unwrap();
        let state = project(&[record], EvidencePolicy::new(100_000), 2_000);
        assert_eq!(state.lots["lot-bronze"], Projected::Known(2.5));
    }

    // Issue #111 (P15, review B-08): the same records, projection version and
    // state version give different states at 08:30 and 09:10, so the state
    // identity must include the evaluation time `as_of` (SAP §2.3).
    #[test]
    fn sap2_3_state_identity_includes_the_evaluation_time() {
        let hour = 3_600_000;
        let records = vec![status_record(("site-a", 120), "pump-p1", "available", 8 * hour)];
        let policy = EvidencePolicy::new(hour as u64);
        let at_0830 = project(&records, policy, 8 * hour + hour / 2);
        let at_0910 = project(&records, policy, 9 * hour + hour / 6);
        assert_eq!(at_0830.state_version, at_0910.state_version);
        assert_ne!(at_0830.assets, at_0910.assets);
        assert_eq!(at_0830.as_of_ms, 8 * hour + hour / 2);
        assert_ne!(at_0830.state_id(), at_0910.state_id(), "different states need different identities");
        // Same inputs, same identity, same state (determinism, rule 1).
        assert_eq!(at_0830.state_id(), project(&records, policy, 8 * hour + hour / 2).state_id());
        assert_eq!(at_0830.state_id().evidence_max_age_ms, hour as u64);
    }
}
