//! Randomized projection invariants: determinism, per-site state versions,
//! correction suppression, simulation isolation, latest-wins and evidence expiry.
//!
//! The expected state is computed by a small independent oracle that follows the
//! documented rules (PO §4, SAP §2.3), so the property test compares the
//! implementation against the contract rather than against itself.

use std::collections::{BTreeMap, BTreeSet};

use poie_ontology::model::AssetStatus;
use poie_state::projection::{project, EvidencePolicy, Projected, ProjectedState};
use poie_state::record::{
    Envelope, Hlc, Namespace, ObservationValue, Payload, Quality, Quantity, Record, RecordId, RecordKind, Source,
    Timestamp,
};

const SEED: u64 = 0x706f_6965_7072_6f6a; // "poieproj"

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

    fn index(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }

    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

const SITES: [&str; 2] = ["site-a", "site-b"];
const ASSETS: [&str; 3] = ["pump-p1", "bus-1", "lathe-1"];
const LOTS: [&str; 2] = ["lot-1", "lot-2"];

struct Recorder {
    seq: BTreeMap<&'static str, u64>,
}

impl Recorder {
    fn new() -> Recorder {
        Recorder { seq: BTreeMap::new() }
    }

    fn envelope(
        &mut self,
        site: &'static str,
        subject: &str,
        event_time: i64,
        namespace: Namespace,
        kind: RecordKind,
    ) -> Envelope {
        let seq = self.seq.entry(site).or_insert(0);
        *seq += 1;
        Envelope::new(
            RecordId::new(site, *seq),
            kind,
            subject,
            Source::Sensor(format!("sensor-{site}")),
            Timestamp(event_time),
            Timestamp(event_time + 1),
            Hlc::new(event_time, 0),
            namespace,
        )
    }

    fn status(
        &mut self,
        site: &'static str,
        asset: &str,
        status: &str,
        event_time: i64,
        simulation: bool,
        now_ms: i64,
    ) -> Record {
        let namespace =
            if simulation { Namespace::simulation(format!("sim-{now_ms}")).unwrap() } else { Namespace::live() };
        let envelope = self.envelope(site, asset, event_time, namespace, RecordKind::Observation);
        let quality = Quality::declared(Some(0.1), Some("test")).unwrap();
        Record::new(envelope, Payload::Observation { value: ObservationValue::Status(status.to_string()), quality })
            .unwrap()
    }

    fn quantity(&mut self, site: &'static str, lot: &str, amount: f64, event_time: i64, simulation: bool) -> Record {
        let namespace = if simulation { Namespace::simulation("sim-lot").unwrap() } else { Namespace::live() };
        let envelope = self.envelope(site, lot, event_time, namespace, RecordKind::Observation);
        let quality = Quality::declared(Some(0.2), Some("test")).unwrap();
        let value = ObservationValue::Quantity(Quantity { value_si: amount, unit: "kg".to_string() });
        Record::new(envelope, Payload::Observation { value, quality }).unwrap()
    }

    fn correction(&mut self, site: &'static str, corrected: RecordId, event_time: i64) -> Record {
        let envelope = self.envelope(site, "ignored", event_time, Namespace::live(), RecordKind::Correction);
        Record::new(envelope, Payload::Correction { corrected, reason: "test correction".to_string() }).unwrap()
    }
}

fn generate(rng: &mut SplitMix64) -> Vec<Record> {
    let mut recorder = Recorder::new();
    let count = 5 + rng.index(16);
    let mut records: Vec<Record> = Vec::new();
    for index in 0..count {
        let site = SITES[rng.index(SITES.len())];
        let event_time = 1_000 + (index as i64) * 100 + rng.index(50) as i64;
        let simulation = rng.unit() < 0.1;
        match rng.index(5) {
            0..=2 => {
                let status = ["available", "degraded", "failed", "unknown"][rng.index(4)];
                let asset = ASSETS[rng.index(ASSETS.len())];
                records.push(recorder.status(site, asset, status, event_time, simulation, index as i64));
            }
            3 => {
                let lot = LOTS[rng.index(LOTS.len())];
                let amount = 0.5 + rng.unit() * 5.0;
                records.push(recorder.quantity(site, lot, amount, event_time, simulation));
            }
            _ => {
                if records.is_empty() {
                    let asset = ASSETS[rng.index(ASSETS.len())];
                    records.push(recorder.status(site, asset, "available", event_time, false, index as i64));
                    continue;
                }
                let target = records[rng.index(records.len())].envelope.record_id.clone();
                records.push(recorder.correction(site, target, event_time));
            }
        }
    }
    records
}

/// Independent oracle following SAP §2.3 / PO §4.
fn expected_state(records: &[Record], policy: EvidencePolicy, now_ms: i64) -> ProjectedState {
    let corrected: BTreeSet<RecordId> = records
        .iter()
        .filter_map(|record| match &record.payload {
            Payload::Correction { corrected, .. } => Some(corrected.clone()),
            _ => None,
        })
        .collect();
    let mut state_version: BTreeMap<String, u64> = BTreeMap::new();
    let mut asset_evidence: BTreeMap<String, (&str, i64)> = BTreeMap::new();
    let mut lot_evidence: BTreeMap<String, (f64, i64)> = BTreeMap::new();
    for record in records {
        if !record.envelope.namespace.is_live() {
            continue;
        }
        let site = &record.envelope.record_id.site;
        let seq = record.envelope.record_id.seq;
        state_version.entry(site.clone()).and_modify(|current| *current = (*current).max(seq)).or_insert(seq);
        if corrected.contains(&record.envelope.record_id) {
            continue;
        }
        let event_time = record.envelope.event_time.0;
        match &record.payload {
            Payload::Observation { value: ObservationValue::Status(status), .. } => {
                asset_evidence.insert(record.envelope.subject.clone(), (status.as_str(), event_time));
            }
            Payload::Observation { value: ObservationValue::Quantity(quantity), .. }
            | Payload::Estimate { value: quantity, .. } => {
                if quantity.value_si.is_finite() {
                    lot_evidence.insert(record.envelope.subject.clone(), (quantity.value_si, event_time));
                }
            }
            _ => {}
        }
    }
    let mut assets = BTreeMap::new();
    for (subject, (status, event_time)) in asset_evidence {
        let expired = now_ms.saturating_sub(event_time) > policy.max_age_ms as i64;
        let status = match status {
            "available" => AssetStatus::Available,
            "degraded" => AssetStatus::Degraded,
            "failed" => AssetStatus::Failed,
            "unknown" => AssetStatus::Unknown,
            _ => continue,
        };
        assets.insert(subject, if expired { Projected::Unknown { expired } } else { Projected::Known(status) });
    }
    let mut lots = BTreeMap::new();
    for (subject, (amount, event_time)) in lot_evidence {
        let expired = now_ms.saturating_sub(event_time) > policy.max_age_ms as i64;
        lots.insert(subject, if expired { Projected::Unknown { expired } } else { Projected::Known(amount) });
    }
    ProjectedState { assets, lots, state_version, projection_version: "poie.projection/0.1".to_string() }
}

#[test]
fn randomized_projections_match_the_contract_oracle() {
    let mut rng = SplitMix64::new(SEED);
    for trial in 0..120 {
        let records = generate(&mut rng);
        let policy = EvidencePolicy::new(50 + rng.index(5_000) as u64);
        let now_ms = 1_000 + rng.index(6_000) as i64;
        let actual = project(&records, policy, now_ms);
        let expected = expected_state(&records, policy, now_ms);
        assert_eq!(actual, expected, "trial {trial}: projection does not match the oracle");

        // Determinism and no mutation of the input.
        let again = project(&records, policy, now_ms);
        assert_eq!(actual, again, "trial {trial}: projection is not deterministic");

        // Simulation records must never contribute to the live state.
        let simulation_only: Vec<Record> =
            records.iter().filter(|record| !record.envelope.namespace.is_live()).cloned().collect();
        let simulated = project(&simulation_only, policy, now_ms);
        assert!(
            simulated.assets.is_empty() && simulated.lots.is_empty(),
            "trial {trial}: simulation leaked into live state"
        );
        assert!(simulated.state_version.is_empty(), "trial {trial}: simulation updated the state version");
    }
}
