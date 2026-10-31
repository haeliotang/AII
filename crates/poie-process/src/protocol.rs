//! Request/response schema for the process-model bridge (M7.1).

use serde::{Deserialize, Serialize};

pub const BRIDGE_SCHEMA: &str = "poie.process-bridge/0.1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelId {
    pub id: String,
    pub version: String,
}

/// Physical inputs of the water-treatment model. Units are fixed by the model
/// documentation and repeated in the request so a mismatch is visible.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameters {
    /// Raw-water inlet flow, m³/h.
    pub inlet_flow_m3_h: f64,
    /// Raw-water turbidity, NTU.
    pub turbidity_ntu: f64,
    /// Filter loading fraction, 0..1.
    pub filter_loading: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeRequest {
    pub schema: String,
    pub model: ModelId,
    pub parameters: Parameters,
    /// Units the caller expects on the response, by field name.
    pub expected_units: std::collections::BTreeMap<String, String>,
    /// Wall-clock deadline for the call, milliseconds.
    pub deadline_ms: u64,
    /// Synthetic fault injection used by tests and demonstrations.
    #[serde(default)]
    pub faults: Faults,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Faults {
    #[serde(default)]
    pub sleep_ms: Option<u64>,
    #[serde(default)]
    pub non_convergence: bool,
    #[serde(default)]
    pub numerical_failure: bool,
    #[serde(default)]
    pub violate_conservation: bool,
    #[serde(default)]
    pub wrong_unit: bool,
    #[serde(default)]
    pub crash: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Validity {
    pub ranges: std::collections::BTreeMap<String, (f64, f64)>,
    pub within: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeResponse {
    pub schema: String,
    pub model: ModelId,
    /// "ok" or "error".
    pub status: String,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub service_level: Option<f64>,
    #[serde(default)]
    pub potable_flow_m3_h: Option<f64>,
    #[serde(default)]
    pub reject_flow_m3_h: Option<f64>,
    #[serde(default)]
    pub head_loss_kpa: Option<f64>,
    #[serde(default)]
    pub energy_kwh: Option<f64>,
    /// Units of the numeric outputs.
    #[serde(default)]
    pub units: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub validity: Option<Validity>,
    /// True when the model parameters are synthetic assumptions.
    #[serde(default)]
    pub synthetic: bool,
}

impl BridgeRequest {
    pub fn new(model: ModelId, parameters: Parameters, deadline_ms: u64) -> BridgeRequest {
        let expected_units = std::collections::BTreeMap::from([
            ("service_level".to_string(), "1".to_string()),
            ("potable_flow_m3_h".to_string(), "m3/h".to_string()),
            ("reject_flow_m3_h".to_string(), "m3/h".to_string()),
            ("head_loss_kpa".to_string(), "kPa".to_string()),
            ("energy_kwh".to_string(), "kWh".to_string()),
        ]);
        BridgeRequest {
            schema: BRIDGE_SCHEMA.to_string(),
            model,
            parameters,
            expected_units,
            deadline_ms,
            faults: Faults::default(),
        }
    }
}
