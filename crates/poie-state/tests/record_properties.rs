//! Randomized record-envelope invariants: validation classification, JSON
//! round-trip for all nine payload kinds, namespace isolation and record-id
//! parsing.

use poie_state::action::{
    Action, ActionState, Authorization, BudgetItem, Precondition, Reversibility, VerificationSpec,
};
use poie_state::record::{
    Envelope, Hlc, Namespace, ObservationValue, Payload, Quality, Quantity, Record, RecordError, RecordId, RecordKind,
    Source, Timestamp, VerificationOutcome,
};

const SEED: u64 = 0x706f_6965_7265_6300; // "poierec"

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

fn action_sample() -> Action {
    Action {
        action_id: "a1".to_string(),
        idempotency_key: "k1".to_string(),
        target: "pump-p1".to_string(),
        command: "replace-sleeve-p1".to_string(),
        plan_ref: Some("plan-1".to_string()),
        policy_version: "policy/0.1".to_string(),
        precondition: Precondition {
            state_version: std::collections::BTreeMap::from([("site-a".to_string(), 1)]),
            predicates: vec!["pump-p1 available".to_string()],
        },
        valid_from: Timestamp(0),
        valid_until: Timestamp(60_000),
        budget: vec![BudgetItem { resource: "sleeve-p1".to_string(), amount_si: 1.0 }],
        reservations: Vec::new(),
        reversibility: Reversibility::Idempotent,
        verification_spec: VerificationSpec {
            observe: "pump status".to_string(),
            condition: "available".to_string(),
            window_ms: 60_000,
        },
        authorization: Some(Authorization {
            by: "operator".to_string(),
            basis: "test".to_string(),
            scope: "dispatch".to_string(),
            valid_until: Timestamp(120_000),
        }),
        redispatch_of: None,
    }
}

fn payload_samples() -> Vec<Payload> {
    vec![
        Payload::Observation {
            value: ObservationValue::Status("degraded".into()),
            quality: Quality::declared(Some(0.1), Some("calibrated")).unwrap(),
        },
        Payload::Estimate {
            value: Quantity { value_si: 2.5, unit: "kg".into() },
            uncertainty: 0.1,
            model_version: "model/0.1".into(),
            inputs: vec![RecordId::new("site-a", 1)],
        },
        Payload::Prediction {
            value: Quantity { value_si: 4.0, unit: "h".into() },
            horizon_ms: 60_000,
            model_version: "model/0.1".into(),
            scenario: "base".into(),
        },
        Payload::DesiredState {
            target: Quantity { value_si: 1.0, unit: "1".into() },
            requester: "person-1".into(),
            valid_until: Timestamp(120_000),
        },
        Payload::Action(Box::new(action_sample())),
        Payload::Transition {
            action_id: "a1".into(),
            from: ActionState::Authorized,
            to: ActionState::Dispatched,
            reason: "dispatch".into(),
            actor: "dispatcher".into(),
        },
        Payload::Receipt {
            action_id: "a1".into(),
            recipient: "exec-1".into(),
            ack: true,
            reason: None,
            duplicate_of: None,
        },
        Payload::Verification {
            action_id: "a1".into(),
            evidence: vec![RecordId::new("site-a", 2)],
            outcome: VerificationOutcome::Verified,
            self_reported: false,
        },
        Payload::Correction { corrected: RecordId::new("site-a", 1), reason: "sensor drift".into() },
    ]
}

fn envelope(kind: RecordKind, subject: &str, site: &str, source: Source) -> Envelope {
    Envelope::new(
        RecordId::new(site, 7),
        kind,
        subject,
        source,
        Timestamp(1_000),
        Timestamp(1_005),
        Hlc::new(1_000, 0),
        Namespace::live(),
    )
}

#[test]
fn record_validation_matches_the_documented_rules() {
    let payloads = payload_samples();
    let mut rng = SplitMix64::new(SEED);
    for trial in 0..120 {
        let payload = payloads[rng.index(payloads.len())].clone();
        let kind = payload.kind();
        let mut env = envelope(kind, "pump-p1", "site-a", Source::Sensor("sensor-1".into()));
        let bad_schema = rng.unit() < 0.1;
        let wrong_kind = rng.unit() < 0.1;
        let empty_subject = rng.unit() < 0.1;
        let empty_site = rng.unit() < 0.1;
        let empty_source = rng.unit() < 0.1;
        if bad_schema {
            env.schema_version = "poie.record/9.9".into();
        }
        if wrong_kind {
            env.kind = if kind == RecordKind::Receipt { RecordKind::Observation } else { RecordKind::Receipt };
        }
        if empty_subject {
            env.subject.clear();
        }
        if empty_site {
            env.site.clear();
            env.record_id.site.clear();
        }
        if empty_source {
            env.source = Source::Model(String::new());
        }

        let expected_valid = !(bad_schema || wrong_kind || empty_subject || empty_site || empty_source);
        match Record::new(env.clone(), payload.clone()) {
            Ok(record) => {
                assert!(expected_valid, "trial {trial}: invalid envelope was accepted");
                assert_eq!(record.kind(), kind, "trial {trial}: kind changed");
                let json = serde_json::to_string(&record).expect("record serializes");
                let restored: Record = serde_json::from_str(&json).expect("record deserializes");
                assert_eq!(restored, record, "trial {trial}: JSON round-trip changed the record");
            }
            Err(error) => {
                assert!(!expected_valid, "trial {trial}: valid envelope was rejected: {error}");
                if bad_schema {
                    assert!(matches!(error, RecordError::UnknownSchemaVersion(_)), "trial {trial}: {error}");
                } else if wrong_kind {
                    assert!(matches!(error, RecordError::KindMismatch { .. }), "trial {trial}: {error}");
                } else if empty_subject {
                    assert!(matches!(error, RecordError::EmptyField("subject")), "trial {trial}: {error}");
                } else if empty_site {
                    assert!(matches!(error, RecordError::EmptyField("site")), "trial {trial}: {error}");
                } else if empty_source {
                    assert!(matches!(error, RecordError::EmptyField("source")), "trial {trial}: {error}");
                }
            }
        }
    }
}

#[test]
fn every_payload_kind_constructs_and_round_trips() {
    for payload in payload_samples() {
        let env = envelope(payload.kind(), "subject", "site-a", Source::Adapter("adapter".into()));
        let record = Record::new(env.clone(), payload.clone()).expect("sample payload is valid");
        let json = serde_json::to_string(&record).unwrap();
        assert_eq!(serde_json::from_str::<Record>(&json).unwrap(), record);

        // A mismatched envelope kind is always rejected, for every payload kind.
        for other in [
            RecordKind::Observation,
            RecordKind::Estimate,
            RecordKind::Prediction,
            RecordKind::DesiredState,
            RecordKind::Action,
            RecordKind::Transition,
            RecordKind::Receipt,
            RecordKind::Verification,
            RecordKind::Correction,
        ] {
            if other == payload.kind() {
                continue;
            }
            let mut wrong = env.clone();
            wrong.kind = other;
            assert!(matches!(Record::new(wrong, payload.clone()), Err(RecordError::KindMismatch { .. })));
        }
    }
}

#[test]
fn namespaces_record_ids_and_verification_independence() {
    let mut rng = SplitMix64::new(SEED ^ 0x5eed);
    for _ in 0..80 {
        let site = format!("site-{}", rng.index(10));
        let seq = rng.next_u64();
        let id = RecordId::new(site.clone(), seq);
        let parsed: RecordId = id.to_string().parse().expect("record id parses back");
        assert_eq!(parsed, id, "record id round-trip");
        assert!(RecordId::parse("no-slash").is_err());
        assert!(RecordId::parse("/7").is_err());
        assert!(RecordId::parse("site/x").is_err());

        assert!(Namespace::live().is_live());
        let branch = format!("branch-{}", rng.index(5));
        let simulation = Namespace::simulation(branch).expect("non-empty branch");
        assert!(!simulation.is_live());
        assert!(Namespace::simulation("").is_err());
    }

    let verification = Payload::Verification {
        action_id: "a".into(),
        evidence: Vec::new(),
        outcome: VerificationOutcome::Verified,
        self_reported: true,
    };
    assert_eq!(verification.verification_is_independent(), Some(false));
    let independent = Payload::Verification {
        action_id: "a".into(),
        evidence: Vec::new(),
        outcome: VerificationOutcome::Verified,
        self_reported: false,
    };
    assert_eq!(independent.verification_is_independent(), Some(true));
    assert_eq!(payload_samples()[0].verification_is_independent(), None);
    assert!(!Namespace::simulation("x").unwrap().is_live());
}
