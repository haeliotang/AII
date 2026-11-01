//! Read-only BaSyx AAS adapter (HTTP GET only).

use std::cell::RefCell;
use std::rc::Rc;

use poie_runtime::adapter::{
    AdapterDeclaration, AdapterError, AdapterKind, DisconnectBehavior, Observation, ObservationQuality,
    ReadOnlyAdapter, TimestampSemantics, UnitDecl,
};
use serde_json::Value;

use crate::aas::{base64url, read_submodel, SubmodelReading};
use crate::http::{get, Endpoint, HttpError, Timeouts};

/// What the last poll saw, shared with the runner for the report.
#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    pub polls: usize,
    pub last_http_status: Option<u16>,
    pub last_body: Option<String>,
    pub last_error: Option<AdapterError>,
    pub last_reading: Option<SubmodelReading>,
    pub last_observations: Vec<Observation>,
}

pub struct BasyxAdapter {
    declaration: AdapterDeclaration,
    endpoint: Endpoint,
    path: String,
    asset: String,
    timeouts: Timeouts,
    clock: fn() -> i64,
    diagnostics: Rc<RefCell<Diagnostics>>,
}

pub fn wall_clock_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

impl BasyxAdapter {
    pub fn new(base_url: &str, submodel_id: &str, asset: &str, compatibility: &str) -> Result<BasyxAdapter, String> {
        let endpoint = Endpoint::parse(base_url)?;
        let declaration = AdapterDeclaration {
            adapter_id: "basyx-aas-readonly".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            kind: AdapterKind::ReadOnly,
            source: format!("basyx-aas-environment http://{}:{}", endpoint.host, endpoint.port),
            units: vec![
                UnitDecl { field: "asset.status".to_string(), unit: "1".to_string() },
                UnitDecl { field: format!("{asset}.flow"), unit: "m3/s".to_string() },
            ],
            timestamp_semantics: TimestampSemantics::EventTime,
            disconnect_behavior: DisconnectBehavior::MarkUnknown,
            compatibility: Some(compatibility.to_string()),
        };
        Ok(BasyxAdapter {
            declaration,
            endpoint,
            path: format!("/submodels/{}", base64url(submodel_id)),
            asset: asset.to_string(),
            timeouts: Timeouts::default(),
            clock: wall_clock_ms,
            diagnostics: Rc::new(RefCell::new(Diagnostics::default())),
        })
    }

    pub fn diagnostics(&self) -> Rc<RefCell<Diagnostics>> {
        Rc::clone(&self.diagnostics)
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    fn fetch(&self, ingest_ms: i64) -> Result<Vec<Observation>, AdapterError> {
        let response = get(&self.endpoint, &self.path, &self.timeouts).map_err(|error| match error {
            // Refused, reset, closed without answer, timed out: the source is
            // not reachable. Never treated as data.
            HttpError::Connect(detail) | HttpError::Transport(detail) => AdapterError::Disconnected(detail),
            HttpError::Malformed(detail) => AdapterError::Protocol(detail),
        })?;
        let body = String::from_utf8_lossy(&response.body).to_string();
        {
            let mut diagnostics = self.diagnostics.borrow_mut();
            diagnostics.last_http_status = Some(response.status);
            diagnostics.last_body = Some(body.clone());
        }
        if response.status != 200 {
            return Err(AdapterError::Protocol(format!("HTTP {} for GET {}", response.status, self.path)));
        }
        let json: Value =
            serde_json::from_str(&body).map_err(|error| AdapterError::Protocol(format!("invalid JSON: {error}")))?;
        let reading = read_submodel(&json).map_err(AdapterError::Protocol)?;
        let (observed_at_ms, quality) = match reading.observed_at {
            Ok(at) => (at, ObservationQuality::Good),
            // No usable event time: keep the status but do not present it as
            // precise evidence.
            Err(_) => (ingest_ms, ObservationQuality::Uncertain),
        };
        let observation = Observation {
            asset: self.asset.clone(),
            status: reading.status,
            observed_at_ms,
            ingested_at_ms: ingest_ms,
            quality,
        };
        self.diagnostics.borrow_mut().last_reading = Some(reading);
        Ok(vec![observation])
    }
}

impl ReadOnlyAdapter for BasyxAdapter {
    fn name(&self) -> &str {
        &self.declaration.adapter_id
    }

    fn declaration(&self) -> &AdapterDeclaration {
        &self.declaration
    }

    fn poll(&mut self) -> Result<Vec<Observation>, AdapterError> {
        let ingest_ms = (self.clock)();
        {
            let mut diagnostics = self.diagnostics.borrow_mut();
            diagnostics.polls += 1;
            diagnostics.last_http_status = None;
            diagnostics.last_body = None;
            diagnostics.last_error = None;
            diagnostics.last_reading = None;
            diagnostics.last_observations.clear();
        }
        let outcome = self.fetch(ingest_ms);
        let mut diagnostics = self.diagnostics.borrow_mut();
        match &outcome {
            Ok(observations) => diagnostics.last_observations = observations.clone(),
            Err(error) => diagnostics.last_error = Some(error.clone()),
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poie_ontology::model::AssetStatus;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn serve_once(response: Vec<u8>) -> (u16, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 2048];
            let _ = socket.read(&mut buffer).unwrap();
            socket.write_all(&response).unwrap();
        });
        (port, handle)
    }

    #[test]
    fn a_submodel_response_becomes_an_event_time_observation() {
        let body = include_str!("../../data/submodel.template.json").replace("__OBSERVED_AT__", "2026-11-01T08:00:00Z");
        let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}", body.len()).into_bytes();
        let (port, server) = serve_once(response);
        let mut adapter =
            BasyxAdapter::new(&format!("http://127.0.0.1:{port}"), "urn:test", "pump-p1", "unit-test").unwrap();
        adapter.clock = || 1_793_520_005_000;
        assert!(adapter.declaration().validate().is_ok());
        let observations = adapter.poll().unwrap();
        server.join().unwrap();
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].status, AssetStatus::Degraded);
        assert_eq!(observations[0].observed_at_ms, 1_793_520_000_000);
        assert_eq!(observations[0].ingested_at_ms, 1_793_520_005_000);
        assert_eq!(observations[0].quality, ObservationQuality::Good);
    }

    #[test]
    fn not_found_is_a_protocol_error_and_a_closed_port_is_a_disconnect() {
        let (port, server) = serve_once(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec());
        let mut adapter =
            BasyxAdapter::new(&format!("http://127.0.0.1:{port}"), "urn:test", "pump-p1", "unit-test").unwrap();
        assert!(matches!(adapter.poll(), Err(AdapterError::Protocol(_))));
        server.join().unwrap();
        assert!(matches!(adapter.poll(), Err(AdapterError::Disconnected(_))));
        assert_eq!(adapter.diagnostics().borrow().polls, 2);
    }
}
