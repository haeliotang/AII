//! Randomized read-only runtime invariants: projection of the latest scripted
//! observation, disconnect behaviour (retain / mark unknown / fail closed),
//! health reporting and evidence expiry.

use std::collections::BTreeMap;
use std::path::PathBuf;

use poie_ontology::model::AssetStatus;
use poie_runtime::adapter::{AdapterDeclaration, DisconnectBehavior, SimulatedAdapter, TimestampSemantics, UnitDecl};
use poie_runtime::runtime::LocalRuntime;
use poie_state::projection::EvidencePolicy;

const SEED: u64 = 0x706f_6965_7274_6d00; // "poiertm"
const ASSETS: [&str; 2] = ["pump-p1", "bus-1"];

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

fn base01() -> poie_ontology::model::Model {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01/model.toml");
    poie_ontology::model::Model::load(&path).expect("Base-01 loads")
}

fn declaration(behavior: DisconnectBehavior) -> AdapterDeclaration {
    AdapterDeclaration {
        adapter_id: "scripted-runtime".to_string(),
        version: "0.1.0".to_string(),
        kind: poie_runtime::adapter::AdapterKind::ReadOnly,
        source: "property-test".to_string(),
        units: vec![UnitDecl { field: "asset.status".to_string(), unit: "1".to_string() }],
        timestamp_semantics: TimestampSemantics::EventTime,
        disconnect_behavior: behavior,
        compatibility: None,
    }
}

#[derive(Clone, Debug)]
enum Poll {
    Observations(Vec<(&'static str, AssetStatus)>),
    Disconnect,
}

fn script(rng: &mut SplitMix64) -> Vec<Poll> {
    let count = 3 + rng.index(10);
    let mut polls = Vec::new();
    for _ in 0..count {
        if rng.unit() < 0.25 {
            polls.push(Poll::Disconnect);
        } else {
            let mut observations = Vec::new();
            for asset in ASSETS {
                if rng.unit() < 0.7 {
                    let status =
                        [AssetStatus::Available, AssetStatus::Degraded, AssetStatus::Failed, AssetStatus::Unknown]
                            [rng.index(4)];
                    observations.push((asset, status));
                }
            }
            polls.push(Poll::Observations(observations));
        }
    }
    polls
}

fn build_adapter(behavior: DisconnectBehavior, polls: &[Poll]) -> SimulatedAdapter {
    let mut adapter = SimulatedAdapter::new(declaration(behavior));
    for (index, poll) in polls.iter().enumerate() {
        let event_time = 1_000 + (index as i64) * 1_000;
        match poll {
            Poll::Observations(observations) => {
                let mapped: Vec<_> = observations
                    .iter()
                    .map(|(asset, status)| SimulatedAdapter::observations(asset, *status, event_time))
                    .collect();
                adapter.then(mapped);
            }
            Poll::Disconnect => {
                adapter.then_disconnect("scripted disconnect");
            }
        }
    }
    adapter
}

#[test]
fn randomized_runtime_sequences_match_the_projection_contract() {
    let mut rng = SplitMix64::new(SEED);
    for trial in 0..80 {
        let behavior = match rng.index(3) {
            0 => DisconnectBehavior::RetainLastKnown,
            1 => DisconnectBehavior::MarkUnknown,
            _ => DisconnectBehavior::FailClosed,
        };
        let max_age_ms = 2_000 + rng.index(6_000) as u64;
        let polls = script(&mut rng);
        let adapter = build_adapter(behavior, &polls);
        let mut runtime =
            LocalRuntime::new("site-a", base01(), vec![Box::new(adapter)], EvidencePolicy::new(max_age_ms))
                .expect("read-only runtime accepts the adapter");

        // Expected evidence per asset: (status, event_time).
        let mut expected: BTreeMap<&'static str, (AssetStatus, i64)> = BTreeMap::new();
        let mut previously_known: BTreeMap<&'static str, ()> = BTreeMap::new();
        for (index, poll) in polls.iter().enumerate() {
            let now_ms = 1_000 + (index as i64) * 1_000;
            let report = runtime.poll(now_ms).expect("poll");
            match poll {
                Poll::Observations(observations) => {
                    assert!(report.health.healthy, "trial {trial}: connected poll reported unhealthy");
                    assert!(!report.disconnected_adapters.contains(&"scripted-runtime".to_string()));
                    for (asset, status) in observations {
                        expected.insert(*asset, (*status, now_ms));
                        previously_known.insert(*asset, ());
                    }
                    assert_eq!(report.accepted_observations, observations.len(), "trial {trial}: accepted count");
                }
                Poll::Disconnect => {
                    assert!(!report.health.healthy, "trial {trial}: disconnected poll reported healthy");
                    assert!(report.disconnected_adapters.contains(&"scripted-runtime".to_string()));
                    if matches!(behavior, DisconnectBehavior::MarkUnknown | DisconnectBehavior::FailClosed) {
                        for asset in previously_known.keys() {
                            expected.insert(*asset, (AssetStatus::Unknown, now_ms));
                        }
                    }
                }
            }

            let model = runtime.projected_model(now_ms).expect("projected model");
            for asset in ASSETS {
                let projected = match expected.get(asset) {
                    Some((status, event_time)) if now_ms - event_time <= max_age_ms as i64 => *status,
                    Some(_) => AssetStatus::Unknown, // expired
                    None => AssetStatus::Unknown,    // no evidence at all
                };
                assert_eq!(
                    model.assets[asset].status, projected,
                    "trial {trial}/poll {index}: projection for {asset} differs from the contract"
                );
            }
        }
    }
}
