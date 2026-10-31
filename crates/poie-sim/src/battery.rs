//! Battery storage with charge/discharge power and capacity limits (M5.3).

use crate::event::SimError;

/// Why an energy request was clipped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnergyLimit {
    None,
    ChargePower,
    DischargePower,
    Full,
    Empty,
}

/// Result of applying an energy request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnergyOutcome {
    pub requested_si: f64,
    pub applied_si: f64,
    pub clipped_si: f64,
    pub reason: EnergyLimit,
}

/// A single battery bank. Energy is in joules (SI), power in watts.
#[derive(Clone, Debug)]
pub struct Battery {
    pub id: String,
    pub capacity_si: f64,
    pub soc_si: f64,
    pub max_charge_w: f64,
    pub max_discharge_w: f64,
}

impl Battery {
    pub fn new(
        id: impl Into<String>,
        capacity_si: f64,
        soc_si: f64,
        max_charge_w: f64,
        max_discharge_w: f64,
    ) -> Result<Battery, SimError> {
        let id = id.into();
        if id.is_empty()
            || !capacity_si.is_finite()
            || capacity_si <= 0.0
            || !soc_si.is_finite()
            || soc_si < 0.0
            || soc_si > capacity_si
            || !max_charge_w.is_finite()
            || max_charge_w < 0.0
            || !max_discharge_w.is_finite()
            || max_discharge_w < 0.0
        {
            return Err(SimError::InvalidFlow(
                "battery needs a positive capacity, in-range SoC and non-negative powers".into(),
            ));
        }
        Ok(Battery { id, capacity_si, soc_si, max_charge_w, max_discharge_w })
    }

    /// Applies a charge (positive) or discharge (negative) request of at most
    /// `|amount_si|` over `duration_s`, clipped by power and capacity/SoC.
    pub fn request(&mut self, amount_si: f64, duration_s: f64) -> EnergyOutcome {
        if amount_si >= 0.0 {
            let power_limit = self.max_charge_w * duration_s;
            let room = self.capacity_si - self.soc_si;
            let applied = amount_si.min(power_limit).min(room);
            self.soc_si += applied;
            let clipped = amount_si - applied;
            let reason = if clipped <= 0.0 {
                EnergyLimit::None
            } else if applied < room {
                EnergyLimit::ChargePower
            } else {
                EnergyLimit::Full
            };
            EnergyOutcome { requested_si: amount_si, applied_si: applied, clipped_si: clipped, reason }
        } else {
            let requested_abs = -amount_si;
            let power_limit = self.max_discharge_w * duration_s;
            let applied_abs = requested_abs.min(power_limit).min(self.soc_si);
            self.soc_si -= applied_abs;
            let clipped = requested_abs - applied_abs;
            let reason = if clipped <= 0.0 {
                EnergyLimit::None
            } else if applied_abs < self.soc_si + applied_abs {
                EnergyLimit::DischargePower
            } else {
                EnergyLimit::Empty
            };
            EnergyOutcome { requested_si: amount_si, applied_si: -applied_abs, clipped_si: -clipped, reason }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charge_is_clipped_by_power_then_capacity() {
        // 10 kWh battery, 2 kW charger: 5 kWh requested over one hour applies 2 kWh.
        let mut battery = Battery::new("bat-1", 36_000_000.0, 0.0, 2_000.0, 3_000.0).unwrap();
        let outcome = battery.request(18_000_000.0, 3_600.0);
        assert!((outcome.applied_si - 7_200_000.0).abs() < 1e-6, "{outcome:?}");
        assert_eq!(outcome.reason, EnergyLimit::ChargePower);
        assert!((battery.soc_si - 7_200_000.0).abs() < 1e-6);

        // Nearly full: only the remaining room is taken.
        let mut full = Battery::new("bat-2", 36_000_000.0, 35_500_000.0, 2_000.0, 3_000.0).unwrap();
        let outcome = full.request(18_000_000.0, 3_600.0);
        assert!((outcome.applied_si - 500_000.0).abs() < 1e-6, "{outcome:?}");
        assert_eq!(outcome.reason, EnergyLimit::Full);
    }

    #[test]
    fn discharge_is_clipped_by_power_then_state_of_charge() {
        // 3 kW discharge over one hour applies 3 kWh from an 18 MJ SoC.
        let mut battery = Battery::new("bat-1", 36_000_000.0, 18_000_000.0, 2_000.0, 3_000.0).unwrap();
        let outcome = battery.request(-18_000_000.0, 3_600.0);
        assert!((outcome.applied_si + 10_800_000.0).abs() < 1e-6, "{outcome:?}");
        assert_eq!(outcome.reason, EnergyLimit::DischargePower);
        assert!((battery.soc_si - 7_200_000.0).abs() < 1e-6);

        let mut empty = Battery::new("bat-2", 36_000_000.0, 100.0, 2_000.0, 3_000.0).unwrap();
        let outcome = empty.request(-18_000_000.0, 3_600.0);
        assert!((outcome.applied_si + 100.0).abs() < 1e-6, "{outcome:?}");
        assert_eq!(outcome.reason, EnergyLimit::Empty);
        assert!(empty.soc_si.abs() < 1e-9);
    }

    #[test]
    fn invalid_battery_parameters_are_rejected() {
        assert!(Battery::new("b", 0.0, 0.0, 1.0, 1.0).is_err());
        assert!(Battery::new("b", 10.0, 20.0, 1.0, 1.0).is_err());
        assert!(Battery::new("", 10.0, 0.0, 1.0, 1.0).is_err());
    }
}
