//! Randomized partition/reconnect invariants for the simulated federation.
//!
//! Fixed-seed property tests: exchange is idempotent, records are never lost or
//! duplicated, merged order is causal (HLC), conflicts are deterministic and
//! derived consistently, and reservations/leases are never silently dropped.

use std::collections::{BTreeMap, BTreeSet};

use poie_federation::federation::{LeaseClaim, ReservationClaim};
use poie_federation::{ConflictKind, Federation};
use poie_state::action::{
    Action, ActionState, Authorization, BudgetItem, Precondition, Reversibility, VerificationSpec,
};
use poie_state::record::{Envelope, Hlc, Namespace, Payload, Record, RecordId, RecordKind, Source, Timestamp};

const SEED: u64 = 0x706f_6965_6665_6400;
const SITES: [&str; 3] = ["site-a", "site-b", "site-c"];

#[derive(Clone, Debug)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> SplitMix64 {
        SplitMix64 { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn index(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }
}

fn action_record(site: &str, seq: u64, hlc: Hlc, action_id: &str, key: &str) -> Record {
    let action = Action {
        action_id: action_id.to_string(),
        idempotency_key: key.to_string(),
        target: "pump-p1".to_string(),
        command: "replace-sleeve-p1".to_string(),
        plan_ref: Some("plan-property".to_string()),
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
            basis: "property test".to_string(),
            scope: "dispatch".to_string(),
            valid_until: Timestamp(hlc.physical_ms + 120_000),
        }),
    };
    Record::new(
        Envelope::new(
            RecordId::new(site, seq),
            RecordKind::Action,
            "pump-p1",
            Source::Person(format!("operator-{site}")),
            Timestamp(hlc.physical_ms),
            Timestamp(hlc.physical_ms),
            hlc,
            Namespace::live(),
        ),
        Payload::Action(Box::new(action)),
    )
    .unwrap()
}

fn transition_record(site: &str, seq: u64, hlc: Hlc, action_id: &str, to: ActionState) -> Record {
    let mut envelope = Envelope::new(
        RecordId::new(site, seq),
        RecordKind::Transition,
        action_id,
        Source::Person(format!("dispatcher-{site}")),
        Timestamp(hlc.physical_ms),
        Timestamp(hlc.physical_ms),
        hlc,
        Namespace::live(),
    );
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

struct Scenario {
    federation: Federation,
    local_ids: BTreeSet<RecordId>,
    reservations: usize,
    leases: usize,
}

fn scenario(rng: &mut SplitMix64) -> Scenario {
    let mut federation = Federation::new();
    let mut local_ids = BTreeSet::new();
    let mut reservations = 0;
    let mut leases = 0;
    for (site_index, site) in SITES.iter().enumerate() {
        federation.add_site(*site, 1_000 + site_index as i64 * 10).unwrap();
        let actions = 1 + rng.index(3);
        let mut seq = 0u64;
        for action_index in 0..actions {
            seq += 1;
            let physical = 1_000 + (site_index as i64) * 10 + seq as i64;
            let key = format!("key-{}-{}", site_index, action_index);
            let record = action_record(site, seq, Hlc::new(physical, 0), &format!("{site}-a{action_index}"), &key);
            local_ids.insert(record.envelope.record_id.clone());
            federation.append(site, record).unwrap();
            if rng.unit() < 0.5 {
                seq += 1;
                let record = transition_record(
                    site,
                    seq,
                    Hlc::new(physical + 1, 0),
                    &format!("{site}-a{action_index}"),
                    ActionState::Dispatched,
                );
                local_ids.insert(record.envelope.record_id.clone());
                federation.append(site, record).unwrap();
            }
        }
        for _ in 0..rng.index(3) {
            reservations += 1;
            federation.reserve(ReservationClaim {
                site: site.to_string(),
                batch: "lot-bronze".to_string(),
                action_id: format!("{site}-reservation-{reservations}"),
                amount_si: 0.5 + rng.unit() * 3.5,
            });
        }
        for _ in 0..rng.index(2) {
            leases += 1;
            let from = 1_000 + rng.index(8_000) as i64;
            federation.grant_lease(LeaseClaim {
                site: site.to_string(),
                executor: "robot-m1".to_string(),
                from_ms: from,
                until_ms: from + 1_000 + rng.index(10_000) as i64,
            });
        }
    }
    Scenario { federation, local_ids, reservations, leases }
}

fn causal_key(record: &Record) -> (i64, u32, String, u64) {
    (
        record.envelope.hlc.physical_ms,
        record.envelope.hlc.logical,
        record.envelope.record_id.site.clone(),
        record.envelope.record_id.seq,
    )
}

fn derive_conflicts(federation: &Federation) -> BTreeMap<&'static str, usize> {
    let report = federation.reconcile(&BTreeMap::from([("lot-bronze".to_string(), 5.0)]), 2_000);
    let mut counts = BTreeMap::new();
    for conflict in &report.conflicts {
        *counts.entry(conflict.kind.as_str()).or_insert(0) += 1;
    }
    counts
}

#[test]
fn randomized_partitions_preserve_the_federation_invariants() {
    let mut rng = SplitMix64::new(SEED);
    for trial in 0..120 {
        let mut scenario = scenario(&mut rng);
        let site_names: Vec<String> = SITES.iter().map(|site| site.to_string()).collect();

        for site in &site_names {
            let site_view = scenario.federation.site(site).unwrap();
            assert!(!site_view.records().is_empty(), "trial {trial}: site {site} has no local records");
            assert!(
                site_view.records().iter().all(|record| record.envelope.record_id.site == *site),
                "trial {trial}: a site sees peer records before exchange"
            );
        }

        let mut pairs = Vec::new();
        for left in 0..site_names.len() {
            for right in (left + 1)..site_names.len() {
                pairs.push((left, right));
            }
        }
        for index in (1..pairs.len()).rev() {
            let swap = rng.index(index + 1);
            pairs.swap(index, swap);
        }
        for (left, right) in &pairs {
            scenario.federation.exchange(&site_names[*left], &site_names[*right]).unwrap();
        }

        for site in &site_names {
            let ids: BTreeSet<RecordId> = scenario.federation.site(site).unwrap().records_ids().into_iter().collect();
            assert_eq!(ids, scenario.local_ids, "trial {trial}: site {site} did not receive the union");
        }
        let merged = scenario.federation.merged();
        let unique: BTreeSet<RecordId> = merged.iter().map(|record| record.envelope.record_id.clone()).collect();
        assert_eq!(unique.len(), merged.len(), "trial {trial}: duplicate records in the merged view");
        assert_eq!(merged.len(), scenario.local_ids.len(), "trial {trial}: records lost in merge");
        for pair in merged.windows(2) {
            assert!(causal_key(&pair[0]) <= causal_key(&pair[1]), "trial {trial}: merged order is not causal");
        }

        let first = scenario.federation.reconcile(&BTreeMap::from([("lot-bronze".to_string(), 5.0)]), 2_000);
        let second = scenario.federation.reconcile(&BTreeMap::from([("lot-bronze".to_string(), 5.0)]), 2_000);
        assert_eq!(first, second, "trial {trial}: reconciliation is not deterministic");
        assert_eq!(first.reservations, scenario.reservations, "trial {trial}: reservations were dropped");
        assert_eq!(first.leases, scenario.leases, "trial {trial}: leases were dropped");
        let mut reported = BTreeMap::new();
        for conflict in &first.conflicts {
            *reported.entry(conflict.kind.as_str()).or_insert(0) += 1;
        }
        assert_eq!(derive_conflicts(&scenario.federation), reported, "trial {trial}: conflict derivation mismatch");
        for kind in reported.keys() {
            assert!(
                [
                    ConflictKind::DuplicateIdempotencyKey,
                    ConflictKind::DoubleDispatch,
                    ConflictKind::OverReservation,
                    ConflictKind::DoubleOwnership
                ]
                .iter()
                .any(|known| known.as_str() == *kind),
                "trial {trial}: unknown conflict kind {kind}"
            );
        }

        for (left, right) in &pairs {
            scenario.federation.exchange(&site_names[*left], &site_names[*right]).unwrap();
        }
        let after = scenario.federation.reconcile(&BTreeMap::from([("lot-bronze".to_string(), 5.0)]), 2_000);
        assert_eq!(
            after.ordered_records, first.ordered_records,
            "trial {trial}: repeated exchange changed the record count"
        );
        assert_eq!(after.conflicts, first.conflicts, "trial {trial}: repeated exchange changed conflicts");
    }
}
