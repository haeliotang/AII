//! Randomized execution-gate safety invariants: takeover and degradation never
//! dispatch, idempotency keys dispatch at most once, validity windows, failure
//! classification and ledger-state consistency.

use std::collections::BTreeSet;

use poie_exec::gate::{DispatchOutcome, ExecutionGate};
use poie_exec::sim::{AdapterFault, SimulatedWritableAdapter};
use poie_state::action::{
    Action, ActionState, Authorization, BudgetItem, Precondition, Reversibility, VerificationSpec,
};
use poie_state::record::Timestamp;

const SEED: u64 = 0x706f_6965_6578_6563; // "poieexec"

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
}

fn action(id: &str, key: &str, from_ms: i64, until_ms: i64) -> Action {
    Action {
        action_id: id.to_string(),
        idempotency_key: key.to_string(),
        target: "pump-p1".to_string(),
        command: "replace-sleeve-p1".to_string(),
        plan_ref: Some("plan-gate-property".to_string()),
        policy_version: "policy/0.1".to_string(),
        precondition: Precondition {
            state_version: std::collections::BTreeMap::from([("site-a".to_string(), 1)]),
            predicates: vec!["pump-p1 available".to_string()],
        },
        valid_from: Timestamp(from_ms),
        valid_until: Timestamp(until_ms),
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
            basis: "property".to_string(),
            scope: "dispatch".to_string(),
            valid_until: Timestamp(until_ms + 60_000),
        }),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
    Normal,
    Takeover,
    Degraded,
    NoIdempotency,
    Expired,
    NotYetValid,
    Crash,
    DropAck,
    Unreachable,
    DuplicateKey,
}

const SCENARIOS: [Scenario; 10] = [
    Scenario::Normal,
    Scenario::Takeover,
    Scenario::Degraded,
    Scenario::NoIdempotency,
    Scenario::Expired,
    Scenario::NotYetValid,
    Scenario::Crash,
    Scenario::DropAck,
    Scenario::Unreachable,
    Scenario::DuplicateKey,
];

#[test]
fn randomized_gate_sequences_preserve_the_safety_invariants() {
    let mut rng = SplitMix64::new(SEED);
    for trial in 0..120 {
        let action_count = 1 + rng.index(5);
        let mut gate = ExecutionGate::new();
        let mut used_keys: BTreeSet<String> = BTreeSet::new();
        let mut previous_key: Option<String> = None;

        for index in 0..action_count {
            let id = format!("a{index}");
            let scenario = SCENARIOS[rng.index(SCENARIOS.len())];
            let (from_ms, until_ms) = match scenario {
                Scenario::Expired => (0, 10),
                Scenario::NotYetValid => (10_000, 20_000),
                _ => (0, 10_000),
            };
            let key = if scenario == Scenario::DuplicateKey {
                previous_key.clone().unwrap_or_else(|| format!("key-{index}"))
            } else {
                format!("key-{index}")
            };
            let action = action(&id, &key, from_ms, until_ms);
            gate.register_authorized(&action).expect("authorized action registers");

            // Configure the gate and executor for this scenario. A fresh adapter
            // keeps each scenario's physical-execution count local.
            gate.release();
            gate.set_degraded(false);
            let mut adapter = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
            let mut supports_idempotency = true;
            match scenario {
                Scenario::Takeover => gate.take_over(),
                Scenario::Degraded => gate.set_degraded(true),
                Scenario::NoIdempotency => supports_idempotency = false,
                Scenario::Crash => adapter.fault = AdapterFault::CrashAfterExecute,
                Scenario::DropAck => adapter.fault = AdapterFault::DropAck,
                Scenario::Unreachable => adapter.fault = AdapterFault::Unreachable,
                _ => {}
            }
            if !supports_idempotency {
                adapter = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(false));
            }
            let physical_before = adapter.physical_executions();
            let now_ms = if scenario == Scenario::NotYetValid { 5_000 } else { 1_000 };
            let outcome = gate.dispatch(&action, &mut adapter, now_ms).expect("dispatch call");
            let physical_after = adapter.physical_executions();
            let state = gate.ledger().state(&id).expect("action is in the ledger");
            let key_already_used = used_keys.contains(&key);

            match scenario {
                Scenario::Normal => {
                    assert!(outcome.dispatched(), "trial {trial}/{id}: normal dispatch failed: {outcome:?}");
                    assert_eq!(
                        physical_after,
                        physical_before + 1,
                        "trial {trial}/{id}: normal dispatch should execute once"
                    );
                    assert_eq!(state, ActionState::Acknowledged, "trial {trial}/{id}: wrong ledger state");
                    used_keys.insert(key.clone());
                }
                Scenario::Takeover => {
                    assert!(
                        matches!(outcome, DispatchOutcome::RecordedOnly { .. }),
                        "trial {trial}/{id}: takeover dispatched: {outcome:?}"
                    );
                    assert_eq!(
                        physical_after, physical_before,
                        "trial {trial}/{id}: takeover caused a physical execution"
                    );
                    assert_eq!(state, ActionState::Authorized, "trial {trial}/{id}: takeover changed the ledger state");
                }
                Scenario::Degraded => {
                    assert!(
                        matches!(outcome, DispatchOutcome::Fallback { .. }),
                        "trial {trial}/{id}: degraded dispatched: {outcome:?}"
                    );
                    assert_eq!(
                        physical_after, physical_before,
                        "trial {trial}/{id}: degraded caused a physical execution"
                    );
                    assert_eq!(state, ActionState::Authorized, "trial {trial}/{id}: degraded changed the ledger state");
                }
                Scenario::NoIdempotency => {
                    assert!(
                        matches!(outcome, DispatchOutcome::Fallback { .. }),
                        "trial {trial}/{id}: non-idempotent executor dispatched: {outcome:?}"
                    );
                    assert_eq!(physical_after, physical_before, "trial {trial}/{id}: non-idempotent executor executed");
                    assert_eq!(state, ActionState::Authorized, "trial {trial}/{id}: state changed without idempotency");
                }
                Scenario::Expired | Scenario::NotYetValid => {
                    assert!(
                        matches!(outcome, DispatchOutcome::Rejected { .. }),
                        "trial {trial}/{id}: invalid window dispatched: {outcome:?}"
                    );
                    assert_eq!(
                        physical_after, physical_before,
                        "trial {trial}/{id}: invalid window caused an execution"
                    );
                    assert_eq!(state, ActionState::Authorized, "trial {trial}/{id}: invalid window changed the state");
                }
                Scenario::Crash => {
                    assert!(
                        matches!(outcome, DispatchOutcome::Rejected { .. }),
                        "trial {trial}/{id}: crash not classified as unknown: {outcome:?}"
                    );
                    assert_eq!(
                        physical_after,
                        physical_before + 1,
                        "trial {trial}/{id}: crash should follow one execution"
                    );
                    assert_eq!(
                        state,
                        ActionState::OutcomeUnknown,
                        "trial {trial}/{id}: crash must leave outcome_unknown"
                    );
                    used_keys.insert(key.clone());
                }
                Scenario::DropAck => {
                    assert!(
                        outcome.dispatched(),
                        "trial {trial}/{id}: lost ACK not reported as dispatched: {outcome:?}"
                    );
                    assert_eq!(physical_after, physical_before + 1, "trial {trial}/{id}: lost ACK should execute once");
                    assert_eq!(
                        state,
                        ActionState::OutcomeUnknown,
                        "trial {trial}/{id}: lost ACK must leave outcome_unknown"
                    );
                    used_keys.insert(key.clone());
                }
                Scenario::Unreachable => {
                    assert!(
                        matches!(outcome, DispatchOutcome::Fallback { .. }),
                        "trial {trial}/{id}: unreachable executor did not fall back: {outcome:?}"
                    );
                    assert_eq!(physical_after, physical_before, "trial {trial}/{id}: unreachable executor executed");
                    assert_eq!(
                        state,
                        ActionState::Authorized,
                        "trial {trial}/{id}: unreachable executor changed the state"
                    );
                }
                Scenario::DuplicateKey => {
                    if key_already_used {
                        assert!(
                            matches!(outcome, DispatchOutcome::Rejected { .. }),
                            "trial {trial}/{id}: duplicate key was dispatched: {outcome:?}"
                        );
                        assert_eq!(
                            physical_after, physical_before,
                            "trial {trial}/{id}: duplicate key caused a physical execution"
                        );
                        assert_eq!(
                            state,
                            ActionState::Authorized,
                            "trial {trial}/{id}: duplicate key changed the state"
                        );
                    } else {
                        assert!(outcome.dispatched(), "trial {trial}/{id}: first use of a key failed: {outcome:?}");
                        used_keys.insert(key.clone());
                    }
                }
            }
            if matches!(scenario, Scenario::Normal | Scenario::Crash | Scenario::DropAck | Scenario::DuplicateKey) {
                previous_key = Some(key);
            }
            if matches!(scenario, Scenario::Crash | Scenario::DropAck) {
                // A crashed/lost-ACK action is never retried here; the gate marks
                // the key as used, so a second dispatch must be rejected.
                let retry = gate.dispatch(&action, &mut adapter, 1_000);
                assert!(retry.is_err(), "trial {trial}/{id}: action was dispatched twice");
                assert_eq!(adapter.physical_executions(), physical_after, "trial {trial}/{id}: retry caused execution");
            }
        }
    }
}
