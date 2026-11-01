//! H4 expansion accounting run (M11.5). See ../PREREG.md.
//!
//! Revised 2026-11-01 for issues #109/#110 (VEC §6.1–6.3 revision):
//! - structural gain/loss on the human-given goal set at the Base-01 operating
//!   threshold (`demonstrated`), at three stages (during construction, built,
//!   commissioned) with the construction and bootstrap draw deducted and the
//!   occupied assets unavailable during construction;
//! - temporal comparison on the 16 H2 development scenarios: the expanded
//!   branch pays the construction draw from the scenario's own stock and
//!   carries the furnace's failure mechanism drawn from a common exogenous
//!   source; a scenario whose stock cannot pay for construction is reported
//!   as "not buildable", never dropped.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use poie_expansion::proposal::{ExpansionError, ExpansionProposal, Stage};
use poie_expansion::structural::{structural_gain, GoalOutcome};
use poie_expansion::temporal::temporal_gain;
use poie_ontology::graph::Goal;
use poie_ontology::model::{AssetStatus, Model, Patch, Verification};
use poie_ontology::time::TimeWindow;
use poie_ontology::units::Quantity;
use poie_plan::plan::{plan, PlanningGoal, PlanningRequest};
use poie_viability::metrics::{RunTrace, TimeSample};

const SEED: u64 = 0x706f_6965_7665_6872;
/// Separate stream of the common exogenous source for added-asset failures, so
/// that the base scenarios (and their draws) are unchanged.
const FAILURE_STREAM: u64 = 0x6675_726e_6163_6531; // "furnace1"
const HORIZON_H: f64 = 24.0;
const SERVICE_DEADLINE_H: f64 = 4.0;
/// Base-01 planning threshold (H2 and H4 plan at `demonstrated`): given by
/// human policy, not by the proposal (VEC §6.3).
const OPERATING_THRESHOLD: Verification = Verification::Demonstrated;

#[derive(Clone, Copy, Debug)]
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

#[derive(Clone, Debug)]
struct Scenario {
    id: String,
    pump_failure_h: f64,
    bus_failure_h: f64,
    energy_kwh: f64,
    interruption_h: f64,
}

fn scenarios() -> Vec<Scenario> {
    let pump = [0.0, 2.0, 4.0, 6.0];
    let bus = [12.0, 16.0, 20.0];
    let energy = [0.5, 1.0, 2.0];
    let interruption = [6.0, 10.0, 14.0];
    let mut rng = SplitMix64::new(SEED);
    let mut out = Vec::new();
    for index in 0..24 {
        if index >= 16 {
            continue; // holdout not used by H4
        }
        out.push(Scenario {
            id: format!("h2-{index:02}"),
            pump_failure_h: pump[rng.index(pump.len())],
            bus_failure_h: bus[rng.index(bus.len())],
            energy_kwh: energy[rng.index(energy.len())],
            interruption_h: interruption[rng.index(interruption.len())],
        });
    }
    out
}

/// Furnace failure time per scenario: exponential with the proposal's MTBF,
/// inverse-transform sampled from the shared uniform stream (VEC §2.3 common
/// exogenous source; review C-08).
fn furnace_failures(count: usize, mtbf_h: f64) -> Vec<f64> {
    let mut rng = SplitMix64::new(SEED ^ FAILURE_STREAM);
    (0..count).map(|_| -mtbf_h * (1.0 - rng.unit()).ln()).collect()
}

fn patched(base: &Model, scenario: &Scenario, now_h: f64) -> Model {
    let mut patch = Patch::default();
    let now_s = now_h * 3600.0;
    if scenario.pump_failure_h * 3600.0 <= now_s {
        patch.asset_status.insert("pump-p1".into(), AssetStatus::Failed);
    }
    if scenario.bus_failure_h * 3600.0 <= now_s {
        patch.asset_status.insert("bus-1".into(), AssetStatus::Failed);
    }
    if scenario.interruption_h * 3600.0 <= now_s {
        for lot in base.lots.iter().filter(|lot| lot.resource == "bronze-bar") {
            patch.lot_quantity.insert(lot.id.clone(), "0 kg".into());
        }
    }
    for lot in base.lots.iter().filter(|lot| lot.resource == "electric-energy") {
        patch.lot_quantity.insert(lot.id.clone(), format!("{} kWh", scenario.energy_kwh));
    }
    base.patched(&patch).expect("scenario patch valid")
}

fn goals() -> Vec<PlanningGoal> {
    vec![
        PlanningGoal {
            goal: Goal::Service("potable-water".into()),
            deadline_s: Some(SERVICE_DEADLINE_H * 3600.0),
            priority: 10,
        },
        PlanningGoal { goal: Goal::Restore("lathe-1".into()), deadline_s: None, priority: 1 },
    ]
}

/// Outcome of one planning branch.
struct Branch {
    success: bool,
    completion_h: f64,
    /// Stored energy left at the end of the plan (terminal state, VEC §2.1).
    terminal_energy_kwh: f64,
}

struct ScenarioOutcome {
    scenario: String,
    base: Branch,
    /// `None`: the scenario's stock cannot pay for construction.
    expanded: Option<Branch>,
    not_buildable: Option<String>,
    furnace_failure_h: f64,
}

fn run_branch(model: &Model, energy_kwh: f64) -> Branch {
    let planned = plan(&PlanningRequest {
        model,
        now_s: 0.0,
        horizon_s: HORIZON_H * 3600.0,
        goals: goals(),
        min_verification: OPERATING_THRESHOLD,
        max_actions: 20,
    })
    .expect("planner total");
    let used_j: f64 = planned
        .actions
        .iter()
        .flat_map(|action| action.allocations.iter())
        .filter(|(resource, _)| resource == "electric-energy")
        .map(|(_, amount)| *amount)
        .sum();
    Branch {
        success: planned.goals_unmet.is_empty() && planned.goals_met.iter().any(|goal| goal == "potable-water"),
        completion_h: planned.completion_s / 3600.0,
        terminal_energy_kwh: energy_kwh - used_j / 3.6e6,
    }
}

fn run_scenario(
    base: &Model,
    proposal: &ExpansionProposal,
    scenario: &Scenario,
    furnace_failure_h: f64,
) -> ScenarioOutcome {
    let base_model = patched(base, scenario, 0.0);
    let base_branch = run_branch(&base_model, scenario.energy_kwh);
    // Construction starts at t = 0 and runs for its declared duration: the
    // critical deadline falls inside it, so the expanded branch plans on the
    // during-construction model (draw deducted, occupied assets unavailable).
    let (expanded, not_buildable) = match proposal.stage_model(&base_model, Stage::DuringConstruction) {
        Ok(model) => (Some(run_branch(&model, scenario.energy_kwh)), None),
        Err(ExpansionError::NotBuildable(reason)) => (None, Some(reason)),
        Err(error) => panic!("stage model: {error}"),
    };
    ScenarioOutcome { scenario: scenario.id.clone(), base: base_branch, expanded, not_buildable, furnace_failure_h }
}

fn sample(at_s: f64, level: f64) -> TimeSample {
    TimeSample {
        at_s,
        service_levels: BTreeMap::from([("potable-water".to_string(), level)]),
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

fn trace(id: &str, branch: &Branch) -> RunTrace {
    let samples = if branch.success {
        vec![sample(0.0, 1.0), sample(branch.completion_h * 3600.0, 1.0)]
    } else {
        vec![sample(0.0, 1.0), sample(SERVICE_DEADLINE_H * 3600.0, 0.0)]
    };
    RunTrace {
        scenario_id: id.to_string(),
        horizon_s: HORIZON_H * 3600.0,
        minimum_levels: BTreeMap::from([("potable-water".to_string(), 1.0)]),
        samples,
        recovery_paths: BTreeMap::new(),
    }
}

fn render(items: &[GoalOutcome]) -> String {
    if items.is_empty() {
        "（无）".to_string()
    } else {
        items
            .iter()
            .map(|item| format!("{}: {} → {}", item.label, item.before.as_str(), item.after.as_str()))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

fn stage_name(stage: Stage) -> &'static str {
    match stage {
        Stage::DuringConstruction => "建造期间",
        Stage::Built => "建成未投运",
        Stage::Commissioned => "投运后（假设达到投运等级）",
    }
}

fn main() -> ExitCode {
    let mut write_dir: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--write" => {
                let Some(path) = args.next() else {
                    eprintln!("--write needs a directory");
                    return ExitCode::from(2);
                };
                write_dir = Some(PathBuf::from(path));
            }
            other => {
                eprintln!("unknown argument {other:?}");
                return ExitCode::from(2);
            }
        }
    }
    let base = {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../scenarios/base-01/model.toml");
        Model::load(&path).expect("Base-01 loads")
    };
    let proposal = {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../scenarios/base-01-expansion/proposal.toml");
        ExpansionProposal::load(&path).expect("proposal loads")
    };
    let goal_set = vec![
        Goal::Service("potable-water".into()),
        Goal::Obtain { resource: "sleeve-p1".into(), quantity: "4 item".parse::<Quantity>().unwrap() },
        Goal::Restore("lathe-1".into()),
        Goal::Obtain { resource: "lathe-nut".into(), quantity: "2 item".parse::<Quantity>().unwrap() },
    ];
    let critical_labels = ["potable-water"];
    let structural =
        structural_gain(&base, &proposal, &goal_set, OPERATING_THRESHOLD).expect("structural gain on Base-01");

    let scenario_list = scenarios();
    let furnace = proposal.failure_modes.iter().find(|mode| mode.asset == "furnace-1").expect("furnace failure mode");
    let failures = furnace_failures(scenario_list.len(), furnace.mtbf_s / 3600.0);
    let outcomes: Vec<ScenarioOutcome> = scenario_list
        .iter()
        .zip(&failures)
        .map(|(scenario, failure)| run_scenario(&base, &proposal, scenario, *failure))
        .collect();
    let base_traces: Vec<RunTrace> = outcomes.iter().map(|outcome| trace(&outcome.scenario, &outcome.base)).collect();
    let expanded_traces: Vec<RunTrace> = outcomes
        .iter()
        .filter_map(|outcome| outcome.expanded.as_ref().map(|branch| trace(&outcome.scenario, branch)))
        .collect();
    let minimum_levels = BTreeMap::from([("potable-water".to_string(), 1.0)]);
    let construction_h = proposal.construction.duration_s / 3600.0;
    let construction = TimeWindow::new(0.0, proposal.construction.duration_s).unwrap();
    let temporal = temporal_gain(&base_traces, &expanded_traces, &minimum_levels, construction);
    let not_buildable = outcomes.iter().filter(|outcome| outcome.expanded.is_none()).count();

    let mut report = String::new();
    writeln!(report, "# H4 扩张核算报告：切屑回收熔炉\n").unwrap();
    writeln!(
        report,
        "预登记：[PREREG.md](PREREG.md)。提案：[proposal.toml](../../scenarios/base-01-expansion/proposal.toml)。\n"
    )
    .unwrap();
    writeln!(
        report,
        "**本报告按 VEC 2026-11-01 修订（issue #109、#110）重跑**：结构层在运营门槛 `{OPERATING_THRESHOLD}` 上、分三个时点评估，并扣除建造与 bootstrap 消耗；时域层的扩张分支从场景库存中支付建造消耗，并带入熔炉自身的故障机制。旧结论见 §7 变更记录。本报告最多给出 `modeled_gain`，不是实际建成证据。\n"
    )
    .unwrap();

    writeln!(report, "## 1. 结构层（VEC §6.2.1）\n").unwrap();
    writeln!(report, "- 运营门槛：`{OPERATING_THRESHOLD}`（人类策略给定，与 H2/H4 规划一致；不由提案给定）").unwrap();
    writeln!(
        report,
        "- 投运等级：`{}`（不低于运营门槛）；熔炉建成前不存在，建成未投运时以当前等级 `modeled` 参与",
        proposal.commissioning.required_verification
    )
    .unwrap();
    let draw = proposal.construction_draw(&base).expect("draw");
    let draw_text = draw
        .iter()
        .map(|(resource, amount)| {
            let unit = base.resources[resource].unit;
            format!("{resource} {} {}", poie_ontology::units::fmt_number(amount / unit.scale), unit.symbol)
        })
        .collect::<Vec<_>>()
        .join("、");
    let occupied: std::collections::BTreeSet<&String> =
        proposal.construction.assets.iter().chain(&proposal.bootstrap_assets).collect();
    writeln!(
        report,
        "- 扣除的建造与 bootstrap 消耗（同一资源取两者较大值）：{draw_text}；建造期间不可用的资产：{}\n",
        occupied.into_iter().cloned().collect::<Vec<_>>().join("、")
    )
    .unwrap();
    writeln!(report, "| 时点 | 获得 | 失去 | 未定（search_limit） |").unwrap();
    writeln!(report, "| --- | --- | --- | --- |").unwrap();
    for stage in &structural.stages {
        writeln!(
            report,
            "| {} | {} | {} | {} |",
            stage_name(stage.stage),
            render(&stage.gained),
            render(&stage.lost),
            render(&stage.undetermined)
        )
        .unwrap();
    }
    writeln!(
        report,
        "\n- 汇总：获得（投运后，以投运达标为前提）{}；失去（任一时点）{}；投运后保持 candidate {}、保持不可行 {}",
        render(&structural.gained),
        render(&structural.lost),
        structural.unchanged_candidate,
        structural.unchanged_blocked
    )
    .unwrap();
    let critical_lost: Vec<&GoalOutcome> =
        structural.lost.iter().filter(|outcome| critical_labels.contains(&outcome.label.as_str())).collect();
    writeln!(
        report,
        "- 失去列表中的关键服务目标：{}\n",
        if critical_lost.is_empty() {
            "无".to_string()
        } else {
            critical_lost
                .iter()
                .map(|outcome| format!("{}（{}）", outcome.label, stage_name(outcome.stage)))
                .collect::<Vec<_>>()
                .join("、")
        }
    )
    .unwrap();

    writeln!(report, "## 2. 时域层（VEC §6.2.2）\n").unwrap();
    let base_success = outcomes.iter().filter(|outcome| outcome.base.success).count();
    let expanded_success =
        outcomes.iter().filter(|outcome| outcome.expanded.as_ref().is_some_and(|b| b.success)).count();
    writeln!(report, "- 场景：{}（H2 开发集；留出集未使用）", outcomes.len()).unwrap();
    writeln!(report, "- 不扩张成功：{base_success}/{}", outcomes.len()).unwrap();
    writeln!(
        report,
        "- 扩张：本地库存无法支付建造消耗（不可建造）{not_buildable}/{}；可建造且成功 {expanded_success}/{}",
        outcomes.len(),
        outcomes.len()
    )
    .unwrap();
    if let Some(reason) = outcomes.iter().find_map(|outcome| outcome.not_buildable.as_ref()) {
        writeln!(report, "- 不可建造的原因（示例）：{reason}").unwrap();
    }
    writeln!(
        report,
        "- 建造期（0–{construction_h:.0} h）关键服务检查（只覆盖可建造的场景）：{}",
        if expanded_traces.is_empty() {
            "无可检查场景".to_string()
        } else if temporal.construction.within_window {
            "通过".to_string()
        } else {
            format!("违反：{}", temporal.construction.violations.join("; "))
        }
    )
    .unwrap();
    for summary in &temporal.base {
        writeln!(
            report,
            "- 不扩张 {}：KM 中位维持时长 {}，截尾 {}/{}",
            summary.service,
            summary
                .median_maintained_s
                .map_or(format!("> {:.0} s", summary.follow_up_s), |value| format!("{value:.0} s")),
            summary.censored,
            summary.scenarios
        )
        .unwrap();
    }
    let non_inferior = temporal.non_inferior("potable-water", 0.0);
    writeln!(
        report,
        "- 关键服务非劣效（δ = 0，未预登记，仅作描述）：{}",
        match non_inferior {
            Some(true) if not_buildable == 0 => "满足",
            _ => "不满足或无法比较（扩张分支在部分或全部场景中不可建造）",
        }
    )
    .unwrap();
    writeln!(
        report,
        "- 扩张分支的扰动集合 = 基线扰动 ∪ 熔炉故障（MTBF {:.0} h，{}；来源：{}）。共同外生源抽得的熔炉故障时刻：{} 个场景在 {HORIZON_H:.0} h 时域内发生",
        furnace.mtbf_s / 3600.0,
        if furnace.synthetic { "synthetic" } else { "非合成" },
        furnace.source,
        outcomes.iter().filter(|outcome| outcome.furnace_failure_h <= HORIZON_H).count()
    )
    .unwrap();
    writeln!(report, "- 时域层只比较供水服务；扩张的结构收益在准备目标（轴套 4）上，只在投运后出现。").unwrap();

    writeln!(report, "\n## 3. 外部依赖\n").unwrap();
    writeln!(report, "- 建造期外部输入：furnace-module 1 item（无本地供应，已列出；未闭合）").unwrap();
    writeln!(
        report,
        "- 维护（upkeep）只声明了时域内的 5 kWh；按设施寿命或年化的维护核算（VEC §6.1 修订）在提案中尚未给出。"
    )
    .unwrap();

    writeln!(report, "\n## 4. 采纳条件逐条核对（VEC §6.3）\n").unwrap();
    writeln!(
        report,
        "1. 投运等级不低于运营门槛：满足（`{}` ≥ `{OPERATING_THRESHOLD}`）；增益以投运达到该等级为前提。",
        proposal.commissioning.required_verification
    )
    .unwrap();
    writeln!(
        report,
        "2. 失去列表无关键服务恢复目标（或已被授权者接受）：{}。",
        if critical_lost.is_empty() { "满足" } else { "**不满足**，且没有授权者接受该风险" }
    )
    .unwrap();
    writeln!(
        report,
        "3. 时域层关键服务不劣于基线、且预登记的唯一主要长期目标超过阈值：**无法满足**（PREREG 未预登记主要长期目标与非劣效界；扩张分支在 {not_buildable}/{} 个场景中不可建造）。",
        outcomes.len()
    )
    .unwrap();
    writeln!(report, "4. 未闭合外部依赖已列出：满足（furnace-module）。").unwrap();
    writeln!(report, "5. 复现清单：见 §6。").unwrap();

    writeln!(report, "\n## 5. 判定（PREREG §6；VEC §6.4）\n").unwrap();
    let negative = !critical_lost.is_empty() || (not_buildable > 0 && base_success > 0);
    if negative {
        let mut reasons = Vec::new();
        if !critical_lost.is_empty() {
            reasons.push("失去列表含关键服务目标（建造占用唯一的维修机器人，供水恢复路线在建造期间失去）".to_string());
        }
        if not_buildable > 0 {
            reasons.push(format!(
                "时域层劣于基线：扩张分支在 {not_buildable}/{} 个场景中无法从本地库存支付建造消耗",
                outcomes.len()
            ));
        }
        writeln!(report, "- **判定：净增益为负**（{}）。", reasons.join("；")).unwrap();
    } else {
        writeln!(report, "- **判定：不确定**。").unwrap();
    }
    writeln!(
        report,
        "- 声明等级：`proposed`。`modeled_gain` 不成立（§6.3 第 2、3 条不满足）；`demonstrated_gain` 需要实际建成（M11.6，外部条件）。"
    )
    .unwrap();

    writeln!(report, "\n## 6. 每场景结果\n").unwrap();
    writeln!(
        report,
        "| 场景 | 不扩张成功 | 不扩张完成 (h) | 不扩张期末储能 (kWh) | 扩张可建造 | 扩张成功 | 扩张完成 (h) | 熔炉故障 (h) |"
    )
    .unwrap();
    writeln!(report, "| --- | --- | --- | --- | --- | --- | --- | --- |").unwrap();
    for outcome in &outcomes {
        let (buildable, success, completion) = match &outcome.expanded {
            Some(branch) => ("true", branch.success.to_string(), format!("{:.2}", branch.completion_h)),
            None => ("false", "-".to_string(), "-".to_string()),
        };
        writeln!(
            report,
            "| {} | {} | {:.2} | {:.2} | {buildable} | {success} | {completion} | {:.1} |",
            outcome.scenario,
            outcome.base.success,
            outcome.base.completion_h,
            outcome.base.terminal_energy_kwh,
            outcome.furnace_failure_h
        )
        .unwrap();
    }
    writeln!(report, "\n## 7. 复现清单与变更记录\n").unwrap();
    writeln!(report, "- 命令：`cargo run -p poie-h4-expansion -- --write experiments/h4-expansion`").unwrap();
    writeln!(
        report,
        "- 种子：H2 开发集种子 `poieweave-h2-2026-10-31`（16 场景）；熔炉故障使用同一种子派生的独立流（`furnace1`）；确定性 runner。"
    )
    .unwrap();
    writeln!(report, "- 原始输出：[`raw/h4.txt`](raw/h4.txt)。").unwrap();
    writeln!(
        report,
        "- 2026-10-31 初版结论：**不确定**（理由：建造资源未在库存中扣除；外部依赖 furnace-module 未闭合）。当时结构层在 `modeled` 门槛上报告获得 obtain:sleeve-p1、失去（无），时域层 16/16 对 16/16。"
    )
    .unwrap();
    writeln!(
        report,
        "- 2026-11-01 按 VEC 修订（issue #109、#110）重跑，结论改为本报告 §5。改变原因是契约修正，而不是数据或预期的调整：评估门槛改为运营门槛 `demonstrated`（提案原自设 `modeled`）；扣除建造与 bootstrap 消耗、建造期间占用资产不可用（暴露了建造期供水恢复路线的失去，以及 H2 场景储能 0.5–2 kWh 不足以支付建造所需 20 kWh）；扩张分支带入熔炉自身故障。"
    )
    .unwrap();

    println!("{report}");
    if let Some(dir) = write_dir {
        if let Err(error) = std::fs::create_dir_all(dir.join("raw")) {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
        let mut raw = String::from(
            "scenario\tbase_success\tbase_completion_h\tbase_terminal_energy_kwh\tbuildable\texpanded_success\texpanded_completion_h\tfurnace_failure_h\n",
        );
        for outcome in &outcomes {
            let (buildable, success, completion) = match &outcome.expanded {
                Some(branch) => (true, branch.success.to_string(), format!("{:.3}", branch.completion_h)),
                None => (false, "-".to_string(), "-".to_string()),
            };
            raw.push_str(&format!(
                "{}\t{}\t{:.3}\t{:.3}\t{}\t{}\t{}\t{:.3}\n",
                outcome.scenario,
                outcome.base.success,
                outcome.base.completion_h,
                outcome.base.terminal_energy_kwh,
                buildable,
                success,
                completion,
                outcome.furnace_failure_h
            ));
        }
        if let Err(error) = std::fs::write(dir.join("REPORT.md"), &report) {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
        if let Err(error) = std::fs::write(dir.join("raw").join("h4.txt"), raw) {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
