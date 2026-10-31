//! Restart recovery rules (SAP §5; M4.7).
//!
//! After a crash the dispatcher rebuilds action states by replaying the
//! transition records written to the log. Replay validates that each transition
//! starts from the current state and is in the normative table. The restart
//! decision then follows SAP §5:
//!
//! - an action last seen `dispatched` becomes `outcome_unknown` — never re-sent;
//! - an `authorized` action must run the full dispatch review again;
//! - an `acknowledged` action keeps waiting for verification;
//! - everything else (including terminal states) is unchanged.

use std::collections::BTreeMap;
use std::fmt;

use crate::action::{is_allowed, ActionState};
use crate::record::{Payload, Record};

/// What the runtime should do with an action after restart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestartDecision {
    Unchanged,
    /// `authorized`: repeat the six dispatch checks (SAP §4, §5.3).
    RequiresReview,
    /// `acknowledged`: keep waiting for verification in the original window (§5.4).
    ContinueVerification,
    /// Append this transition before doing anything else (SAP §5.2).
    TransitionTo(ActionState),
}

/// Rebuilds action states from the transition records in log order.
///
/// Only `live`-namespace records participate. An `action` record creates the
/// action in `proposed`; each `transition` record must start from the current
/// state and be allowed by SAP §3.3.
pub fn replay_action_states(records: &[Record]) -> Result<BTreeMap<String, ActionState>, RecoveryError> {
    let mut states: BTreeMap<String, ActionState> = BTreeMap::new();
    for record in records {
        if !record.envelope.namespace.is_live() {
            continue;
        }
        match &record.payload {
            Payload::Action(action) => {
                if states.contains_key(&action.action_id) {
                    return Err(RecoveryError(format!("duplicate action record for {:?}", action.action_id)));
                }
                states.insert(action.action_id.clone(), ActionState::Proposed);
            }
            Payload::Transition { action_id, from, to, .. } => {
                let Some(current) = states.get(action_id).copied() else {
                    return Err(RecoveryError(format!("transition for unknown action {action_id:?}")));
                };
                if current != *from {
                    return Err(RecoveryError(format!(
                        "action {action_id:?}: transition starts at {from}, but the log says {current}"
                    )));
                }
                if !is_allowed(*from, *to) {
                    return Err(RecoveryError(format!("action {action_id:?}: {from} -> {to} is not in the table")));
                }
                states.insert(action_id.clone(), *to);
            }
            _ => {}
        }
    }
    Ok(states)
}

/// SAP §5 restart decision for one recovered state.
pub fn restart_decision(state: ActionState) -> RestartDecision {
    match state {
        ActionState::Dispatched => RestartDecision::TransitionTo(ActionState::OutcomeUnknown),
        ActionState::Authorized => RestartDecision::RequiresReview,
        ActionState::Acknowledged => RestartDecision::ContinueVerification,
        _ => RestartDecision::Unchanged,
    }
}

pub fn restart_plan(states: &BTreeMap<String, ActionState>) -> BTreeMap<String, RestartDecision> {
    states.iter().map(|(action_id, state)| (action_id.clone(), restart_decision(*state))).collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryError(pub String);

impl fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "log replay failed: {}", self.0)
    }
}

impl std::error::Error for RecoveryError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::record::{Envelope, Hlc, Namespace, RecordId, RecordKind, Source, Timestamp};

    fn timestamp(ms: i64) -> Timestamp {
        Timestamp(ms)
    }

    fn action() -> Action {
        Action {
            action_id: "a1".to_string(),
            idempotency_key: "k1".to_string(),
            target: "pump-p1".to_string(),
            command: "replace-sleeve".to_string(),
            plan_ref: None,
            policy_version: "policy-3".to_string(),
            precondition: crate::action::Precondition {
                state_version: BTreeMap::from([("site-a".to_string(), 1)]),
                predicates: Vec::new(),
            },
            valid_from: timestamp(1_000),
            valid_until: timestamp(9_000),
            budget: Vec::new(),
            reservations: Vec::new(),
            reversibility: crate::action::Reversibility::Idempotent,
            verification_spec: crate::action::VerificationSpec {
                observe: "pump-p1.status".to_string(),
                condition: "available".to_string(),
                window_ms: 60_000,
            },
            authorization: None,
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

    fn action_record(seq: u64) -> Record {
        let envelope = envelope(seq, RecordKind::Action);
        Record::new(envelope, Payload::Action(Box::new(action()))).unwrap()
    }

    fn transition_record(seq: u64, from: ActionState, to: ActionState) -> Record {
        let envelope = envelope(seq, RecordKind::Transition);
        let payload = Payload::Transition {
            action_id: "a1".to_string(),
            from,
            to,
            reason: "test".to_string(),
            actor: "test".to_string(),
        };
        Record::new(envelope, payload).unwrap()
    }

    #[test]
    fn replay_rebuilds_states_and_rejects_inconsistent_logs() {
        let records = vec![
            action_record(1),
            transition_record(2, ActionState::Proposed, ActionState::Validated),
            transition_record(3, ActionState::Validated, ActionState::Authorized),
        ];
        let states = replay_action_states(&records).unwrap();
        assert_eq!(states["a1"], ActionState::Authorized);

        let mut inconsistent = records.clone();
        inconsistent.push(transition_record(4, ActionState::Dispatched, ActionState::Acknowledged));
        assert!(replay_action_states(&inconsistent).is_err());

        let mut unknown = vec![transition_record(1, ActionState::Proposed, ActionState::Validated)];
        unknown.insert(0, transition_record(0, ActionState::Proposed, ActionState::Validated));
        assert!(replay_action_states(&unknown[1..]).is_err());
    }

    #[test]
    fn restart_decisions_follow_sap_section_5() {
        assert_eq!(
            restart_decision(ActionState::Dispatched),
            RestartDecision::TransitionTo(ActionState::OutcomeUnknown)
        );
        assert_eq!(restart_decision(ActionState::Authorized), RestartDecision::RequiresReview);
        assert_eq!(restart_decision(ActionState::Acknowledged), RestartDecision::ContinueVerification);
        for state in [ActionState::Proposed, ActionState::Validated, ActionState::Verified, ActionState::Failed] {
            assert_eq!(restart_decision(state), RestartDecision::Unchanged);
        }
    }
}
