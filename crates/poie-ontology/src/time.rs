//! Time-aware state (M5.1; Physical Ontology §3.8 draft).
//!
//! * explicit status changes take effect at their time;
//! * a failure prediction covers a time window: inside it the status is a
//!   `predicted_failure` (a warning, not a fact); after the window, without new
//!   evidence the status is `unknown` (the prediction has expired);
//! * an explicit status change at or after the prediction's start overrides the
//!   prediction — predictions never overrule evidence;
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
pub enum TimelineStatus {
    Known(AssetStatus),
    /// Inside a predicted failure window: a warning, not a fact.
    PredictedFailure,
    /// The prediction window passed without new evidence.
    Unknown,
}

/// Status over time for one asset.
#[derive(Clone, Debug, PartialEq)]
pub struct AssetTimeline {
    pub asset: String,
    initial: AssetStatus,
    changes: Vec<StatusChange>,
    predicted_failure: Option<TimeWindow>,
}

impl AssetTimeline {
    pub fn new(asset: impl Into<String>, initial: AssetStatus) -> AssetTimeline {
        AssetTimeline { asset: asset.into(), initial, changes: Vec::new(), predicted_failure: None }
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

    /// Status from explicit changes only: predictions are warnings and do not
    /// change the current status.
    pub fn explicit_status_at(&self, t_s: f64) -> AssetStatus {
        self.changes.iter().rev().find(|change| change.at_s <= t_s).map(|change| change.status).unwrap_or(self.initial)
    }

    /// Status at `t_s` under the rules in the module docs.
    pub fn status_at(&self, t_s: f64) -> TimelineStatus {
        let latest = self.changes.iter().rev().find(|change| change.at_s <= t_s);
        if let Some(window) = self.predicted_failure {
            let prediction_applies = latest.is_none_or(|change| change.at_s <= window.from_s);
            if prediction_applies {
                if window.contains(t_s) {
                    return TimelineStatus::PredictedFailure;
                }
                if t_s > window.to_s {
                    return TimelineStatus::Unknown;
                }
            }
        }
        TimelineStatus::Known(latest.map(|change| change.status).unwrap_or(self.initial))
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
}

impl fmt::Display for TimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeError::NonFinite => f.write_str("time and stock values must be finite"),
            TimeError::InvalidWindow { from_s, to_s } => {
                write!(f, "invalid time window [{from_s}, {to_s}]: from must not exceed to")
            }
            TimeError::PredictionInThePast => f.write_str("prediction windows must not end in the past"),
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
        assert_eq!(timeline.status_at(5.0), TimelineStatus::Known(AssetStatus::Available));
        assert_eq!(timeline.status_at(15.0), TimelineStatus::Known(AssetStatus::Degraded));
        assert_eq!(timeline.status_at(20.0), TimelineStatus::Known(AssetStatus::Failed));
    }

    #[test]
    fn a_prediction_is_a_warning_and_expires_to_unknown() {
        let mut timeline = AssetTimeline::new("bus-1", AssetStatus::Available);
        timeline.predict_failure(TimeWindow::new(60.0, 120.0).unwrap()).unwrap();
        assert_eq!(timeline.status_at(30.0), TimelineStatus::Known(AssetStatus::Available));
        assert_eq!(timeline.status_at(90.0), TimelineStatus::PredictedFailure);
        assert_eq!(timeline.status_at(200.0), TimelineStatus::Unknown);
    }

    #[test]
    fn evidence_after_the_prediction_overrides_it() {
        let mut timeline = AssetTimeline::new("bus-1", AssetStatus::Available);
        timeline.predict_failure(TimeWindow::new(60.0, 120.0).unwrap()).unwrap();
        timeline.schedule_change(70.0, AssetStatus::Failed).unwrap();
        assert_eq!(timeline.status_at(90.0), TimelineStatus::Known(AssetStatus::Failed));
        assert_eq!(timeline.status_at(150.0), TimelineStatus::Known(AssetStatus::Failed));
    }

    #[test]
    fn explicit_status_ignores_predictions() {
        let mut timeline = AssetTimeline::new("bus-1", AssetStatus::Available);
        timeline.predict_failure(TimeWindow::new(60.0, 120.0).unwrap()).unwrap();
        timeline.schedule_change(100.0, AssetStatus::Degraded).unwrap();
        assert_eq!(timeline.explicit_status_at(90.0), AssetStatus::Available);
        assert_eq!(timeline.explicit_status_at(110.0), AssetStatus::Degraded);
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
    fn invalid_windows_and_non_finite_values_are_rejected() {
        assert!(matches!(TimeWindow::new(10.0, 5.0), Err(TimeError::InvalidWindow { .. })));
        assert!(matches!(TimeWindow::new(f64::NAN, 5.0), Err(TimeError::NonFinite)));
        let mut timeline = AssetTimeline::new("a", AssetStatus::Available);
        assert!(matches!(timeline.schedule_change(f64::INFINITY, AssetStatus::Failed), Err(TimeError::NonFinite)));
    }
}
