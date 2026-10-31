//! Independent verification signals (M10.3).
//!
//! Every action needs at least one confirmation source that is independent of
//! the executor. Verification by the executor itself is not accepted.

use std::fmt;

/// An independent signal that can confirm whether an action had its effect.
pub trait VerificationSignal {
    fn source(&self) -> &str;
    /// `Some(true/false)` when the signal can judge, `None` when it has no
    /// observation for this action.
    fn observe(&mut self, action_id: &str) -> Option<bool>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct VerificationOutcome {
    pub action_id: String,
    pub source: String,
    pub confirmed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerificationError {
    NoSignals,
    NoIndependentSource { executor_source: String },
    NotObserved,
}

impl fmt::Display for VerificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VerificationError::NoSignals => f.write_str("no verification signal was supplied"),
            VerificationError::NoIndependentSource { executor_source } => {
                write!(f, "no verification source independent of the executor {executor_source:?}")
            }
            VerificationError::NotObserved => f.write_str("independent signals have no observation for this action"),
        }
    }
}

impl std::error::Error for VerificationError {}

/// Requires a signal whose source differs from the executor's.
pub fn verify_action(
    action_id: &str,
    executor_source: &str,
    signals: &mut [Box<dyn VerificationSignal>],
) -> Result<VerificationOutcome, VerificationError> {
    if signals.is_empty() {
        return Err(VerificationError::NoSignals);
    }
    let mut independent = signals.iter_mut().filter(|signal| signal.source() != executor_source).peekable();
    if independent.peek().is_none() {
        return Err(VerificationError::NoIndependentSource { executor_source: executor_source.to_string() });
    }
    for signal in independent {
        if let Some(confirmed) = signal.observe(action_id) {
            return Ok(VerificationOutcome {
                action_id: action_id.to_string(),
                source: signal.source().to_string(),
                confirmed,
            });
        }
    }
    Err(VerificationError::NotObserved)
}

/// Test/demonstration signal with a fixed observation.
pub struct FixedSignal {
    pub source: String,
    pub result: Option<bool>,
}

impl VerificationSignal for FixedSignal {
    fn source(&self) -> &str {
        &self.source
    }

    fn observe(&mut self, _action_id: &str) -> Option<bool> {
        self.result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signals(entries: Vec<(&str, Option<bool>)>) -> Vec<Box<dyn VerificationSignal>> {
        entries
            .into_iter()
            .map(|(source, result)| {
                Box::new(FixedSignal { source: source.to_string(), result }) as Box<dyn VerificationSignal>
            })
            .collect()
    }

    #[test]
    fn an_independent_signal_confirms_the_action() {
        let mut provided = signals(vec![("pump-status-sensor", Some(true))]);
        let outcome = verify_action("a1", "sim-executor", &mut provided).unwrap();
        assert_eq!(outcome.source, "pump-status-sensor");
        assert!(outcome.confirmed);
    }

    #[test]
    fn executor_only_verification_is_rejected() {
        let mut provided = signals(vec![("sim-executor", Some(true))]);
        let error = verify_action("a1", "sim-executor", &mut provided).unwrap_err();
        assert!(matches!(error, VerificationError::NoIndependentSource { .. }));
    }

    #[test]
    fn no_observation_is_reported_not_assumed() {
        let mut provided = signals(vec![("pump-status-sensor", None)]);
        assert_eq!(verify_action("a1", "sim-executor", &mut provided), Err(VerificationError::NotObserved));
        assert_eq!(verify_action("a1", "sim-executor", &mut signals(vec![])), Err(VerificationError::NoSignals));
    }
}
