//! Local shadow runtime: adapter polling, observation records, projection with
//! evidence expiry, and health checks (M9.3). No cloud connection is required.

use std::collections::BTreeMap;
use std::path::PathBuf;

use poie_ontology::model::{AssetStatus, Model};
use poie_state::bridge::patched_model;
use poie_state::projection::{project, EvidencePolicy, Projected};
use poie_state::record::{
    Envelope, Hlc, Namespace, ObservationValue, Payload, Quality, Record, RecordId, RecordKind, Source, Timestamp,
};

use crate::adapter::{AdapterError, DisconnectBehavior, Observation, ReadOnlyAdapter};

#[derive(Clone, Debug, PartialEq)]
pub struct AdapterHealth {
    pub connected: bool,
    pub last_observation_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HealthReport {
    pub adapters: BTreeMap<String, AdapterHealth>,
    pub expired_evidence: usize,
    pub healthy: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PollReport {
    pub accepted_observations: usize,
    pub disconnected_adapters: Vec<String>,
    pub projected_assets: BTreeMap<String, AssetStatus>,
    pub expired_evidence: usize,
    pub health: HealthReport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeError {
    WritableAdapter(String),
    InvalidAdapter(String),
    Adapter(String),
    Projection(String),
    Model(String),
    Log(String),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeError::WritableAdapter(id) => {
                write!(f, "writable adapter {id:?} is not accepted by the shadow runtime")
            }
            RuntimeError::InvalidAdapter(detail) => write!(f, "invalid adapter declaration: {detail}"),
            RuntimeError::Adapter(detail) => write!(f, "adapter error: {detail}"),
            RuntimeError::Projection(detail) => write!(f, "projection error: {detail}"),
            RuntimeError::Model(detail) => write!(f, "model error: {detail}"),
            RuntimeError::Log(detail) => write!(f, "log error: {detail}"),
        }
    }
}

impl std::error::Error for RuntimeError {}

/// Local, offline shadow runtime. It only accepts read-only adapters.
pub struct LocalRuntime {
    site: String,
    pub model: Model,
    adapters: Vec<Box<dyn ReadOnlyAdapter>>,
    policy: EvidencePolicy,
    records: Vec<Record>,
    seq: u64,
    log_path: Option<PathBuf>,
    last_assets: BTreeMap<String, BTreeMap<String, AssetStatus>>,
    health: BTreeMap<String, AdapterHealth>,
}

impl LocalRuntime {
    pub fn new(
        site: impl Into<String>,
        model: Model,
        adapters: Vec<Box<dyn ReadOnlyAdapter>>,
        policy: EvidencePolicy,
    ) -> Result<LocalRuntime, RuntimeError> {
        for adapter in &adapters {
            let declaration = adapter.declaration();
            declaration.validate().map_err(|error| RuntimeError::InvalidAdapter(error.to_string()))?;
            if !declaration.is_read_only() {
                return Err(RuntimeError::WritableAdapter(declaration.adapter_id.clone()));
            }
        }
        let mut health = BTreeMap::new();
        for adapter in &adapters {
            health
                .entry(adapter.name().to_string())
                .or_insert(AdapterHealth { connected: true, last_observation_ms: None });
        }
        Ok(LocalRuntime {
            site: site.into(),
            model,
            adapters,
            policy,
            records: Vec::new(),
            seq: 0,
            log_path: None,
            last_assets: BTreeMap::new(),
            health,
        })
    }

    /// Enables an append-only JSON Lines adapter log. Offline by default.
    pub fn with_log(mut self, path: impl Into<PathBuf>) -> LocalRuntime {
        self.log_path = Some(path.into());
        self
    }

    pub fn poll(&mut self, now_ms: i64) -> Result<PollReport, RuntimeError> {
        let mut accepted = 0usize;
        let mut disconnected = Vec::new();
        for adapter_index in 0..self.adapters.len() {
            let name = self.adapters[adapter_index].name().to_string();
            let declaration = self.adapters[adapter_index].declaration().clone();
            let outcome = self.adapters[adapter_index].poll();
            match outcome {
                Ok(observations) => {
                    self.health
                        .insert(name.clone(), AdapterHealth { connected: true, last_observation_ms: Some(now_ms) });
                    let mut known = BTreeMap::new();
                    for observation in observations {
                        if !self.model.assets.contains_key(&observation.asset) {
                            continue;
                        }
                        known.insert(observation.asset.clone(), observation.status);
                        self.append_observation(&name, &observation)?;
                        accepted += 1;
                    }
                    // Remember every asset this adapter has ever reported, so a
                    // later disconnect marks all of them unknown (not only the
                    // assets of the most recent successful poll).
                    self.last_assets.entry(name).or_default().extend(known);
                }
                Err(error) => {
                    self.health.insert(name.clone(), AdapterHealth { connected: false, last_observation_ms: None });
                    disconnected.push(name.clone());
                    if matches!(
                        declaration.disconnect_behavior,
                        DisconnectBehavior::MarkUnknown | DisconnectBehavior::FailClosed
                    ) {
                        let assets = self.last_assets.get(&name).cloned().unwrap_or_default();
                        for asset in assets.keys() {
                            let observation = Observation {
                                asset: asset.clone(),
                                status: AssetStatus::Unknown,
                                observed_at_ms: now_ms,
                                ingested_at_ms: now_ms,
                                quality: crate::adapter::ObservationQuality::Bad,
                            };
                            self.append_observation(&name, &observation)?;
                        }
                    }
                    let _ = error;
                }
            }
        }
        let health = self.health(now_ms)?;
        let projected = self.projected_model(now_ms)?;
        let projected_assets = projected.assets.iter().map(|(id, asset)| (id.clone(), asset.status)).collect();
        Ok(PollReport {
            accepted_observations: accepted,
            disconnected_adapters: disconnected,
            projected_assets,
            expired_evidence: health.expired_evidence,
            health,
        })
    }

    /// Projects the observation log at `now_ms`, applies evidence expiry, and
    /// returns the ontology model patched to the projected asset statuses.
    pub fn projected_model(&self, now_ms: i64) -> Result<Model, RuntimeError> {
        let state = project(&self.records, self.policy, now_ms);
        patched_model(&self.model, &state).map_err(|error| RuntimeError::Model(error.to_string()))
    }

    pub fn health(&self, now_ms: i64) -> Result<HealthReport, RuntimeError> {
        let state = project(&self.records, self.policy, now_ms);
        let expired_evidence =
            state.assets.values().filter(|value| matches!(value, Projected::Unknown { expired: true })).count();
        let healthy = self.health.values().all(|adapter| adapter.connected);
        Ok(HealthReport { adapters: self.health.clone(), expired_evidence, healthy })
    }

    pub fn records(&self) -> &[Record] {
        &self.records
    }

    fn append_observation(&mut self, adapter: &str, observation: &Observation) -> Result<(), RuntimeError> {
        let envelope = Envelope::new(
            RecordId::new(&self.site, self.seq),
            RecordKind::Observation,
            &observation.asset,
            Source::Adapter(adapter.to_string()),
            Timestamp(observation.observed_at_ms),
            Timestamp(observation.ingested_at_ms),
            Hlc::new(observation.observed_at_ms, 0),
            Namespace::live(),
        );
        let quality = Quality::declared(None, Some(&format!("adapter:{}", observation.quality.as_str())))
            .map_err(|error| RuntimeError::Projection(error.to_string()))?;
        let payload = Payload::Observation { value: ObservationValue::Status(observation.status.to_string()), quality };
        let record = Record::new(envelope, payload).map_err(|error| RuntimeError::Projection(error.to_string()))?;
        if let Some(path) = &self.log_path {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|error| RuntimeError::Log(error.to_string()))?;
            let line = serde_json::to_string(&record).map_err(|error| RuntimeError::Log(error.to_string()))?;
            writeln!(file, "{line}").map_err(|error| RuntimeError::Log(error.to_string()))?;
        }
        self.records.push(record);
        self.seq += 1;
        Ok(())
    }
}

/// Convenience: report the adapter error without hiding its class.
pub fn describe_adapter_error(error: &AdapterError) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{AdapterKind, SimulatedAdapter, TimestampSemantics, UnitDecl};
    use poie_state::projection::EvidencePolicy;
    use std::path::PathBuf;

    fn base01() -> Model {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01/model.toml");
        Model::load(&path).unwrap()
    }

    fn runtime(adapter: SimulatedAdapter, max_age_ms: u64) -> LocalRuntime {
        LocalRuntime::new("site-a", base01(), vec![Box::new(adapter)], EvidencePolicy::new(max_age_ms)).unwrap()
    }

    #[test]
    fn observations_are_projected_and_expire() {
        let adapter = SimulatedAdapter::new(SimulatedAdapter::declarations("sim-1", "scripted"))
            .with_observations(vec![SimulatedAdapter::observations("pump-p1", AssetStatus::Available, 1_000)]);
        let mut runtime = runtime(adapter, 5_000);
        let report = runtime.poll(1_000).unwrap();
        assert_eq!(report.accepted_observations, 1);
        let projected = runtime.projected_model(2_000).unwrap();
        assert_eq!(projected.assets["pump-p1"].status, AssetStatus::Available);
        // After the evidence-expiry window the status becomes unknown.
        let expired = runtime.projected_model(10_000).unwrap();
        assert_eq!(expired.assets["pump-p1"].status, AssetStatus::Unknown);
        let health = runtime.health(10_000).unwrap();
        assert_eq!(health.expired_evidence, 1);
    }

    #[test]
    fn a_disconnect_marks_assets_unknown() {
        let adapter = SimulatedAdapter::new(SimulatedAdapter::declarations("sim-1", "scripted"))
            .with_observations(vec![SimulatedAdapter::observations("pump-p1", AssetStatus::Available, 1_000)]);
        let mut adapter = adapter;
        adapter.then_disconnect("cable unplugged");
        let mut runtime = runtime(adapter, 60_000);
        runtime.poll(1_000).unwrap();
        let report = runtime.poll(2_000).unwrap();
        assert_eq!(report.disconnected_adapters, vec!["sim-1"]);
        assert!(!report.health.healthy);
        let projected = runtime.projected_model(2_000).unwrap();
        assert_eq!(projected.assets["pump-p1"].status, AssetStatus::Unknown);
    }

    #[test]
    fn writable_adapters_are_rejected() {
        let mut declaration = SimulatedAdapter::declarations("writer", "test");
        declaration.kind = AdapterKind::Writable { authorization_required: true };
        let adapter = SimulatedAdapter::new(declaration);
        let result = LocalRuntime::new("site-a", base01(), vec![Box::new(adapter)], EvidencePolicy::new(1_000));
        assert!(matches!(result, Err(RuntimeError::WritableAdapter(_))));
    }

    #[test]
    fn declarations_need_units_and_timestamp_semantics() {
        let mut declaration = SimulatedAdapter::declarations("sim", "test");
        declaration.units = vec![UnitDecl { field: "asset.status".into(), unit: "1".into() }];
        declaration.timestamp_semantics = TimestampSemantics::EventTime;
        assert!(declaration.validate().is_ok());
        declaration.units.clear();
        assert!(declaration.validate().is_err());
    }
}
