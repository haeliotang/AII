//! The discrete-event simulator (M5.4).
//!
//! Events are popped in `(time, sequence)` order, so the same scheduled input
//! always produces the same log — including when the run is split across several
//! [`Simulation::run_until`] calls. Fixed trajectories are plain scheduled events;
//! state-dependent disturbances are declarative [`StateRule`]s evaluated after
//! every event. Every internal [`EventKind::Transfer`] is checked for mass
//! conservation; external stock flows are recorded separately.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::fmt;

use poie_ontology::model::{AssetStatus, LotQuality, Model};
use poie_ontology::units::Dimension;

use crate::battery::Battery;
use crate::event::{Event, EventKind, SimError};

const DEFAULT_MASS_TOLERANCE: f64 = 0.01;
const ROUNDING_TOLERANCE: f64 = 1e-9;

#[derive(Clone, Debug, PartialEq)]
pub struct LogEntry {
    pub at_s: f64,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExternalFlow {
    pub at_s: f64,
    pub resource: String,
    pub delta_si: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConservationViolation {
    Imbalanced { at_s: f64, transfer: String, input_kg: f64, output_kg: f64, asset_kg: f64 },
    Unchecked { at_s: f64, transfer: String, missing: Vec<String> },
}

#[derive(Clone, Debug, PartialEq)]
pub enum RuleCondition {
    AssetIs { asset: String, status: AssetStatus },
    StockBelow { resource: String, amount_si: f64 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum RuleEffect {
    SetAssetStatus { asset: String, status: AssetStatus },
    StockFlow { resource: String, delta_si: f64, external: bool },
    EnergyRequest { battery: String, amount_si: f64, duration_s: f64 },
}

/// A state-dependent disturbance: when `when` holds on the current state and the
/// rule has not fired yet, its effect is scheduled after `delay_s`.
#[derive(Clone, Debug, PartialEq)]
pub struct StateRule {
    pub id: String,
    pub when: RuleCondition,
    pub delay_s: f64,
    pub then: RuleEffect,
}

impl StateRule {
    pub fn new(
        id: impl Into<String>,
        when: RuleCondition,
        delay_s: f64,
        then: RuleEffect,
    ) -> Result<StateRule, SimError> {
        let id = id.into();
        if id.is_empty() || !delay_s.is_finite() || delay_s < 0.0 {
            return Err(SimError::InvalidFlow("state rule needs an id and a non-negative delay".into()));
        }
        Ok(StateRule { id, when, delay_s, then })
    }
}

#[derive(Clone, Debug)]
struct QueuedEvent {
    at_s: f64,
    seq: u64,
    event: Event,
}

impl PartialEq for QueuedEvent {
    fn eq(&self, other: &Self) -> bool {
        self.at_s == other.at_s && self.seq == other.seq && self.event == other.event
    }
}

impl Eq for QueuedEvent {}

impl PartialOrd for QueuedEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueuedEvent {
    /// Reversed so `BinaryHeap` pops the earliest `(time, sequence)` first.
    fn cmp(&self, other: &Self) -> Ordering {
        other.at_s.total_cmp(&self.at_s).then_with(|| other.seq.cmp(&self.seq))
    }
}

pub struct Simulation<'m> {
    model: &'m Model,
    queue: BinaryHeap<QueuedEvent>,
    next_seq: u64,
    now_s: f64,
    assets: BTreeMap<String, AssetStatus>,
    stocks: BTreeMap<String, f64>,
    batteries: BTreeMap<String, Battery>,
    rules: Vec<StateRule>,
    fired_rules: BTreeSet<String>,
    log: Vec<LogEntry>,
    external_flows: Vec<ExternalFlow>,
    violations: Vec<ConservationViolation>,
}

impl<'m> Simulation<'m> {
    pub fn new(model: &'m Model) -> Simulation<'m> {
        let assets = model.assets.iter().map(|(id, asset)| (id.clone(), asset.status)).collect();
        let mut stocks: BTreeMap<String, f64> = BTreeMap::new();
        for lot in model.lots.iter().filter(|lot| lot.quality == LotQuality::Accepted) {
            *stocks.entry(lot.resource.clone()).or_insert(0.0) += lot.si;
        }
        Simulation {
            model,
            queue: BinaryHeap::new(),
            next_seq: 0,
            now_s: 0.0,
            assets,
            stocks,
            batteries: BTreeMap::new(),
            rules: Vec::new(),
            fired_rules: BTreeSet::new(),
            log: Vec::new(),
            external_flows: Vec::new(),
            violations: Vec::new(),
        }
    }

    pub fn now_s(&self) -> f64 {
        self.now_s
    }

    pub fn schedule(&mut self, event: Event) -> Result<(), SimError> {
        if event.at_s < self.now_s - 1e-9 {
            return Err(SimError::EventInThePast { at_s: event.at_s, now_s: self.now_s });
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        self.queue.push(QueuedEvent { at_s: event.at_s, seq, event });
        Ok(())
    }

    pub fn add_battery(&mut self, battery: Battery) -> Result<(), SimError> {
        if self.batteries.insert(battery.id.clone(), battery).is_some() {
            return Err(SimError::InvalidFlow("duplicate battery id".into()));
        }
        Ok(())
    }

    pub fn add_rule(&mut self, rule: StateRule) -> Result<(), SimError> {
        if self.rules.iter().any(|existing| existing.id == rule.id) {
            return Err(SimError::InvalidFlow(format!("duplicate state rule id {:?}", rule.id)));
        }
        self.rules.push(rule);
        Ok(())
    }

    /// Runs all events with `at_s <= t_end`.
    pub fn run_until(&mut self, t_end: f64) -> Result<(), SimError> {
        if !t_end.is_finite() {
            return Err(SimError::InvalidTime(t_end));
        }
        while let Some(next) = self.queue.peek() {
            if next.at_s > t_end {
                break;
            }
            let queued = self.queue.pop().expect("peeked event exists");
            self.now_s = queued.at_s;
            self.apply(&queued.event)?;
            self.evaluate_rules()?;
        }
        Ok(())
    }

    pub fn run_to_completion(&mut self) -> Result<(), SimError> {
        while let Some(next) = self.queue.pop() {
            self.now_s = next.at_s;
            self.apply(&next.event)?;
            self.evaluate_rules()?;
        }
        Ok(())
    }

    pub fn assets(&self) -> &BTreeMap<String, AssetStatus> {
        &self.assets
    }

    pub fn stocks(&self) -> &BTreeMap<String, f64> {
        &self.stocks
    }

    pub fn batteries(&self) -> &BTreeMap<String, Battery> {
        &self.batteries
    }

    pub fn log(&self) -> &[LogEntry] {
        &self.log
    }

    pub fn external_flows(&self) -> &[ExternalFlow] {
        &self.external_flows
    }

    pub fn violations(&self) -> &[ConservationViolation] {
        &self.violations
    }

    /// Canonical rendering of the run. Two runs with the same scheduled input must
    /// produce the same digest (M5.5 replay basis).
    pub fn digest(&self) -> String {
        let mut out = String::new();
        for entry in &self.log {
            out.push_str(&format!("log {:.6} {}\n", entry.at_s, entry.text));
        }
        for (asset, status) in &self.assets {
            out.push_str(&format!("asset {asset}={status}\n"));
        }
        for (resource, quantity) in &self.stocks {
            out.push_str(&format!("stock {resource}={quantity:.6}\n"));
        }
        for (battery, energy) in &self.batteries {
            out.push_str(&format!("battery {battery}={:.6}\n", energy.soc_si));
        }
        for flow in &self.external_flows {
            out.push_str(&format!("external {:.6} {} {:+.6}\n", flow.at_s, flow.resource, flow.delta_si));
        }
        for violation in &self.violations {
            out.push_str(&format!("violation {violation:?}\n"));
        }
        out
    }

    fn apply(&mut self, event: &Event) -> Result<(), SimError> {
        self.log.push(LogEntry { at_s: event.at_s, text: event.describe() });
        match &event.kind {
            EventKind::SetAssetStatus { asset, status } => {
                if !self.model.assets.contains_key(asset) {
                    return Err(SimError::UnknownAsset(asset.clone()));
                }
                self.assets.insert(asset.clone(), *status);
            }
            EventKind::StockFlow { resource, delta_si, external } => {
                if !self.model.resources.contains_key(resource) {
                    return Err(SimError::UnknownResource(resource.clone()));
                }
                *self.stocks.entry(resource.clone()).or_insert(0.0) += delta_si;
                if *external {
                    self.external_flows.push(ExternalFlow {
                        at_s: event.at_s,
                        resource: resource.clone(),
                        delta_si: *delta_si,
                    });
                }
            }
            EventKind::Transfer { id, inputs, outputs, asset_mass_change_kg } => {
                let mut input_kg = 0.0;
                let mut output_kg = 0.0;
                let mut missing = Vec::new();
                for flow in inputs {
                    match self.mass_kg(&flow.resource, flow.amount_si)? {
                        Some(mass) => input_kg += mass,
                        None => missing.push(flow.resource.clone()),
                    }
                    *self.stocks.entry(flow.resource.clone()).or_insert(0.0) -= flow.amount_si;
                }
                for flow in outputs {
                    match self.mass_kg(&flow.resource, flow.amount_si)? {
                        Some(mass) => output_kg += mass,
                        None => missing.push(flow.resource.clone()),
                    }
                    *self.stocks.entry(flow.resource.clone()).or_insert(0.0) += flow.amount_si;
                }
                if !missing.is_empty() {
                    missing.sort();
                    missing.dedup();
                    self.violations.push(ConservationViolation::Unchecked {
                        at_s: event.at_s,
                        transfer: id.clone(),
                        missing,
                    });
                } else {
                    let residual = (input_kg - output_kg - asset_mass_change_kg).abs();
                    let scale = input_kg.max(output_kg);
                    if residual > DEFAULT_MASS_TOLERANCE * scale + ROUNDING_TOLERANCE {
                        self.violations.push(ConservationViolation::Imbalanced {
                            at_s: event.at_s,
                            transfer: id.clone(),
                            input_kg,
                            output_kg,
                            asset_kg: *asset_mass_change_kg,
                        });
                    }
                }
            }
            EventKind::EnergyRequest { battery, amount_si, duration_s } => {
                let Some(storage) = self.batteries.get_mut(battery) else {
                    return Err(SimError::UnknownBattery(battery.clone()));
                };
                let outcome = storage.request(*amount_si, *duration_s);
                self.log.push(LogEntry {
                    at_s: event.at_s,
                    text: format!(
                        "energy {battery}: requested {:+.0} J applied {:+.0} J clipped {:+.0} J ({:?})",
                        outcome.requested_si, outcome.applied_si, outcome.clipped_si, outcome.reason
                    ),
                });
            }
        }
        Ok(())
    }

    fn evaluate_rules(&mut self) -> Result<(), SimError> {
        let rules = self.rules.clone();
        for rule in &rules {
            if self.fired_rules.contains(&rule.id) || !self.condition_holds(&rule.when) {
                continue;
            }
            self.fired_rules.insert(rule.id.clone());
            self.log.push(LogEntry { at_s: self.now_s, text: format!("rule {} fired", rule.id) });
            let kind = match &rule.then {
                RuleEffect::SetAssetStatus { asset, status } => {
                    EventKind::SetAssetStatus { asset: asset.clone(), status: *status }
                }
                RuleEffect::StockFlow { resource, delta_si, external } => {
                    EventKind::StockFlow { resource: resource.clone(), delta_si: *delta_si, external: *external }
                }
                RuleEffect::EnergyRequest { battery, amount_si, duration_s } => EventKind::EnergyRequest {
                    battery: battery.clone(),
                    amount_si: *amount_si,
                    duration_s: *duration_s,
                },
            };
            self.schedule(Event::new(self.now_s + rule.delay_s, kind)?)?;
        }
        Ok(())
    }

    fn condition_holds(&self, condition: &RuleCondition) -> bool {
        match condition {
            RuleCondition::AssetIs { asset, status } => self.assets.get(asset) == Some(status),
            RuleCondition::StockBelow { resource, amount_si } => {
                self.stocks.get(resource).copied().unwrap_or(0.0) < *amount_si
            }
        }
    }

    fn mass_kg(&self, resource: &str, amount_si: f64) -> Result<Option<f64>, SimError> {
        let Some(definition) = self.model.resources.get(resource) else {
            return Err(SimError::UnknownResource(resource.to_string()));
        };
        if definition.is_energy() {
            return Ok(None);
        }
        if definition.unit.dim == Dimension::MASS {
            return Ok(Some(amount_si));
        }
        Ok(definition.mass_per_unit_kg.map(|mass| amount_si / definition.unit.scale * mass))
    }
}

impl fmt::Debug for Simulation<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Simulation")
            .field("now_s", &self.now_s)
            .field("assets", &self.assets)
            .field("stocks", &self.stocks)
            .field("log_len", &self.log.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battery::Battery;
    use crate::event::{Event, EventKind, Flow};
    use std::path::PathBuf;

    fn base01() -> Model {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/base-01/model.toml");
        Model::load(&path).expect("Base-01 loads")
    }

    fn transfer(id: &str, inputs: Vec<(&str, f64)>, outputs: Vec<(&str, f64)>) -> Event {
        let flows = |list: Vec<(&str, f64)>| {
            list.into_iter().map(|(resource, amount_si)| Flow { resource: resource.to_string(), amount_si }).collect()
        };
        Event::new(
            0.0,
            EventKind::Transfer {
                id: id.to_string(),
                inputs: flows(inputs),
                outputs: flows(outputs),
                asset_mass_change_kg: 0.0,
            },
        )
        .unwrap()
    }

    #[test]
    fn a_balanced_transfer_passes_conservation() {
        let model = base01();
        let mut sim = Simulation::new(&model);
        sim.schedule(transfer(
            "turn-sleeve",
            vec![("bronze-bar", 1.2), ("cutting-insert", 1.0)],
            vec![("sleeve-p1-raw", 1.0), ("bronze-swarf", 0.4), ("worn-insert", 1.0)],
        ))
        .unwrap();
        sim.run_to_completion().unwrap();
        assert!(sim.violations().is_empty(), "{:?}", sim.violations());
    }

    #[test]
    fn an_unbalanced_transfer_is_reported() {
        let model = base01();
        let mut sim = Simulation::new(&model);
        sim.schedule(transfer("bad", vec![("bronze-bar", 1.2)], vec![("sleeve-p1-raw", 1.0)])).unwrap();
        sim.run_to_completion().unwrap();
        assert!(matches!(sim.violations()[0], ConservationViolation::Imbalanced { .. }));
    }

    #[test]
    fn fixed_trajectory_changes_state_and_external_flow_is_recorded() {
        let model = base01();
        let mut sim = Simulation::new(&model);
        sim.schedule(
            Event::new(10.0, EventKind::SetAssetStatus { asset: "bus-1".into(), status: AssetStatus::Failed }).unwrap(),
        )
        .unwrap();
        sim.schedule(
            Event::new(20.0, EventKind::StockFlow { resource: "bronze-bar".into(), delta_si: 5.0, external: true })
                .unwrap(),
        )
        .unwrap();
        sim.run_to_completion().unwrap();
        assert_eq!(sim.assets()["bus-1"], AssetStatus::Failed);
        assert_eq!(sim.external_flows().len(), 1);
        assert_eq!(sim.stocks()["bronze-bar"], 3.0 + 5.0);
    }

    #[test]
    fn a_state_rule_schedules_a_dependent_failure() {
        let model = base01();
        let mut sim = Simulation::new(&model);
        sim.add_rule(
            StateRule::new(
                "bus-failure-hits-pump",
                RuleCondition::AssetIs { asset: "bus-1".into(), status: AssetStatus::Failed },
                30.0,
                RuleEffect::SetAssetStatus { asset: "pump-p1".into(), status: AssetStatus::Failed },
            )
            .unwrap(),
        )
        .unwrap();
        sim.schedule(
            Event::new(10.0, EventKind::SetAssetStatus { asset: "bus-1".into(), status: AssetStatus::Failed }).unwrap(),
        )
        .unwrap();
        sim.run_to_completion().unwrap();
        assert_eq!(sim.assets()["pump-p1"], AssetStatus::Failed);
        assert_eq!(sim.now_s(), 40.0);
        assert!(sim.log().iter().any(|entry| entry.text == "rule bus-failure-hits-pump fired"));
    }

    fn schedule_sample(sim: &mut Simulation<'_>) {
        sim.schedule(
            Event::new(10.0, EventKind::SetAssetStatus { asset: "bus-1".into(), status: AssetStatus::Failed }).unwrap(),
        )
        .unwrap();
        sim.schedule(
            Event::new(20.0, EventKind::StockFlow { resource: "bronze-bar".into(), delta_si: 1.0, external: true })
                .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn chunked_runs_produce_the_same_digest_as_one_run() {
        let model = base01();
        let mut once = Simulation::new(&model);
        schedule_sample(&mut once);
        once.run_to_completion().unwrap();

        let mut chunked = Simulation::new(&model);
        schedule_sample(&mut chunked);
        chunked.run_until(15.0).unwrap();
        chunked.run_to_completion().unwrap();

        assert_eq!(once.digest(), chunked.digest());
    }

    #[test]
    fn battery_requests_are_clipped_and_logged() {
        let model = base01();
        let mut sim = Simulation::new(&model);
        sim.add_battery(Battery::new("bat-1", 36_000_000.0, 0.0, 2_000.0, 3_000.0).unwrap()).unwrap();
        sim.schedule(
            Event::new(
                0.0,
                EventKind::EnergyRequest { battery: "bat-1".into(), amount_si: 18_000_000.0, duration_s: 3_600.0 },
            )
            .unwrap(),
        )
        .unwrap();
        sim.run_to_completion().unwrap();
        assert!((sim.batteries()["bat-1"].soc_si - 7_200_000.0).abs() < 1e-6);
        assert!(sim.log().iter().any(|entry| entry.text.contains("ChargePower")));
    }

    #[test]
    fn scheduling_in_the_past_is_rejected() {
        let model = base01();
        let mut sim = Simulation::new(&model);
        sim.schedule(
            Event::new(10.0, EventKind::SetAssetStatus { asset: "bus-1".into(), status: AssetStatus::Failed }).unwrap(),
        )
        .unwrap();
        sim.run_to_completion().unwrap();
        assert!(matches!(
            sim.schedule(
                Event::new(5.0, EventKind::StockFlow { resource: "bronze-bar".into(), delta_si: 1.0, external: true })
                    .unwrap()
            ),
            Err(SimError::EventInThePast { .. })
        ));
    }
}
