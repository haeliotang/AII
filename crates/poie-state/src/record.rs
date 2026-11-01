//! Record kinds and the common envelope (SAP §2.1–2.2).
//!
//! Every record carries the envelope below. `event_time` (source clock) and
//! `recorded_time` (local log clock) are deliberately separate, and facts
//! (observation / estimate / prediction), intentions (desired_state / action)
//! and outcomes (transition / receipt / verification / correction) are
//! different kinds — they must not be collapsed into one another.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::action::{Action, ActionState, ReceiptKind};

/// Version string carried by every record envelope (SAP §2.1).
pub const RECORD_SCHEMA_VERSION: &str = "poie.record/0.2";

/// `<site>/<seq>` record identity (SAP §2.1).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RecordId {
    pub site: String,
    pub seq: u64,
}

impl RecordId {
    pub fn new(site: impl Into<String>, seq: u64) -> RecordId {
        RecordId { site: site.into(), seq }
    }

    pub fn parse(value: &str) -> Result<RecordId, RecordError> {
        let Some((site, seq)) = value.rsplit_once('/') else {
            return Err(RecordError::InvalidRecordId(value.to_string()));
        };
        if site.is_empty() {
            return Err(RecordError::InvalidRecordId(value.to_string()));
        }
        let seq = seq.parse().map_err(|_| RecordError::InvalidRecordId(value.to_string()))?;
        Ok(RecordId { site: site.to_string(), seq })
    }
}

impl fmt::Display for RecordId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.site, self.seq)
    }
}

impl FromStr for RecordId {
    type Err = RecordError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        RecordId::parse(s)
    }
}

/// Milliseconds since the Unix epoch, on the stated clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Timestamp(pub i64);

impl Timestamp {
    pub fn from_millis(millis: i64) -> Timestamp {
        Timestamp(millis)
    }
}

/// Hybrid logical clock value (SAP §2.1, §6.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Hlc {
    pub physical_ms: i64,
    pub logical: u32,
}

impl Hlc {
    pub fn new(physical_ms: i64, logical: u32) -> Hlc {
        Hlc { physical_ms, logical }
    }
}

/// `live` or a simulation branch (SAP §7). Records from a simulation branch must
/// never be projected into live state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Namespace {
    Live,
    Simulation(String),
}

impl Namespace {
    pub fn live() -> Namespace {
        Namespace::Live
    }

    pub fn simulation(name: impl Into<String>) -> Result<Namespace, RecordError> {
        let name = name.into();
        if name.is_empty() {
            return Err(RecordError::EmptyField("namespace"));
        }
        Ok(Namespace::Simulation(name))
    }

    pub fn is_live(&self) -> bool {
        matches!(self, Namespace::Live)
    }
}

/// Where the content came from (SAP §2.1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    Sensor(String),
    Adapter(String),
    Model(String),
    Person(String),
}

impl Source {
    fn identifier(&self) -> &str {
        match self {
            Source::Sensor(id) | Source::Adapter(id) | Source::Model(id) | Source::Person(id) => id,
        }
    }
}

/// The nine record kinds required by SAP §2.2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordKind {
    Observation,
    Estimate,
    Prediction,
    DesiredState,
    Action,
    Transition,
    Receipt,
    Verification,
    Correction,
}

impl RecordKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RecordKind::Observation => "observation",
            RecordKind::Estimate => "estimate",
            RecordKind::Prediction => "prediction",
            RecordKind::DesiredState => "desired_state",
            RecordKind::Action => "action",
            RecordKind::Transition => "transition",
            RecordKind::Receipt => "receipt",
            RecordKind::Verification => "verification",
            RecordKind::Correction => "correction",
        }
    }
}

impl fmt::Display for RecordKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A dimensioned value in SI coherent units, with the declared unit recorded for
/// display (Physical Ontology §2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Quantity {
    pub value_si: f64,
    pub unit: String,
}

/// Measurement uncertainty or a quality marker (SAP §2.2, observation).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Quality {
    pub uncertainty: Option<f64>,
    pub grade: Option<String>,
}

impl Quality {
    pub fn declared(uncertainty: Option<f64>, grade: Option<&str>) -> Result<Quality, RecordError> {
        let quality = Quality { uncertainty, grade: grade.map(str::to_string) };
        if quality.uncertainty.is_none() && quality.grade.is_none() {
            return Err(RecordError::MissingQuality);
        }
        if let Some(uncertainty) = quality.uncertainty {
            if uncertainty < 0.0 || !uncertainty.is_finite() {
                return Err(RecordError::InvalidQuantity("quality.uncertainty"));
            }
        }
        Ok(quality)
    }
}

/// What an observation reports: a measured quantity or a discrete status.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ObservationValue {
    Quantity(Quantity),
    Status(String),
}

/// Kind-specific payload. `Record::new` checks that the payload matches the
/// envelope's `kind`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Payload {
    Observation {
        value: ObservationValue,
        quality: Quality,
    },
    Estimate {
        value: Quantity,
        uncertainty: f64,
        model_version: String,
        inputs: Vec<RecordId>,
    },
    Prediction {
        value: Quantity,
        horizon_ms: u64,
        model_version: String,
        scenario: String,
    },
    DesiredState {
        target: Quantity,
        requester: String,
        valid_until: Timestamp,
    },
    Action(Box<Action>),
    Transition {
        action_id: String,
        from: ActionState,
        to: ActionState,
        reason: String,
        actor: String,
    },
    /// Executor receipt (SAP §2.2): ACK (`ack`, no `duplicate_of`), NACK
    /// (`!ack`: the executor declares it did not execute) or DUPLICATE (`ack`
    /// with `duplicate_of`: the key was already accepted for that action;
    /// nothing new was executed). See [`Payload::receipt_kind`].
    Receipt {
        action_id: String,
        recipient: String,
        ack: bool,
        reason: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duplicate_of: Option<String>,
    },
    Verification {
        action_id: String,
        evidence: Vec<RecordId>,
        outcome: VerificationOutcome,
        self_reported: bool,
    },
    Correction {
        corrected: RecordId,
        reason: String,
    },
}

/// Whether verification confirmed the intended effect (SAP §2.2, §3.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationOutcome {
    Verified,
    Contradicted,
}

impl Payload {
    /// For verification records, whether the evidence is independent of the
    /// executor (SAP §3.2 supplementary rule 3). `None` for other payloads.
    pub fn verification_is_independent(&self) -> Option<bool> {
        match self {
            Payload::Verification { self_reported, .. } => Some(!self_reported),
            _ => None,
        }
    }

    /// For receipt records, the receipt kind (SAP §3.3). `None` for other
    /// payloads.
    pub fn receipt_kind(&self) -> Option<ReceiptKind> {
        match self {
            Payload::Receipt { ack: false, .. } => Some(ReceiptKind::Nack),
            Payload::Receipt { duplicate_of: Some(_), .. } => Some(ReceiptKind::Duplicate),
            Payload::Receipt { .. } => Some(ReceiptKind::Ack),
            _ => None,
        }
    }

    pub fn kind(&self) -> RecordKind {
        match self {
            Payload::Observation { .. } => RecordKind::Observation,
            Payload::Estimate { .. } => RecordKind::Estimate,
            Payload::Prediction { .. } => RecordKind::Prediction,
            Payload::DesiredState { .. } => RecordKind::DesiredState,
            Payload::Action(_) => RecordKind::Action,
            Payload::Transition { .. } => RecordKind::Transition,
            Payload::Receipt { .. } => RecordKind::Receipt,
            Payload::Verification { .. } => RecordKind::Verification,
            Payload::Correction { .. } => RecordKind::Correction,
        }
    }
}

/// The common envelope every record must carry (SAP §2.1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub record_id: RecordId,
    pub kind: RecordKind,
    pub subject: String,
    pub site: String,
    pub source: Source,
    pub event_time: Timestamp,
    pub recorded_time: Timestamp,
    pub hlc: Hlc,
    pub causes: Vec<RecordId>,
    pub schema_version: String,
    pub namespace: Namespace,
}

impl Envelope {
    /// Builds an envelope with the current schema version.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        record_id: RecordId,
        kind: RecordKind,
        subject: impl Into<String>,
        source: Source,
        event_time: Timestamp,
        recorded_time: Timestamp,
        hlc: Hlc,
        namespace: Namespace,
    ) -> Envelope {
        Envelope {
            site: record_id.site.clone(),
            record_id,
            kind,
            subject: subject.into(),
            source,
            event_time,
            recorded_time,
            hlc,
            causes: Vec::new(),
            schema_version: RECORD_SCHEMA_VERSION.to_string(),
            namespace,
        }
    }

    pub fn with_causes(mut self, causes: Vec<RecordId>) -> Envelope {
        self.causes = causes;
        self
    }
}

/// A payload attached to its envelope; construction validates the pairing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub envelope: Envelope,
    pub payload: Payload,
}

impl Record {
    pub fn new(envelope: Envelope, payload: Payload) -> Result<Record, RecordError> {
        if envelope.schema_version != RECORD_SCHEMA_VERSION {
            return Err(RecordError::UnknownSchemaVersion(envelope.schema_version));
        }
        if envelope.kind != payload.kind() {
            return Err(RecordError::KindMismatch { envelope: envelope.kind, payload: payload.kind() });
        }
        if envelope.subject.is_empty() {
            return Err(RecordError::EmptyField("subject"));
        }
        if envelope.site.is_empty() {
            return Err(RecordError::EmptyField("site"));
        }
        if envelope.source.identifier().is_empty() {
            return Err(RecordError::EmptyField("source"));
        }
        if let Payload::Receipt { ack: false, duplicate_of: Some(_), .. } = &payload {
            return Err(RecordError::NackWithDuplicate);
        }
        Ok(Record { envelope, payload })
    }

    pub fn kind(&self) -> RecordKind {
        self.envelope.kind
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RecordError {
    EmptyField(&'static str),
    UnknownSchemaVersion(String),
    KindMismatch {
        envelope: RecordKind,
        payload: RecordKind,
    },
    InvalidRecordId(String),
    InvalidQuantity(&'static str),
    MissingQuality,
    /// A NACK means "not executed"; it cannot also name an earlier delivery.
    NackWithDuplicate,
}

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecordError::EmptyField(field) => write!(f, "field {field:?} must not be empty"),
            RecordError::UnknownSchemaVersion(version) => {
                write!(f, "unsupported record schema version {version:?}; expected {RECORD_SCHEMA_VERSION:?}")
            }
            RecordError::KindMismatch { envelope, payload } => {
                write!(f, "envelope kind {envelope:?} does not match payload kind {payload:?}")
            }
            RecordError::InvalidRecordId(value) => write!(f, "record id {value:?} must be <site>/<seq>"),
            RecordError::InvalidQuantity(field) => write!(f, "field {field:?} must be a finite non-negative number"),
            RecordError::MissingQuality => f.write_str("observation needs an uncertainty or a quality grade"),
            RecordError::NackWithDuplicate => f.write_str("a NACK receipt must not carry duplicate_of"),
        }
    }
}

impl std::error::Error for RecordError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope() -> Envelope {
        Envelope::new(
            RecordId::new("site-a", 7),
            RecordKind::Observation,
            "pump-p1",
            Source::Sensor("vibration-1".into()),
            Timestamp::from_millis(1_000),
            Timestamp::from_millis(1_005),
            Hlc::new(1_000, 0),
            Namespace::live(),
        )
    }

    fn observation() -> Payload {
        Payload::Observation {
            value: ObservationValue::Status("degraded".into()),
            quality: Quality::declared(None, Some("calibrated")).unwrap(),
        }
    }

    #[test]
    fn record_ids_round_trip_and_reject_bad_input() {
        let id: RecordId = "site-a/42".parse().unwrap();
        assert_eq!(id, RecordId::new("site-a", 42));
        assert_eq!(id.to_string(), "site-a/42");
        assert!(RecordId::parse("no-slash").is_err());
        assert!(RecordId::parse("/7").is_err());
        assert!(RecordId::parse("site/x").is_err());
    }

    #[test]
    fn valid_observation_record_is_accepted() {
        let record = Record::new(envelope(), observation()).unwrap();
        assert_eq!(record.kind(), RecordKind::Observation);
    }

    #[test]
    fn unknown_schema_is_rejected() {
        let mut env = envelope();
        env.schema_version = "poie.record/9.0".into();
        let error = Record::new(env, observation()).unwrap_err();
        assert!(matches!(error, RecordError::UnknownSchemaVersion(_)));
    }

    #[test]
    fn envelope_kind_must_match_the_payload() {
        let mut env = envelope();
        env.kind = RecordKind::Receipt;
        let error = Record::new(env, observation()).unwrap_err();
        assert!(matches!(error, RecordError::KindMismatch { .. }));
    }

    #[test]
    fn empty_subject_and_source_are_rejected() {
        let mut env = envelope();
        env.subject = String::new();
        assert!(matches!(Record::new(env, observation()), Err(RecordError::EmptyField("subject"))));

        let mut env = envelope();
        env.source = Source::Model(String::new());
        assert!(matches!(Record::new(env, observation()), Err(RecordError::EmptyField("source"))));
    }

    #[test]
    fn observation_requires_quality_evidence() {
        assert!(matches!(Quality::declared(None, None), Err(RecordError::MissingQuality)));
    }

    #[test]
    fn simulation_namespace_is_distinct_from_live() {
        assert!(Namespace::live().is_live());
        let sim = Namespace::simulation("branch-1").unwrap();
        assert!(!sim.is_live());
        assert!(Namespace::simulation("").is_err());
    }

    #[test]
    fn receipts_distinguish_ack_nack_and_duplicate() {
        let receipt = |ack: bool, duplicate_of: Option<&str>| {
            let mut env = envelope();
            env.kind = RecordKind::Receipt;
            Record::new(
                env,
                Payload::Receipt {
                    action_id: "a2".into(),
                    recipient: "exec-1".into(),
                    ack,
                    reason: None,
                    duplicate_of: duplicate_of.map(str::to_string),
                },
            )
        };
        assert_eq!(receipt(true, None).unwrap().payload.receipt_kind(), Some(ReceiptKind::Ack));
        assert_eq!(receipt(false, None).unwrap().payload.receipt_kind(), Some(ReceiptKind::Nack));
        let duplicate = receipt(true, Some("a1")).unwrap();
        assert_eq!(duplicate.payload.receipt_kind(), Some(ReceiptKind::Duplicate));
        assert!(matches!(receipt(false, Some("a1")), Err(RecordError::NackWithDuplicate)));
        // An ACK without duplicate_of serializes exactly as before.
        let json = serde_json::to_string(&receipt(true, None).unwrap()).unwrap();
        assert!(!json.contains("duplicate_of"));
        let back: Record = serde_json::from_str(&serde_json::to_string(&duplicate).unwrap()).unwrap();
        assert_eq!(back, duplicate);
    }
}
