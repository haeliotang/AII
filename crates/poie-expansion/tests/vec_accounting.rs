//! Issue #109 (AI-preview review P10, P11, P23) and the expansion side of #110
//! (P12): expansion accounting rules of the Viability & Expansion Contract.
//!
//! Each test is named after the contract clause it checks and reproduces a
//! counterexample from docs/status/ai-preview/contract-review-{a,b,c}.zh-CN.md.

use std::collections::BTreeMap;
use std::path::PathBuf;

use poie_expansion::proposal::ExpansionProposal;
use poie_expansion::structural::structural_gain;
use poie_expansion::temporal::temporal_gain;
use poie_ontology::graph::Goal;
use poie_ontology::model::{Model, Patch, Verification};
use poie_ontology::time::TimeWindow;
use poie_ontology::units::Quantity;
use poie_viability::metrics::{RunTrace, TimeSample};

/// The operating threshold of Base-01 planning (H2/H4 plan at `demonstrated`).
const OPERATING_THRESHOLD: Verification = Verification::Demonstrated;

fn workspace(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(path)
}

fn base01() -> Model {
    Model::load(&workspace("scenarios/base-01/model.toml")).expect("Base-01 loads")
}

fn proposal_text() -> String {
    std::fs::read_to_string(workspace("scenarios/base-01-expansion/proposal.toml")).expect("proposal text")
}

fn proposal() -> ExpansionProposal {
    ExpansionProposal::load(&workspace("scenarios/base-01-expansion/proposal.toml")).expect("proposal loads")
}

fn temp_file(tag: &str, text: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("poie-vec-accounting-{}-{tag}.toml", std::process::id()));
    std::fs::write(&path, text).expect("write temp file");
    path
}

fn load_text(tag: &str, text: &str) -> Result<ExpansionProposal, String> {
    let path = temp_file(tag, text);
    let loaded = ExpansionProposal::load(&path).map_err(|error| error.to_string());
    let _ = std::fs::remove_file(&path);
    loaded
}

fn obtain(resource: &str, quantity: &str) -> Goal {
    Goal::Obtain { resource: resource.into(), quantity: quantity.parse::<Quantity>().unwrap() }
}

fn with_commissioning(text: &str, level: &str) -> String {
    text.lines()
        .map(|line| {
            if line.trim_start().starts_with("required_verification") {
                format!("required_verification = \"{level}\"")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// VEC §6.2.1 (review C-04): the expanded model must deduct the construction
// and bootstrap draw. With exactly 1.2 kg of bronze bar, a second sleeve is a
// candidate before the expansion; building the furnace consumes 1 kg, and the
// remelted bar only exists after commissioning. The goal must appear in the
// "lost" list — this is the contract's own example of a loss.
#[test]
fn vec6_2_construction_draw_is_deducted_and_can_lose_a_goal() {
    let mut patch = Patch::default();
    patch.lot_quantity.insert("lot-bronze".into(), "1.2 kg".into());
    let base = base01().patched(&patch).unwrap();
    let goals = vec![obtain("sleeve-p1", "2 item")];
    let gain = structural_gain(&base, &proposal(), &goals, OPERATING_THRESHOLD).expect("gain computes");
    assert!(gain.lost.iter().any(|outcome| outcome.label == "obtain:sleeve-p1"), "{gain:?}");
}

// VEC §6.1/§6.2.1 (review C-04): assets occupied by construction (robot-m1)
// are unavailable while the furnace is being built; recovery goals that need
// them are lost for that period and must be listed.
#[test]
fn vec6_2_assets_occupied_by_construction_are_unavailable() {
    let goals = vec![Goal::Restore("pump-p1".into())];
    let gain = structural_gain(&base01(), &proposal(), &goals, OPERATING_THRESHOLD).expect("gain computes");
    assert!(gain.lost.iter().any(|outcome| outcome.label == "restore:pump-p1"), "{gain:?}");
}

/// One producer whose per-run yield is so small that covering the goal needs
/// more runs than the solver's run budget: the base verdict is `search_limit`.
const TINY_YIELD_MODEL: &str = r#"
schema = "poie.physical-ontology/0.3"
name = "tiny-yield"

[[resource]]
id = "ore"
unit = "kg"

[[resource]]
id = "dust"
unit = "kg"

[[lot]]
id = "lot-ore"
resource = "ore"
quantity = "0.005 kg"
quality = "accepted"

[[process]]
id = "grind"
inputs = [{ resource = "ore", quantity = "1 g" }]
outputs = [{ resource = "dust", quantity = "1 g" }]
verification = "verified"
"#;

const STOCK_PROPOSAL: &str = r#"
schema = "poie.expansion-proposal/0.1"
id = "buy-stock"
title = "Add a lot of the end product"

[[adds.lots]]
id = "lot-dust"
resource = "dust"
quantity = "100 kg"
quality = "accepted"

[construction]
duration = "1 h"

[commissioning]
required_verification = "verified"
inspection_method = "receiving inspection"
"#;

// VEC §6.2.1 (reviews A-12, B-12): `search_limit` is undecided. A goal that
// moves from `search_limit` to `candidate` is not a gain (the base result was
// never decided), and must be listed separately.
#[test]
fn vec6_2_search_limit_to_candidate_is_not_a_gain() {
    let model_path = temp_file("tiny-yield", TINY_YIELD_MODEL);
    let base = Model::load(&model_path).expect("tiny-yield model loads");
    let _ = std::fs::remove_file(&model_path);
    let proposal = load_text("stock", STOCK_PROPOSAL).expect("stock proposal loads");
    let goals = vec![obtain("dust", "100 kg")];
    let gain = structural_gain(&base, &proposal, &goals, Verification::Verified).expect("gain computes");
    assert!(gain.gained.is_empty(), "search_limit -> candidate counted as a gain: {gain:?}");
}

// VEC §6.3 (review C-05, counterexample 1/3): the evaluation threshold is the
// operating threshold given by human policy. A proposal whose commissioning
// level is below that threshold can never be used in operation; evaluating it
// at its own lower level is not allowed.
#[test]
fn vec6_3_commissioning_below_the_operating_threshold_is_rejected() {
    let proposal = load_text("modeled", &with_commissioning(&proposal_text(), "modeled")).expect("loads");
    let goals = vec![obtain("sleeve-p1", "4 item")];
    let result = structural_gain(&base01(), &proposal, &goals, OPERATING_THRESHOLD);
    assert!(result.is_err(), "commissioning `modeled` evaluated at a `demonstrated` threshold: {result:?}");
}

// VEC §6.3 / §1.5 (review C-05): an unbuilt facility has no field record, so
// its processes and capabilities cannot claim more than `modeled`.
#[test]
fn vec6_3_unbuilt_process_cannot_claim_more_than_modeled() {
    let text = proposal_text().replace("verification = \"modeled\"", "verification = \"demonstrated\"");
    let text = text.replace("\"scrap-recycling\" = \"modeled\"", "\"scrap-recycling\" = \"demonstrated\"");
    assert!(load_text("claims", &text).is_err(), "an unbuilt process claimed `demonstrated`");
}

// VEC §2.3 / §6.2 (review C-08): the expanded branch is evaluated on the base
// disturbance set plus the failure mechanisms of the added assets. A proposal
// that adds an asset without declaring its failure mechanism is incomplete.
#[test]
fn vec6_2_added_assets_need_a_failure_mechanism() {
    let text = proposal_text();
    let without = match text.find("[[failure_modes]]") {
        Some(index) => text[..index].to_string(),
        None => text,
    };
    assert!(load_text("no-failure", &without).is_err(), "added asset furnace-1 has no failure mechanism");
}

fn sample(at_s: f64, level: Option<f64>) -> TimeSample {
    TimeSample {
        at_s,
        service_levels: level.map(|level| BTreeMap::from([("potable-water".to_string(), level)])).unwrap_or_default(),
        violations: Vec::new(),
        external_inputs: BTreeMap::new(),
        stock: BTreeMap::new(),
        produced: BTreeMap::new(),
        recycle_inputs_kg: BTreeMap::new(),
        recycle_reusable_kg: BTreeMap::new(),
        health: BTreeMap::new(),
        affected_services: Vec::new(),
        affected_assets: Vec::new(),
    }
}

// VEC §4.1 (review C-02, counterexample 3): a missing service reading is
// unknown, not full service. A construction window with no reading for a
// critical service must not pass the construction-period check.
#[test]
fn vec4_1_missing_reading_during_construction_is_not_a_pass() {
    let trace = RunTrace {
        scenario_id: "gap".into(),
        horizon_s: 10_000.0,
        minimum_levels: BTreeMap::from([("potable-water".to_string(), 1.0)]),
        samples: vec![sample(0.0, Some(1.0)), sample(500.0, None), sample(5_000.0, Some(1.0))],
        recovery_paths: BTreeMap::new(),
    };
    let levels = BTreeMap::from([("potable-water".to_string(), 1.0)]);
    let window = TimeWindow::new(0.0, 2_000.0).unwrap();
    let gain = temporal_gain(std::slice::from_ref(&trace), std::slice::from_ref(&trace), &levels, window);
    assert!(!gain.construction.within_window, "missing reading treated as full service: {gain:?}");
}
