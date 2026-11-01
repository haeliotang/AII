//! Time-aware state (M5.1; Physical Ontology §3.8, §4).
//!
//! * the initial status and every explicit status change are evidence that
//!   takes effect at its time (the initial status at `t = 0`);
//! * evidence supports the status only while it is valid: with an evidence
//!   validity `v`, evidence older than `v` projects to `unknown` (PO §4 rule 4);
//!   without a declared validity, evidence does not expire;
//! * a failure prediction covers a time window. Inside it the status is still
//!   the one the evidence supports, plus a `predicted_failure` warning (a
//!   warning, not a fact). Evidence at or after the window start supersedes
//!   the warning. After the window the prediction has no effect at all: the
//!   status follows the evidence and its validity — predictions never overrule
//!   evidence (PO §3.8 rule 2, issue #111);
//! * stock changes accumulate over time.
//!
//! Times are seconds (`f64`). The ontology stays dimensioned; time-valued
//! configuration of the model itself is a later step (M5.6).

use std::fmt;

use crate::model::AssetStatus;

/// A closed time window `[from_s, to_s]`.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct TimeWindow {
    pub from_s: f64,
    pub to_s: f64,
}

impl TimeWindow {
    pub fn new(from_s: f64, to_s: f64) -> Result<TimeWindow, TimeError> {
        if !from_s.is_finite() || !to_s.is_finite() {
            return Err(TimeError::NonFinite);
        }
        if from_s > to_s {
            return Err(TimeError::InvalidWindow { from_s, to_s });
        }
        Ok(TimeWindow { from_s, to_s })
    }

    pub fn contains(&self, t_s: f64) -> bool {
        self.from_s <= t_s && t_s <= self.to_s
    }

    pub fn duration_s(&self) -> f64 {
        self.to_s - self.from_s
    }
}

/// A status change that takes effect at `at_s`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StatusChange {
    pub at_s: f64,
    pub status: AssetStatus,
}

/// What the time-aware state says about an asset at one instant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimelineStatus {
    /// The status the evidence supports; `Unknown` when the latest evidence has
    /// expired (or says `unknown`).
    pub status: AssetStatus,
    /// The latest evidence is older than the evidence validity.
    pub expired: bool,
    /// Inside a failure-prediction window that no newer evidence has
    /// superseded: a warning, not a fact.
    pub predicted_failure: bool,
}

/// Status over time for one asset.
#[derive(Clone, Debug, PartialEq)]
pub struct AssetTimeline {
    pub asset: String,
    initial: AssetStatus,
    changes: Vec<StatusChange>,
    predicted_failure: Option<TimeWindow>,
    evidence_validity_s: Option<f64>,
}

impl AssetTimeline {
    pub fn new(asset: impl Into<String>, initial: AssetStatus) -> AssetTimeline {
        AssetTimeline {
            asset: asset.into(),
            initial,
            changes: Vec::new(),
            predicted_failure: None,
            evidence_validity_s: None,
        }
    }

    pub fn schedule_change(&mut self, at_s: f64, status: AssetStatus) -> Result<(), TimeError> {
        if !at_s.is_finite() {
            return Err(TimeError::NonFinite);
        }
        self.changes.push(StatusChange { at_s, status });
        self.changes.sort_by(|a, b| a.at_s.partial_cmp(&b.at_s).unwrap_or(std::cmp::Ordering::Equal));
        Ok(())
    }

    pub fn predict_failure(&mut self, window: TimeWindow) -> Result<(), TimeError> {
        if window.contains(0.0) && window.from_s < 0.0 {
            return Err(TimeError::PredictionInThePast);
        }
        self.predicted_failure = Some(window);
        Ok(())
    }

    pub fn predicted_failure(&self) -> Option<TimeWindow> {
        self.predicted_failure
    }

    /// Sets how long evidence supports a status (PO §4 rule 4). `None` means the
    /// evidence does not expire.
    pub fn set_evidence_validity(&mut self, validity_s: Option<f64>) -> Result<(), TimeError> {
        if let Some(validity) = validity_s {
            if !validity.is_finite() || validity < 0.0 {
                return Err(TimeError::InvalidValidity(validity));
            }
        }
        self.evidence_validity_s = validity_s;
        Ok(())
    }

    pub fn evidence_validity(&self) -> Option<f64> {
        self.evidence_validity_s
    }

    /// The latest evidence at or before `t_s`: `(time, status)`. The initial
    /// status counts as evidence at `t = 0`.
    fn latest_evidence(&self, t_s: f64) -> (f64, AssetStatus) {
        self.changes
            .iter()
            .rev()
            .find(|change| change.at_s <= t_s)
            .map(|change| (change.at_s, change.status))
            .unwrap_or((0.0, self.initial))
    }

    /// Status at `t_s` under the rules in the module docs. The same function is
    /// used by every consumer (time-version evaluation included), so the
    /// timeline has a single semantics.
    pub fn status_at(&self, t_s: f64) -> TimelineStatus {
        let (evidence_at, evidence_status) = self.latest_evidence(t_s);
        let expired = self.evidence_validity_s.is_some_and(|validity| t_s - evidence_at > validity);
        let status = if expired { AssetStatus::Unknown } else { evidence_status };
        let predicted_failure =
            self.predicted_failure.is_some_and(|window| window.contains(t_s) && !self.superseded(window, t_s));
        TimelineStatus { status, expired, predicted_failure }
    }

    /// The failure-prediction window if it overlaps `span` and is not
    /// superseded by evidence at or after its start (before the span begins).
    /// Reports use this to flag steps that rely on an asset during a predicted
    /// failure (PO §3.8 rule 2).
    pub fn prediction_overlapping(&self, span: TimeWindow) -> Option<TimeWindow> {
        let window = self.predicted_failure?;
        let overlaps = window.from_s <= span.to_s && span.from_s <= window.to_s;
        if overlaps && !self.superseded(window, span.from_s.max(window.from_s)) {
            Some(window)
        } else {
            None
        }
    }

    /// Evidence at or after the window start and at or before `t_s` supersedes
    /// the prediction.
    fn superseded(&self, window: TimeWindow, t_s: f64) -> bool {
        self.changes.iter().any(|change| change.at_s >= window.from_s && change.at_s <= t_s)
    }
}

/// A stock change at `at_s` (positive supply, negative consumption).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StockChange {
    pub at_s: f64,
    pub delta_si: f64,
}

/// Stock over time for one resource or lot.
#[derive(Clone, Debug, PartialEq)]
pub struct StockTimeline {
    pub resource: String,
    initial_si: f64,
    changes: Vec<StockChange>,
}

impl StockTimeline {
    pub fn new(resource: impl Into<String>, initial_si: f64) -> Result<StockTimeline, TimeError> {
        if !initial_si.is_finite() {
            return Err(TimeError::NonFinite);
        }
        Ok(StockTimeline { resource: resource.into(), initial_si, changes: Vec::new() })
    }

    pub fn schedule_change(&mut self, at_s: f64, delta_si: f64) -> Result<(), TimeError> {
        if !at_s.is_finite() || !delta_si.is_finite() {
            return Err(TimeError::NonFinite);
        }
        self.changes.push(StockChange { at_s, delta_si });
        self.changes.sort_by(|a, b| a.at_s.partial_cmp(&b.at_s).unwrap_or(std::cmp::Ordering::Equal));
        Ok(())
    }

    pub fn available_at(&self, t_s: f64) -> f64 {
        self.initial_si
            + self.changes.iter().filter(|change| change.at_s <= t_s).map(|change| change.delta_si).sum::<f64>()
    }

    /// Lowest availability over `window`, used to check that a plan never drives
    /// the stock negative.
    pub fn min_available(&self, window: TimeWindow) -> Result<f64, TimeError> {
        let mut minimum = self.available_at(window.from_s);
        for change in self.changes.iter().filter(|change| change.at_s > window.from_s && change.at_s <= window.to_s) {
            minimum = minimum.min(self.available_at(change.at_s));
        }
        Ok(minimum)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TimeError {
    NonFinite,
    InvalidWindow { from_s: f64, to_s: f64 },
    PredictionInThePast,
    InvalidValidity(f64),
}

impl fmt::Display for TimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeError::NonFinite => f.write_str("time and stock values must be finite"),
            TimeError::InvalidWindow { from_s, to_s } => {
                write!(f, "invalid time window [{from_s}, {to_s}]: from must not exceed to")
            }
            TimeError::PredictionInThePast => f.write_str("prediction windows must not end in the past"),
            TimeError::InvalidValidity(validity) => {
                write!(f, "evidence validity must be finite and non-negative, got {validity}")
            }
        }
    }
}

impl std::error::Error for TimeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_changes_take_effect_at_their_time() {
        let mut timeline = AssetTimeline::new("pump-p1", AssetStatus::Available);
        timeline.schedule_change(10.0, AssetStatus::Degraded).unwrap();
        timeline.schedule_change(20.0, AssetStatus::Failed).unwrap();
        assert_eq!(timeline.status_at(5.0).status, AssetStatus::Available);
        assert_eq!(timeline.status_at(15.0).status, AssetStatus::Degraded);
        assert_eq!(timeline.status_at(20.0).status, AssetStatus::Failed);
    }

    #[test]
    fn a_prediction_is_a_warning_and_ends_with_its_window() {
        // Before #111 the status became `unknown` after the window; PO §3.8 now
        // leaves the status to the evidence and its validity (A-09).
        let mut timeline = AssetTimeline::new("bus-1", AssetStatus::Available);
        timeline.predict_failure(TimeWindow::new(60.0, 120.0).unwrap()).unwrap();
        assert!(!timeline.status_at(30.0).predicted_failure);
        assert!(timeline.status_at(90.0).predicted_failure);
        let after = timeline.status_at(200.0);
        assert_eq!(after.status, AssetStatus::Available);
        assert!(!after.predicted_failure);
        timeline.set_evidence_validity(Some(150.0)).unwrap();
        assert_eq!(timeline.status_at(200.0).status, AssetStatus::Unknown);
    }

    #[test]
    fn evidence_after_the_prediction_overrides_it() {
        let mut timeline = AssetTimeline::new("bus-1", AssetStatus::Available);
        timeline.predict_failure(TimeWindow::new(60.0, 120.0).unwrap()).unwrap();
        timeline.schedule_change(70.0, AssetStatus::Failed).unwrap();
        assert_eq!(
            timeline.status_at(90.0),
            TimelineStatus { status: AssetStatus::Failed, expired: false, predicted_failure: false }
        );
        assert_eq!(timeline.status_at(150.0).status, AssetStatus::Failed);
    }

    #[test]
    fn predictions_overlapping_a_span_are_reported_unless_superseded() {
        let mut timeline = AssetTimeline::new("bus-1", AssetStatus::Available);
        timeline.predict_failure(TimeWindow::new(60.0, 120.0).unwrap()).unwrap();
        assert!(timeline.prediction_overlapping(TimeWindow::new(0.0, 50.0).unwrap()).is_none());
        assert!(timeline.prediction_overlapping(TimeWindow::new(30.0, 70.0).unwrap()).is_some());
        timeline.schedule_change(65.0, AssetStatus::Available).unwrap();
        assert!(timeline.prediction_overlapping(TimeWindow::new(80.0, 90.0).unwrap()).is_none());
        assert!(timeline.prediction_overlapping(TimeWindow::new(30.0, 70.0).unwrap()).is_some());
    }

    // Issue #111 (P14, reviews A-09 / B-10): PO §3.8 and §4 must agree. A
    // prediction is a warning; once its window ends the status follows the
    // evidence-validity rule of §4, never an unconditional `unknown`.

    #[test]
    fn po3_8_valid_observation_survives_the_end_of_a_prediction_window() {
        // A-09 counterexample 1: observed available at 0 h, valid for 24 h;
        // predicted failure in [2 h, 4 h]; at 5 h the observation is still valid.
        let mut timeline = AssetTimeline::new("pump-p1", AssetStatus::Available);
        timeline.set_evidence_validity(Some(24.0 * 3600.0)).unwrap();
        timeline.predict_failure(TimeWindow::new(2.0 * 3600.0, 4.0 * 3600.0).unwrap()).unwrap();
        let at_5h = timeline.status_at(5.0 * 3600.0);
        assert_eq!(at_5h.status, AssetStatus::Available);
        assert!(!at_5h.predicted_failure);
        // A fresh observation at 1.9 h is not downgraded after the window either.
        timeline.schedule_change(1.9 * 3600.0, AssetStatus::Available).unwrap();
        assert_eq!(timeline.status_at(5.0 * 3600.0).status, AssetStatus::Available);
    }

    #[test]
    fn po3_8_inside_the_window_the_status_is_the_evidence_plus_a_warning() {
        // A-09 counterexample 2: the warning does not replace the status.
        let mut timeline = AssetTimeline::new("bus-1", AssetStatus::Available);
        timeline.predict_failure(TimeWindow::new(60.0, 120.0).unwrap()).unwrap();
        let inside = timeline.status_at(90.0);
        assert_eq!(inside.status, AssetStatus::Available);
        assert!(inside.predicted_failure);
        timeline.schedule_change(100.0, AssetStatus::Failed).unwrap();
        let after_evidence = timeline.status_at(110.0);
        assert_eq!(after_evidence.status, AssetStatus::Failed);
        assert!(!after_evidence.predicted_failure, "evidence inside the window supersedes the prediction");
    }

    #[test]
    fn po4_4_timeline_evidence_older_than_its_validity_is_unknown() {
        // B-10: available at 0, validity 24 h, no further evidence; at 5 d the
        // state must be `unknown`, not the last value.
        let mut timeline = AssetTimeline::new("pump-p1", AssetStatus::Available);
        timeline.set_evidence_validity(Some(24.0 * 3600.0)).unwrap();
        assert_eq!(timeline.status_at(24.0 * 3600.0).status, AssetStatus::Available);
        let at_5d = timeline.status_at(5.0 * 86400.0);
        assert_eq!(at_5d.status, AssetStatus::Unknown);
        assert!(at_5d.expired);
        // New evidence restarts the validity period.
        timeline.schedule_change(4.5 * 86400.0, AssetStatus::Degraded).unwrap();
        assert_eq!(timeline.status_at(5.0 * 86400.0).status, AssetStatus::Degraded);
    }

    #[test]
    fn stock_accumulates_and_minimum_detects_shortfalls() {
        let mut stock = StockTimeline::new("bronze-bar", 3.0).unwrap();
        stock.schedule_change(10.0, -2.0).unwrap();
        stock.schedule_change(20.0, 4.0).unwrap();
        assert_eq!(stock.available_at(5.0), 3.0);
        assert_eq!(stock.available_at(15.0), 1.0);
        assert_eq!(stock.available_at(25.0), 5.0);
        assert_eq!(stock.min_available(TimeWindow::new(0.0, 25.0).unwrap()).unwrap(), 1.0);
    }

    #[test]
    fn invalid_windows_and_non_finite_values_are_rejected() {
        assert!(matches!(TimeWindow::new(10.0, 5.0), Err(TimeError::InvalidWindow { .. })));
        assert!(matches!(TimeWindow::new(f64::NAN, 5.0), Err(TimeError::NonFinite)));
        let mut timeline = AssetTimeline::new("a", AssetStatus::Available);
        assert!(matches!(timeline.schedule_change(f64::INFINITY, AssetStatus::Failed), Err(TimeError::NonFinite)));
    }
}
