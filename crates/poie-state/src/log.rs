//! Append-only local record log (SAP §1, §5; ADR-0003).
//!
//! One record per JSON line, written with a single `write_all` followed by
//! `sync_data`. There is no update or delete API: corrections are new records.
//! On open, a trailing incomplete line left by a crash is truncated; a corrupt
//! line anywhere else is an error and is never silently dropped. Sequence
//! numbers must increase strictly per site.

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::record::Record;

/// A single-writer, append-only log file (ADR-0003).
pub struct AppendLog {
    path: PathBuf,
    last_seq: BTreeMap<String, u64>,
}

impl AppendLog {
    /// Creates (or truncates) a log at `path`.
    pub fn create(path: impl Into<PathBuf>) -> Result<AppendLog, LogError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(&path, b"")?;
        AppendLog::open(path)
    }

    /// Opens an existing log, or creates an empty one. Recovers a trailing
    /// incomplete line left by a crash (ADR-0003 §3.3).
    pub fn open(path: impl Into<PathBuf>) -> Result<AppendLog, LogError> {
        let path = path.into();
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::write(&path, b"")?;
                Vec::new()
            }
            Err(error) => return Err(error.into()),
        };
        let (records, valid_end) = parse_prefix(&bytes)?;
        if valid_end < bytes.len() {
            let file = OpenOptions::new().write(true).open(&path)?;
            file.set_len(valid_end as u64)?;
            file.sync_all()?;
        }
        let mut last_seq = BTreeMap::new();
        for record in &records {
            last_seq.insert(record.envelope.record_id.site.clone(), record.envelope.record_id.seq);
        }
        Ok(AppendLog { path, last_seq })
    }

    /// Appends one record. The sequence number must be strictly greater than the
    /// site's previous record.
    pub fn append(&mut self, record: &Record) -> Result<(), LogError> {
        let site = record.envelope.record_id.site.clone();
        let seq = record.envelope.record_id.seq;
        if let Some(previous) = self.last_seq.get(&site) {
            if seq <= *previous {
                return Err(LogError::SequenceOutOfOrder { site, last: *previous, got: seq });
            }
        }
        let mut line = serde_json::to_vec(record).map_err(LogError::Serialize)?;
        line.push(b'\n');
        let mut file = OpenOptions::new().append(true).open(&self.path)?;
        file.write_all(&line)?;
        file.sync_data()?;
        self.last_seq.insert(site, seq);
        Ok(())
    }

    /// All records in append order.
    pub fn records(&self) -> Result<Vec<Record>, LogError> {
        let bytes = fs::read(&self.path)?;
        let (records, valid_end) = parse_prefix(&bytes)?;
        if valid_end != bytes.len() {
            return Err(LogError::Corrupt(
                "the log has an incomplete trailing record; reopen the log to recover it".to_string(),
            ));
        }
        Ok(records)
    }

    /// Records written by one site, in sequence order.
    pub fn records_for_site(&self, site: &str) -> Result<Vec<Record>, LogError> {
        Ok(self.records()?.into_iter().filter(|record| record.envelope.record_id.site == site).collect())
    }

    pub fn last_seq(&self, site: &str) -> Option<u64> {
        self.last_seq.get(site).copied()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Parses the longest valid prefix of complete lines. Returns the records and the
/// byte offset after the last complete line (used to truncate a crash tail).
fn parse_prefix(bytes: &[u8]) -> Result<(Vec<Record>, usize), LogError> {
    let mut records = Vec::new();
    let mut last_seq: BTreeMap<String, u64> = BTreeMap::new();
    let mut offset = 0usize;
    let mut valid_end = 0usize;
    while offset < bytes.len() {
        let Some(relative_end) = bytes[offset..].iter().position(|byte| *byte == b'\n') else {
            break; // trailing incomplete line: recoverable
        };
        let line_end = offset + relative_end;
        let line = &bytes[offset..line_end];
        if line.is_empty() {
            return Err(LogError::Corrupt(format!("empty line at byte {offset}")));
        }
        let record: Record = serde_json::from_slice(line)
            .map_err(|error| LogError::Corrupt(format!("line at byte {offset}: {error}")))?;
        let site = record.envelope.record_id.site.clone();
        let seq = record.envelope.record_id.seq;
        match last_seq.get(&site) {
            Some(previous) if seq <= *previous => {
                return Err(LogError::SequenceOutOfOrder { site, last: *previous, got: seq });
            }
            _ => {}
        }
        last_seq.insert(site, seq);
        records.push(record);
        offset = line_end + 1;
        valid_end = offset;
    }
    Ok((records, valid_end))
}

#[derive(Debug)]
pub enum LogError {
    Io(io::Error),
    Serialize(serde_json::Error),
    Corrupt(String),
    SequenceOutOfOrder { site: String, last: u64, got: u64 },
}

impl fmt::Display for LogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogError::Io(error) => write!(f, "log io error: {error}"),
            LogError::Serialize(error) => write!(f, "cannot serialize record: {error}"),
            LogError::Corrupt(detail) => write!(f, "corrupt log: {detail}"),
            LogError::SequenceOutOfOrder { site, last, got } => {
                write!(f, "site {site:?}: sequence {got} is not greater than the last sequence {last}")
            }
        }
    }
}

impl std::error::Error for LogError {}

impl From<io::Error> for LogError {
    fn from(error: io::Error) -> LogError {
        LogError::Io(error)
    }
}

impl From<serde_json::Error> for LogError {
    fn from(error: serde_json::Error) -> LogError {
        LogError::Serialize(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{
        Envelope, Hlc, Namespace, ObservationValue, Payload, Quality, RecordId, RecordKind, Source, Timestamp,
    };

    fn temp_path(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("poie-log-test-{}-{name}-{unique}", std::process::id()))
    }

    fn record(site: &str, seq: u64, status: &str) -> Record {
        let envelope = Envelope::new(
            RecordId::new(site, seq),
            RecordKind::Observation,
            "pump-p1",
            Source::Sensor("sensor-1".into()),
            Timestamp(1_000 + seq as i64),
            Timestamp(2_000 + seq as i64),
            Hlc::new(1_000 + seq as i64, 0),
            Namespace::live(),
        );
        let payload = Payload::Observation {
            value: ObservationValue::Status(status.to_string()),
            quality: Quality::declared(None, Some("calibrated")).unwrap(),
        };
        Record::new(envelope, payload).unwrap()
    }

    #[test]
    fn records_survive_reopen_in_order() {
        let path = temp_path("reopen");
        {
            let mut log = AppendLog::create(&path).unwrap();
            log.append(&record("site-a", 1, "failed")).unwrap();
            log.append(&record("site-a", 2, "degraded")).unwrap();
            log.append(&record("site-b", 1, "available")).unwrap();
        }
        let log = AppendLog::open(&path).unwrap();
        let records = log.records().unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(records[1].envelope.record_id, RecordId::new("site-a", 2));
        assert_eq!(log.last_seq("site-a"), Some(2));
        assert_eq!(log.last_seq("site-b"), Some(1));
        assert_eq!(log.records_for_site("site-b").unwrap().len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sequence_numbers_must_increase_per_site() {
        let path = temp_path("sequence");
        let mut log = AppendLog::create(&path).unwrap();
        log.append(&record("site-a", 2, "failed")).unwrap();
        assert!(matches!(log.append(&record("site-a", 2, "degraded")), Err(LogError::SequenceOutOfOrder { .. })));
        assert!(matches!(log.append(&record("site-a", 1, "degraded")), Err(LogError::SequenceOutOfOrder { .. })));
        log.append(&record("site-b", 1, "available")).unwrap();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_crash_tail_is_truncated_on_open() {
        let path = temp_path("crash-tail");
        {
            let mut log = AppendLog::create(&path).unwrap();
            log.append(&record("site-a", 1, "failed")).unwrap();
        }
        // Simulate a crash in the middle of writing the next line.
        {
            let mut file = OpenOptions::new().append(true).open(&path).unwrap();
            file.write_all(b"{\"envelope\":{\"record_id\"").unwrap();
        }
        let mut log = AppendLog::open(&path).unwrap();
        assert_eq!(log.records().unwrap().len(), 1);
        log.append(&record("site-a", 2, "degraded")).unwrap();
        assert_eq!(log.records().unwrap().len(), 2);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupt_middle_lines_are_errors_not_silently_dropped() {
        let path = temp_path("corrupt");
        {
            let mut log = AppendLog::create(&path).unwrap();
            log.append(&record("site-a", 1, "failed")).unwrap();
        }
        {
            let mut file = OpenOptions::new().append(true).open(&path).unwrap();
            file.write_all(b"this is not json\n").unwrap();
        }
        assert!(matches!(AppendLog::open(&path), Err(LogError::Corrupt(_))));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reopening_does_not_rewrite_or_lose_records() {
        let path = temp_path("no-rewrite");
        {
            let mut log = AppendLog::create(&path).unwrap();
            log.append(&record("site-a", 1, "failed")).unwrap();
        }
        let first = AppendLog::open(&path).unwrap().records().unwrap();
        {
            let mut log = AppendLog::open(&path).unwrap();
            log.append(&record("site-a", 2, "degraded")).unwrap();
        }
        let second = AppendLog::open(&path).unwrap();
        let records = second.records().unwrap();
        assert_eq!(records[0], first[0], "append-only: the first record must not change");
        assert_eq!(records.len(), 2);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn payloads_round_trip_through_json() {
        let original = record("site-a", 7, "degraded");
        let json = serde_json::to_string(&original).unwrap();
        let decoded: Record = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, original);
    }
}
