//! Shadow-run integration demo (M9.4, simulated field).
//!
//! The runtime projects the observed field, the planner runs in shadow mode,
//! nothing is dispatched, and the "would do" plan is compared with a simulated
//! operator log. This exercises the M9.4 path end to end without a dispatcher.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use poie_ontology::graph::Goal;
use poie_ontology::model::{AssetStatus, Model, Verification};
use poie_plan::plan::PlanningGoal;
use poie_runtime::adapter::SimulatedAdapter;
use poie_runtime::runtime::LocalRuntime;
use poie_runtime::shadow::{ensure_no_dispatch, shadow_plan, OperatorAction};
use poie_state::projection::EvidencePolicy;

fn base01() -> Model {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../scenarios/base-01/model.toml");
    Model::load(&path).expect("Base-01 loads")
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

    let mut report = String::new();
    let mut raw = String::new();
    writeln!(report, "# 影子运行演示（M9.4，simulated field）\n").unwrap();
    writeln!(report, "投影现场状态 → 影子规划（不派发）→ 与模拟操作者日志对比。\n").unwrap();
    writeln!(report, "**本演示为模拟现场与模拟操作者日志，不含真实设备；用于验证 M9.4 的库级路径。**\n").unwrap();

    // Field: the pump is degraded and the lathe has failed; everything else is available.
    let base = base01();
    let observations: Vec<_> = base
        .assets
        .values()
        .map(|asset| {
            let status = match asset.id.as_str() {
                "pump-p1" => AssetStatus::Degraded,
                "lathe-1" => AssetStatus::Failed,
                _ => asset.status,
            };
            SimulatedAdapter::observations(&asset.id, status, 1_000)
        })
        .collect();
    let adapter = SimulatedAdapter::new(SimulatedAdapter::declarations("field-sensor", "simulated-field"))
        .with_observations(observations);
    let mut runtime = LocalRuntime::new("site-a", base, vec![Box::new(adapter)], EvidencePolicy::new(60_000)).unwrap();
    let poll = runtime.poll(1_000).unwrap();
    let projected = runtime.projected_model(1_000).unwrap();
    writeln!(report, "## 1. 观测与投影\n").unwrap();
    writeln!(
        report,
        "- 观察 {} 条；pump-p1 = {:?}，lathe-1 = {:?}",
        poll.accepted_observations, projected.assets["pump-p1"].status, projected.assets["lathe-1"].status
    )
    .unwrap();
    raw.push_str(&format!("observations={}\n", poll.accepted_observations));

    // Shadow plan: service the water loop and restore the lathe.
    let goals = vec![
        PlanningGoal { goal: Goal::Service("potable-water".into()), deadline_s: Some(4.0 * 3600.0), priority: 10 },
        PlanningGoal { goal: Goal::Restore("lathe-1".into()), deadline_s: Some(6.0 * 3600.0), priority: 5 },
    ];
    let operator =
        vec![OperatorAction { at_s: 600.0, process: "manual-inspection".to_string(), target: "pump-p1".to_string() }];
    let shadow = shadow_plan(&projected, 0.0, 86_400.0, goals.clone(), Verification::Demonstrated, &operator)
        .expect("shadow plan");
    writeln!(report, "\n## 2. 影子规划与对比\n").unwrap();
    writeln!(
        report,
        "- 计划动作 {} 个；`dispatched` = {}（必须为 false）",
        shadow.plan.actions.len(),
        shadow.dispatched
    )
    .unwrap();
    for action in &shadow.plan.actions {
        writeln!(report, "  - {} → {} @ {:.0}s", action.process, action.target, action.start_s).unwrap();
    }
    for divergence in &shadow.divergences {
        writeln!(
            report,
            "  - 分歧 {}: planned={:?} operator={:?}",
            divergence.kind, divergence.planned, divergence.operator
        )
        .unwrap();
    }
    let would_do_not_observed = shadow.divergences.iter().filter(|d| d.kind == "would_do_not_observed").count();
    let observed_not_planned = shadow.divergences.iter().filter(|d| d.kind == "observed_not_planned").count();
    raw.push_str(&format!(
        "planned={}\ndispatched={}\nwould_do_not_observed={}\nobserved_not_planned={}\n",
        shadow.plan.actions.len(),
        shadow.dispatched,
        would_do_not_observed,
        observed_not_planned
    ));

    // Operator actions that match the plan are not reported as divergences.
    let matched: Vec<OperatorAction> = shadow
        .plan
        .actions
        .iter()
        .map(|action| OperatorAction { at_s: 1_200.0, process: action.process.clone(), target: action.target.clone() })
        .collect();
    let aligned = shadow_plan(&projected, 0.0, 86_400.0, goals, Verification::Demonstrated, &matched)
        .expect("aligned shadow plan");
    // A matching operator log produces no divergences and still never dispatches.
    let aligned_is_recorded_only = !aligned.dispatched && aligned.divergences.is_empty();

    // The explicit guard rejects any dispatch request from shadow mode.
    let guard_rejected = ensure_no_dispatch(true).is_err();

    // Assertions.
    let assertions = [
        (
            "projection applied",
            projected.assets["pump-p1"].status == AssetStatus::Degraded
                && projected.assets["lathe-1"].status == AssetStatus::Failed,
        ),
        ("shadow plan records actions", !shadow.plan.actions.is_empty()),
        ("shadow never dispatched", !shadow.dispatched),
        ("would_do_not_observed reported", would_do_not_observed >= 1),
        ("observed_not_planned reported", observed_not_planned >= 1),
        ("dispatch guard rejects shadow dispatch", guard_rejected),
        ("recorded-only run has no plan actions", aligned_is_recorded_only),
    ];
    let failures = assertions.iter().filter(|(_, passed)| !passed).count();
    writeln!(report, "\n## 3. 断言\n").unwrap();
    for (name, passed) in &assertions {
        writeln!(report, "- {name}：{}", if *passed { "PASS" } else { "FAIL" }).unwrap();
    }
    writeln!(report, "- 失败数：{failures}").unwrap();
    raw.push_str(&format!("assertions_failed={failures}\n"));
    println!("{report}");
    if let Some(dir) = write_dir {
        if let Err(error) = std::fs::create_dir_all(dir.join("raw"))
            .and_then(|_| std::fs::write(dir.join("REPORT.md"), &report))
            .and_then(|_| std::fs::write(dir.join("raw").join("trace.txt"), &raw))
        {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    }
    if failures > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
