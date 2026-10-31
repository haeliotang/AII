//! Expansion proposal format (VEC §6.1, M11.1).

use std::fmt;
use std::path::Path;

use serde::Deserialize;

use poie_ontology::model::{
    AssetDoc, FlowDoc, LotDoc, Model, ModelError, Patch, ProcessDoc, ResourceDoc, Verification,
};
use poie_ontology::units::Quantity;

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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExpansionError {
    Io(String),
    Parse(String),
    Invalid(String),
    Model(String),
}

impl fmt::Display for ExpansionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExpansionError::Io(detail) => write!(f, "cannot read proposal: {detail}"),
            ExpansionError::Parse(detail) => write!(f, "cannot parse proposal: {detail}"),
            ExpansionError::Invalid(detail) => write!(f, "invalid proposal: {detail}"),
            ExpansionError::Model(detail) => write!(f, "expanded model is invalid: {detail}"),
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

    /// Applies the increment to the base model (the rest of the model is unchanged).
    pub fn expand(&self, base: &Model) -> Result<Model, ExpansionError> {
        base.patched(&self.to_patch()).map_err(|error: ModelError| ExpansionError::Model(error.to_string()))
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
