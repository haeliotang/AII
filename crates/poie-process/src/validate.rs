//! Response validation (M7.3): units, finiteness, validity range, conservation.

use std::collections::BTreeMap;

use crate::client::{BridgeError, BridgeErrorClass};
use crate::protocol::BridgeResponse;

#[derive(Clone, Debug, PartialEq)]
pub struct ValidationReport {
    pub checks: Vec<String>,
    pub synthetic: bool,
}

fn fail(class: BridgeErrorClass, detail: impl Into<String>) -> Result<ValidationReport, BridgeError> {
    Err(BridgeError { class, detail: detail.into() })
}

/// Validates a response before it may be used. Fails closed: every check must pass.
pub fn validate_response(
    response: &BridgeResponse,
    expected_units: &BTreeMap<String, String>,
    expected_inlet_m3_h: f64,
) -> Result<ValidationReport, BridgeError> {
    let mut checks = Vec::new();
    if response.schema != crate::protocol::BRIDGE_SCHEMA {
        return fail(BridgeErrorClass::Protocol, format!("unexpected schema {:?}", response.schema));
    }
    if response.status != "ok" {
        let class = match response.error.as_deref() {
            Some("out_of_validity") => BridgeErrorClass::OutOfValidity,
            Some("non_convergence") => BridgeErrorClass::NonConvergence,
            Some("numerical_failure") => BridgeErrorClass::NumericalFailure,
            Some("model_mismatch") => BridgeErrorClass::ModelMismatch,
            _ => BridgeErrorClass::Protocol,
        };
        return fail(class, response.message.clone().unwrap_or_else(|| "model returned an error".into()));
    }
    let Some(validity) = &response.validity else {
        return fail(BridgeErrorClass::OutOfValidity, "response has no validity declaration");
    };
    if !validity.within {
        return fail(BridgeErrorClass::OutOfValidity, "inputs are outside the model's declared validity range");
    }
    checks.push("validity".to_string());

    for field in [
        response.service_level,
        response.potable_flow_m3_h,
        response.reject_flow_m3_h,
        response.head_loss_kpa,
        response.energy_kwh,
    ] {
        match field {
            Some(value) if !value.is_finite() => {
                return fail(BridgeErrorClass::NumericalFailure, "response contains a non-finite value");
            }
            Some(_) => {}
            None => return fail(BridgeErrorClass::Protocol, "response is missing a numeric output"),
        }
    }
    checks.push("finite".to_string());

    if response.units.is_empty() {
        return fail(BridgeErrorClass::DimensionMismatch, "response declares no units");
    }
    for (field, expected) in expected_units {
        match response.units.get(field) {
            Some(actual) if actual == expected => {}
            Some(actual) => {
                return fail(
                    BridgeErrorClass::DimensionMismatch,
                    format!("{field}: expected unit {expected:?}, got {actual:?}"),
                );
            }
            None => return fail(BridgeErrorClass::DimensionMismatch, format!("{field}: missing unit")),
        }
    }
    checks.push("units".to_string());

    let service_level = response.service_level.unwrap_or(0.0);
    if !(0.0..=1.0).contains(&service_level) {
        return fail(BridgeErrorClass::OutOfValidity, "service level outside 0..=1");
    }
    checks.push("service_level_range".to_string());

    // Mass conservation: inlet flow = potable + reject within 1%.
    let outlet = response.potable_flow_m3_h.unwrap_or(0.0) + response.reject_flow_m3_h.unwrap_or(0.0);
    let tolerance = 0.01 * expected_inlet_m3_h.abs() + 1e-9;
    if (outlet - expected_inlet_m3_h).abs() > tolerance {
        return fail(
            BridgeErrorClass::Conservation,
            format!("inlet {expected_inlet_m3_h:.3} m3/h but potable+reject = {outlet:.3} m3/h"),
        );
    }
    checks.push("conservation".to_string());

    Ok(ValidationReport { checks, synthetic: response.synthetic })
}
