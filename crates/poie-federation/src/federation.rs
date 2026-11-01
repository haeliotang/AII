//! Federation: log exchange and reconnect reconciliation (M12.1, M12.3, M12.4).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use poie_state::action::ActionState;
use poie_state::record::{Payload, Record, RecordId};

use crate::site::{FederationError, Site};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictKind {
    DuplicateIdempotencyKey,
    DoubleDispatch,
    OverReservation,
    DoubleOwnership,
    SequenceCollision,
}

impl ConflictKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ConflictKind::DuplicateIdempotencyKey => "duplicate_idempotency_key",
            ConflictKind::DoubleDispatch => "double_dispatch",
            ConflictKind::OverReservation => "over_reservation",
            ConflictKind::DoubleOwnership => "double_ownership",
            ConflictKind::SequenceCollision => "sequence_collision",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    pub kind: ConflictKind,
    pub detail: String,
    pub records: Vec<RecordId>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExchangeReport {
    pub transferred_ab: usize,
    pub transferred_ba: usize,
    pub duplicates_skipped: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReconciliationReport {
    pub conflicts: Vec<Conflict>,
    pub ordered_records: usize,
    pub duplicate_actions: usize,
    pub reservations: usize,
    pub leases: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReservationClaim {
    pub site: String,
    pub batch: String,
    pub action_id: String,
    pub amount_si: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LeaseClaim {
    pub site: String,
    pub executor: String,
    pub from_ms: i64,
    pub until_ms: i64,
}

pub struct Federation {
    sites: BTreeMap<String, Site>,
    reservations: Vec<ReservationClaim>,
    leases: Vec<LeaseClaim>,
}

impl Default for Federation {
    fn default() -> Self {
        Federation::new()
    }
}

impl Federation {
    pub fn new() -> Federation {
        Federation { sites: BTreeMap::new(), reservations: Vec::new(), leases: Vec::new() }
    }

    pub fn add_site(&mut self, name: impl Into<String>, start_ms: i64) -> Result<(), FederationError> {
        let name = name.into();
        self.sites.insert(name.clone(), Site::new(name, start_ms));
        Ok(())
    }

    pub fn site(&self, name: &str) -> Option<&Site> {
        self.sites.get(name)
    }

    pub fn site_mut(&mut self, name: &str) -> Result<&mut Site, FederationError> {
        self.sites.get_mut(name).ok_or_else(|| FederationError::UnknownSite(name.to_string()))
    }

    pub fn append(&mut self, site: &str, record: Record) -> Result<(), FederationError> {
        self.site_mut(site)?.append_local(record)
    }

    pub fn reserve(&mut self, claim: ReservationClaim) {
        self.reservations.push(claim);
    }

    pub fn grant_lease(&mut self, claim: LeaseClaim) {
        self.leases.push(claim);
    }

    /// Reconnection: exchanges both logs, deduplicating by record id, then
    /// advances each site's HLC past the received records.
    pub fn exchange(&mut self, left: &str, right: &str) -> Result<ExchangeReport, FederationError> {
        if !self.sites.contains_key(left) {
            return Err(FederationError::UnknownSite(left.to_string()));
        }
        if !self.sites.contains_key(right) {
            return Err(FederationError::UnknownSite(right.to_string()));
        }
        let left_records = self.sites[left].records().to_vec();
        let right_records = self.sites[right].records().to_vec();
        let transferred_ab = {
            let target = self.sites.get_mut(right).expect("checked");
            target.merge(&left_records)
        };
        let transferred_ba = {
            let target = self.sites.get_mut(left).expect("checked");
            target.merge(&right_records)
        };
        for record in left_records.iter().chain(right_records.iter()) {
            for site in self.sites.values_mut() {
                site.clock.observe(&record.envelope.hlc, record.envelope.recorded_time.0);
            }
        }
        let duplicates_skipped =
            left_records.len().saturating_sub(transferred_ab) + right_records.len().saturating_sub(transferred_ba);
        Ok(ExchangeReport { transferred_ab, transferred_ba, duplicates_skipped })
    }

    /// All records from all sites, deduplicated and causally ordered by HLC
    /// (physical, logical), then site and sequence.
    pub fn merged(&self) -> Vec<Record> {
        let mut seen: BTreeSet<RecordId> = BTreeSet::new();
        let mut records: Vec<Record> = Vec::new();
        for site in self.sites.values() {
            for record in site.records() {
                if seen.insert(record.envelope.record_id.clone()) {
                    records.push(record.clone());
                }
            }
        }
        records.sort_by(|left, right| {
            left.envelope
                .hlc
                .physical_ms
                .cmp(&right.envelope.hlc.physical_ms)
                .then_with(|| left.envelope.hlc.logical.cmp(&right.envelope.hlc.logical))
                .then_with(|| left.envelope.record_id.site.cmp(&right.envelope.record_id.site))
                .then_with(|| left.envelope.record_id.seq.cmp(&right.envelope.record_id.seq))
        });
        records
    }

    /// Reconnect reconciliation. Never applies last-write-wins: every suspicious
    /// combination becomes a conflict for a human to resolve.
    pub fn reconcile(&self, available: &BTreeMap<String, f64>, skew_bound_ms: i64) -> ReconciliationReport {
        let records = self.merged();
        let mut report = ReconciliationReport {
            ordered_records: records.len(),
            reservations: self.reservations.len(),
            leases: self.leases.len(),
            ..ReconciliationReport::default()
        };

        // 0. A record id names one record (SAP §2.1). After a rollback a site can
        //    reuse `<site>/<seq>` for different content; deduplication by id
        //    would then let the logs diverge silently (issue #111, review B-06).
        let mut by_id: BTreeMap<&RecordId, Vec<(&str, &Record)>> = BTreeMap::new();
        for (name, site) in &self.sites {
            for record in site.records() {
                by_id.entry(&record.envelope.record_id).or_default().push((name.as_str(), record));
            }
        }
        for (id, copies) in by_id {
            let first = copies[0].1;
            if let Some((other_site, _)) = copies.iter().find(|(_, record)| *record != first) {
                report.conflicts.push(Conflict {
                    kind: ConflictKind::SequenceCollision,
                    detail: format!(
                        "record {}/{} has different content at {} and {other_site}: a sequence number was reused",
                        id.site, id.seq, copies[0].0
                    ),
                    records: vec![id.clone()],
                });
            }
        }

        // 1. One idempotency key must not be used by actions of two sites
        //    (SAP §6.4 rule 3). Within one site, several actions may share a
        //    key as a re-dispatch chain (SAP §3.2 rule 2); the site's own
        //    ledger enforces that chain, so it is not a conflict here.
        let mut by_key: BTreeMap<String, Vec<(&str, &str, RecordId)>> = BTreeMap::new();
        for record in &records {
            if let Payload::Action(action) = &record.payload {
                by_key.entry(action.idempotency_key.clone()).or_default().push((
                    &action.action_id,
                    &record.envelope.site,
                    record.envelope.record_id.clone(),
                ));
            }
        }
        for (key, actions) in &by_key {
            let distinct_actions: BTreeSet<&str> = actions.iter().map(|(action_id, _, _)| *action_id).collect();
            let sites: BTreeSet<&str> = actions.iter().map(|(_, site, _)| *site).collect();
            if sites.len() > 1 {
                report.duplicate_actions += 1;
                report.conflicts.push(Conflict {
                    kind: ConflictKind::DuplicateIdempotencyKey,
                    detail: format!("idempotency key {key:?} used by {} actions", distinct_actions.len()),
                    records: actions.iter().map(|(_, _, record)| record.clone()).collect(),
                });
            }
        }

        // 2. The same action must not be dispatched from two sites.
        let mut dispatches: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut dispatch_records: BTreeMap<String, Vec<RecordId>> = BTreeMap::new();
        for record in &records {
            if let Payload::Transition { action_id, to, .. } = &record.payload {
                if *to == ActionState::Dispatched {
                    dispatches.entry(action_id.clone()).or_default().insert(record.envelope.site.clone());
                    dispatch_records.entry(action_id.clone()).or_default().push(record.envelope.record_id.clone());
                }
            }
        }
        for (action_id, sites) in dispatches {
            if sites.len() > 1 {
                report.conflicts.push(Conflict {
                    kind: ConflictKind::DoubleDispatch,
                    detail: format!("action {action_id:?} dispatched by sites {:?}", sites),
                    records: dispatch_records.remove(&action_id).unwrap_or_default(),
                });
            }
        }

        // 3. Reservations must not exceed the physically available batch.
        let mut reserved: BTreeMap<&str, f64> = BTreeMap::new();
        for claim in &self.reservations {
            *reserved.entry(claim.batch.as_str()).or_insert(0.0) += claim.amount_si;
        }
        for (batch, total) in reserved {
            if let Some(available) = available.get(batch) {
                if total > available + 1e-9 {
                    report.conflicts.push(Conflict {
                        kind: ConflictKind::OverReservation,
                        detail: format!("batch {batch:?}: reserved {total}, available {available}"),
                        records: Vec::new(),
                    });
                }
            }
        }

        // 4. Command ownership: overlapping leases for one executor from
        //    different sites are double control. The assumed clock-skew bound is
        //    included conservatively by widening each interval.
        let mut by_executor: BTreeMap<&str, Vec<&LeaseClaim>> = BTreeMap::new();
        for lease in &self.leases {
            by_executor.entry(lease.executor.as_str()).or_default().push(lease);
        }
        for (executor, leases) in by_executor {
            for (index, first) in leases.iter().enumerate() {
                for second in leases.iter().skip(index + 1) {
                    if first.site == second.site {
                        continue;
                    }
                    let overlap = first.from_ms - skew_bound_ms < second.until_ms + skew_bound_ms
                        && second.from_ms - skew_bound_ms < first.until_ms + skew_bound_ms;
                    if overlap {
                        report.conflicts.push(Conflict {
                            kind: ConflictKind::DoubleOwnership,
                            detail: format!(
                                "executor {executor:?} leased by {} until {} and by {} until {} within the skew bound",
                                first.site, first.until_ms, second.site, second.until_ms
                            ),
                            records: Vec::new(),
                        });
                    }
                }
            }
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poie_state::action::{Action, Authorization, BudgetItem, Precondition, Reversibility, VerificationSpec};
    use poie_state::record::{Envelope, Hlc, Namespace, Record, RecordId, RecordKind, Source, Timestamp};

    fn envelope(site: &str, seq: u64, hlc: Hlc) -> Envelope {
        Envelope::new(
            RecordId::new(site, seq),
            RecordKind::Action,
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
            plan_ref: Some("plan-1".to_string()),
            policy_version: "policy/0.1".to_string(),
            precondition: Precondition {
                state_version: BTreeMap::from([(site.to_string(), seq)]),
                predicates: vec!["pump-p1 available".to_string()],
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
                by: "operator".to_string(),
                basis: "maintenance".to_string(),
                scope: "dispatch".to_string(),
                valid_until: Timestamp(hlc.physical_ms + 120_000),
            }),
            redispatch_of: None,
        };
        Record::new(envelope(site, seq, hlc), Payload::Action(Box::new(action))).unwrap()
    }

    fn transition_record(site: &str, seq: u64, hlc: Hlc, action_id: &str, to: ActionState) -> Record {
        let mut envelope = envelope(site, seq, hlc);
        envelope.kind = RecordKind::Transition;
        Record::new(
            envelope,
            Payload::Transition {
                action_id: action_id.to_string(),
                from: ActionState::Authorized,
                to,
                reason: "dispatch".to_string(),
                actor: format!("dispatcher-{site}"),
            },
        )
        .unwrap()
    }

    #[test]
    fn exchange_deduplicates_and_orders_records() {
        let mut federation = Federation::new();
        federation.add_site("site-a", 1_000).unwrap();
        federation.add_site("site-b", 1_000).unwrap();
        federation.append("site-a", action_record("site-a", 1, Hlc::new(1_100, 0), "a1", "key-a")).unwrap();
        federation.append("site-b", action_record("site-b", 1, Hlc::new(1_050, 0), "b1", "key-b")).unwrap();
        let report = federation.exchange("site-a", "site-b").unwrap();
        assert_eq!(report.transferred_ab, 1);
        assert_eq!(report.transferred_ba, 1);
        let merged = federation.merged();
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].envelope.record_id.site, "site-b");
        // A second exchange transfers nothing new.
        let report = federation.exchange("site-a", "site-b").unwrap();
        assert_eq!(report.transferred_ab + report.transferred_ba, 0);
        assert_eq!(report.duplicates_skipped, 4);
    }

    #[test]
    fn one_idempotency_key_used_by_two_actions_is_a_conflict() {
        let mut federation = Federation::new();
        federation.add_site("site-a", 1_000).unwrap();
        federation.add_site("site-b", 1_000).unwrap();
        federation.append("site-a", action_record("site-a", 1, Hlc::new(1_100, 0), "a1", "shared-key")).unwrap();
        federation.append("site-b", action_record("site-b", 1, Hlc::new(1_200, 0), "b1", "shared-key")).unwrap();
        federation.exchange("site-a", "site-b").unwrap();
        let report = federation.reconcile(&BTreeMap::new(), 2_000);
        assert!(report.conflicts.iter().any(|conflict| conflict.kind == ConflictKind::DuplicateIdempotencyKey));
        assert_eq!(report.duplicate_actions, 1);
    }

    #[test]
    fn double_dispatch_is_reported() {
        let mut federation = Federation::new();
        federation.add_site("site-a", 1_000).unwrap();
        federation.add_site("site-b", 1_000).unwrap();
        federation
            .append("site-a", transition_record("site-a", 1, Hlc::new(1_100, 0), "a1", ActionState::Dispatched))
            .unwrap();
        federation
            .append("site-b", transition_record("site-b", 1, Hlc::new(1_200, 0), "a1", ActionState::Dispatched))
            .unwrap();
        federation.exchange("site-a", "site-b").unwrap();
        let report = federation.reconcile(&BTreeMap::new(), 2_000);
        assert!(report.conflicts.iter().any(|conflict| conflict.kind == ConflictKind::DoubleDispatch));
    }

    #[test]
    fn over_reservation_is_reported_without_dropping_claims() {
        let mut federation = Federation::new();
        federation.add_site("site-a", 1_000).unwrap();
        federation.add_site("site-b", 1_000).unwrap();
        federation.reserve(ReservationClaim {
            site: "site-a".into(),
            batch: "lot-bronze".into(),
            action_id: "a1".into(),
            amount_si: 4.0,
        });
        federation.reserve(ReservationClaim {
            site: "site-b".into(),
            batch: "lot-bronze".into(),
            action_id: "b1".into(),
            amount_si: 3.0,
        });
        let report = federation.reconcile(&BTreeMap::from([("lot-bronze".to_string(), 5.0)]), 2_000);
        assert!(report.conflicts.iter().any(|conflict| conflict.kind == ConflictKind::OverReservation));
        assert_eq!(report.reservations, 2);
    }

    #[test]
    fn overlapping_leases_are_double_ownership_but_sequential_transfer_is_fine() {
        let mut federation = Federation::new();
        federation.add_site("site-a", 1_000).unwrap();
        federation.add_site("site-b", 1_000).unwrap();
        federation.grant_lease(LeaseClaim {
            site: "site-a".into(),
            executor: "robot-m1".into(),
            from_ms: 0,
            until_ms: 10_000,
        });
        federation.grant_lease(LeaseClaim {
            site: "site-b".into(),
            executor: "robot-m1".into(),
            from_ms: 5_000,
            until_ms: 20_000,
        });
        let report = federation.reconcile(&BTreeMap::new(), 2_000);
        assert!(report.conflicts.iter().any(|conflict| conflict.kind == ConflictKind::DoubleOwnership));

        let mut sequential = Federation::new();
        sequential.add_site("site-a", 1_000).unwrap();
        sequential.add_site("site-b", 1_000).unwrap();
        sequential.grant_lease(LeaseClaim {
            site: "site-a".into(),
            executor: "robot-m1".into(),
            from_ms: 0,
            until_ms: 10_000,
        });
        sequential.grant_lease(LeaseClaim {
            site: "site-b".into(),
            executor: "robot-m1".into(),
            from_ms: 15_000,
            until_ms: 25_000,
        });
        let report = sequential.reconcile(&BTreeMap::new(), 2_000);
        assert!(report.conflicts.is_empty());
    }

    /// Review B-04 counterexample 3 (issue #107): a legitimate same-site
    /// re-dispatch that reuses its original key (SAP §3.2 rule 2) is not a
    /// cross-site key conflict.
    #[test]
    fn sap6_4_same_site_redispatch_chain_is_not_a_key_conflict() {
        let mut federation = Federation::new();
        federation.add_site("site-a", 1_000).unwrap();
        federation.add_site("site-b", 1_000).unwrap();
        federation.append("site-a", action_record("site-a", 1, Hlc::new(1_100, 0), "a1", "k")).unwrap();
        let mut retry = action_record("site-a", 2, Hlc::new(1_200, 0), "a2", "k");
        if let Payload::Action(action) = &mut retry.payload {
            action.redispatch_of = Some("a1".to_string());
        }
        federation.append("site-a", retry).unwrap();
        federation.exchange("site-a", "site-b").unwrap();
        let report = federation.reconcile(&BTreeMap::new(), 2_000);
        assert!(
            !report.conflicts.iter().any(|conflict| conflict.kind == ConflictKind::DuplicateIdempotencyKey),
            "{:?}",
            report.conflicts
        );
        assert_eq!(report.duplicate_actions, 0);
    }

    // Issue #111 (P16, review B-06): site-a writes A/1 (`dose-1`), syncs it to
    // site-b, is restored from an empty snapshot and writes a different A/1
    // (`flush-7`). The two logs must not diverge silently.
    #[test]
    fn sap6_4_reused_sequence_with_different_content_is_a_collision() {
        let mut federation = Federation::new();
        federation.add_site("site-a", 1_000).unwrap();
        federation.add_site("site-b", 1_000).unwrap();
        federation.append("site-a", action_record("site-a", 1, Hlc::new(1_100, 0), "dose-1", "key-dose")).unwrap();
        federation.exchange("site-a", "site-b").unwrap();
        // Rollback: site-a restarts from an empty snapshot.
        federation.add_site("site-a", 1_000).unwrap();
        federation.append("site-a", action_record("site-a", 1, Hlc::new(1_300, 0), "flush-7", "key-flush")).unwrap();
        federation.exchange("site-a", "site-b").unwrap();
        let report = federation.reconcile(&BTreeMap::new(), 2_000);
        let collision = report
            .conflicts
            .iter()
            .find(|conflict| conflict.kind == ConflictKind::SequenceCollision)
            .expect("a reused record id with different content is a sequence collision");
        assert_eq!(collision.records, vec![RecordId::new("site-a", 1)]);
    }

    #[test]
    fn sap6_4_a_rolled_back_site_learns_its_high_water_mark_from_peers() {
        let mut federation = Federation::new();
        federation.add_site("site-a", 1_000).unwrap();
        federation.add_site("site-b", 1_000).unwrap();
        federation.append("site-a", action_record("site-a", 1, Hlc::new(1_100, 0), "dose-1", "key-dose")).unwrap();
        federation.exchange("site-a", "site-b").unwrap();
        federation.add_site("site-a", 1_000).unwrap();
        // Exchanging before writing restores the site's own history ...
        federation.exchange("site-a", "site-b").unwrap();
        assert_eq!(federation.site("site-a").unwrap().last_seq(), 1);
        // ... so the old sequence number cannot be reused.
        assert!(federation
            .append("site-a", action_record("site-a", 1, Hlc::new(1_300, 0), "flush-7", "key-flush"))
            .is_err());
        federation.append("site-a", action_record("site-a", 2, Hlc::new(1_300, 0), "flush-7", "key-flush")).unwrap();
        let report = federation.reconcile(&BTreeMap::new(), 2_000);
        assert!(report.conflicts.iter().all(|conflict| conflict.kind != ConflictKind::SequenceCollision));
    }
}
