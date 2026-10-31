//! Randomized unit/quantity invariants: SI conversion round-trips, dimension
//! compatibility, parsing and the rounding tolerance (PO §2).

use poie_ontology::units::{approx_le, fmt_number, unit, Quantity, UnitError};

const SEED: u64 = 0x706f_6965_756e_6974; // "poieunit"

const GROUPS: &[&[&str]] = &[
    &["item"],
    &["kg", "g"],
    &["m", "mm"],
    &["m3", "L"],
    &["s", "min", "h", "d"],
    &["J", "kJ", "MJ", "kWh"],
    &["W", "kW"],
    &["m3/h"],
    &["K"],
];

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

    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn value(&mut self) -> f64 {
        let magnitude = 10f64.powf(-6.0 + self.unit() * 12.0);
        let sign = if self.unit() < 0.5 { -1.0 } else { 1.0 };
        sign * magnitude
    }

    fn value_rounded(&mut self) -> f64 {
        let raw = self.value();
        format!("{raw:.6}").parse::<f64>().unwrap_or(raw)
    }
}

fn close(left: f64, right: f64) -> bool {
    let scale = left.abs().max(right.abs()).max(1.0);
    (left - right).abs() <= 1e-9 * scale
}

#[test]
fn conversions_round_trip_and_respect_dimensions() {
    let mut rng = SplitMix64::new(SEED);
    for trial in 0..200 {
        let group = GROUPS[rng.index(GROUPS.len())];
        let from = unit(group[rng.index(group.len())]).expect("known unit");
        let to = unit(group[rng.index(group.len())]).expect("known unit");
        let value = rng.value();
        let quantity = Quantity::new(value, from);

        // SI value is value * scale; converting back to the source unit recovers it.
        assert!(close(quantity.si, value * from.scale), "trial {trial}: SI conversion");
        assert!(close(quantity.in_unit(from).unwrap(), value), "trial {trial}: source round-trip");
        // Converting to another unit in the same dimension is value * scale_from / scale_to.
        assert!(
            close(quantity.in_unit(to).unwrap(), value * from.scale / to.scale),
            "trial {trial}: cross-unit conversion"
        );
        assert_eq!(quantity.dim, from.dim, "trial {trial}: dimension tag");
    }
}

#[test]
fn arithmetic_parsing_and_errors_follow_the_unit_table() {
    let mut rng = SplitMix64::new(SEED ^ 0xbeef);
    for trial in 0..200 {
        let group = GROUPS[rng.index(GROUPS.len())];
        let left_unit = unit(group[rng.index(group.len())]).expect("known unit");
        let right_unit = unit(group[rng.index(group.len())]).expect("known unit");
        let left = rng.value_rounded();
        let right = rng.value_rounded();

        // Parsing "<number> <unit>" agrees with direct construction.
        let parsed: Quantity = format!("{} {}", fmt_number(left), left_unit.symbol).parse().expect("parses");
        assert!(close(parsed.si, Quantity::new(left, left_unit).si), "trial {trial}: parse");

        // Addition/subtraction within one dimension.
        let sum = Quantity::new(left, left_unit).checked_add(Quantity::new(right, right_unit)).unwrap();
        let expected = left * left_unit.scale + right * right_unit.scale;
        assert!(close(sum.si, expected), "trial {trial}: addition");
        let difference = Quantity::new(left, left_unit).checked_sub(Quantity::new(right, right_unit)).unwrap();
        let expected = left * left_unit.scale - right * right_unit.scale;
        assert!(close(difference.si, expected), "trial {trial}: subtraction");
        assert_eq!(sum.dim, left_unit.dim, "trial {trial}: sum dimension");

        // Mixing dimensions is an error, not a silent conversion.
        let other_group = GROUPS[rng.index(GROUPS.len())];
        let other = unit(other_group[rng.index(other_group.len())]).expect("known unit");
        if other.dim != left_unit.dim {
            assert!(matches!(
                Quantity::new(left, left_unit).checked_add(Quantity::new(right, other)),
                Err(UnitError::DimensionMismatch { .. })
            ));
            assert!(matches!(Quantity::new(left, left_unit).in_unit(other), Err(UnitError::DimensionMismatch { .. })));
        }

        // Malformed / unknown / non-finite inputs are classified errors.
        assert!(matches!("12".parse::<Quantity>(), Err(UnitError::Malformed(_))));
        assert!(matches!("12 furlongs".parse::<Quantity>(), Err(UnitError::UnknownUnit(_))));
        assert!(matches!("NaN kg".parse::<Quantity>(), Err(UnitError::NotFinite(_))));
        assert!(matches!("inf kg".parse::<Quantity>(), Err(UnitError::NotFinite(_))));

        // Rounding tolerance is symmetric at equality and rejects a clearly larger value.
        let magnitude = left.abs().max(1.0);
        assert!(approx_le(magnitude, magnitude));
        assert!(!approx_le(magnitude, magnitude - 2.0 * poie_ontology::units::rounding_tolerance(magnitude)));
        assert!(!approx_le(magnitude + 2.0 * poie_ontology::units::rounding_tolerance(magnitude), magnitude));
    }
}
