//! Randomized append-only-log invariants (SAP §1, §5; ADR-0003):
//! append order survives reopen, sequence numbers are enforced, crash tails are
//! truncated, and corrupt middle lines are errors rather than silent drops.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use poie_state::log::{AppendLog, LogError};
use poie_state::record::{
    Envelope, Hlc, Namespace, ObservationValue, Payload, Quality, Record, RecordId, RecordKind, Source, Timestamp,
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

const SEED: u64 = 0x706f_6965_6c6f_6700; // "poielog"

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
}

fn temp_path(label: &str) -> PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("poie-log-{label}-{}-{id}-{nanos}.jsonl", std::process::id()))
}

fn record(site: &str, seq: u64, status: &str) -> Record {
    let envelope = Envelope::new(
        RecordId::new(site, seq),
        RecordKind::Observation,
        format!("asset-{seq}"),
        Source::Sensor("sensor-1".into()),
        Timestamp(seq as i64),
        Timestamp(seq as i64),
        Hlc::new(seq as i64, 0),
        Namespace::live(),
    );
    let quality = Quality::declared(Some(0.1), Some("test")).unwrap();
    Record::new(envelope, Payload::Observation { value: ObservationValue::Status(status.to_string()), quality })
        .unwrap()
}

#[test]
fn randomized_append_reopen_preserves_order_and_sequence_rules() {
    let path = temp_path("random");
    let mut rng = SplitMix64::new(SEED);
    let mut log = AppendLog::create(&path).unwrap();
    let mut expected: Vec<(String, u64, String)> = Vec::new();
    let mut last: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    for step in 0..30 {
        let site = if rng.index(2) == 0 { "site-a" } else { "site-b" };
        let seq = last.get(site).copied().unwrap_or(0) + 1 + rng.index(3) as u64;
        let status = ["available", "degraded", "failed"][rng.index(3)];
        log.append(&record(site, seq, status)).unwrap();
        last.insert(site.to_string(), seq);
        expected.push((site.to_string(), seq, status.to_string()));

        // An out-of-order append must be rejected and must not add a record.
        if step % 4 == 0 {
            let error = log.append(&record(site, seq, status));
            assert!(matches!(error, Err(LogError::SequenceOutOfOrder { .. })), "step {step}: {error:?}");
            assert_eq!(log.records().unwrap().len(), expected.len(), "step {step}: rejected append added a record");
        }

        // Reopen at random points and keep appending.
        if step % 5 == 0 {
            drop(log);
            let reopened = AppendLog::open(&path).unwrap();
            assert_eq!(reopened.records().unwrap().len(), expected.len(), "step {step}: reopen lost records");
            assert_eq!(reopened.last_seq(site), Some(seq), "step {step}: wrong last seq after reopen");
            log = reopened;
        }
    }

    let records = log.records().unwrap();
    assert_eq!(records.len(), expected.len(), "final record count");
    for (record, (site, seq, status)) in records.iter().zip(expected.iter()) {
        assert_eq!(record.envelope.record_id.site, *site);
        assert_eq!(record.envelope.record_id.seq, *seq);
        match &record.payload {
            Payload::Observation { value: ObservationValue::Status(actual), .. } => assert_eq!(actual, status),
            other => panic!("unexpected payload {other:?}"),
        }
    }
    let site_a = log.records_for_site("site-a").unwrap().len();
    assert_eq!(site_a, expected.iter().filter(|(site, ..)| site == "site-a").count());
    drop(log);
    let _ = fs::remove_file(&path);
}

#[test]
fn crash_tail_is_truncated_and_new_appends_continue() {
    let path = temp_path("tail");
    let mut log = AppendLog::create(&path).unwrap();
    for seq in 1..=5 {
        log.append(&record("site-a", seq, "available")).unwrap();
    }
    drop(log);
    // Simulate a crash mid-write: a partial JSON line without a newline.
    let mut bytes = fs::read(&path).unwrap();
    bytes.extend_from_slice(br#"{"envelope":{"record_id":"#);
    fs::write(&path, &bytes).unwrap();

    let mut reopened = AppendLog::open(&path).unwrap();
    assert_eq!(reopened.records().unwrap().len(), 5, "partial tail must be discarded");
    assert_eq!(reopened.last_seq("site-a"), Some(5));
    reopened.append(&record("site-a", 6, "degraded")).unwrap();
    assert_eq!(reopened.records().unwrap().len(), 6);
    drop(reopened);

    // A complete but invalid line in the middle is an error, never a silent drop.
    let text = fs::read_to_string(&path).unwrap();
    let mut lines: Vec<&str> = text.lines().collect();
    lines[2] = "{\"not\":\"a record\"}";
    fs::write(&path, lines.join("\n") + "\n").unwrap();
    match AppendLog::open(&path) {
        Err(LogError::Corrupt(_)) => {}
        Err(other) => panic!("middle corruption reported the wrong error: {other}"),
        Ok(_) => panic!("middle corruption was accepted"),
    }
    let _ = fs::remove_file(&path);
}

#[test]
fn an_incomplete_tail_is_reported_until_reopened() {
    let path = temp_path("incomplete");
    let mut log = AppendLog::create(&path).unwrap();
    log.append(&record("site-a", 1, "available")).unwrap();
    drop(log);
    let mut bytes = fs::read(&path).unwrap();
    bytes.extend_from_slice(br#"{"partial":"#);
    fs::write(&path, &bytes).unwrap();

    // Opening recovers (truncates the tail); but a log opened before the tail
    // existed must report the incomplete record rather than silently ignoring it.
    let recovered = AppendLog::open(&path).unwrap();
    assert_eq!(recovered.records().unwrap().len(), 1);
    drop(recovered);
    let mut bytes = fs::read(&path).unwrap();
    bytes.extend_from_slice(br#"{"partial":"#);
    fs::write(&path, &bytes).unwrap();
    let reopened = AppendLog::open(&path).unwrap();
    assert_eq!(reopened.records().unwrap().len(), 1);
    let _ = fs::remove_file(&path);
}
