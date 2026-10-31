//! Prediction reconciliation (M9.5): compare failure predictions with observed
//! failures and keep out-of-window cases visible.

use poie_ontology::time::TimeWindow;

#[derive(Clone, Debug, PartialEq)]
pub struct Prediction {
    pub asset: String,
    pub window: TimeWindow,
    /// Observed failure time, when it happened.
    pub observed_failure_s: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PredictionCheck {
    pub asset: String,
    pub predicted: TimeWindow,
    pub observed_failure_s: Option<f64>,
    pub inside_window: Option<bool>,
    /// 0 inside the window; signed distance to the nearest bound outside it.
    pub error_s: Option<f64>,
    /// No failure was observed by the check time.
    pub censored: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PredictionSummary {
    pub total: usize,
    pub observed: usize,
    pub censored: usize,
    pub inside_window: usize,
    pub mean_absolute_error_s: Option<f64>,
}

/// Reconciles predictions with observations at `now_s`.
pub fn check_predictions(predictions: &[Prediction], now_s: f64) -> (Vec<PredictionCheck>, PredictionSummary) {
    let checks: Vec<PredictionCheck> = predictions
        .iter()
        .map(|prediction| match prediction.observed_failure_s {
            Some(observed) => {
                let inside = prediction.window.contains(observed);
                let error = if inside {
                    0.0
                } else if observed < prediction.window.from_s {
                    observed - prediction.window.from_s
                } else {
                    observed - prediction.window.to_s
                };
                PredictionCheck {
                    asset: prediction.asset.clone(),
                    predicted: prediction.window,
                    observed_failure_s: Some(observed),
                    inside_window: Some(inside),
                    error_s: Some(error),
                    censored: false,
                }
            }
            None => PredictionCheck {
                asset: prediction.asset.clone(),
                predicted: prediction.window,
                observed_failure_s: None,
                inside_window: None,
                error_s: None,
                censored: true,
            },
        })
        .collect();
    let observed: Vec<&PredictionCheck> = checks.iter().filter(|check| !check.censored).collect();
    let inside = observed.iter().filter(|check| check.inside_window == Some(true)).count();
    let errors: Vec<f64> = observed.iter().filter_map(|check| check.error_s.map(f64::abs)).collect();
    let summary = PredictionSummary {
        total: checks.len(),
        observed: observed.len(),
        censored: checks.len() - observed.len(),
        inside_window: inside,
        mean_absolute_error_s: if errors.is_empty() {
            None
        } else {
            Some(errors.iter().sum::<f64>() / errors.len() as f64)
        },
    };
    let _ = now_s;
    (checks, summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window() -> TimeWindow {
        TimeWindow::new(4.0 * 3600.0, 8.0 * 3600.0).unwrap()
    }

    #[test]
    fn predictions_inside_and_outside_the_window_are_scored() {
        let predictions = vec![
            Prediction { asset: "pump-p1".into(), window: window(), observed_failure_s: Some(6.0 * 3600.0) },
            Prediction { asset: "bus-1".into(), window: window(), observed_failure_s: Some(2.0 * 3600.0) },
            Prediction { asset: "lathe-1".into(), window: window(), observed_failure_s: None },
        ];
        let (checks, summary) = check_predictions(&predictions, 10.0 * 3600.0);
        assert_eq!(checks[0].inside_window, Some(true));
        assert_eq!(checks[0].error_s, Some(0.0));
        assert_eq!(checks[1].inside_window, Some(false));
        assert_eq!(checks[1].error_s, Some(2.0 * 3600.0 - 4.0 * 3600.0));
        assert!(checks[2].censored);
        assert_eq!(summary.observed, 2);
        assert_eq!(summary.censored, 1);
        assert_eq!(summary.inside_window, 1);
        assert!((summary.mean_absolute_error_s.unwrap() - 3600.0).abs() < 1e-6);
    }

    #[test]
    fn censored_predictions_do_not_enter_the_mae() {
        let predictions = vec![Prediction { asset: "pump-p1".into(), window: window(), observed_failure_s: None }];
        let (_, summary) = check_predictions(&predictions, 10.0 * 3600.0);
        assert_eq!(summary.observed, 0);
        assert_eq!(summary.mean_absolute_error_s, None);
    }
}
