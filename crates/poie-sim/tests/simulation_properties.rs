//! Randomized simulation invariants: deterministic digest (including chunked
//! runs), event ordering, conservation classification, external flows, stock
//! accounting and battery state-of-charge bounds.

use std::collections::BTreeMap;
use std::path::PathBuf;

use poie_ontology::model::{AssetStatus, LotQuality, Model};
use poie_sim::battery::Battery;
use poie_sim::event::{Event, EventKind, Flow};
use poie_sim::simulation::{ConservationViolation, Simulation};

const SEED: u64 = 0x706f_6965_7369_6d00; // "poiesim"

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
}

fn base01() -> Model {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01/model.toml");
    Model::load(&path).expect("Base-01 loads")
}

struct Generated {
    events: Vec<Event>,
    imbalanced: Vec<String>,
    external: usize,
    stock_deltas: BTreeMap<String, f64>,
}

fn generate(rng: &mut SplitMix64, model: &Model) -> Generated {
    let asset_ids: Vec<&str> = model.assets.keys().map(String::as_str).collect();
    let resources: Vec<&str> =
        model.resources.keys().map(String::as_str).filter(|resource| !model.resources[*resource].is_energy()).collect();
    let count = 3 + rng.index(10);
    let mut events = Vec::new();
    let mut imbalanced = Vec::new();
    let mut external = 0;
    let mut stock_deltas: BTreeMap<String, f64> = BTreeMap::new();
    for index in 0..count {
        let at = (index as f64) * 10.0 + rng.unit() * 5.0;
        let kind = rng.index(4);
        let event = match kind {
            0 => {
                let asset = asset_ids[rng.index(asset_ids.len())];
                let status = [AssetStatus::Available, AssetStatus::Degraded, AssetStatus::Failed, AssetStatus::Unknown]
                    [rng.index(4)];
                Event::new(at, EventKind::SetAssetStatus { asset: asset.to_string(), status }).unwrap()
            }
            1 => {
                let resource = resources[rng.index(resources.len())];
                let delta = if rng.unit() < 0.5 { -1.0 } else { 1.0 } * (0.1 + rng.unit());
                let is_external = rng.unit() < 0.4;
                if is_external {
                    external += 1;
                }
                *stock_deltas.entry(resource.to_string()).or_insert(0.0) += delta;
                Event::new(
                    at,
                    EventKind::StockFlow { resource: resource.to_string(), delta_si: delta, external: is_external },
                )
                .unwrap()
            }
            2 => {
                // Balanced transfer: the same material leaves and enters.
                let resource = resources[rng.index(resources.len())];
                let amount = 0.1 + rng.unit() * 2.0;
                Event::new(
                    at,
                    EventKind::Transfer {
                        id: format!("balanced-{index}"),
                        inputs: vec![Flow { resource: resource.to_string(), amount_si: amount }],
                        outputs: vec![Flow { resource: resource.to_string(), amount_si: amount }],
                        asset_mass_change_kg: 0.0,
                    },
                )
                .unwrap()
            }
            _ => {
                // Imbalanced transfer: half the input leaves as output.
                let resource = resources[rng.index(resources.len())];
                let amount = 0.5 + rng.unit() * 2.0;
                let id = format!("imbalanced-{index}");
                imbalanced.push(id.clone());
                *stock_deltas.entry(resource.to_string()).or_insert(0.0) -= amount * 0.5;
                Event::new(
                    at,
                    EventKind::Transfer {
                        id,
                        inputs: vec![Flow { resource: resource.to_string(), amount_si: amount }],
                        outputs: vec![Flow { resource: resource.to_string(), amount_si: amount * 0.5 }],
                        asset_mass_change_kg: 0.0,
                    },
                )
                .unwrap()
            }
        };
        events.push(event);
    }
    // Add a battery request at the end (energy never participates in mass checks).
    let energy = Event::new(
        (count as f64 + 1.0) * 10.0,
        EventKind::EnergyRequest { battery: "bat-1".to_string(), amount_si: 18_000_000.0, duration_s: 3_600.0 },
    )
    .unwrap();
    events.push(energy);
    Generated { events, imbalanced, external, stock_deltas }
}

fn battery() -> Battery {
    Battery::new("bat-1", 36_000_000.0, 0.0, 2_000.0, 3_000.0).unwrap()
}

fn initial_stocks(model: &Model) -> BTreeMap<String, f64> {
    let mut stocks = BTreeMap::new();
    for lot in model.lots.iter().filter(|lot| lot.quality == LotQuality::Accepted) {
        *stocks.entry(lot.resource.clone()).or_insert(0.0) += lot.si;
    }
    stocks
}

#[test]
fn randomized_simulations_are_deterministic_and_conserve_mass() {
    let model = base01();
    let mut rng = SplitMix64::new(SEED);
    for trial in 0..80 {
        let generated = generate(&mut rng, &model);
        let mut once = Simulation::new(&model);
        once.add_battery(battery()).unwrap();
        for event in &generated.events {
            once.schedule(event.clone()).unwrap();
        }
        once.run_to_completion().unwrap();

        // Determinism: a fresh simulation reproduces the run, and chunked runs
        // across arbitrary time boundaries produce the same digest.
        let mut twice = Simulation::new(&model);
        twice.add_battery(battery()).unwrap();
        for event in &generated.events {
            twice.schedule(event.clone()).unwrap();
        }
        twice.run_to_completion().unwrap();
        assert_eq!(once.digest(), twice.digest(), "trial {trial}: simulation is not deterministic");

        let mut chunked = Simulation::new(&model);
        chunked.add_battery(battery()).unwrap();
        for event in &generated.events {
            chunked.schedule(event.clone()).unwrap();
        }
        let split = 5.0 + rng.unit() * 40.0;
        chunked.run_until(split).unwrap();
        chunked.run_to_completion().unwrap();
        assert_eq!(once.digest(), chunked.digest(), "trial {trial}: chunked run changed the digest");

        // Event log is ordered by simulation time.
        for pair in once.log().windows(2) {
            assert!(pair[0].at_s <= pair[1].at_s + 1e-9, "trial {trial}: event log is not ordered");
        }

        // Conservation: exactly the imbalanced transfers are flagged.
        let flagged: Vec<String> = once
            .violations()
            .iter()
            .filter_map(|violation| match violation {
                ConservationViolation::Imbalanced { transfer, .. } => Some(transfer.clone()),
                ConservationViolation::Unchecked { .. } => None,
            })
            .collect();
        let flagged: std::collections::BTreeSet<String> = flagged.into_iter().collect();
        let expected: std::collections::BTreeSet<String> = generated.imbalanced.iter().cloned().collect();
        assert_eq!(flagged, expected, "trial {trial}: conservation classification mismatch");

        // External flows are recorded exactly once per external stock flow.
        assert_eq!(once.external_flows().len(), generated.external, "trial {trial}: external flow accounting mismatch");

        // Stock accounting: initial stock plus all generated deltas.
        let mut expected_stocks = initial_stocks(&model);
        for (resource, delta) in &generated.stock_deltas {
            *expected_stocks.entry(resource.clone()).or_insert(0.0) += delta;
        }
        for (resource, expected) in &expected_stocks {
            let actual = once.stocks().get(resource).copied().unwrap_or(0.0);
            assert!(
                (actual - expected).abs() < 1e-6,
                "trial {trial}: stock {resource} is {actual}, expected {expected}"
            );
        }

        // Battery SoC stays within capacity.
        let battery = &once.batteries()["bat-1"];
        assert!(
            battery.soc_si >= -1e-6 && battery.soc_si <= battery.capacity_si + 1e-6,
            "trial {trial}: SoC out of bounds"
        );
    }
}
