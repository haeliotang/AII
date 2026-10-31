//! Randomized Rust-side bridge-client properties: every injected fault is
//! classified, successes are validated (units, conservation, synthetic flag),
//! and errors are never returned as model results.

use std::path::PathBuf;

use poie_process::client::{BridgeClient, BridgeErrorClass};
use poie_process::protocol::{BridgeRequest, ModelId, Parameters};

const SEED: u64 = 0x706f_6965_6272_6467; // "poiebrdg"

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

fn script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../python/poie_models/bridge_server.py")
}

fn model() -> ModelId {
    ModelId { id: "water-treatment".to_string(), version: "0.1.0".to_string() }
}

fn request(parameters: Parameters, faults: poie_process::protocol::Faults) -> BridgeRequest {
    let mut request = BridgeRequest::new(model(), parameters, 5_000);
    request.faults = faults;
    request
}

fn in_range(rng: &mut SplitMix64) -> Parameters {
    Parameters {
        inlet_flow_m3_h: 5.0 + rng.unit() * 10.0,
        turbidity_ntu: 5.0 + rng.unit() * 45.0,
        filter_loading: rng.unit() * 0.9,
    }
}

#[test]
fn randomized_requests_and_faults_are_classified_and_never_faked() {
    let client = BridgeClient::new(script());
    let mut rng = SplitMix64::new(SEED);
    for trial in 0..40 {
        let within = rng.index(3) != 0;
        let parameters = if within {
            in_range(&mut rng)
        } else {
            Parameters { inlet_flow_m3_h: 20.0 + rng.unit(), turbidity_ntu: 20.0, filter_loading: 0.5 }
        };
        let fault_kind = rng.index(6);
        let mut faults = poie_process::protocol::Faults::default();
        match fault_kind {
            1 => faults.non_convergence = true,
            2 => faults.numerical_failure = true,
            3 => faults.wrong_unit = true,
            4 => faults.violate_conservation = true,
            5 => faults.crash = true,
            _ => {}
        }
        let expected = if fault_kind == 5 {
            BridgeErrorClass::Crashed
        } else if !within {
            BridgeErrorClass::OutOfValidity
        } else {
            match fault_kind {
                1 => BridgeErrorClass::NonConvergence,
                2 => BridgeErrorClass::NumericalFailure,
                3 => BridgeErrorClass::DimensionMismatch,
                _ => BridgeErrorClass::Conservation,
            }
        };
        let inlet = parameters.inlet_flow_m3_h;
        let outcome = client.call(&request(parameters, faults));
        match outcome {
            Err(error) => {
                assert!(fault_kind != 0 || !within, "trial {trial}: clean in-range request rejected: {error}");
                assert_eq!(error.class, expected, "trial {trial}: wrong class for fault {fault_kind}: {error}");
                assert!(!error.detail.trim().is_empty(), "trial {trial}: error without detail");
            }
            Ok((response, report)) => {
                assert!(fault_kind == 0 && within, "trial {trial}: injected fault returned a result");
                assert_eq!(response.status, "ok");
                assert!(response.synthetic, "trial {trial}: synthetic flag missing");
                assert!(report.synthetic, "trial {trial}: validation report lost the synthetic flag");
                assert!(report.checks.iter().any(|check| check == "units"), "trial {trial}: units were not checked");
                let level = response.service_level.expect("service level");
                assert!((0.0..=1.0).contains(&level), "trial {trial}: service level out of bounds");
                let mass = response.potable_flow_m3_h.unwrap() + response.reject_flow_m3_h.unwrap();
                assert!((mass - inlet).abs() <= 0.01 * inlet, "trial {trial}: mass balance");
            }
        }
    }

    // A missing interpreter is a crash, never a usable result.
    let bad = BridgeClient::with_python("definitely-not-python3", script());
    let error = bad.call(&request(in_range(&mut rng), Default::default())).unwrap_err();
    assert_eq!(error.class, BridgeErrorClass::Crashed);
}

#[test]
fn slow_models_time_out_and_are_killed() {
    let client = BridgeClient::new(script());
    let mut rng = SplitMix64::new(SEED ^ 0x51e0);
    let faults = poie_process::protocol::Faults { sleep_ms: Some(3_000), ..Default::default() };
    let mut request = request(in_range(&mut rng), faults);
    request.deadline_ms = 0; // client waits deadline + 1s
    let started = std::time::Instant::now();
    let error = client.call(&request).expect_err("a sleeping model must time out");
    assert_eq!(error.class, BridgeErrorClass::Timeout, "{error}");
    assert!(started.elapsed() < std::time::Duration::from_secs(5), "timeout took too long");
}
