//! Physical quantities and units (Physical Ontology §2).
//!
//! Quantities are stored in coherent SI units together with their dimension.
//! Only the units in [`UNITS`] are accepted; anything else is rejected rather
//! than guessed. Offset units such as °C are deliberately unsupported in v0.1.

use std::fmt;
use std::str::FromStr;

/// Integer exponents of the seven SI base dimensions: L, M, T, I, Θ, N, J.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Dimension(pub [i8; 7]);

impl Dimension {
    pub const NONE: Dimension = Dimension([0, 0, 0, 0, 0, 0, 0]);
    pub const LENGTH: Dimension = Dimension([1, 0, 0, 0, 0, 0, 0]);
    pub const MASS: Dimension = Dimension([0, 1, 0, 0, 0, 0, 0]);
    pub const TIME: Dimension = Dimension([0, 0, 1, 0, 0, 0, 0]);
    pub const TEMPERATURE: Dimension = Dimension([0, 0, 0, 0, 1, 0, 0]);
    pub const VOLUME: Dimension = Dimension([3, 0, 0, 0, 0, 0, 0]);
    pub const ENERGY: Dimension = Dimension([2, 1, -2, 0, 0, 0, 0]);
    pub const POWER: Dimension = Dimension([2, 1, -3, 0, 0, 0, 0]);
    pub const VOLUME_FLOW: Dimension = Dimension([3, 0, -1, 0, 0, 0, 0]);
}

impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const NAMES: [&str; 7] = ["L", "M", "T", "I", "Θ", "N", "J"];
        let parts: Vec<String> = self
            .0
            .iter()
            .zip(NAMES)
            .filter(|(e, _)| **e != 0)
            .map(|(e, n)| if *e == 1 { n.to_string() } else { format!("{n}^{e}") })
            .collect();
        if parts.is_empty() {
            write!(f, "1")
        } else {
            write!(f, "{}", parts.join("·"))
        }
    }
}

/// A unit symbol with its dimension and its factor to the coherent SI unit.
#[derive(Debug, PartialEq)]
pub struct Unit {
    pub symbol: &'static str,
    pub dim: Dimension,
    pub scale: f64,
}

/// The v0.1 unit table (Physical Ontology §2.2).
pub static UNITS: &[Unit] = &[
    Unit { symbol: "item", dim: Dimension::NONE, scale: 1.0 },
    Unit { symbol: "kg", dim: Dimension::MASS, scale: 1.0 },
    Unit { symbol: "g", dim: Dimension::MASS, scale: 1e-3 },
    Unit { symbol: "m", dim: Dimension::LENGTH, scale: 1.0 },
    Unit { symbol: "mm", dim: Dimension::LENGTH, scale: 1e-3 },
    Unit { symbol: "m3", dim: Dimension::VOLUME, scale: 1.0 },
    Unit { symbol: "L", dim: Dimension::VOLUME, scale: 1e-3 },
    Unit { symbol: "s", dim: Dimension::TIME, scale: 1.0 },
    Unit { symbol: "min", dim: Dimension::TIME, scale: 60.0 },
    Unit { symbol: "h", dim: Dimension::TIME, scale: 3600.0 },
    Unit { symbol: "d", dim: Dimension::TIME, scale: 86400.0 },
    Unit { symbol: "J", dim: Dimension::ENERGY, scale: 1.0 },
    Unit { symbol: "kJ", dim: Dimension::ENERGY, scale: 1e3 },
    Unit { symbol: "MJ", dim: Dimension::ENERGY, scale: 1e6 },
    Unit { symbol: "kWh", dim: Dimension::ENERGY, scale: 3.6e6 },
    Unit { symbol: "W", dim: Dimension::POWER, scale: 1.0 },
    Unit { symbol: "kW", dim: Dimension::POWER, scale: 1e3 },
    Unit { symbol: "m3/h", dim: Dimension::VOLUME_FLOW, scale: 1.0 / 3600.0 },
    Unit { symbol: "K", dim: Dimension::TEMPERATURE, scale: 1.0 },
];

pub fn unit(symbol: &str) -> Option<&'static Unit> {
    UNITS.iter().find(|u| u.symbol == symbol)
}

/// Tolerance used only to absorb floating-point rounding (Physical Ontology §2.3).
pub fn rounding_tolerance(magnitude: f64) -> f64 {
    1e-12 + 1e-9 * magnitude.abs()
}

/// `a <= b` up to rounding tolerance.
pub fn approx_le(a: f64, b: f64) -> bool {
    a <= b + rounding_tolerance(a.abs().max(b.abs()))
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Quantity {
    /// Value in the coherent SI unit of `dim`.
    pub si: f64,
    pub dim: Dimension,
}

impl Quantity {
    pub fn new(value: f64, unit: &Unit) -> Quantity {
        Quantity { si: value * unit.scale, dim: unit.dim }
    }

    /// Value expressed in `unit`; fails if the dimensions differ.
    pub fn in_unit(&self, unit: &Unit) -> Result<f64, UnitError> {
        if self.dim != unit.dim {
            return Err(UnitError::DimensionMismatch { left: self.dim, right: unit.dim });
        }
        Ok(self.si / unit.scale)
    }

    pub fn checked_add(self, other: Quantity) -> Result<Quantity, UnitError> {
        if self.dim != other.dim {
            return Err(UnitError::DimensionMismatch { left: self.dim, right: other.dim });
        }
        Ok(Quantity { si: self.si + other.si, dim: self.dim })
    }

    pub fn checked_sub(self, other: Quantity) -> Result<Quantity, UnitError> {
        self.checked_add(Quantity { si: -other.si, dim: other.dim })
    }
}

#[derive(Debug, PartialEq)]
pub enum UnitError {
    Malformed(String),
    UnknownUnit(String),
    NotFinite(String),
    DimensionMismatch { left: Dimension, right: Dimension },
}

impl fmt::Display for UnitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnitError::Malformed(s) => write!(f, "malformed quantity {s:?}; expected \"<number> <unit>\""),
            UnitError::UnknownUnit(u) => write!(f, "unknown unit {u:?} (not in the v0.1 unit table)"),
            UnitError::NotFinite(s) => write!(f, "quantity {s:?} is not a finite number"),
            UnitError::DimensionMismatch { left, right } => {
                write!(f, "dimension mismatch: {left} vs {right}")
            }
        }
    }
}

impl std::error::Error for UnitError {}

impl FromStr for Quantity {
    type Err = UnitError;

    /// Parses `"<number> <unit>"`, e.g. `"1.2 kg"` or `"0.5 kWh"`.
    fn from_str(s: &str) -> Result<Quantity, UnitError> {
        let mut parts = s.split_whitespace();
        let (Some(number), Some(symbol), None) = (parts.next(), parts.next(), parts.next()) else {
            return Err(UnitError::Malformed(s.to_string()));
        };
        let value: f64 = number.parse().map_err(|_| UnitError::Malformed(s.to_string()))?;
        if !value.is_finite() {
            return Err(UnitError::NotFinite(s.to_string()));
        }
        let unit = unit(symbol).ok_or_else(|| UnitError::UnknownUnit(symbol.to_string()))?;
        Ok(Quantity::new(value, unit))
    }
}

/// Formats a number without trailing zeros, e.g. `1.2`, `0.05`, `3`.
pub fn fmt_number(x: f64) -> String {
    let s = format!("{x:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_converts_to_si() {
        let q: Quantity = "0.5 kWh".parse().unwrap();
        assert_eq!(q.dim, Dimension::ENERGY);
        assert!((q.si - 1.8e6).abs() < 1e-6);
        assert!((q.in_unit(unit("kJ").unwrap()).unwrap() - 1800.0).abs() < 1e-9);
    }

    #[test]
    fn power_and_energy_do_not_mix() {
        let e: Quantity = "1 kWh".parse().unwrap();
        let p: Quantity = "1 kW".parse().unwrap();
        assert!(matches!(e.checked_add(p), Err(UnitError::DimensionMismatch { .. })));
        assert!(e.in_unit(unit("kW").unwrap()).is_err());
    }

    #[test]
    fn rejects_unknown_and_offset_units() {
        assert_eq!("20 degC".parse::<Quantity>(), Err(UnitError::UnknownUnit("degC".into())));
        assert!(matches!("kg".parse::<Quantity>(), Err(UnitError::Malformed(_))));
        assert!(matches!("1 kg extra".parse::<Quantity>(), Err(UnitError::Malformed(_))));
        assert!(matches!("inf kg".parse::<Quantity>(), Err(UnitError::NotFinite(_))));
    }

    #[test]
    fn same_dimension_adds() {
        let a: Quantity = "1 kg".parse().unwrap();
        let b: Quantity = "250 g".parse().unwrap();
        assert!((a.checked_add(b).unwrap().si - 1.25).abs() < 1e-12);
        assert!((a.checked_sub(b).unwrap().si - 0.75).abs() < 1e-12);
    }

    #[test]
    fn formats_numbers_compactly() {
        assert_eq!(fmt_number(1.2), "1.2");
        assert_eq!(fmt_number(3.0), "3");
        assert_eq!(fmt_number(0.05), "0.05");
        assert_eq!(fmt_number(-0.0000001), "0");
    }

    #[test]
    fn displays_dimensions() {
        assert_eq!(Dimension::ENERGY.to_string(), "L^2·M·T^-2");
        assert_eq!(Dimension::NONE.to_string(), "1");
    }
}
