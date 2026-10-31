//! Read-only adapter contract (M9.1).

use std::collections::BTreeMap;
use std::fmt;

use poie_ontology::model::AssetStatus;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterKind {
    ReadOnly,
    /// Writable adapters may dispatch commands; they are not accepted by the
    /// shadow runtime.
    Writable {
        authorization_required: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimestampSemantics {
    /// Observation time is the source's event time.
    EventTime,
    /// Observation time is when the runtime ingested it.
    IngestTime,
    /// The source does not give a reliable timestamp; the runtime must not use
    /// the observation as precise evidence.
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisconnectBehavior {
    /// Keep the last evidence; the evidence-expiry policy decides when it turns
    /// unknown.
    RetainLastKnown,
    /// Mark the adapter's assets unknown immediately on disconnect.
    MarkUnknown,
    /// Mark unknown and report the runtime unhealthy until reconnection.
    FailClosed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UnitDecl {
    pub field: String,
    pub unit: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdapterDeclaration {
    pub adapter_id: String,
    pub version: String,
    pub kind: AdapterKind,
    /// Data source (protocol/endpoint identifier).
    pub source: String,
    /// Declared units of every reported field.
    pub units: Vec<UnitDecl>,
    pub timestamp_semantics: TimestampSemantics,
    pub disconnect_behavior: DisconnectBehavior,
    /// Compatibility test reference, when the adapter was tested against a
    /// concrete version of the external system.
    pub compatibility: Option<String>,
}

impl AdapterDeclaration {
    pub fn validate(&self) -> Result<(), AdapterError> {
        if self.adapter_id.trim().is_empty() {
            return Err(AdapterError::Invalid("adapter_id must not be empty".into()));
        }
        if self.version.trim().is_empty() {
            return Err(AdapterError::Invalid("adapter version must not be empty".into()));
        }
        if self.source.trim().is_empty() {
            return Err(AdapterError::Invalid("adapter source must not be empty".into()));
        }
        if self.units.is_empty() {
            return Err(AdapterError::Invalid("adapter must declare at least one unit".into()));
        }
        for unit in &self.units {
            if unit.field.trim().is_empty() || unit.unit.trim().is_empty() {
                return Err(AdapterError::Invalid("unit declarations need field and unit".into()));
            }
        }
        if self.timestamp_semantics == TimestampSemantics::Unknown {
            // Not an error, but an adapter that cannot timestamp cannot be used
            // for time-sensitive evidence; the runtime records that limitation.
        }
        Ok(())
    }

    pub fn is_read_only(&self) -> bool {
        matches!(self.kind, AdapterKind::ReadOnly)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservationQuality {
    Good,
    Uncertain,
    Stale,
    Bad,
}

impl ObservationQuality {
    pub fn as_str(self) -> &'static str {
        match self {
            ObservationQuality::Good => "good",
            ObservationQuality::Uncertain => "uncertain",
            ObservationQuality::Stale => "stale",
            ObservationQuality::Bad => "bad",
        }
    }
}

/// One observation of an asset's status.
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    pub asset: String,
    pub status: AssetStatus,
    /// Source event time in milliseconds.
    pub observed_at_ms: i64,
    /// Runtime ingest time in milliseconds.
    pub ingested_at_ms: i64,
    pub quality: ObservationQuality,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdapterError {
    Invalid(String),
    Disconnected(String),
    Protocol(String),
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdapterError::Invalid(detail) => write!(f, "invalid adapter: {detail}"),
            AdapterError::Disconnected(detail) => write!(f, "adapter disconnected: {detail}"),
            AdapterError::Protocol(detail) => write!(f, "adapter protocol error: {detail}"),
        }
    }
}

impl std::error::Error for AdapterError {}

/// The only adapter trait the shadow runtime accepts.
pub trait ReadOnlyAdapter {
    fn name(&self) -> &str;
    fn declaration(&self) -> &AdapterDeclaration;
    fn poll(&mut self) -> Result<Vec<Observation>, AdapterError>;
}

/// Scripted adapter used by tests and shadow demonstrations. It can return
/// observations or a configured disconnect.
#[derive(Clone, Debug)]
pub struct SimulatedAdapter {
    declaration: AdapterDeclaration,
    script: Vec<Result<Vec<Observation>, AdapterError>>,
    polls: usize,
}

impl SimulatedAdapter {
    pub fn new(declaration: AdapterDeclaration) -> SimulatedAdapter {
        SimulatedAdapter { declaration, script: Vec::new(), polls: 0 }
    }

    pub fn with_observations(mut self, observations: Vec<Observation>) -> SimulatedAdapter {
        self.script.push(Ok(observations));
        self
    }

    pub fn with_disconnect(mut self, detail: impl Into<String>) -> SimulatedAdapter {
        self.script.push(Err(AdapterError::Disconnected(detail.into())));
        self
    }

    /// Adds a poll that returns the given observations (a scripted sequence).
    pub fn then(&mut self, observations: Vec<Observation>) -> &mut SimulatedAdapter {
        self.script.push(Ok(observations));
        self
    }

    pub fn then_disconnect(&mut self, detail: impl Into<String>) -> &mut SimulatedAdapter {
        self.script.push(Err(AdapterError::Disconnected(detail.into())));
        self
    }

    pub fn declarations(adapter_id: &str, source: &str) -> AdapterDeclaration {
        AdapterDeclaration {
            adapter_id: adapter_id.to_string(),
            version: "0.1.0".to_string(),
            kind: AdapterKind::ReadOnly,
            source: source.to_string(),
            units: vec![UnitDecl { field: "asset.status".to_string(), unit: "1".to_string() }],
            timestamp_semantics: TimestampSemantics::EventTime,
            disconnect_behavior: DisconnectBehavior::MarkUnknown,
            compatibility: None,
        }
    }

    pub fn observations(asset: &str, status: AssetStatus, at_ms: i64) -> Observation {
        Observation {
            asset: asset.to_string(),
            status,
            observed_at_ms: at_ms,
            ingested_at_ms: at_ms,
            quality: ObservationQuality::Good,
        }
    }

    pub fn last_assets(observations: &[Observation]) -> BTreeMap<String, AssetStatus> {
        observations.iter().map(|observation| (observation.asset.clone(), observation.status)).collect()
    }
}

impl ReadOnlyAdapter for SimulatedAdapter {
    fn name(&self) -> &str {
        &self.declaration.adapter_id
    }

    fn declaration(&self) -> &AdapterDeclaration {
        &self.declaration
    }

    fn poll(&mut self) -> Result<Vec<Observation>, AdapterError> {
        let index = self.polls.min(self.script.len().saturating_sub(1));
        self.polls += 1;
        match self.script.get(index) {
            Some(Ok(observations)) => Ok(observations.clone()),
            Some(Err(error)) => Err(error.clone()),
            None => Ok(Vec::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declarations_are_validated() {
        let declaration = SimulatedAdapter::declarations("ditto-proxy", "eclipse-ditto/3.5");
        assert!(declaration.validate().is_ok());
        assert!(declaration.is_read_only());

        let mut broken = declaration.clone();
        broken.units.clear();
        assert!(broken.validate().is_err());
        broken = declaration.clone();
        broken.version.clear();
        assert!(broken.validate().is_err());
    }

    #[test]
    fn simulated_adapters_script_observations_and_disconnects() {
        let mut adapter = SimulatedAdapter::new(SimulatedAdapter::declarations("a", "test"))
            .with_observations(vec![SimulatedAdapter::observations("pump-p1", AssetStatus::Degraded, 1_000)]);
        adapter.then_disconnect("cable unplugged");
        let observations = adapter.poll().unwrap();
        assert_eq!(observations.len(), 1);
        assert!(matches!(adapter.poll(), Err(AdapterError::Disconnected(_))));
    }
}
