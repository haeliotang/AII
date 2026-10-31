//! Subprocess client for the JSON-line bridge (M7.1, M7.4).
//!
//! The model runs in a separate Python process. The call has a wall-clock
//! deadline; a timeout kills the process and is reported as `Timeout`, never as
//! a model result. Crashes, protocol errors, non-convergence, numerical failure,
//! validity violations and dimension/conservation errors are all distinct.

use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use crate::protocol::{BridgeRequest, BridgeResponse};
use crate::validate::{validate_response, ValidationReport};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgeErrorClass {
    Timeout,
    Crashed,
    Protocol,
    ModelMismatch,
    OutOfValidity,
    NonConvergence,
    NumericalFailure,
    Conservation,
    DimensionMismatch,
}

impl BridgeErrorClass {
    pub fn as_str(self) -> &'static str {
        match self {
            BridgeErrorClass::Timeout => "timeout",
            BridgeErrorClass::Crashed => "crashed",
            BridgeErrorClass::Protocol => "protocol_error",
            BridgeErrorClass::ModelMismatch => "model_mismatch",
            BridgeErrorClass::OutOfValidity => "out_of_validity",
            BridgeErrorClass::NonConvergence => "non_convergence",
            BridgeErrorClass::NumericalFailure => "numerical_failure",
            BridgeErrorClass::Conservation => "conservation_violation",
            BridgeErrorClass::DimensionMismatch => "dimension_mismatch",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BridgeError {
    pub class: BridgeErrorClass,
    pub detail: String,
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.class.as_str(), self.detail)
    }
}

impl std::error::Error for BridgeError {}

pub struct BridgeClient {
    python: String,
    script: PathBuf,
}

impl BridgeClient {
    pub fn new(script: impl Into<PathBuf>) -> BridgeClient {
        BridgeClient { python: "python3".to_string(), script: script.into() }
    }

    pub fn with_python(python: impl Into<String>, script: impl Into<PathBuf>) -> BridgeClient {
        BridgeClient { python: python.into(), script: script.into() }
    }

    /// Calls the model. Errors are classified; no error is ever turned into a
    /// usable result.
    pub fn call(&self, request: &BridgeRequest) -> Result<(BridgeResponse, ValidationReport), BridgeError> {
        let mut child = Command::new(&self.python)
            .arg(&self.script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| BridgeError {
                class: BridgeErrorClass::Crashed,
                detail: format!("cannot start {} {}: {error}", self.python, self.script.display()),
            })?;
        let line = serde_json::to_string(request).map_err(|error| BridgeError {
            class: BridgeErrorClass::Protocol,
            detail: format!("cannot serialize request: {error}"),
        })?;
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(error) = writeln!(stdin, "{line}") {
                let _ = child.kill();
                let _ = child.wait();
                return Err(BridgeError {
                    class: BridgeErrorClass::Crashed,
                    detail: format!("cannot write to the model process: {error}"),
                });
            }
        }
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(BridgeError { class: BridgeErrorClass::Crashed, detail: "model process has no stdout".into() });
        };
        let (sender, receiver) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            let result = reader.read_line(&mut line);
            let _ = sender.send((result, line));
        });
        let timeout = Duration::from_millis(request.deadline_ms.saturating_add(1_000));
        let outcome = receiver.recv_timeout(timeout);
        reader.join().ok();
        let result = match outcome {
            Ok((Ok(0), _)) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(BridgeError {
                    class: BridgeErrorClass::Crashed,
                    detail: "model process exited without a response".into(),
                });
            }
            Ok((Ok(_), line)) => line.trim().to_string(),
            Ok((Err(error), _)) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(BridgeError {
                    class: BridgeErrorClass::Protocol,
                    detail: format!("cannot read the response: {error}"),
                });
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(BridgeError {
                    class: BridgeErrorClass::Timeout,
                    detail: format!("no response within {} ms", request.deadline_ms),
                });
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(BridgeError {
                    class: BridgeErrorClass::Crashed,
                    detail: "model process disconnected".into(),
                });
            }
        };
        let child_status = child.wait();
        if let Ok(status) = child_status {
            if !status.success() && result.is_empty() {
                return Err(BridgeError {
                    class: BridgeErrorClass::Crashed,
                    detail: format!("model process exited with {status}"),
                });
            }
        }
        let response: BridgeResponse = serde_json::from_str(&result).map_err(|error| BridgeError {
            class: BridgeErrorClass::Protocol,
            detail: format!("cannot parse response {result:?}: {error}"),
        })?;
        if response.model != request.model {
            return Err(BridgeError {
                class: BridgeErrorClass::ModelMismatch,
                detail: format!(
                    "requested {} v{}, got {} v{}",
                    request.model.id, request.model.version, response.model.id, response.model.version
                ),
            });
        }
        let validation = validate_response(&response, &request.expected_units, request.parameters.inlet_flow_m3_h)?;
        Ok((response, validation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{BridgeRequest, Faults, ModelId, Parameters};

    fn client() -> BridgeClient {
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../python/poie_models/bridge_server.py");
        BridgeClient::new(script)
    }

    fn request() -> BridgeRequest {
        let model = ModelId { id: "water-treatment".into(), version: "0.1.0".into() };
        let parameters = Parameters { inlet_flow_m3_h: 12.0, turbidity_ntu: 10.0, filter_loading: 0.2 };
        BridgeRequest::new(model, parameters, 5_000)
    }

    #[test]
    fn a_valid_call_returns_a_validated_result() {
        let (response, validation) = client().call(&request()).unwrap();
        assert_eq!(response.status, "ok");
        assert!(response.synthetic);
        assert!(validation.checks.iter().any(|check| check == "conservation"));
        let level = response.service_level.unwrap();
        assert!((0.0..=1.0).contains(&level));
        let potable = response.potable_flow_m3_h.unwrap();
        let reject = response.reject_flow_m3_h.unwrap();
        assert!((potable + reject - 12.0).abs() < 0.01);
    }

    #[test]
    fn out_of_validity_is_reported_not_extrapolated() {
        let mut request = request();
        request.parameters.inlet_flow_m3_h = 30.0;
        let error = client().call(&request).unwrap_err();
        assert_eq!(error.class, BridgeErrorClass::OutOfValidity);
    }

    #[test]
    fn timeout_kills_the_process_and_is_not_a_result() {
        let mut request = request();
        request.deadline_ms = 100;
        request.faults = Faults { sleep_ms: Some(2_000), ..Faults::default() };
        let error = client().call(&request).unwrap_err();
        assert_eq!(error.class, BridgeErrorClass::Timeout);
    }

    #[test]
    fn failures_are_classified_separately() {
        let cases = [
            (Faults { non_convergence: true, ..Faults::default() }, BridgeErrorClass::NonConvergence),
            (Faults { numerical_failure: true, ..Faults::default() }, BridgeErrorClass::NumericalFailure),
            (Faults { violate_conservation: true, ..Faults::default() }, BridgeErrorClass::Conservation),
            (Faults { wrong_unit: true, ..Faults::default() }, BridgeErrorClass::DimensionMismatch),
            (Faults { crash: true, ..Faults::default() }, BridgeErrorClass::Crashed),
        ];
        for (faults, expected) in cases {
            let mut request = request();
            request.faults = faults;
            let error = client().call(&request).unwrap_err();
            assert_eq!(error.class, expected, "{error}");
        }
    }

    #[test]
    fn a_model_version_mismatch_is_rejected() {
        let mut request = request();
        request.model.version = "9.9.9".into();
        let error = client().call(&request).unwrap_err();
        assert_eq!(error.class, BridgeErrorClass::ModelMismatch);
    }
}
