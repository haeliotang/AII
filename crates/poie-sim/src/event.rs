//! Events and flows for the simulator.

use poie_ontology::model::AssetStatus;

/// A resource flow inside a transfer.
#[derive(Clone, Debug, PartialEq)]
pub struct Flow {
    pub resource: String,
    pub amount_si: f64,
}

/// One scheduled change.
#[derive(Clone, Debug, PartialEq)]
pub enum EventKind {
    /// Changes an asset's status (evidence-style disturbance).
    SetAssetStatus { asset: String, status: AssetStatus },
    /// Adds (positive) or removes (negative) stock of one resource.
    StockFlow { resource: String, delta_si: f64, external: bool },
    /// An internal conversion: inputs are consumed, outputs produced, and the
    /// mass must balance (Physical Ontology §5).
    Transfer { id: String, inputs: Vec<Flow>, outputs: Vec<Flow>, asset_mass_change_kg: f64 },
    /// Charges (positive) or discharges (negative) a battery over a duration.
    EnergyRequest { battery: String, amount_si: f64, duration_s: f64 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub at_s: f64,
    pub kind: EventKind,
}

impl Event {
    pub fn new(at_s: f64, kind: EventKind) -> Result<Event, SimError> {
        if !at_s.is_finite() || at_s < 0.0 {
            return Err(SimError::InvalidTime(at_s));
        }
        validate_kind(&kind)?;
        Ok(Event { at_s, kind })
    }

    /// Stable description used in the event log and the replay digest.
    pub fn describe(&self) -> String {
        match &self.kind {
            EventKind::SetAssetStatus { asset, status } => format!("{:.3}s set {asset}={status}", self.at_s),
            EventKind::StockFlow { resource, delta_si, external } => {
                format!("{:.3}s stock {resource} {delta_si:+.6} (external={external})", self.at_s)
            }
            EventKind::Transfer { id, inputs, outputs, asset_mass_change_kg } => format!(
                "{:.3}s transfer {id}: in [{}] out [{}] asset {asset_mass_change_kg:+.6} kg",
                self.at_s,
                describe_flows(inputs),
                describe_flows(outputs)
            ),
            EventKind::EnergyRequest { battery, amount_si, duration_s } => {
                format!("{:.3}s battery {battery} request {amount_si:+.6} J over {duration_s:.3}s", self.at_s)
            }
        }
    }
}

fn describe_flows(flows: &[Flow]) -> String {
    flows.iter().map(|flow| format!("{}={:.6}", flow.resource, flow.amount_si)).collect::<Vec<_>>().join(",")
}

fn validate_kind(kind: &EventKind) -> Result<(), SimError> {
    match kind {
        EventKind::SetAssetStatus { asset, .. } => {
            if asset.is_empty() {
                return Err(SimError::InvalidFlow("asset must not be empty".into()));
            }
        }
        EventKind::StockFlow { resource, delta_si, .. } => {
            if resource.is_empty() || !delta_si.is_finite() {
                return Err(SimError::InvalidFlow("stock flow needs a resource and a finite delta".into()));
            }
        }
        EventKind::Transfer { id, inputs, outputs, asset_mass_change_kg } => {
            if id.is_empty() || inputs.is_empty() || outputs.is_empty() || !asset_mass_change_kg.is_finite() {
                return Err(SimError::InvalidFlow("transfer needs id, inputs, outputs".into()));
            }
        }
        EventKind::EnergyRequest { battery, amount_si, duration_s } => {
            if battery.is_empty() || !amount_si.is_finite() || !duration_s.is_finite() || *duration_s <= 0.0 {
                return Err(SimError::InvalidFlow(
                    "energy request needs battery, finite amount and positive duration".into(),
                ));
            }
        }
    }
    Ok(())
}

/// Errors raised by the simulator.
#[derive(Clone, Debug, PartialEq)]
pub enum SimError {
    InvalidTime(f64),
    InvalidFlow(String),
    UnknownAsset(String),
    UnknownResource(String),
    UnknownBattery(String),
    EventInThePast { at_s: f64, now_s: f64 },
}

impl std::fmt::Display for SimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SimError::InvalidTime(time) => write!(f, "invalid event time {time}"),
            SimError::InvalidFlow(detail) => write!(f, "invalid event: {detail}"),
            SimError::UnknownAsset(asset) => write!(f, "unknown asset {asset:?}"),
            SimError::UnknownResource(resource) => write!(f, "unknown resource {resource:?}"),
            SimError::UnknownBattery(battery) => write!(f, "unknown battery {battery:?}"),
            SimError::EventInThePast { at_s, now_s } => write!(f, "event at {at_s} is before the current time {now_s}"),
        }
    }
}

impl std::error::Error for SimError {}
