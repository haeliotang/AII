//! Randomized candidate-route invariants (PO §6.3 rule 14, §6.5):
//! bounded enumeration, deduplication, limit monotonicity, per-route summary
//! consistency and the external-dependency union.

use std::collections::BTreeSet;
use std::path::PathBuf;

use poie_ontology::graph::{self, Goal, Query, Verdict, DEFAULT_MAX_DEPTH};
use poie_ontology::model::{AssetStatus, LotQuality, Model, Patch, Verification};
use poie_ontology::units::Quantity;

const SEED: u64 = 0x706f_6965_726f_7574; // "poierout"

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

fn random_model(rng: &mut SplitMix64) -> Model {
    let base = base01();
    let mut patch = Patch::default();
    for asset in base.assets.keys() {
        if rng.unit() < 0.2 {
            patch.asset_status.insert(asset.clone(), AssetStatus::Failed);
        } else if rng.unit() < 0.2 {
            patch.asset_status.insert(asset.clone(), AssetStatus::Degraded);
        }
    }
    for lot in base.lots.iter().filter(|lot| lot.quality == LotQuality::Accepted) {
        if rng.unit() < 0.25 {
            let unit = base.resources[&lot.resource].unit.symbol;
            patch.lot_quantity.insert(lot.id.clone(), format!("0 {unit}"));
        }
    }
    base.patched(&patch).expect("generated patch is valid")
}

fn goals() -> Vec<Goal> {
    let mut goals =
        vec![Goal::Service("potable-water".into()), Goal::Restore("lathe-1".into()), Goal::Restore("pump-p1".into())];
    for (resource, quantity) in [("sleeve-p1", "1 item"), ("sleeve-p1", "4 item"), ("bronze-bar", "2 kg")] {
        goals.push(Goal::Obtain { resource: resource.into(), quantity: quantity.parse::<Quantity>().unwrap() });
    }
    goals
}

#[test]
fn enumeration_is_bounded_unique_and_monotone() {
    let mut rng = SplitMix64::new(SEED);
    let pool = goals();
    for trial in 0..120 {
        let model = random_model(&mut rng);
        let goal = pool[rng.index(pool.len())].clone();
        let min = match rng.index(3) {
            0 => Verification::Declared,
            1 => Verification::Modeled,
            _ => Verification::Demonstrated,
        };
        let query = Query { goal, min_verification: min, max_depth: DEFAULT_MAX_DEPTH };
        let small = graph::enumerate(&model, &query, 3);
        let large = graph::enumerate(&model, &query, 16);
        assert!(small.len() <= 3, "trial {trial}: limit 3 exceeded");
        assert!(large.len() <= 16, "trial {trial}: limit 16 exceeded");

        // Deduplication by signature.
        let signatures: BTreeSet<String> = large.iter().map(|route| route.signature()).collect();
        assert_eq!(signatures.len(), large.len(), "trial {trial}: duplicate routes");

        // Enumeration only produces routes for a candidate verdict.
        let verdict = graph::solve(&model, &query).verdict;
        if verdict != Verdict::Candidate {
            assert!(large.is_empty(), "trial {trial}: routes for a non-candidate verdict {verdict:?}");
        }

        // A larger limit extends the smaller enumeration as a prefix.
        let small_signatures: Vec<String> = small.iter().map(|route| route.signature()).collect();
        let large_signatures: Vec<String> = large.iter().map(|route| route.signature()).collect();
        assert_eq!(
            small_signatures.as_slice(),
            &large_signatures[..small_signatures.len().min(large_signatures.len())],
            "trial {trial}: enumeration is not prefix-monotone in the limit"
        );

        // Per-route summary consistency.
        for route in &large {
            for (resource, amount) in &route.consumed {
                assert!(amount.is_finite() && *amount > 0.0, "trial {trial}: consumed {resource} is not positive");
            }
            for dependency in &route.external_dependencies {
                assert!(route.consumed.contains_key(dependency), "trial {trial}: dependency {dependency} not consumed");
            }
            if let Some(verification) = route.verification {
                assert!(verification >= min, "trial {trial}: route verification below the query minimum");
            }
        }
    }
}

#[test]
fn external_dependency_report_is_the_union_of_route_dependencies() {
    let mut rng = SplitMix64::new(SEED ^ 0x6e70);
    let pool = goals();
    for trial in 0..80 {
        let model = random_model(&mut rng);
        let goal = pool[rng.index(pool.len())].clone();
        let query = Query { goal, min_verification: Verification::Modeled, max_depth: DEFAULT_MAX_DEPTH };
        if graph::solve(&model, &query).verdict != Verdict::Candidate {
            continue;
        }
        let routes = graph::enumerate(&model, &query, 8);
        let report = graph::external_dependency_report(&model, &query, 8);
        assert_eq!(report.candidate_routes, routes.len(), "trial {trial}: candidate route count");
        let route_dependencies: BTreeSet<&str> =
            routes.iter().flat_map(|route| route.external_dependencies.iter().map(String::as_str)).collect();
        let reported: BTreeSet<&str> = report.resources().into_iter().collect();
        assert_eq!(reported, route_dependencies, "trial {trial}: dependency union mismatch");
        let resources: Vec<&str> = report.resources();
        let unique: BTreeSet<&str> = resources.iter().copied().collect();
        assert_eq!(unique.len(), resources.len(), "trial {trial}: duplicate dependencies in the report");
    }
}
