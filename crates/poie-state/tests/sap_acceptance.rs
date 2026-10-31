//! SAP §9 acceptance cases implemented so far.
//!
//! Covered: SAP-T1 (crash after dispatch), T2 (ACK lost, no repeat), T3
//! (duplicate delivery), T4 (stale precondition before dispatch), T9 (executor
//! restarts losing its dedup table) and T10 (self-reported verification).
//!
//! Still open: T5–T8 (partition expiry, site ownership/leases, clock skew) and
//! T11 (simulation branch dispatch) — they need the ownership/partition work in
//! M4.6–M4.8 and are tracked in the development plan.

use std::collections::{BTreeMap, BTreeSet};

use poie_state::action::{
    redispatch_allowed, Action, ActionLedger, ActionState, Authorization, Precondition, Reversibility, VerificationSpec,
};
use poie_state::executor::{Command, DeliveryOutcome, ExecutorFaults, SimExecutor};
use poie_state::log::AppendLog;
use poie_state::record::{
    Envelope, Hlc, Namespace, ObservationValue, Payload, Quality, Record, RecordId, RecordKind, Source, Timestamp,
    VerificationOutcome,
};
use poie_state::recovery::{replay_action_states, restart_decision, RestartDecision};
use poie_state::review::{review_dispatch, DispatchContext};

fn timestamp(ms: i64) -> Timestamp {
    Timestamp(ms)
}

fn action() -> Action {
    Action {
        action_id: "a1".to_string(),
        idempotency_key: "k1".to_string(),
        target: "pump-p1".to_string(),
        command: "replace-sleeve".to_string(),
        plan_ref: Some("plan-7".to_string()),
        policy_version: "policy-3".to_string(),
        precondition: Precondition {
            state_version: BTreeMap::from([("site-a".to_string(), 40)]),
            predicates: vec!["pump-p1.status == degraded".to_string()],
        },
        valid_from: timestamp(1_000),
        valid_until: timestamp(9_000),
        budget: Vec::new(),
        reservations: vec!["lot-sleeve-01".to_string()],
        reversibility: Reversibility::Idempotent,
        verification_spec: VerificationSpec {
            observe: "pump-p1.status".to_string(),
            condition: "available".to_string(),
            window_ms: 60_000,
        },
        authorization: Some(Authorization {
            by: "operator-1".to_string(),
            basis: "policy-3".to_string(),
            scope: "pump-p1".to_string(),
            valid_until: timestamp(8_000),
        }),
    }
}

fn envelope(seq: u64, kind: RecordKind) -> Envelope {
    Envelope::new(
        RecordId::new("site-a", seq),
        kind,
        "pump-p1",
        Source::Model("planner".into()),
        timestamp(1_000 + seq as i64),
        timestamp(2_000 + seq as i64),
        Hlc::new(1_000 + seq as i64, 0),
        Namespace::live(),
    )
}

fn action_record(seq: u64, action: &Action) -> Record {
    Record::new(envelope(seq, RecordKind::Action), Payload::Action(Box::new(action.clone()))).unwrap()
}

fn transition_record(seq: u64, from: ActionState, to: ActionState) -> Record {
    let payload = Payload::Transition {
        action_id: "a1".to_string(),
        from,
        to,
        reason: "sap acceptance".to_string(),
        actor: "test".to_string(),
    };
    Record::new(envelope(seq, RecordKind::Transition), payload).unwrap()
}

fn temp_path(name: &str) -> std::path::PathBuf {
    let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("poie-sap-{}-{name}-{unique}.log", std::process::id()))
}

fn ledger_at(action: &Action, state: ActionState) -> ActionLedger {
    let mut ledger = ActionLedger::new();
    ledger.add(action).unwrap();
    let path = match state {
        ActionState::Proposed => vec![],
        ActionState::Validated => vec![ActionState::Validated],
        ActionState::Authorized => vec![ActionState::Validated, ActionState::Authorized],
        ActionState::Dispatched => {
            vec![ActionState::Validated, ActionState::Authorized, ActionState::Dispatched]
        }
        other => panic!("ledger_at does not build {other}"),
    };
    for next in path {
        ledger.apply(&action.action_id, next).unwrap();
    }
    ledger
}

#[test]
fn sap_t1_crash_after_dispatch_becomes_outcome_unknown_without_resend() {
    let a = action();
    let path = temp_path("t1");
    {
        let mut log = AppendLog::create(&path).unwrap();
        log.append(&action_record(1, &a)).unwrap();
        log.append(&transition_record(2, ActionState::Proposed, ActionState::Validated)).unwrap();
        log.append(&transition_record(3, ActionState::Validated, ActionState::Authorized)).unwrap();
        log.append(&transition_record(4, ActionState::Authorized, ActionState::Dispatched)).unwrap();
    }

    let records = AppendLog::open(&path).unwrap().records().unwrap();
    let states = replay_action_states(&records).unwrap();
    assert_eq!(states[&a.action_id], ActionState::Dispatched);

    let mut ledger = ledger_at(&a, ActionState::Dispatched);
    match restart_decision(states[&a.action_id]) {
        RestartDecision::TransitionTo(next) => {
            ledger.apply(&a.action_id, next).unwrap();
        }
        other => panic!("unexpected restart decision: {other:?}"),
    }
    assert_eq!(ledger.state(&a.action_id), Some(ActionState::OutcomeUnknown));
    assert!(ledger.apply(&a.action_id, ActionState::Dispatched).is_err(), "must not re-send");
    assert_eq!(ledger.dispatched_by("k1"), Some("a1"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn sap_t2_lost_ack_does_not_repeat_the_physical_action_and_verification_finishes_it() {
    let a = action();
    let mut executor = SimExecutor::new(60_000);
    executor.set_faults(ExecutorFaults { drop_receipt: true, nack_next: false });

    let command = Command::from_action(&a);
    let report = executor.deliver(&command, 1_500);
    assert_eq!(report.outcome, DeliveryOutcome::Executed);
    assert!(!report.receipt_delivered, "the ACK is lost");

    // The dispatcher cannot know the outcome; the executor's dedup table still
    // prevents a second physical action when the same command is re-delivered.
    let retry = executor.deliver(&command, 1_600);
    assert_eq!(retry.outcome, DeliveryOutcome::DuplicateRejected);
    assert_eq!(executor.execution_count("k1"), 1);

    // Runtime times out -> outcome_unknown; independent evidence then verifies it.
    let mut ledger = ledger_at(&a, ActionState::Dispatched);
    ledger.apply(&a.action_id, ActionState::OutcomeUnknown).unwrap();
    ledger.apply(&a.action_id, ActionState::Verified).unwrap();
    assert_eq!(ledger.state(&a.action_id), Some(ActionState::Verified));
}

#[test]
fn sap_t3_duplicate_delivery_executes_once() {
    let a = action();
    let mut executor = SimExecutor::new(60_000);
    let command = Command::from_action(&a);
    assert_eq!(executor.deliver(&command, 1_500).outcome, DeliveryOutcome::Executed);
    assert_eq!(executor.deliver(&command, 1_500).outcome, DeliveryOutcome::DuplicateRejected);
    assert_eq!(executor.physical_actions().len(), 1);
    assert_eq!(executor.execution_count("k1"), 1);
}

#[test]
fn sap_t4_a_changed_precondition_rejects_before_dispatch() {
    let a = action();
    let mut ledger = ledger_at(&a, ActionState::Authorized);
    let context = DispatchContext {
        now_ms: 5_000,
        current_state_version: BTreeMap::from([("site-a".to_string(), 39)]),
        holds_command_ownership: true,
        reservations_held: true,
        predicates_holding: BTreeSet::new(),
        authorization_revoked: false,
    };
    let rejections = review_dispatch(&a, &ledger, &context).unwrap_err();
    assert!(rejections.iter().any(|rejection| rejection.reason == "stale_precondition"), "{rejections:?}");
    ledger.apply(&a.action_id, ActionState::Rejected).unwrap();
    assert_eq!(ledger.state(&a.action_id), Some(ActionState::Rejected));
}

#[test]
fn sap_t9_losing_the_dedup_table_is_a_recorded_risk_and_blocks_irreversible_redispatch() {
    let a = action();
    let mut executor = SimExecutor::new(60_000);
    let command = Command::from_action(&a);
    executor.deliver(&command, 1_500);
    executor.restart_losing_dedup();
    assert_eq!(executor.deliver(&command, 1_600).outcome, DeliveryOutcome::Executed);
    assert_eq!(executor.execution_count("k1"), 2, "known risk: the lost dedup table re-executes");

    assert!(!redispatch_allowed(Reversibility::Irreversible, ActionState::OutcomeUnknown, false));
    assert!(redispatch_allowed(Reversibility::Irreversible, ActionState::OutcomeUnknown, true));
    assert!(redispatch_allowed(Reversibility::Idempotent, ActionState::OutcomeUnknown, false));
}

#[test]
fn sap_t10_self_reported_verification_is_not_independent() {
    let verification = |self_reported: bool| {
        let payload = Payload::Verification {
            action_id: "a1".to_string(),
            evidence: Vec::new(),
            outcome: VerificationOutcome::Verified,
            self_reported,
        };
        Record::new(envelope(5, RecordKind::Verification), payload).unwrap()
    };
    assert_eq!(verification(true).payload.verification_is_independent(), Some(false));
    assert_eq!(verification(false).payload.verification_is_independent(), Some(true));

    let observation = Payload::Observation {
        value: ObservationValue::Status("available".into()),
        quality: Quality::declared(None, Some("calibrated")).unwrap(),
    };
    assert_eq!(observation.verification_is_independent(), None);
}
