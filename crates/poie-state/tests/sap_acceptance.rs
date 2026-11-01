//! SAP §9 acceptance cases implemented so far.
//!
//! Covered: SAP-T1 (crash after dispatch, plus a per-write-point sweep), T2
//! (ACK lost, no repeat), T3 (duplicate delivery), T4 (stale precondition before
//! dispatch), T5 (expiry survives a partition), T6 (remote reservation has no
//! effect until applied), T7 (lease expiry stops the holder), T8 (clock skew is
//! detected), T9 (executor restarts losing its dedup table), T10 (self-reported
//! verification), T11 (a simulation branch cannot dispatch a field action),
//! and the issue #107 / #108 cases: T12 (expiry takeover with unresolved
//! predecessor actions), T13 (a superseded lease epoch is fenced), T14 (dedup
//! window shorter than the validity) and T15 (re-dispatch with the original key,
//! DUPLICATE receipt).

use std::collections::{BTreeMap, BTreeSet};

use poie_state::action::{
    redispatch_allowed, Action, ActionLedger, ActionState, Authorization, Precondition, Reversibility, VerificationSpec,
};
use poie_state::executor::{Command, DeliveryOutcome, ExecutorFaults, SimExecutor};
use poie_state::isolation::ensure_live_dispatch;
use poie_state::log::AppendLog;
use poie_state::ownership::{OwnershipError, OwnershipRegistry};
use poie_state::record::{
    Envelope, Hlc, Namespace, ObservationValue, Payload, Quality, Record, RecordId, RecordKind, Source, Timestamp,
    VerificationOutcome,
};
use poie_state::recovery::{replay_action_states, restart_decision, RestartDecision};
use poie_state::reservations::{RemoteRequest, ReservationBook};
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
        redispatch_of: None,
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
    assert_eq!(retry.outcome, DeliveryOutcome::Duplicate);
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
    assert_eq!(executor.deliver(&command, 1_500).outcome, DeliveryOutcome::Duplicate);
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
        ownership_dispatch_deadline_ms: None,
        executor_dedup_window_ms: None,
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

    // Issue #107: an irreversible action is never re-dispatched while the
    // original is `outcome_unknown` (whatever the policy); only after the check
    // moved it on (here: `failed`). An idempotent one needs the site policy.
    assert!(!redispatch_allowed(Reversibility::Irreversible, ActionState::OutcomeUnknown, false));
    assert!(!redispatch_allowed(Reversibility::Irreversible, ActionState::OutcomeUnknown, true));
    assert!(redispatch_allowed(Reversibility::Irreversible, ActionState::Failed, false));
    assert!(redispatch_allowed(Reversibility::Idempotent, ActionState::OutcomeUnknown, true));
    assert!(!redispatch_allowed(Reversibility::Idempotent, ActionState::OutcomeUnknown, false));
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

/// One persistent write point in the dispatch flow (M4.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CrashPoint {
    ActionRecord,
    Validated,
    Authorized,
    Dispatched,
    Acknowledged,
}

fn script_dispatch_log(path: &std::path::Path, crash: CrashPoint) -> Action {
    let a = action();
    let mut log = AppendLog::create(path).unwrap();
    log.append(&action_record(1, &a)).unwrap();
    if crash == CrashPoint::ActionRecord {
        return a;
    }
    log.append(&transition_record(2, ActionState::Proposed, ActionState::Validated)).unwrap();
    if crash == CrashPoint::Validated {
        return a;
    }
    log.append(&transition_record(3, ActionState::Validated, ActionState::Authorized)).unwrap();
    if crash == CrashPoint::Authorized {
        return a;
    }
    log.append(&transition_record(4, ActionState::Authorized, ActionState::Dispatched)).unwrap();
    if crash == CrashPoint::Dispatched {
        return a;
    }
    log.append(&transition_record(5, ActionState::Dispatched, ActionState::Acknowledged)).unwrap();
    a
}

#[test]
fn sap_t1_crash_at_every_persistent_write_point_recovers_consistently() {
    let cases = [
        (CrashPoint::ActionRecord, ActionState::Proposed, RestartDecision::Unchanged),
        (CrashPoint::Validated, ActionState::Validated, RestartDecision::Unchanged),
        (CrashPoint::Authorized, ActionState::Authorized, RestartDecision::RequiresReview),
        (CrashPoint::Dispatched, ActionState::Dispatched, RestartDecision::TransitionTo(ActionState::OutcomeUnknown)),
        (CrashPoint::Acknowledged, ActionState::Acknowledged, RestartDecision::ContinueVerification),
    ];
    for (crash, expected_state, expected_decision) in cases {
        let path = temp_path(&format!("t1-{crash:?}"));
        script_dispatch_log(&path, crash);

        let records = AppendLog::open(&path).unwrap().records().unwrap();
        let states = replay_action_states(&records).unwrap();
        assert_eq!(states["a1"], expected_state, "{crash:?}");
        assert_eq!(restart_decision(expected_state), expected_decision, "{crash:?}");

        let a = action();
        if let RestartDecision::TransitionTo(next) = expected_decision {
            let mut ledger = ActionLedger::restore(&[a], &states).unwrap();
            ledger.apply("a1", next).unwrap();
            assert_eq!(ledger.state("a1"), Some(ActionState::OutcomeUnknown));
            // After recovery the same key must still block a second dispatch.
            assert_eq!(ledger.dispatched_by("k1"), Some("a1"));
        }
        let _ = std::fs::remove_file(&path);
    }
}

#[test]
fn restart_preserves_the_dispatched_idempotency_key_guard() {
    let path = temp_path("restart-key-guard");
    script_dispatch_log(&path, CrashPoint::Dispatched);
    let records = AppendLog::open(&path).unwrap().records().unwrap();
    let states = replay_action_states(&records).unwrap();

    let first = action();
    let mut second = action();
    second.action_id = "a2".to_string();
    let mut ledger = ActionLedger::restore(&[first, second.clone()], &states).unwrap();
    assert_eq!(ledger.dispatched_by("k1"), Some("a1"));

    ledger.apply("a2", ActionState::Validated).unwrap();
    ledger.apply("a2", ActionState::Authorized).unwrap();
    assert!(matches!(
        ledger.apply("a2", ActionState::Dispatched),
        Err(poie_state::action::LedgerError::DuplicateDispatch { .. })
    ));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn sap_t11_a_simulation_branch_cannot_dispatch_a_field_action() {
    let branch = Namespace::simulation("h1-preview").unwrap();
    let error = ensure_live_dispatch(&branch, "a1").unwrap_err();
    assert_eq!(error.reason, "simulation_cannot_dispatch");

    // The gate is independent of the action's lifecycle: even a fully authorized
    // action is refused before any review or physical effect.
    let _authorized = ledger_at(&action(), ActionState::Authorized);
    assert!(ensure_live_dispatch(&Namespace::live(), "a1").is_ok());
}

#[test]
fn sap_t5_an_action_that_expires_during_a_partition_stays_expired_after_reconnect() {
    let a = action();
    let mut ledger = ledger_at(&a, ActionState::Authorized);

    // While disconnected, the action's window passes; the dispatcher sees it on
    // the next review and refuses.
    let context = DispatchContext {
        now_ms: 20_000,
        current_state_version: BTreeMap::from([("site-a".to_string(), 40)]),
        holds_command_ownership: true,
        reservations_held: true,
        predicates_holding: BTreeSet::from(["pump-p1.status == degraded".to_string()]),
        authorization_revoked: false,
        ownership_dispatch_deadline_ms: None,
        executor_dedup_window_ms: None,
    };
    let rejections = review_dispatch(&a, &ledger, &context).unwrap_err();
    assert!(rejections.iter().any(|rejection| rejection.reason == "expired"), "{rejections:?}");
    ledger.apply("a1", ActionState::Expired).unwrap();

    // The expiry is persisted and survives reconnect/restart; it is not revived.
    let path = temp_path("t5");
    {
        let mut log = AppendLog::create(&path).unwrap();
        log.append(&action_record(1, &a)).unwrap();
        log.append(&transition_record(2, ActionState::Proposed, ActionState::Validated)).unwrap();
        log.append(&transition_record(3, ActionState::Validated, ActionState::Authorized)).unwrap();
        log.append(&transition_record(4, ActionState::Authorized, ActionState::Expired)).unwrap();
    }
    let records = AppendLog::open(&path).unwrap().records().unwrap();
    let states = replay_action_states(&records).unwrap();
    assert_eq!(states["a1"], ActionState::Expired);
    assert_eq!(restart_decision(ActionState::Expired), RestartDecision::Unchanged);
    let mut restored = ActionLedger::restore(&[a], &states).unwrap();
    assert!(restored.apply("a1", ActionState::Dispatched).is_err(), "an expired action must not be revived");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn sap_t6_a_remote_reservation_has_no_effect_until_the_holding_site_applies_it() {
    let mut book = ReservationBook::new();
    book.add_batch("lot-bronze", 5.0);
    let request = RemoteRequest {
        from_site: "site-b".to_string(),
        batch: "lot-bronze".to_string(),
        action_id: "a2".to_string(),
        amount_si: 4.0,
    };

    // During the partition the holding site has not reserved anything for the
    // remote action, so a dispatch is refused at the reservation check.
    assert_eq!(book.reserved("lot-bronze"), 0.0);
    let context = DispatchContext {
        now_ms: 5_000,
        current_state_version: BTreeMap::from([("site-a".to_string(), 40)]),
        holds_command_ownership: true,
        reservations_held: false,
        predicates_holding: BTreeSet::from(["pump-p1.status == degraded".to_string()]),
        authorization_revoked: false,
        ownership_dispatch_deadline_ms: None,
        executor_dedup_window_ms: None,
    };
    let rejections = review_dispatch(&action(), &ledger_at(&action(), ActionState::Authorized), &context).unwrap_err();
    assert!(rejections.iter().any(|rejection| rejection.reason == "reservations_not_held"));

    // Reconnect: the request applies subject to the cap, and the total never
    // exceeds the available quantity.
    book.apply_remote(&request).unwrap();
    assert_eq!(book.reserved("lot-bronze"), 4.0);
    let over = RemoteRequest {
        from_site: "site-c".to_string(),
        batch: "lot-bronze".to_string(),
        action_id: "a3".to_string(),
        amount_si: 3.0,
    };
    assert!(book.apply_remote(&over).is_err());
    assert_eq!(book.reserved("lot-bronze"), 4.0);
    assert_eq!(book.remaining("lot-bronze"), 1.0);
}

#[test]
fn sap_t7_the_lease_holder_stops_at_expiry_and_the_next_holder_takes_over() {
    let mut registry = OwnershipRegistry::new();
    registry.grant("robot-m1", "site-a", 10_000, 1_000).unwrap();
    assert_eq!(registry.may_dispatch("robot-m1", "site-a", 5_000, 0), Ok(()));
    assert!(matches!(registry.may_dispatch("robot-m1", "site-a", 9_500, 0), Err(OwnershipError::LeaseExpired { .. })));
    // Issue #108: the next holder waits until `until + skew_bound` (here
    // 12_000) and takes over through the expiry path, not a plain grant.
    assert!(matches!(
        registry.take_over_expired("robot-m1", "site-b", 30_000, 11_000, Handover::Unreconciled),
        Err(OwnershipError::TakeoverTooEarly { .. })
    ));
    registry
        .take_over_expired("robot-m1", "site-b", 30_000, 12_001, Handover::Reconciled { unresolved: Vec::new() })
        .unwrap();
    assert!(matches!(registry.may_dispatch("robot-m1", "site-a", 12_001, 0), Err(OwnershipError::NotHeld { .. })));
    assert_eq!(registry.may_dispatch("robot-m1", "site-b", 20_000, 0), Ok(()));
}

#[test]
fn sap_t8_excessive_clock_skew_is_detected_and_never_treated_as_a_valid_lease() {
    let mut registry = OwnershipRegistry::new();
    registry.grant("robot-m1", "site-a", 10_000, 1_000).unwrap();
    assert_eq!(registry.may_dispatch("robot-m1", "site-a", 5_000, 500), Ok(()));
    assert!(matches!(registry.may_dispatch("robot-m1", "site-a", 5_000, 5_000), Err(OwnershipError::ClockSkew { .. })));
}

// ---------------------------------------------------------------------------
// Issue #107 / #108 counterexamples (AI-preview review A-03, A-04, A-11, B-01,
// B-02, B-03, B-04, B-05, B-07, C-12), pre-registered before the fix.
// ---------------------------------------------------------------------------

use poie_state::action::{receipt_transition, ReceiptKind};
use poie_state::ownership::Handover;

const TO_DISPATCHED: [ActionState; 3] = [ActionState::Validated, ActionState::Authorized, ActionState::Dispatched];
const TO_AUTHORIZED: [ActionState; 2] = [ActionState::Validated, ActionState::Authorized];

fn variant(id: &str, reversibility: Reversibility, redispatch_of: Option<&str>) -> Action {
    let mut a = action();
    a.action_id = id.to_string();
    a.reversibility = reversibility;
    a.redispatch_of = redispatch_of.map(str::to_string);
    a
}

fn drive(ledger: &mut ActionLedger, id: &str, path: &[ActionState]) {
    for state in path {
        ledger.apply(id, *state).unwrap_or_else(|error| panic!("{id} -> {state}: {error}"));
    }
}

fn dispatch_context() -> DispatchContext {
    DispatchContext {
        now_ms: 5_000,
        current_state_version: BTreeMap::from([("site-a".to_string(), 40)]),
        holds_command_ownership: true,
        reservations_held: true,
        predicates_holding: BTreeSet::from(["pump-p1.status == degraded".to_string()]),
        authorization_revoked: false,
        ownership_dispatch_deadline_ms: None,
        executor_dedup_window_ms: None,
    }
}

fn key_rejected(action: &Action, ledger: &ActionLedger) -> bool {
    match review_dispatch(action, ledger, &dispatch_context()) {
        Ok(()) => false,
        Err(rejections) => rejections.iter().any(|r| r.check == poie_state::review::ReviewCheck::IdempotencyKey),
    }
}

/// A-03 / C-12: an irreversible re-dispatch is refused while the original is
/// `outcome_unknown`, and allowed with the original key once the outcome was
/// checked (the original left `outcome_unknown`).
#[test]
fn sap3_2_irreversible_redispatch_waits_for_the_outcome_check_then_reuses_the_key() {
    let a1 = variant("a1", Reversibility::Irreversible, None);
    let a2 = variant("a2", Reversibility::Irreversible, Some("a1"));
    let mut ledger = ActionLedger::new();
    ledger.set_unchecked_idempotent_redispatch(true);
    ledger.add(&a1).unwrap();
    ledger.add(&a2).unwrap();
    drive(&mut ledger, "a1", &TO_DISPATCHED);
    drive(&mut ledger, "a1", &[ActionState::OutcomeUnknown]);
    drive(&mut ledger, "a2", &TO_AUTHORIZED);

    assert!(key_rejected(&a2, &ledger), "irreversible re-dispatch before the outcome check must be refused");
    assert!(ledger.apply("a2", ActionState::Dispatched).is_err());

    // The verifier checks the site: the relief valve did not open.
    drive(&mut ledger, "a1", &[ActionState::Failed]);
    assert_eq!(review_dispatch(&a2, &ledger, &dispatch_context()), Ok(()));
    ledger.apply("a2", ActionState::Dispatched).expect("checked re-dispatch with the original key");
    assert_eq!(ledger.dispatched_by("k1"), Some("a2"));
}

/// B-04 counterexample 1: with the site policy on, an idempotent action may be
/// re-dispatched with its original key while the original is
/// `outcome_unknown`; without the policy (or for a non-idempotent action) not.
#[test]
fn sap3_2_policy_allowed_idempotent_redispatch_reuses_the_key_while_outcome_unknown() {
    for (policy, reversibility, allowed) in [
        (true, Reversibility::Idempotent, true),
        (false, Reversibility::Idempotent, false),
        (true, Reversibility::Irreversible, false),
        (true, Reversibility::Reversible, false),
    ] {
        let a1 = variant("a1", reversibility, None);
        let a2 = variant("a2", reversibility, Some("a1"));
        let mut ledger = ActionLedger::new();
        ledger.set_unchecked_idempotent_redispatch(policy);
        ledger.add(&a1).unwrap();
        ledger.add(&a2).unwrap();
        drive(&mut ledger, "a1", &TO_DISPATCHED);
        drive(&mut ledger, "a1", &[ActionState::OutcomeUnknown]);
        drive(&mut ledger, "a2", &TO_AUTHORIZED);
        assert_eq!(!key_rejected(&a2, &ledger), allowed, "review: policy {policy}, {reversibility:?}");
        assert_eq!(ledger.apply("a2", ActionState::Dispatched).is_ok(), allowed, "ledger: {policy}, {reversibility:?}");
    }
}

/// B-04 counterexample 2: a NACK (executor declares it did not execute) ends in
/// `failed`; the same intent can then be retried with its original key.
#[test]
fn sap4_5_a_nacked_intent_can_be_retried_with_its_original_key() {
    let a1 = variant("a1", Reversibility::Irreversible, None);
    let a2 = variant("a2", Reversibility::Irreversible, Some("a1"));
    let mut ledger = ActionLedger::new();
    ledger.add(&a1).unwrap();
    ledger.add(&a2).unwrap();
    drive(&mut ledger, "a1", &TO_DISPATCHED);
    drive(&mut ledger, "a1", &[receipt_transition(ReceiptKind::Nack)]);
    assert_eq!(ledger.state("a1"), Some(ActionState::Failed));
    drive(&mut ledger, "a2", &TO_AUTHORIZED);
    assert!(!key_rejected(&a2, &ledger));
    ledger.apply("a2", ActionState::Dispatched).expect("retry of a NACKed intent");
}

/// Guard (A-03): a second action that does not reference the key's current
/// holder never dispatches the key, in every state that occupies it, before
/// and after a restart; and a referencing action is refused while the holder is
/// still dispatched / acknowledged or already verified.
#[test]
fn sap4_5_an_occupied_key_is_reused_only_by_a_referencing_redispatch() {
    let paths: [(ActionState, &[ActionState]); 5] = [
        (ActionState::Dispatched, &[]),
        (ActionState::Acknowledged, &[ActionState::Acknowledged]),
        (ActionState::OutcomeUnknown, &[ActionState::OutcomeUnknown]),
        (ActionState::Verified, &[ActionState::Acknowledged, ActionState::Verified]),
        (ActionState::Failed, &[ActionState::Failed]),
    ];
    for (state, tail) in paths {
        let a1 = variant("a1", Reversibility::Idempotent, None);
        let stranger = variant("a2", Reversibility::Idempotent, None);
        let mut ledger = ActionLedger::new();
        ledger.set_unchecked_idempotent_redispatch(true);
        ledger.add(&a1).unwrap();
        ledger.add(&stranger).unwrap();
        drive(&mut ledger, "a1", &TO_DISPATCHED);
        drive(&mut ledger, "a1", tail);
        drive(&mut ledger, "a2", &TO_AUTHORIZED);
        assert!(key_rejected(&stranger, &ledger), "{state}: unreferenced reuse must be refused");
        assert!(ledger.apply("a2", ActionState::Dispatched).is_err(), "{state}");

        let states = BTreeMap::from([("a1".to_string(), state), ("a2".to_string(), ActionState::Authorized)]);
        let mut restored = ActionLedger::restore(&[a1.clone(), stranger.clone()], &states).unwrap();
        assert!(restored.apply("a2", ActionState::Dispatched).is_err(), "{state} after restore");

        if matches!(state, ActionState::Dispatched | ActionState::Acknowledged | ActionState::Verified) {
            let referencing = variant("a3", Reversibility::Idempotent, Some("a1"));
            ledger.add(&referencing).unwrap();
            drive(&mut ledger, "a3", &TO_AUTHORIZED);
            assert!(key_rejected(&referencing, &ledger), "{state}: holder is not outcome_unknown / failed");
            assert!(ledger.apply("a3", ActionState::Dispatched).is_err(), "{state}");
        }
    }
}

/// A-04 / B-05: a DUPLICATE receipt is not a NACK; it must not turn an action
/// into `failed` (which means "not executed").
#[test]
fn sap3_3_a_duplicate_receipt_is_not_a_nack() {
    let a1 = variant("a1", Reversibility::Idempotent, None);
    let a2 = variant("a2", Reversibility::Idempotent, Some("a1"));
    let mut executor = SimExecutor::new(60_000);
    executor.set_faults(ExecutorFaults { drop_receipt: true, nack_next: false });
    let first = executor.deliver(&Command::from_action(&a1), 1_500);
    assert_eq!(first.outcome, DeliveryOutcome::Executed);
    executor.set_faults(ExecutorFaults::default());

    let mut ledger = ActionLedger::new();
    ledger.set_unchecked_idempotent_redispatch(true);
    ledger.add(&a1).unwrap();
    ledger.add(&a2).unwrap();
    drive(&mut ledger, "a1", &TO_DISPATCHED);
    drive(&mut ledger, "a1", &[ActionState::OutcomeUnknown]);
    drive(&mut ledger, "a2", &TO_AUTHORIZED);
    ledger.apply("a2", ActionState::Dispatched).expect("policy-allowed idempotent re-dispatch");

    let second = executor.deliver(&Command::from_action(&a2), 1_600);
    assert_eq!(second.outcome.receipt_kind(), ReceiptKind::Duplicate);
    assert_eq!(receipt_transition(second.outcome.receipt_kind()), ActionState::Acknowledged);
    ledger.apply("a2", receipt_transition(second.outcome.receipt_kind())).unwrap();
    assert_eq!(ledger.state("a2"), Some(ActionState::Acknowledged));
    assert_eq!(executor.execution_count("k1"), 1);
    assert_eq!(receipt_transition(ReceiptKind::Nack), ActionState::Failed);
    assert_eq!(receipt_transition(ReceiptKind::Ack), ActionState::Acknowledged);
}

/// B-03: a command still valid for an hour, redelivered after five minutes to
/// an executor that keeps dedup records for one minute, must not execute twice.
#[test]
fn sap4_7_redelivery_inside_validity_but_outside_a_short_dedup_window_never_executes_twice() {
    let mut executor = SimExecutor::new(60_000);
    let mut command = Command::from_action(&action());
    command.valid_until_ms = 3_600_000;
    executor.deliver(&command, 0);
    executor.deliver(&command, 300_000);
    assert!(executor.execution_count("k1") <= 1, "executed {} times", executor.execution_count("k1"));
}

/// B-03: the dispatcher refuses a command whose remaining validity (plus the
/// skew bound) exceeds the executor's declared dedup window.
#[test]
fn sap4_7_review_rejects_validity_beyond_the_executor_dedup_window() {
    let a = action();
    let ledger = ledger_at(&a, ActionState::Authorized);
    let mut context = dispatch_context();
    context.executor_dedup_window_ms = Some(1_000);
    let rejections = review_dispatch(&a, &ledger, &context).unwrap_err();
    assert!(rejections.iter().any(|r| r.reason == "validity_exceeds_dedup_window"), "{rejections:?}");
    context.executor_dedup_window_ms = Some(60_000);
    assert_eq!(review_dispatch(&a, &ledger, &context), Ok(()));
}

/// B-01: the dispatcher refuses a command whose validity outlives its lease's
/// dispatch deadline (`lease.until - skew_bound`).
#[test]
fn sap4_3_review_rejects_validity_beyond_the_lease_dispatch_deadline() {
    let a = action();
    let ledger = ledger_at(&a, ActionState::Authorized);
    let mut context = dispatch_context();
    context.ownership_dispatch_deadline_ms = Some(8_000);
    let rejections = review_dispatch(&a, &ledger, &context).unwrap_err();
    assert!(rejections.iter().any(|r| r.reason == "validity_exceeds_lease"), "{rejections:?}");
    context.ownership_dispatch_deadline_ms = Some(9_000);
    assert_eq!(review_dispatch(&a, &ledger, &context), Ok(()));
}

fn on_target(id: &str, key: &str, target: &str, valid_until: i64, reversibility: Reversibility) -> Action {
    let mut a = action();
    a.action_id = id.to_string();
    a.idempotency_key = key.to_string();
    a.target = target.to_string();
    a.valid_until = timestamp(valid_until);
    a.reversibility = reversibility;
    a
}

/// B-01: site A's command, still in flight when site B takes over, reaches the
/// executor after B's command. The executor must fence it by lease epoch even
/// when the time-based bound fails (here: the executor clock lags).
#[test]
fn sap6_1_a_superseded_lease_epoch_is_fenced_at_the_executor() {
    let mut registry = OwnershipRegistry::new();
    registry.grant("valve-v3", "site-a", 10_000, 0).unwrap();
    let a1 = on_target("a1", "ka", "valve-v3", 8_000, Reversibility::Idempotent);
    let epoch_a = registry.check_dispatch("valve-v3", "site-a", 7_000, 0, &a1).unwrap();

    let epoch_b = registry.take_over_expired("valve-v3", "site-b", 40_000, 12_001, Handover::Unreconciled).unwrap();
    let b1 = on_target("b1", "kb", "valve-v3", 30_000, Reversibility::Idempotent);
    assert_eq!(registry.check_dispatch("valve-v3", "site-b", 12_500, 0, &b1), Ok(epoch_b));

    // The executor clock is ~4.5 s slow (outside the assumed bound): a1 still
    // looks valid when it finally arrives after b1.
    let mut executor = SimExecutor::new(60_000);
    let b_report = executor.deliver(&Command::from_action(&b1).with_lease_epoch(epoch_b), 7_800);
    assert_eq!(b_report.outcome, DeliveryOutcome::Executed);
    let a_report = executor.deliver(&Command::from_action(&a1).with_lease_epoch(epoch_a), 7_900);
    assert_ne!(a_report.outcome, DeliveryOutcome::Executed, "stale epoch executed after takeover");
    assert_eq!(a_report.outcome.receipt_kind(), ReceiptKind::Nack);
    assert_eq!(executor.physical_actions(), ["b1".to_string()]);
    assert!(epoch_b > epoch_a, "a takeover must advance the lease epoch");
}

/// B-01: a command's validity must not outlive `lease.until - skew_bound`.
#[test]
fn sap6_1_command_validity_must_not_outlive_the_lease() {
    let mut registry = OwnershipRegistry::new();
    registry.grant("valve-v3", "site-a", 10_000, 0).unwrap();
    let long = on_target("a1", "k", "valve-v3", 30_000, Reversibility::Idempotent);
    assert!(matches!(
        registry.check_dispatch("valve-v3", "site-a", 5_000, 0, &long),
        Err(OwnershipError::ValidityBeyondLease { .. })
    ));
    let bounded = on_target("a2", "k2", "valve-v3", 8_000, Reversibility::Idempotent);
    assert!(registry.check_dispatch("valve-v3", "site-a", 5_000, 0, &bounded).is_ok());
}

/// A-11 / B-07: the taker waits until `until + skew_bound`; an expired lease is
/// taken over, not silently re-granted (rule 2 vs rule 3).
#[test]
fn sap6_1_taker_waits_until_expiry_plus_skew_bound() {
    let mut registry = OwnershipRegistry::new();
    registry.grant("robot-m1", "site-a", 10_000, 0).unwrap();
    for now in [10_001, 11_000, 12_000] {
        assert!(
            matches!(
                registry.take_over_expired("robot-m1", "site-b", 30_000, now, Handover::Unreconciled),
                Err(OwnershipError::TakeoverTooEarly { .. })
            ),
            "takeover at {now}"
        );
    }
    assert!(matches!(
        registry.grant("robot-m1", "site-b", 30_000, 12_001),
        Err(OwnershipError::TakeoverRequired { .. })
    ));
    assert!(registry.take_over_expired("robot-m1", "site-b", 30_000, 12_001, Handover::Unreconciled).is_ok());
}

/// B-02 (SAP-T12): an expiry takeover while the predecessor may hold an
/// `outcome_unknown` irreversible action stays conservative until reconciled.
#[test]
fn sap6_1_expiry_takeover_with_unresolved_predecessor_actions_is_conservative() {
    for handover in [Handover::Unreconciled, Handover::Reconciled { unresolved: vec!["a-dose".to_string()] }] {
        let mut registry = OwnershipRegistry::new();
        registry.grant("pump-p", "site-a", 10_000, 0).unwrap();
        registry.take_over_expired("pump-p", "site-b", 40_000, 12_001, handover.clone()).unwrap();
        let dose = on_target("b-dose", "kb", "pump-p", 30_000, Reversibility::Irreversible);
        let close = on_target("b-close", "kc", "pump-p", 30_000, Reversibility::Idempotent);
        assert!(
            matches!(
                registry.check_dispatch("pump-p", "site-b", 13_000, 0, &dose),
                Err(OwnershipError::PendingHandover { .. })
            ),
            "{handover:?}"
        );
        assert!(registry.check_dispatch("pump-p", "site-b", 13_000, 0, &close).is_ok());
        registry.mark_reconciled("pump-p", "site-b").unwrap();
        assert!(registry.check_dispatch("pump-p", "site-b", 13_000, 0, &dose).is_ok());
    }

    let mut registry = OwnershipRegistry::new();
    registry.grant("pump-p", "site-a", 10_000, 0).unwrap();
    registry
        .take_over_expired("pump-p", "site-b", 40_000, 12_001, Handover::Reconciled { unresolved: Vec::new() })
        .unwrap();
    let dose = on_target("b-dose", "kb", "pump-p", 30_000, Reversibility::Irreversible);
    assert!(registry.check_dispatch("pump-p", "site-b", 13_000, 0, &dose).is_ok());
}

/// SAP §6.1 rule 2: a negotiated release + grant needs no expiry wait but still
/// advances the lease epoch.
#[test]
fn sap6_1_negotiated_release_then_grant_advances_the_epoch() {
    let mut registry = OwnershipRegistry::new();
    registry.grant("robot-m1", "site-a", 10_000, 0).unwrap();
    let epoch_a = registry.lease("robot-m1").unwrap().epoch;
    registry.release("robot-m1", "site-a").unwrap();
    registry.grant("robot-m1", "site-b", 20_000, 5_000).unwrap();
    assert!(registry.lease("robot-m1").unwrap().epoch > epoch_a);
}
