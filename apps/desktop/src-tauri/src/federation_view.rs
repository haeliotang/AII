//! Federation snapshot from the real `poie-federation` reconciler.
//!
//! Two simulated sites exchange logs, then reconcile over-reservation,
//! duplicate idempotency keys, double dispatch and overlapping leases. Every
//! conflict is preserved; this is not last-write-wins.

use std::collections::BTreeMap;

use poie_federation::federation::{Conflict, Federation, LeaseClaim, ReservationClaim};
use poie_federation::site::Site;
use poie_state::action::{Action, Authorization, BudgetItem, Precondition, Reversibility, VerificationSpec};
use poie_state::record::{Envelope, Hlc, Namespace, Payload, Record, RecordId, RecordKind, Source, Timestamp};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FederationSiteView {
    pub id: String,
    pub status: String,
    pub hlc: String,
    pub records: usize,
    pub reservations: usize,
    pub leases: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FederationConflictView {
    pub kind: String,
    pub subject: String,
    pub detail: String,
    pub sites: Vec<String>,
    pub records: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FederationSnapshot {
    pub sites: Vec<FederationSiteView>,
    pub conflicts: Vec<FederationConflictView>,
    pub merged_records: usize,
    pub duplicate_actions: usize,
    pub reservations: usize,
    pub leases: usize,
    pub transferred_ab: usize,
    pub transferred_ba: usize,
    pub duplicates_skipped: usize,
}

fn envelope(site: &str, seq: u64, hlc: Hlc, kind: RecordKind) -> Envelope {
    Envelope::new(
        RecordId::new(site, seq),
        kind,
        "pump-p1",
        Source::Person(format!("operator-{site}")),
        Timestamp(hlc.physical_ms),
        Timestamp(hlc.physical_ms),
        hlc,
        Namespace::live(),
    )
}

fn action_record(site: &str, seq: u64, hlc: Hlc, action_id: &str, key: &str) -> Record {
    let action = Action {
        action_id: action_id.to_string(),
        idempotency_key: key.to_string(),
        target: "pump-p1".to_string(),
        command: "replace-sleeve-p1".to_string(),
        plan_ref: Some("plan-base01".to_string()),
        policy_version: "PO/SAP 2026-11-01".to_string(),
        precondition: Precondition {
            state_version: BTreeMap::from([(site.to_string(), 1)]),
            predicates: vec!["pump-p1 accessible".to_string()],
        },
        valid_from: Timestamp(hlc.physical_ms),
        valid_until: Timestamp(hlc.physical_ms + 60_000),
        budget: vec![BudgetItem { resource: "sleeve-p1".to_string(), amount_si: 1.0 }],
        reservations: Vec::new(),
        reversibility: Reversibility::Idempotent,
        verification_spec: VerificationSpec {
            observe: "pump status".to_string(),
            condition: "pump available".to_string(),
            window_ms: 60_000,
        },
        authorization: Some(Authorization {
            by: "maintainer".to_string(),
            basis: "preview".to_string(),
            scope: "dispatch".to_string(),
            valid_until: Timestamp(hlc.physical_ms + 120_000),
        }),
        redispatch_of: None,
    };
    Record::new(envelope(site, seq, hlc, RecordKind::Action), Payload::Action(Box::new(action)))
        .expect("valid action record")
}

fn dispatch_record(site: &str, seq: u64, hlc: Hlc, action_id: &str) -> Record {
    Record::new(
        envelope(site, seq, hlc, RecordKind::Transition),
        Payload::Transition {
            action_id: action_id.to_string(),
            from: poie_state::action::ActionState::Authorized,
            to: poie_state::action::ActionState::Dispatched,
            reason: "preview dispatch".to_string(),
            actor: format!("dispatcher-{site}"),
        },
    )
    .expect("valid transition record")
}

fn subject(conflict: &Conflict) -> String {
    if let Some(record) = conflict.records.first() {
        return format!("{}/{}", record.site, record.seq);
    }
    conflict.detail.split('"').nth(1).map(str::to_string).unwrap_or_else(|| conflict.kind.as_str().to_string())
}

fn aggregate_site(site: &Site, reservations: &[ReservationClaim], leases: &[LeaseClaim]) -> FederationSiteView {
    let hlc = site.clock.last();
    FederationSiteView {
        id: site.name.clone(),
        status: "online".to_string(),
        hlc: format!("{:08x}.{:04}", hlc.physical_ms, hlc.logical),
        records: site.records().len(),
        reservations: reservations.iter().filter(|claim| claim.site == site.name).count(),
        leases: leases.iter().filter(|claim| claim.site == site.name).count(),
    }
}

pub fn snapshot() -> Result<FederationSnapshot, String> {
    let mut federation = Federation::new();
    federation.add_site("site-a", 1_000).map_err(|error| error.to_string())?;
    federation.add_site("site-b", 1_000).map_err(|error| error.to_string())?;

    let action_id = "potable-water-replace-sleeve-p1";
    let shared_key = "shared-idempotency-key";
    federation
        .append("site-a", action_record("site-a", 1, Hlc::new(1_100, 0), action_id, shared_key))
        .map_err(|error| error.to_string())?;
    federation
        .append("site-a", dispatch_record("site-a", 2, Hlc::new(1_200, 0), action_id))
        .map_err(|error| error.to_string())?;
    federation
        .append("site-b", action_record("site-b", 1, Hlc::new(1_150, 0), action_id, shared_key))
        .map_err(|error| error.to_string())?;
    federation
        .append("site-b", dispatch_record("site-b", 2, Hlc::new(1_250, 0), action_id))
        .map_err(|error| error.to_string())?;

    let reservations = vec![
        ReservationClaim {
            site: "site-a".into(),
            batch: "lot-bronze".into(),
            action_id: action_id.into(),
            amount_si: 4.0,
        },
        ReservationClaim {
            site: "site-b".into(),
            batch: "lot-bronze".into(),
            action_id: action_id.into(),
            amount_si: 3.0,
        },
    ];
    for claim in &reservations {
        federation.reserve(claim.clone());
    }
    let leases = vec![
        LeaseClaim { site: "site-a".into(), executor: "robot-m1".into(), from_ms: 0, until_ms: 10_000 },
        LeaseClaim { site: "site-b".into(), executor: "robot-m1".into(), from_ms: 5_000, until_ms: 20_000 },
    ];
    for claim in &leases {
        federation.grant_lease(claim.clone());
    }

    let exchange = federation.exchange("site-a", "site-b").map_err(|error| error.to_string())?;
    let report = federation.reconcile(&BTreeMap::from([("lot-bronze".to_string(), 5.0)]), 2_000);

    let mut conflicts: Vec<FederationConflictView> = report
        .conflicts
        .iter()
        .map(|conflict| FederationConflictView {
            kind: conflict.kind.as_str().to_string(),
            subject: subject(conflict),
            detail: conflict.detail.clone(),
            sites: if conflict.records.is_empty() {
                vec!["site-a".to_string(), "site-b".to_string()]
            } else {
                conflict.records.iter().map(|record| record.site.clone()).collect()
            },
            records: conflict.records.iter().map(|record| format!("{}/{}", record.site, record.seq)).collect(),
        })
        .collect();
    conflicts.sort_by(|left, right| left.kind.cmp(&right.kind));

    let mut sites: Vec<FederationSiteView> = ["site-a", "site-b"]
        .iter()
        .filter_map(|id| federation.site(id))
        .map(|site| aggregate_site(site, &reservations, &leases))
        .collect();
    sites.sort_by(|left, right| left.id.cmp(&right.id));

    Ok(FederationSnapshot {
        sites,
        conflicts,
        merged_records: report.ordered_records,
        duplicate_actions: report.duplicate_actions,
        reservations: report.reservations,
        leases: report.leases,
        transferred_ab: exchange.transferred_ab,
        transferred_ba: exchange.transferred_ba,
        duplicates_skipped: exchange.duplicates_skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconciler_preserves_the_expected_conflicts() {
        let snapshot = snapshot().unwrap();
        assert_eq!(snapshot.sites.len(), 2);
        assert!(snapshot.merged_records >= 4);
        assert_eq!(snapshot.reservations, 2);
        assert_eq!(snapshot.leases, 2);
        assert!(snapshot.conflicts.iter().any(|conflict| conflict.kind == "duplicate_idempotency_key"));
        assert!(snapshot.conflicts.iter().any(|conflict| conflict.kind == "double_dispatch"));
        assert!(snapshot.conflicts.iter().any(|conflict| conflict.kind == "over_reservation"));
        assert!(snapshot.conflicts.iter().any(|conflict| conflict.kind == "double_ownership"));
        let json = serde_json::to_value(&snapshot).unwrap();
        assert!(json.get("mergedRecords").is_some());
        assert!(json.get("duplicateActions").is_some());
        assert!(json["conflicts"].as_array().is_some_and(|items| items.len() == 4));
    }
}
