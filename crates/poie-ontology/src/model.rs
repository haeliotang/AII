//! Entities of the Physical Ontology (§3), model loading, validation and the
//! mass-conservation check (§5).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use serde::Deserialize;

use crate::units::{self, Dimension, Quantity, Unit};

pub const MODEL_SCHEMA: &str = "poie.physical-ontology/0.1";
pub const DEFAULT_MASS_TOLERANCE: f64 = 0.01;

#[derive(Debug, Clone, PartialEq)]
pub struct ModelError(pub String);

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ModelError {}

macro_rules! bail {
    ($($arg:tt)*) => { return Err(ModelError(format!($($arg)*))) };
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetStatus {
    Available,
    Degraded,
    Failed,
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LotQuality {
    Accepted,
    Quarantined,
    Rejected,
}

/// Verification level of a process (§3.5), ordered from weakest to strongest.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verification {
    Declared,
    Modeled,
    Demonstrated,
    Verified,
}

impl fmt::Display for Verification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Verification::Declared => "declared",
            Verification::Modeled => "modeled",
            Verification::Demonstrated => "demonstrated",
            Verification::Verified => "verified",
        })
    }
}

impl fmt::Display for AssetStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AssetStatus::Available => "available",
            AssetStatus::Degraded => "degraded",
            AssetStatus::Failed => "failed",
            AssetStatus::Unknown => "unknown",
        })
    }
}

// ---------------------------------------------------------------------------
// Serialized form (TOML)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelDoc {
    pub schema: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, rename = "resource")]
    pub resources: Vec<ResourceDoc>,
    #[serde(default, rename = "asset")]
    pub assets: Vec<AssetDoc>,
    #[serde(default, rename = "lot")]
    pub lots: Vec<LotDoc>,
    #[serde(default, rename = "process")]
    pub processes: Vec<ProcessDoc>,
    #[serde(default, rename = "service")]
    pub services: Vec<ServiceDoc>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceDoc {
    pub id: String,
    pub unit: String,
    pub mass_per_unit: Option<String>,
    #[serde(default)]
    pub description: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetDoc {
    pub id: String,
    pub kind: String,
    pub status: AssetStatus,
    #[serde(default)]
    pub status_evidence: Option<String>,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub provides: Vec<String>,
    #[serde(default)]
    pub description: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LotDoc {
    pub id: String,
    pub resource: String,
    pub quantity: String,
    pub quality: LotQuality,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub evidence: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowDoc {
    pub resource: String,
    pub quantity: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessDoc {
    pub id: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub inputs: Vec<FlowDoc>,
    #[serde(default)]
    pub outputs: Vec<FlowDoc>,
    #[serde(default)]
    pub occupies: Vec<String>,
    #[serde(default)]
    pub requires_capabilities: Vec<String>,
    #[serde(default)]
    pub restores: Vec<String>,
    pub duration: Option<String>,
    pub verification: Verification,
    pub asset_mass_change: Option<String>,
    pub mass_tolerance: Option<f64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceDoc {
    pub id: String,
    #[serde(default)]
    pub description: String,
    pub requires: Vec<String>,
}

// ---------------------------------------------------------------------------
// Validated model
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Resource {
    pub id: String,
    pub unit: &'static Unit,
    /// Mass in kg of one declared unit; `None` for energy or undeclared.
    pub mass_per_unit_kg: Option<f64>,
}

impl Resource {
    pub fn is_energy(&self) -> bool {
        self.unit.dim == Dimension::ENERGY
    }

    /// Mass in kg of `si` (amount in SI units), if known. Energy has no mass here.
    fn mass_kg(&self, si: f64) -> Option<f64> {
        if self.unit.dim == Dimension::MASS {
            Some(si)
        } else {
            self.mass_per_unit_kg.map(|m| si / self.unit.scale * m)
        }
    }

    /// Formats an SI amount in the resource's declared unit, e.g. `1.2 kg`.
    pub fn fmt_amount(&self, si: f64) -> String {
        format!("{} {}", units::fmt_number(si / self.unit.scale), self.unit.symbol)
    }
}

#[derive(Clone, Debug)]
pub struct Asset {
    pub id: String,
    pub kind: String,
    pub status: AssetStatus,
    pub requires: Vec<String>,
    pub provides: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Lot {
    pub id: String,
    pub resource: String,
    pub si: f64,
    pub quality: LotQuality,
}

#[derive(Clone, Debug)]
pub struct Flow {
    pub resource: String,
    pub si: f64,
}

#[derive(Clone, Debug)]
pub struct Process {
    pub id: String,
    pub inputs: Vec<Flow>,
    pub outputs: Vec<Flow>,
    pub occupies: Vec<String>,
    pub requires_capabilities: Vec<String>,
    pub restores: Vec<String>,
    pub duration_s: Option<f64>,
    pub verification: Verification,
    pub asset_mass_change_kg: f64,
    pub mass_tolerance: f64,
}

#[derive(Clone, Debug)]
pub struct Service {
    pub id: String,
    pub requires: Vec<String>,
}

/// Result of the per-process mass check (§5).
#[derive(Clone, Debug, PartialEq)]
pub enum MassBalance {
    Balanced {
        input_kg: f64,
        output_kg: f64,
        asset_kg: f64,
    },
    /// Some material flows have no declared mass.
    Unchecked {
        missing: Vec<String>,
    },
    Violated {
        input_kg: f64,
        output_kg: f64,
        asset_kg: f64,
    },
}

#[derive(Clone, Debug)]
pub struct Model {
    pub doc: ModelDoc,
    pub name: String,
    pub resources: BTreeMap<String, Resource>,
    pub assets: BTreeMap<String, Asset>,
    pub lots: Vec<Lot>,
    pub processes: Vec<Process>,
    pub services: BTreeMap<String, Service>,
    /// resource id -> indices of processes that output it (file order).
    pub producers: BTreeMap<String, Vec<usize>>,
    /// asset id -> indices of processes that restore it (file order).
    pub restorers: BTreeMap<String, Vec<usize>>,
    /// capability id -> assets that provide it (file order).
    pub providers: BTreeMap<String, Vec<String>>,
    /// process id -> mass check result.
    pub mass_balance: BTreeMap<String, MassBalance>,
}

fn check_slug(category: &str, id: &str) -> Result<(), ModelError> {
    let mut chars = id.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    let rest_ok = chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    if !(first_ok && rest_ok) {
        bail!("{category} id {id:?} must match [a-z0-9][a-z0-9._-]*");
    }
    Ok(())
}

fn parse_quantity(context: &str, s: &str) -> Result<Quantity, ModelError> {
    s.parse().map_err(|e| ModelError(format!("{context}: {e}")))
}

fn parse_dim(context: &str, s: &str, dim: Dimension) -> Result<Quantity, ModelError> {
    let q = parse_quantity(context, s)?;
    if q.dim != dim {
        bail!("{context}: expected dimension {dim}, got {} in {s:?}", q.dim);
    }
    Ok(q)
}

impl Model {
    pub fn load(path: &Path) -> Result<Model, ModelError> {
        let text =
            std::fs::read_to_string(path).map_err(|e| ModelError(format!("cannot read {}: {e}", path.display())))?;
        let doc: ModelDoc = toml::from_str(&text).map_err(|e| ModelError(format!("{}: {e}", path.display())))?;
        Model::from_doc(doc)
    }

    pub fn from_doc(doc: ModelDoc) -> Result<Model, ModelError> {
        if doc.schema != MODEL_SCHEMA {
            bail!("unsupported schema {:?}; expected {MODEL_SCHEMA:?}", doc.schema);
        }

        let mut resources = BTreeMap::new();
        for r in &doc.resources {
            check_slug("resource", &r.id)?;
            let unit = units::unit(&r.unit)
                .ok_or_else(|| ModelError(format!("resource {}: unknown unit {:?}", r.id, r.unit)))?;
            let mass_per_unit_kg = match &r.mass_per_unit {
                Some(s) => {
                    if unit.dim == Dimension::ENERGY {
                        bail!("resource {}: energy resources must not declare mass_per_unit", r.id);
                    }
                    let q = parse_dim(&format!("resource {}.mass_per_unit", r.id), s, Dimension::MASS)?;
                    if q.si <= 0.0 {
                        bail!("resource {}: mass_per_unit must be positive", r.id);
                    }
                    Some(q.si)
                }
                None => None,
            };
            let res = Resource { id: r.id.clone(), unit, mass_per_unit_kg };
            if resources.insert(r.id.clone(), res).is_some() {
                bail!("duplicate resource id {:?}", r.id);
            }
        }

        let mut assets = BTreeMap::new();
        for a in &doc.assets {
            check_slug("asset", &a.id)?;
            let asset = Asset {
                id: a.id.clone(),
                kind: a.kind.clone(),
                status: a.status,
                requires: a.requires.clone(),
                provides: a.provides.clone(),
            };
            if assets.insert(a.id.clone(), asset).is_some() {
                bail!("duplicate asset id {:?}", a.id);
            }
        }
        for a in assets.values() {
            for r in &a.requires {
                if !assets.contains_key(r) {
                    bail!("asset {} requires unknown asset {r:?}", a.id);
                }
            }
            for c in &a.provides {
                check_slug("capability", c)?;
            }
        }
        check_requires_acyclic(&assets)?;

        let mut lots = Vec::new();
        let mut lot_ids = BTreeSet::new();
        for l in &doc.lots {
            check_slug("lot", &l.id)?;
            if !lot_ids.insert(l.id.clone()) {
                bail!("duplicate lot id {:?}", l.id);
            }
            let res = resources
                .get(&l.resource)
                .ok_or_else(|| ModelError(format!("lot {}: unknown resource {:?}", l.id, l.resource)))?;
            let q = parse_dim(&format!("lot {}", l.id), &l.quantity, res.unit.dim)?;
            if q.si < 0.0 {
                bail!("lot {}: quantity must not be negative", l.id);
            }
            lots.push(Lot { id: l.id.clone(), resource: l.resource.clone(), si: q.si, quality: l.quality });
        }

        let mut processes = Vec::new();
        let mut process_ids = BTreeSet::new();
        for p in &doc.processes {
            check_slug("process", &p.id)?;
            if !process_ids.insert(p.id.clone()) {
                bail!("duplicate process id {:?}", p.id);
            }
            let flows = |list: &[FlowDoc], what: &str| -> Result<Vec<Flow>, ModelError> {
                list.iter()
                    .map(|f| {
                        let res = resources.get(&f.resource).ok_or_else(|| {
                            ModelError(format!("process {} {what}: unknown resource {:?}", p.id, f.resource))
                        })?;
                        let q =
                            parse_dim(&format!("process {} {what} {}", p.id, f.resource), &f.quantity, res.unit.dim)?;
                        if q.si <= 0.0 {
                            bail!("process {} {what} {}: quantity must be positive", p.id, f.resource);
                        }
                        Ok(Flow { resource: f.resource.clone(), si: q.si })
                    })
                    .collect()
            };
            let inputs = flows(&p.inputs, "input")?;
            let outputs = flows(&p.outputs, "output")?;
            for a in p.occupies.iter().chain(&p.restores) {
                if !assets.contains_key(a) {
                    bail!("process {} references unknown asset {a:?}", p.id);
                }
            }
            if let Some(a) = p.occupies.iter().find(|a| p.restores.contains(a)) {
                bail!("process {} occupies the asset it restores ({a}); that is always a cycle", p.id);
            }
            for c in &p.requires_capabilities {
                check_slug("capability", c)?;
            }
            if outputs.is_empty() && p.restores.is_empty() {
                bail!("process {} has neither outputs nor restores", p.id);
            }
            let duration_s = p
                .duration
                .as_deref()
                .map(|d| parse_dim(&format!("process {} duration", p.id), d, Dimension::TIME).map(|q| q.si))
                .transpose()?;
            let asset_mass_change_kg = p
                .asset_mass_change
                .as_deref()
                .map(|m| parse_dim(&format!("process {} asset_mass_change", p.id), m, Dimension::MASS).map(|q| q.si))
                .transpose()?
                .unwrap_or(0.0);
            let mass_tolerance = p.mass_tolerance.unwrap_or(DEFAULT_MASS_TOLERANCE);
            if !(0.0..1.0).contains(&mass_tolerance) {
                bail!("process {}: mass_tolerance must be in [0, 1)", p.id);
            }
            processes.push(Process {
                id: p.id.clone(),
                inputs,
                outputs,
                occupies: p.occupies.clone(),
                requires_capabilities: p.requires_capabilities.clone(),
                restores: p.restores.clone(),
                duration_s,
                verification: p.verification,
                asset_mass_change_kg,
                mass_tolerance,
            });
        }

        let mut services = BTreeMap::new();
        for s in &doc.services {
            check_slug("service", &s.id)?;
            if s.requires.is_empty() {
                bail!("service {} must require at least one asset", s.id);
            }
            for a in &s.requires {
                if !assets.contains_key(a) {
                    bail!("service {} requires unknown asset {a:?}", s.id);
                }
            }
            let svc = Service { id: s.id.clone(), requires: s.requires.clone() };
            if services.insert(s.id.clone(), svc).is_some() {
                bail!("duplicate service id {:?}", s.id);
            }
        }

        let mut producers: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut restorers: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, p) in processes.iter().enumerate() {
            for f in &p.outputs {
                let list = producers.entry(f.resource.clone()).or_default();
                if !list.contains(&i) {
                    list.push(i);
                }
            }
            for a in &p.restores {
                restorers.entry(a.clone()).or_default().push(i);
            }
        }
        let mut providers: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for a in &doc.assets {
            for c in &a.provides {
                providers.entry(c.clone()).or_default().push(a.id.clone());
            }
        }

        let mut mass_balance = BTreeMap::new();
        for p in &processes {
            let mb = check_mass(p, &resources);
            if let MassBalance::Violated { input_kg, output_kg, asset_kg } = mb {
                bail!(
                    "process {} violates mass conservation: inputs {} kg, outputs {} kg, into assets {} kg (tolerance {})",
                    p.id,
                    units::fmt_number(input_kg),
                    units::fmt_number(output_kg),
                    units::fmt_number(asset_kg),
                    p.mass_tolerance
                );
            }
            mass_balance.insert(p.id.clone(), mb);
        }

        Ok(Model {
            name: doc.name.clone(),
            doc,
            resources,
            assets,
            lots,
            processes,
            services,
            producers,
            restorers,
            providers,
            mass_balance,
        })
    }

    pub fn process(&self, id: &str) -> Option<&Process> {
        self.processes.iter().find(|p| p.id == id)
    }

    /// Resource types with no local producing process (§6.5).
    pub fn external_resources(&self) -> Vec<&str> {
        self.resources.keys().filter(|r| !self.producers.contains_key(*r)).map(String::as_str).collect()
    }

    /// Returns a copy of this model with `patch` applied and re-validated.
    pub fn patched(&self, patch: &Patch) -> Result<Model, ModelError> {
        let mut doc = self.doc.clone();
        for (id, status) in &patch.asset_status {
            let Some(a) = doc.assets.iter_mut().find(|a| &a.id == id) else {
                bail!("patch: unknown asset {id:?}");
            };
            a.status = *status;
        }
        for (id, qty) in &patch.lot_quantity {
            let Some(l) = doc.lots.iter_mut().find(|l| &l.id == id) else {
                bail!("patch: unknown lot {id:?}");
            };
            l.quantity = qty.clone();
        }
        for (id, quality) in &patch.lot_quality {
            let Some(l) = doc.lots.iter_mut().find(|l| &l.id == id) else {
                bail!("patch: unknown lot {id:?}");
            };
            l.quality = *quality;
        }
        for (id, level) in &patch.process_verification {
            let Some(p) = doc.processes.iter_mut().find(|p| &p.id == id) else {
                bail!("patch: unknown process {id:?}");
            };
            p.verification = *level;
        }
        Model::from_doc(doc)
    }
}

/// Targeted modifications applied to a base model for one scenario case.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    #[serde(default)]
    pub asset_status: BTreeMap<String, AssetStatus>,
    #[serde(default)]
    pub lot_quantity: BTreeMap<String, String>,
    #[serde(default)]
    pub lot_quality: BTreeMap<String, LotQuality>,
    #[serde(default)]
    pub process_verification: BTreeMap<String, Verification>,
}

fn check_requires_acyclic(assets: &BTreeMap<String, Asset>) -> Result<(), ModelError> {
    // 0 = unvisited, 1 = on stack, 2 = done
    fn visit<'a>(
        id: &'a str,
        assets: &'a BTreeMap<String, Asset>,
        state: &mut BTreeMap<&'a str, u8>,
        stack: &mut Vec<&'a str>,
    ) -> Result<(), ModelError> {
        match state.get(id) {
            Some(2) => return Ok(()),
            Some(1) => {
                stack.push(id);
                bail!("asset requires relation has a cycle: {}", stack.join(" -> "));
            }
            _ => {}
        }
        state.insert(id, 1);
        stack.push(id);
        for r in &assets[id].requires {
            visit(r, assets, state, stack)?;
        }
        stack.pop();
        state.insert(id, 2);
        Ok(())
    }
    let mut state = BTreeMap::new();
    for id in assets.keys() {
        visit(id, assets, &mut state, &mut Vec::new())?;
    }
    Ok(())
}

fn check_mass(p: &Process, resources: &BTreeMap<String, Resource>) -> MassBalance {
    let mut missing = Vec::new();
    let mut sum = |flows: &[Flow]| -> f64 {
        let mut total = 0.0;
        for f in flows {
            let r = &resources[&f.resource];
            if r.is_energy() {
                continue;
            }
            match r.mass_kg(f.si) {
                Some(m) => total += m,
                None => missing.push(f.resource.clone()),
            }
        }
        total
    };
    let input_kg = sum(&p.inputs);
    let output_kg = sum(&p.outputs);
    if !missing.is_empty() {
        missing.sort();
        missing.dedup();
        return MassBalance::Unchecked { missing };
    }
    let asset_kg = p.asset_mass_change_kg;
    let residual = (input_kg - output_kg - asset_kg).abs();
    let scale = input_kg.max(output_kg);
    if residual <= p.mass_tolerance * scale + units::rounding_tolerance(scale) {
        MassBalance::Balanced { input_kg, output_kg, asset_kg }
    } else {
        MassBalance::Violated { input_kg, output_kg, asset_kg }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(extra: &str) -> ModelDoc {
        let text = format!(
            r#"
schema = "poie.physical-ontology/0.1"
name = "test"

[[resource]]
id = "bar"
unit = "kg"

[[resource]]
id = "part"
unit = "item"
mass_per_unit = "0.8 kg"

[[resource]]
id = "chips"
unit = "kg"

[[resource]]
id = "energy"
unit = "kWh"

[[asset]]
id = "lathe"
kind = "lathe"
status = "available"
{extra}
"#
        );
        toml::from_str(&text).unwrap()
    }

    #[test]
    fn balanced_process_loads() {
        let m = Model::from_doc(doc(r#"
[[process]]
id = "turn"
inputs = [{ resource = "bar", quantity = "1.2 kg" }, { resource = "energy", quantity = "2 kWh" }]
outputs = [{ resource = "part", quantity = "1 item" }, { resource = "chips", quantity = "400 g" }]
occupies = ["lathe"]
verification = "demonstrated"
"#))
        .unwrap();
        assert!(matches!(m.mass_balance["turn"], MassBalance::Balanced { .. }));
        assert_eq!(m.external_resources(), vec!["bar", "energy"]);
    }

    #[test]
    fn unmodeled_loss_is_rejected() {
        let err = Model::from_doc(doc(r#"
[[process]]
id = "turn"
inputs = [{ resource = "bar", quantity = "1.2 kg" }]
outputs = [{ resource = "part", quantity = "1 item" }]
verification = "demonstrated"
"#))
        .unwrap_err();
        assert!(err.0.contains("violates mass conservation"), "{err}");
    }

    #[test]
    fn missing_mass_is_unchecked_not_balanced() {
        let mut d = doc(r#"
[[process]]
id = "turn"
inputs = [{ resource = "bar", quantity = "0.8 kg" }]
outputs = [{ resource = "part", quantity = "1 item" }]
verification = "demonstrated"
"#);
        d.resources[1].mass_per_unit = None;
        let m = Model::from_doc(d).unwrap();
        assert_eq!(m.mass_balance["turn"], MassBalance::Unchecked { missing: vec!["part".into()] });
    }

    #[test]
    fn wrong_dimension_in_flow_is_rejected() {
        let err = Model::from_doc(doc(r#"
[[process]]
id = "turn"
inputs = [{ resource = "energy", quantity = "2 kW" }]
outputs = [{ resource = "chips", quantity = "1 kg" }]
verification = "declared"
"#))
        .unwrap_err();
        assert!(err.0.contains("expected dimension"), "{err}");
    }

    #[test]
    fn occupying_own_restore_target_is_rejected() {
        let err = Model::from_doc(doc(r#"
[[process]]
id = "self-fix"
occupies = ["lathe"]
restores = ["lathe"]
verification = "declared"
"#))
        .unwrap_err();
        assert!(err.0.contains("always a cycle"), "{err}");
    }

    #[test]
    fn cyclic_requires_is_rejected() {
        let err = Model::from_doc(doc(r#"
[[asset]]
id = "a"
kind = "x"
status = "available"
requires = ["b"]

[[asset]]
id = "b"
kind = "x"
status = "available"
requires = ["a"]
"#))
        .unwrap_err();
        assert!(err.0.contains("cycle"), "{err}");
    }

    #[test]
    fn bad_ids_and_units_are_rejected() {
        let mut d = doc("");
        d.resources[0].unit = "lb".into();
        assert!(Model::from_doc(d).unwrap_err().0.contains("unknown unit"));
        let mut d = doc("");
        d.assets[0].id = "Lathe".into();
        assert!(Model::from_doc(d).unwrap_err().0.contains("must match"));
    }
}
