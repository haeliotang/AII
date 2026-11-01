//! Expansion proposal format (VEC §6.1, M11.1).

use std::fmt;
use std::path::Path;

use serde::Deserialize;

use std::collections::BTreeMap;

use poie_ontology::model::{
    AssetDoc, AssetStatus, FlowDoc, LotDoc, LotQuality, Model, ModelError, Patch, ProcessDoc, ResourceDoc, Verification,
};
use poie_ontology::units::{fmt_number, Quantity};

pub const PROPOSAL_SCHEMA: &str = "poie.expansion-proposal/0.1";

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposalDoc {
    schema: String,
    id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    adds: AddsDoc,
    construction: ConstructionDoc,
    commissioning: CommissioningDoc,
    #[serde(default)]
    upkeep: UpkeepDoc,
    #[serde(default)]
    external_inputs: Vec<FlowDoc>,
    #[serde(default)]
    bootstrap: BootstrapDoc,
    #[serde(default)]
    failure_modes: Vec<FailureModeDoc>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FailureModeDoc {
    asset: String,
    mtbf: String,
    #[serde(default)]
    synthetic: bool,
    source: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AddsDoc {
    #[serde(default)]
    resources: Vec<ResourceDoc>,
    #[serde(default)]
    assets: Vec<AssetDoc>,
    #[serde(default)]
    processes: Vec<ProcessDoc>,
    #[serde(default)]
    lots: Vec<LotDoc>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConstructionDoc {
    duration: String,
    #[serde(default)]
    resources: Vec<FlowDoc>,
    #[serde(default)]
    energy: Option<String>,
    #[serde(default)]
    assets: Vec<String>,
    #[serde(default)]
    goals: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommissioningDoc {
    required_verification: Verification,
    inspection_method: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpkeepDoc {
    #[serde(default)]
    resources: Vec<FlowDoc>,
    #[serde(default)]
    processes: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct BootstrapDoc {
    #[serde(default)]
    assets: Vec<String>,
    #[serde(default)]
    resources: Vec<FlowDoc>,
}

#[derive(Clone, Debug)]
pub struct ProposalAdds {
    pub resources: Vec<ResourceDoc>,
    pub assets: Vec<AssetDoc>,
    pub processes: Vec<ProcessDoc>,
    pub lots: Vec<LotDoc>,
}

impl ProposalAdds {
    pub fn is_empty(&self) -> bool {
        self.resources.is_empty() && self.assets.is_empty() && self.processes.is_empty() && self.lots.is_empty()
    }
}

#[derive(Clone, Debug)]
pub struct Construction {
    pub duration_s: f64,
    pub resources: Vec<FlowDoc>,
    pub energy: Option<String>,
    pub assets: Vec<String>,
    pub goals: Vec<String>,
}

/// Failure mechanism of an added asset (VEC §2.3, §6.2): the expanded branch is
/// evaluated on the base disturbance set plus these mechanisms.
#[derive(Clone, Debug, PartialEq)]
pub struct FailureMode {
    pub asset: String,
    pub mtbf_s: f64,
    pub synthetic: bool,
    pub source: String,
}

/// The three structural evaluation points of an expansion (VEC §6.2.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    /// Construction and bootstrap draw deducted; occupied assets unavailable;
    /// the additions do not exist yet.
    DuringConstruction,
    /// Draw deducted; additions present at their current verification level.
    Built,
    /// Draw deducted; additions at the declared commissioning level. A
    /// hypothesis: the claim holds only if commissioning reaches that level.
    Commissioned,
}

impl Stage {
    pub const ALL: [Stage; 3] = [Stage::DuringConstruction, Stage::Built, Stage::Commissioned];

    pub fn as_str(self) -> &'static str {
        match self {
            Stage::DuringConstruction => "during_construction",
            Stage::Built => "built",
            Stage::Commissioned => "commissioned",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Commissioning {
    pub required_verification: Verification,
    pub inspection_method: String,
}

#[derive(Clone, Debug)]
pub struct ExpansionProposal {
    pub id: String,
    pub title: String,
    pub description: String,
    pub adds: ProposalAdds,
    pub construction: Construction,
    pub commissioning: Commissioning,
    pub upkeep_resources: Vec<FlowDoc>,
    pub upkeep_processes: Vec<String>,
    pub external_inputs: Vec<FlowDoc>,
    pub bootstrap_assets: Vec<String>,
    pub bootstrap_resources: Vec<FlowDoc>,
    pub failure_modes: Vec<FailureMode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExpansionError {
    Io(String),
    Parse(String),
    Invalid(String),
    Model(String),
    /// The local stock cannot pay for the construction and bootstrap draw.
    NotBuildable(String),
    /// The proposal cannot be evaluated against this operating threshold.
    Threshold(String),
}

impl fmt::Display for ExpansionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExpansionError::Io(detail) => write!(f, "cannot read proposal: {detail}"),
            ExpansionError::Parse(detail) => write!(f, "cannot parse proposal: {detail}"),
            ExpansionError::Invalid(detail) => write!(f, "invalid proposal: {detail}"),
            ExpansionError::Model(detail) => write!(f, "expanded model is invalid: {detail}"),
            ExpansionError::NotBuildable(detail) => write!(f, "not buildable from local stock: {detail}"),
            ExpansionError::Threshold(detail) => write!(f, "operating threshold: {detail}"),
        }
    }
}

impl std::error::Error for ExpansionError {}

impl ExpansionProposal {
    pub fn load(path: &Path) -> Result<ExpansionProposal, ExpansionError> {
        let text = std::fs::read_to_string(path).map_err(|error| ExpansionError::Io(error.to_string()))?;
        let doc: ProposalDoc = toml::from_str(&text).map_err(|error| ExpansionError::Parse(error.to_string()))?;
        if doc.schema != PROPOSAL_SCHEMA {
            return Err(ExpansionError::Invalid(format!("unsupported proposal schema {:?}", doc.schema)));
        }
        if doc.id.trim().is_empty() || doc.title.trim().is_empty() {
            return Err(ExpansionError::Invalid("proposal id and title are required".into()));
        }
        let duration: Quantity = doc
            .construction
            .duration
            .parse()
            .map_err(|error| ExpansionError::Invalid(format!("construction duration: {error}")))?;
        if duration.si <= 0.0 {
            return Err(ExpansionError::Invalid("construction duration must be positive".into()));
        }
        if doc.commissioning.inspection_method.trim().is_empty() {
            return Err(ExpansionError::Invalid("commissioning inspection method is required".into()));
        }
        // VEC §6.3 / §1.5: an unbuilt facility has no field record; its
        // processes and capabilities enter at most at `modeled`.
        for process in &doc.adds.processes {
            if process.verification > Verification::Modeled {
                return Err(ExpansionError::Invalid(format!(
                    "added process {} claims {} before it is built; at most modeled",
                    process.id, process.verification
                )));
            }
        }
        for asset in &doc.adds.assets {
            for (capability, level) in &asset.capability_verification {
                if *level > Verification::Modeled {
                    return Err(ExpansionError::Invalid(format!(
                        "added asset {} claims {level} for {capability} before it is built; at most modeled",
                        asset.id
                    )));
                }
            }
        }
        // VEC §2.3 / §6.2: every added asset declares its failure mechanism.
        let mut failure_modes = Vec::new();
        for mode in &doc.failure_modes {
            if !doc.adds.assets.iter().any(|asset| asset.id == mode.asset) {
                return Err(ExpansionError::Invalid(format!("failure mode for unknown added asset {}", mode.asset)));
            }
            let mtbf: Quantity = mode
                .mtbf
                .parse()
                .map_err(|error| ExpansionError::Invalid(format!("failure mode {} mtbf: {error}", mode.asset)))?;
            if !mtbf.si.is_finite() || mtbf.si <= 0.0 {
                return Err(ExpansionError::Invalid(format!("failure mode {} mtbf must be positive", mode.asset)));
            }
            if mode.source.trim().is_empty() {
                return Err(ExpansionError::Invalid(format!("failure mode {} needs a parameter source", mode.asset)));
            }
            failure_modes.push(FailureMode {
                asset: mode.asset.clone(),
                mtbf_s: mtbf.si,
                synthetic: mode.synthetic,
                source: mode.source.clone(),
            });
        }
        for asset in &doc.adds.assets {
            if !failure_modes.iter().any(|mode| mode.asset == asset.id) {
                return Err(ExpansionError::Invalid(format!(
                    "added asset {} has no failure mechanism (VEC §6.2: the expanded branch carries its own failures)",
                    asset.id
                )));
            }
        }
        let proposal = ExpansionProposal {
            id: doc.id,
            title: doc.title,
            description: doc.description,
            adds: ProposalAdds {
                resources: doc.adds.resources,
                assets: doc.adds.assets,
                processes: doc.adds.processes,
                lots: doc.adds.lots,
            },
            construction: Construction {
                duration_s: duration.si,
                resources: doc.construction.resources,
                energy: doc.construction.energy,
                assets: doc.construction.assets,
                goals: doc.construction.goals,
            },
            commissioning: Commissioning {
                required_verification: doc.commissioning.required_verification,
                inspection_method: doc.commissioning.inspection_method,
            },
            upkeep_resources: doc.upkeep.resources,
            upkeep_processes: doc.upkeep.processes,
            external_inputs: doc.external_inputs,
            bootstrap_assets: doc.bootstrap.assets,
            bootstrap_resources: doc.bootstrap.resources,
            failure_modes,
        };
        if proposal.adds.is_empty() {
            return Err(ExpansionError::Invalid(
                "proposal must add at least one resource, asset, process or lot".into(),
            ));
        }
        Ok(proposal)
    }

    /// The capability-graph increment as an ontology patch.
    pub fn to_patch(&self) -> Patch {
        Patch {
            add_resource: self.adds.resources.clone(),
            add_asset: self.adds.assets.clone(),
            add_process: self.adds.processes.clone(),
            add_lot: self.adds.lots.clone(),
            ..Patch::default()
        }
    }

    /// Applies the increment to the base model (the rest of the model is
    /// unchanged). This is the planned post-commissioning capability graph
    /// *without* any construction cost; use [`ExpansionProposal::stage_model`]
    /// for accounting (VEC §6.2).
    pub fn expand(&self, base: &Model) -> Result<Model, ExpansionError> {
        base.patched(&self.to_patch()).map_err(|error: ModelError| ExpansionError::Model(error.to_string()))
    }

    /// Resources drawn by construction and bootstrap, per resource in SI. The
    /// bootstrap list names the part of the construction draw taken from
    /// critical-service stock, so the two are merged by maximum, not summed.
    pub fn construction_draw(&self, base: &Model) -> Result<BTreeMap<String, f64>, ExpansionError> {
        let mut construction: BTreeMap<String, f64> = BTreeMap::new();
        let mut bootstrap: BTreeMap<String, f64> = BTreeMap::new();
        for (target, flows) in
            [(&mut construction, &self.construction.resources), (&mut bootstrap, &self.bootstrap_resources)]
        {
            for flow in flows {
                let quantity: Quantity = flow
                    .quantity
                    .parse()
                    .map_err(|error| ExpansionError::Invalid(format!("draw of {}: {error}", flow.resource)))?;
                let Some(resource) = base.resources.get(&flow.resource) else {
                    return Err(ExpansionError::Invalid(format!("draw of unknown resource {}", flow.resource)));
                };
                if quantity.dim != resource.unit.dim {
                    return Err(ExpansionError::Invalid(format!("draw of {} has the wrong dimension", flow.resource)));
                }
                *target.entry(flow.resource.clone()).or_insert(0.0) += quantity.si;
            }
        }
        for (resource, amount) in bootstrap {
            let entry = construction.entry(resource).or_insert(0.0);
            *entry = entry.max(amount);
        }
        Ok(construction)
    }

    /// The model at one evaluation stage (VEC §6.2.1): the base with the
    /// construction and bootstrap draw deducted from accepted lots, then
    /// - `DuringConstruction`: construction and bootstrap assets unavailable
    ///   (modelled as `failed`, so they appear as blocking root causes), no
    ///   additions;
    /// - `Built`: additions present at their current (at most `modeled`) level;
    /// - `Commissioned`: additions raised to the commissioning level.
    ///
    /// Returns `NotBuildable` when the accepted stock cannot pay for the draw.
    pub fn stage_model(&self, base: &Model, stage: Stage) -> Result<Model, ExpansionError> {
        let mut patch = Patch::default();
        for (resource, amount) in self.construction_draw(base)? {
            let unit = base.resources[&resource].unit;
            let mut remaining = amount;
            for lot in base.lots.iter().filter(|lot| lot.resource == resource && lot.quality == LotQuality::Accepted) {
                if remaining <= 0.0 {
                    break;
                }
                let take = lot.si.min(remaining);
                remaining -= take;
                patch.lot_quantity.insert(
                    lot.id.clone(),
                    format!("{} {}", fmt_number((lot.si - take).max(0.0) / unit.scale), unit.symbol),
                );
            }
            if remaining > poie_ontology::units::rounding_tolerance(amount) {
                return Err(ExpansionError::NotBuildable(format!(
                    "{resource}: needs {} {}, accepted stock short by {} {}",
                    fmt_number(amount / unit.scale),
                    unit.symbol,
                    fmt_number(remaining / unit.scale),
                    unit.symbol
                )));
            }
        }
        match stage {
            Stage::DuringConstruction => {
                for asset in self.construction.assets.iter().chain(&self.bootstrap_assets) {
                    if !base.assets.contains_key(asset) {
                        return Err(ExpansionError::Invalid(format!("construction occupies unknown asset {asset}")));
                    }
                    patch.asset_status.insert(asset.clone(), AssetStatus::Failed);
                }
            }
            Stage::Built | Stage::Commissioned => {
                let adds = self.to_patch();
                patch.add_resource = adds.add_resource;
                patch.add_asset = adds.add_asset;
                patch.add_process = adds.add_process;
                patch.add_lot = adds.add_lot;
                if stage == Stage::Commissioned {
                    let level = self.commissioning.required_verification;
                    for asset in &mut patch.add_asset {
                        for value in asset.capability_verification.values_mut() {
                            *value = (*value).max(level);
                        }
                    }
                    for process in &mut patch.add_process {
                        process.verification = process.verification.max(level);
                    }
                }
            }
        }
        base.patched(&patch).map_err(|error: ModelError| ExpansionError::Model(error.to_string()))
    }

    /// VEC §6.3: the evaluation threshold is the operating threshold of the
    /// base, given by human policy. A commissioning level below it means the
    /// new process could never be used in operation.
    pub fn check_threshold(&self, operating_threshold: Verification) -> Result<(), ExpansionError> {
        if self.commissioning.required_verification < operating_threshold {
            return Err(ExpansionError::Threshold(format!(
                "commissioning level {} is below the operating threshold {operating_threshold}",
                self.commissioning.required_verification
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn proposal_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01-expansion/proposal.toml")
    }

    #[test]
    fn the_base01_expansion_proposal_loads_and_expands() {
        let proposal = ExpansionProposal::load(&proposal_path()).unwrap();
        assert_eq!(proposal.id, "recycle-furnace");
        assert!(proposal.construction.duration_s > 0.0);
        let base_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01/model.toml");
        let base = Model::load(&base_path).unwrap();
        let expanded = proposal.expand(&base).unwrap();
        assert!(expanded.assets.contains_key("furnace-1"));
        assert!(expanded.processes.iter().any(|process| process.id == "remelt-swarf"));
    }

    #[test]
    fn proposals_need_additions_and_a_valid_duration() {
        let text = r#"
schema = "poie.expansion-proposal/0.1"
id = "empty"
title = "empty"
[construction]
duration = "0 h"
[commissioning]
required_verification = "modeled"
inspection_method = "none"
"#;
        let path = std::env::temp_dir().join(format!("poie-proposal-{}.toml", std::process::id()));
        std::fs::write(&path, text).unwrap();
        let error = ExpansionProposal::load(&path).unwrap_err();
        assert!(matches!(error, ExpansionError::Invalid(_)), "{error}");
        let _ = std::fs::remove_file(&path);
    }
}
