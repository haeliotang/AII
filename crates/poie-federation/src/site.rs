//! One simulated site's record log (M12.1).

use std::collections::BTreeSet;

use poie_state::record::{Record, RecordId};

use crate::hlc::HlcClock;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FederationError {
    UnknownSite(String),
    WrongSite { expected: String, actual: String },
    NonMonotonicSequence { site: String, seq: u64, last: u64 },
}

impl std::fmt::Display for FederationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FederationError::UnknownSite(site) => write!(f, "unknown site {site:?}"),
            FederationError::WrongSite { expected, actual } => {
                write!(f, "record belongs to site {actual:?}, expected {expected:?}")
            }
            FederationError::NonMonotonicSequence { site, seq, last } => {
                write!(f, "site {site:?}: sequence {seq} is not after {last}")
            }
        }
    }
}

impl std::error::Error for FederationError {}

#[derive(Clone, Debug)]
pub struct Site {
    pub name: String,
    pub clock: HlcClock,
    records: Vec<Record>,
    ids: BTreeSet<RecordId>,
}

impl Site {
    pub fn new(name: impl Into<String>, start_ms: i64) -> Site {
        Site { name: name.into(), clock: HlcClock::new(start_ms), records: Vec::new(), ids: BTreeSet::new() }
    }

    pub fn records(&self) -> &[Record] {
        &self.records
    }

    pub fn contains(&self, id: &RecordId) -> bool {
        self.ids.contains(id)
    }

    pub fn last_seq(&self) -> u64 {
        self.records
            .iter()
            .filter(|record| record.envelope.record_id.site == self.name)
            .map(|record| record.envelope.record_id.seq)
            .max()
            .unwrap_or(0)
    }

    /// Appends a locally produced record; enforces the site and a strictly
    /// increasing per-site sequence.
    pub fn append_local(&mut self, record: Record) -> Result<(), FederationError> {
        if record.envelope.record_id.site != self.name || record.envelope.site != self.name {
            return Err(FederationError::WrongSite {
                expected: self.name.clone(),
                actual: record.envelope.record_id.site.clone(),
            });
        }
        let last = self.last_seq();
        if record.envelope.record_id.seq <= last {
            return Err(FederationError::NonMonotonicSequence {
                site: self.name.clone(),
                seq: record.envelope.record_id.seq,
                last,
            });
        }
        self.ids.insert(record.envelope.record_id.clone());
        self.records.push(record);
        Ok(())
    }

    /// Merges records received from another site. Records are deduplicated by
    /// `RecordId`; missing ones are inserted in `(site, seq)` order. Returns the
    /// number of newly inserted records.
    pub fn merge(&mut self, incoming: &[Record]) -> usize {
        let mut inserted = 0;
        let mut sorted: Vec<&Record> = incoming.iter().collect();
        sorted.sort_by(|left, right| {
            left.envelope
                .record_id
                .site
                .cmp(&right.envelope.record_id.site)
                .then_with(|| left.envelope.record_id.seq.cmp(&right.envelope.record_id.seq))
        });
        for record in sorted {
            if self.ids.insert(record.envelope.record_id.clone()) {
                self.records.push(record.clone());
                inserted += 1;
            }
        }
        self.records.sort_by(|left, right| {
            left.envelope
                .record_id
                .site
                .cmp(&right.envelope.record_id.site)
                .then_with(|| left.envelope.record_id.seq.cmp(&right.envelope.record_id.seq))
        });
        inserted
    }

    pub fn records_ids(&self) -> Vec<RecordId> {
        self.records.iter().map(|record| record.envelope.record_id.clone()).collect()
    }
}
