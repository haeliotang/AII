//! H4 expansion accounting run (M11.5). See ../PREREG.md.
//!
//! Structural gain/loss on the human-given goal set, plus a temporal comparison
//! of base vs expanded on the 16 H2 development scenarios (same traces). The
//! construction resource draw is NOT deducted in the stock model; the report
//! states that limitation explicitly.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use poie_expansion::proposal::ExpansionProposal;
use poie_expansion::structural::{structural_gain, GoalOutcome};
use poie_expansion::temporal::temporal_gain;
use poie_ontology::graph::Goal;
use poie_ontology::model::{AssetStatus, Model, Patch, Verification};
use poie_ontology::time::TimeWindow;
use poie_ontology::units::Quantity;
use poie_plan::plan::{plan, PlanningGoal, PlanningRequest};
use poie_viability::metrics::{RunTrace, TimeSample};

const SEED: u64 = 0x706f_6965_7665_6872;
const HORIZON_H: f64 = 24.0;
const SERVICE_DEADLINE_H: f64 = 4.0;

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

struct ScenarioOutcome {
    scenario: String,
    base_success: bool,
    expanded_success: bool,
    base_completion_h: f64,
    expanded_completion_h: f64,
}

fn run_scenario(base: &Model, proposal: &ExpansionProposal, scenario: &Scenario) -> ScenarioOutcome {
    let base_model = patched(base, scenario, 0.0);
    let expanded_model = proposal.expand(&base_model).expect("expansion applies");
    let base_plan = plan(&PlanningRequest {
        model: &base_model,
        now_s: 0.0,
        horizon_s: HORIZON_H * 3600.0,
        goals: goals(),
        min_verification: Verification::Demonstrated,
        max_actions: 20,
    })
    .expect("planner total");
    let expanded_plan = plan(&PlanningRequest {
        model: &expanded_model,
        now_s: 0.0,
        horizon_s: HORIZON_H * 3600.0,
        goals: goals(),
        min_verification: Verification::Demonstrated,
        max_actions: 20,
    })
    .expect("planner total");
    let success = |plan: &poie_plan::plan::Plan| {
        plan.goals_unmet.is_empty() && plan.goals_met.iter().any(|goal| goal == "potable-water")
    };
    ScenarioOutcome {
        scenario: scenario.id.clone(),
        base_success: success(&base_plan),
        expanded_success: success(&expanded_plan),
        base_completion_h: base_plan.completion_s / 3600.0,
        expanded_completion_h: expanded_plan.completion_s / 3600.0,
    }
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

fn trace(id: &str, success: bool, completion_h: f64) -> RunTrace {
    let samples = if success {
        vec![sample(0.0, 1.0), sample(completion_h * 3600.0, 1.0)]
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
    let structural = structural_gain(&base, &proposal, &goal_set, Verification::Modeled).expect("structural gain");

    let mut outcomes = Vec::new();
    for scenario in scenarios() {
        outcomes.push(run_scenario(&base, &proposal, &scenario));
    }
    let base_traces: Vec<RunTrace> = outcomes
        .iter()
        .map(|outcome| trace(&outcome.scenario, outcome.base_success, outcome.base_completion_h))
        .collect();
    let expanded_traces: Vec<RunTrace> = outcomes
        .iter()
        .map(|outcome| trace(&outcome.scenario, outcome.expanded_success, outcome.expanded_completion_h))
        .collect();
    let minimum_levels = BTreeMap::from([("potable-water".to_string(), 1.0)]);
    let construction = TimeWindow::new(0.0, 8.0 * 3600.0).unwrap();
    let temporal = temporal_gain(&base_traces, &expanded_traces, &minimum_levels, construction);

    let mut report = String::new();
    writeln!(report, "# H4 扩张核算报告：切屑回收熔炉\n").unwrap();
    writeln!(
        report,
        "预登记：[PREREG.md](PREREG.md)。提案：[proposal.toml](../../scenarios/base-01-expansion/proposal.toml)。\n"
    )
    .unwrap();
    writeln!(
        report,
        "**局限：建造资源与占用尚未在库存模型中扣除；本报告最多给出 `modeled_gain`，不是实际建成证据。**\n"
    )
    .unwrap();
    writeln!(report, "## 1. 结构层（VEC §6.2.1）\n").unwrap();
    let render = |items: &[GoalOutcome]| -> String {
        if items.is_empty() {
            "（无）".to_string()
        } else {
            items
                .iter()
                .map(|item| format!("{}: {} → {}", item.label, item.before.as_str(), item.after.as_str()))
                .collect::<Vec<_>>()
                .join("; ")
        }
    };
    writeln!(report, "- 获得：{}", render(&structural.gained)).unwrap();
    writeln!(report, "- 失去：{}", render(&structural.lost)).unwrap();
    writeln!(
        report,
        "- 保持 candidate：{}；保持不可行：{}\n",
        structural.unchanged_candidate, structural.unchanged_blocked
    )
    .unwrap();
    writeln!(report, "## 2. 时域层（VEC §6.2.2）\n").unwrap();
    let base_success = outcomes.iter().filter(|outcome| outcome.base_success).count();
    let expanded_success = outcomes.iter().filter(|outcome| outcome.expanded_success).count();
    writeln!(report, "- 场景：{}（H2 开发集；留出集未使用）", outcomes.len()).unwrap();
    writeln!(
        report,
        "- 不扩张成功：{base_success}/{}；扩张成功：{expanded_success}/{}",
        outcomes.len(),
        outcomes.len()
    )
    .unwrap();
    writeln!(
        report,
        "- 建造期（0–8 h）关键服务检查：{}",
        if temporal.construction.within_window {
            "通过".to_string()
        } else {
            format!("违反：{}", temporal.construction.violations.join("; "))
        }
    )
    .unwrap();
    for summary in &temporal.expanded {
        writeln!(
            report,
            "- {}：min maintained {:?} s，censored {}",
            summary.service, summary.min_maintained_s, summary.censored
        )
        .unwrap();
    }
    for (service, delta) in &temporal.min_maintained_delta_s {
        writeln!(report, "- {} 维持时长变化：{:?} s", service, delta).unwrap();
    }
    writeln!(
        report,
        "- 时域层只比较供水服务；扩张的结构收益在准备目标（轴套 4、车床螺母 2）上，
  未反映到时域表中，因此时域层本身不构成正增益证据。"
    )
    .unwrap();
    writeln!(report, "\n## 3. 外部依赖\n").unwrap();
    writeln!(report, "- 建造期外部输入：furnace-module 1 item（无本地供应，已列出；未闭合）").unwrap();
    writeln!(report, "\n## 4. 判定（VEC §6.4）\n").unwrap();
    let uncertain_reasons = {
        let mut reasons = Vec::new();
        if !structural.lost.is_empty() {
            reasons.push("存在失去目标".to_string());
        }
        if !temporal.construction.within_window {
            reasons.push("建造期关键服务水平违反".to_string());
        }
        if structural.gained.is_empty() {
            reasons.push("结构层无获得".to_string());
        }
        reasons.push("建造资源未在库存中扣除".to_string());
        reasons.push("外部依赖 furnace-module 未闭合".to_string());
        reasons
    };
    if !structural.lost.is_empty() {
        writeln!(report, "- **判定：净增益为负**（失去列表非空）。").unwrap();
    } else if structural.gained.is_empty() && !temporal.construction.within_window {
        writeln!(report, "- **判定：净增益为负**（无获得且建造期违反）。").unwrap();
    } else {
        writeln!(report, "- **判定：不确定**（原因：{}）。", uncertain_reasons.join("；")).unwrap();
        writeln!(
            report,
            "- 声明等级：`modeled_gain` 不足以主张；`demonstrated_gain` 需要实际建成（M11.6，外部条件）。"
        )
        .unwrap();
    }
    writeln!(report, "\n## 5. 每场景结果\n").unwrap();
    writeln!(report, "| 场景 | 不扩张成功 | 扩张成功 | 不扩张完成 (h) | 扩张完成 (h) |").unwrap();
    writeln!(report, "| --- | --- | --- | --- | --- |").unwrap();
    for outcome in &outcomes {
        writeln!(
            report,
            "| {} | {} | {} | {:.2} | {:.2} |",
            outcome.scenario,
            outcome.base_success,
            outcome.expanded_success,
            outcome.base_completion_h,
            outcome.expanded_completion_h
        )
        .unwrap();
    }
    writeln!(report, "\n## 6. 复现清单\n").unwrap();
    writeln!(report, "- 命令：`cargo run -p poie-h4-expansion -- --write experiments/h4-expansion`").unwrap();
    writeln!(report, "- 种子：H2 开发集种子 `poieweave-h2-2026-10-31`（16 场景）；确定性 runner。").unwrap();
    writeln!(report, "- 原始输出：[`raw/h4.txt`](raw/h4.txt)。").unwrap();

    println!("{report}");
    if let Some(dir) = write_dir {
        if let Err(error) = std::fs::create_dir_all(dir.join("raw")) {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
        let raw = outcomes
            .iter()
            .map(|outcome| {
                format!(
                    "{}\t{}\t{}\t{:.3}\t{:.3}\n",
                    outcome.scenario,
                    outcome.base_success,
                    outcome.expanded_success,
                    outcome.base_completion_h,
                    outcome.expanded_completion_h
                )
            })
            .collect::<String>();
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
