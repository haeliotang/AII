//! End-to-end pipeline demo (simulated field).
//!
//! read-only adapter -> projected state with evidence expiry -> joint plan ->
//! SAP-validated authorized actions -> execution gate (takeover, degradation) ->
//! simulated writable adapter -> independent verification -> prediction check.
//!
//! The demo is deterministic and explicitly simulated; it exists as integration
//! evidence that the crates compose without a bypass path.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use poie_exec::gate::{DispatchOutcome, ExecutionGate};
use poie_exec::sim::SimulatedWritableAdapter;
use poie_exec::verify::{verify_action, FixedSignal, VerificationSignal};
use poie_ontology::graph::Goal;
use poie_ontology::model::{AssetStatus, Model, Verification};
use poie_ontology::time::TimeWindow;
use poie_plan::plan::{plan, PlanningGoal, PlanningRequest};
use poie_runtime::adapter::SimulatedAdapter;
use poie_runtime::prediction::{check_predictions, Prediction};
use poie_runtime::runtime::LocalRuntime;
use poie_state::action::{Action, Authorization, BudgetItem, Precondition, Reversibility, VerificationSpec};
use poie_state::projection::EvidencePolicy;
use poie_state::record::Timestamp;

fn base01() -> Model {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../scenarios/base-01/model.toml");
    Model::load(&path).expect("Base-01 loads")
}

fn action_from_plan(plan: &poie_plan::plan::Plan, index: usize, id: &str) -> Action {
    let planned = &plan.actions[index];
    let start_ms = (planned.start_s * 1000.0) as i64;
    let end_ms = (planned.end_s * 1000.0) as i64;
    Action {
        action_id: id.to_string(),
        idempotency_key: format!("{id}-key"),
        target: planned.target.clone(),
        command: planned.process.clone(),
        plan_ref: Some(plan.plan_id.clone()),
        policy_version: "policy/0.1".to_string(),
        precondition: Precondition {
            state_version: BTreeMap::from([("site-a".to_string(), 1)]),
            predicates: planned.preconditions.clone(),
        },
        valid_from: Timestamp(start_ms),
        valid_until: Timestamp(end_ms + 60_000),
        budget: planned
            .allocations
            .iter()
            .filter(|(_, amount)| *amount > 0.0)
            .map(|(resource, amount)| BudgetItem { resource: resource.clone(), amount_si: *amount })
            .collect(),
        reservations: Vec::new(),
        reversibility: Reversibility::Idempotent,
        verification_spec: VerificationSpec {
            observe: format!("{} status", planned.target),
            condition: "effect observed".to_string(),
            window_ms: 60_000,
        },
        authorization: Some(Authorization {
            by: "operator-1".to_string(),
            basis: "end-to-end demo".to_string(),
            scope: "dispatch".to_string(),
            valid_until: Timestamp(end_ms + 3_600_000),
        }),
        redispatch_of: None,
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

    let mut report = String::new();
    let mut raw = String::new();
    writeln!(report, "# 端到端管线演示（simulated）\n").unwrap();
    writeln!(
        report,
        "只读适配器 → 投影（证据过期）→ 联合规划 → SAP 校验 → 执行门（接管/降级）→ 模拟执行 → 独立验证 → 预测核对。\n"
    )
    .unwrap();
    writeln!(report, "**本演示为模拟现场，不含真实设备；用于证明各 crate 可组合且不存在绕过授权/验证的路径。**\n")
        .unwrap();

    // 1. Field observation and projection. The simulated field adapter reports
    // every asset so the projection covers the whole model; the pump is degraded.
    let base = base01();
    let observations: Vec<_> =
        base.assets.values().map(|asset| SimulatedAdapter::observations(&asset.id, asset.status, 1_000)).collect();
    let adapter = SimulatedAdapter::new(SimulatedAdapter::declarations("field-sensor", "simulated-field"))
        .with_observations(observations);
    let mut runtime = LocalRuntime::new("site-a", base, vec![Box::new(adapter)], EvidencePolicy::new(60_000)).unwrap();
    let poll = runtime.poll(1_000).unwrap();
    let projected = runtime.projected_model(1_000).unwrap();
    writeln!(report, "## 1. 观测与投影\n").unwrap();
    writeln!(report, "- 观察数：{}，断连适配器：{}", poll.accepted_observations, poll.disconnected_adapters.len())
        .unwrap();
    writeln!(report, "- pump-p1 投影状态：{:?}", projected.assets["pump-p1"].status).unwrap();
    raw.push_str(&format!(
        "step1\tobservations={}\tpump={:?}\n",
        poll.accepted_observations, projected.assets["pump-p1"].status
    ));

    // 2. Plan on the projected model.
    let plan = plan(&PlanningRequest {
        model: &projected,
        now_s: 0.0,
        horizon_s: 86_400.0,
        goals: vec![PlanningGoal {
            goal: Goal::Service("potable-water".into()),
            deadline_s: Some(4.0 * 3600.0),
            priority: 10,
        }],
        min_verification: Verification::Demonstrated,
        max_actions: 10,
    })
    .expect("planner total");
    writeln!(report, "\n## 2. 规划\n").unwrap();
    writeln!(report, "- 计划 {} 个动作；完成 {:.0}s", plan.actions.len(), plan.completion_s).unwrap();
    raw.push_str(&format!("step2\tactions={}\tcompletion_s={:.0}\n", plan.actions.len(), plan.completion_s));

    // 3. SAP validation and execution.
    let mut gate = ExecutionGate::new();
    let mut action = action_from_plan(&plan, 0, "e2e-a1");
    action.action_id = "e2e-a1".to_string();
    gate.register_authorized(&action).expect("authorized action registers");
    let mut executor = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
    let outcome = gate.dispatch(&action, &mut executor, 0).expect("dispatch");
    let state = gate.ledger().state("e2e-a1").map(|state| state.as_str().to_string()).unwrap_or_default();
    let mut signals: Vec<Box<dyn VerificationSignal>> = vec![
        Box::new(FixedSignal { source: "pump-status-sensor".to_string(), result: Some(true) }),
        Box::new(FixedSignal { source: "pressure-sensor".to_string(), result: Some(true) }),
    ];
    let verification = verify_action("e2e-a1", "sim-executor", &mut signals).expect("independent verification");
    writeln!(report, "\n## 3. SAP 校验、派发与独立验证\n").unwrap();
    writeln!(report, "- 派发结果：{:?}", outcome).unwrap();
    writeln!(report, "- SAP 状态：{state}；独立验证来源：{}", verification.source).unwrap();
    raw.push_str(&format!(
        "step3\tdispatched={}\tstate={state}\tverified_by={}\n",
        outcome.dispatched(),
        verification.source
    ));

    // 4. Human takeover drill.
    let mut takeover_action = action_from_plan(&plan, 0, "e2e-a2");
    takeover_action.action_id = "e2e-a2".to_string();
    takeover_action.idempotency_key = "e2e-a2-key".to_string();
    let mut takeover_gate = ExecutionGate::new();
    takeover_gate.register_authorized(&takeover_action).expect("registers");
    takeover_gate.take_over();
    let mut takeover_executor = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
    let takeover_outcome = takeover_gate.dispatch(&takeover_action, &mut takeover_executor, 0).expect("gate");
    writeln!(report, "\n## 4. 人工接管演练\n").unwrap();
    writeln!(report, "- 接管期间结果：{:?}；物理执行 {}", takeover_outcome, takeover_executor.physical_executions())
        .unwrap();
    raw.push_str(&format!(
        "step4\toutcome={:?}\tphysical={}\n",
        takeover_outcome,
        takeover_executor.physical_executions()
    ));

    // 5. Degradation drill.
    let mut degraded_action = action_from_plan(&plan, 0, "e2e-a3");
    degraded_action.action_id = "e2e-a3".to_string();
    degraded_action.idempotency_key = "e2e-a3-key".to_string();
    let mut degraded_gate = ExecutionGate::new();
    degraded_gate.register_authorized(&degraded_action).expect("registers");
    degraded_gate.set_degraded(true);
    let mut degraded_executor = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
    let degraded_outcome = degraded_gate.dispatch(&degraded_action, &mut degraded_executor, 0).expect("gate");
    writeln!(report, "\n## 5. 安全降级演练\n").unwrap();
    writeln!(report, "- 降级期间结果：{:?}；物理执行 {}", degraded_outcome, degraded_executor.physical_executions())
        .unwrap();
    raw.push_str(&format!(
        "step5\toutcome={:?}\tphysical={}\n",
        degraded_outcome,
        degraded_executor.physical_executions()
    ));

    // 6. Prediction check.
    let predictions = vec![
        Prediction {
            asset: "pump-p1".into(),
            window: TimeWindow::new(4.0 * 3600.0, 8.0 * 3600.0).unwrap(),
            observed_failure_s: Some(6.0 * 3600.0),
        },
        Prediction {
            asset: "bus-1".into(),
            window: TimeWindow::new(5.0 * 3600.0, 9.0 * 3600.0).unwrap(),
            observed_failure_s: Some(16.0 * 3600.0),
        },
        Prediction {
            asset: "lathe-1".into(),
            window: TimeWindow::new(10.0 * 3600.0, 14.0 * 3600.0).unwrap(),
            observed_failure_s: None,
        },
    ];
    let (checks, summary) = check_predictions(&predictions, 24.0 * 3600.0);
    writeln!(report, "\n## 6. 预测核对\n").unwrap();
    for check in &checks {
        writeln!(
            report,
            "- {}：窗口内 {:?}，误差 {:?}，截尾 {}",
            check.asset, check.inside_window, check.error_s, check.censored
        )
        .unwrap();
    }
    writeln!(
        report,
        "- 汇总：观测 {}，截尾 {}，窗口内 {}，MAE {:?} s",
        summary.observed, summary.censored, summary.inside_window, summary.mean_absolute_error_s
    )
    .unwrap();
    raw.push_str(&format!(
        "step6\tobserved={}\tcensored={}\tinside={}\n",
        summary.observed, summary.censored, summary.inside_window
    ));

    // 7. Assertions: no bypass, expected outcomes.
    let assertions = [
        ("projection applied", projected.assets["pump-p1"].status == AssetStatus::Degraded),
        ("plan produced an action", !plan.actions.is_empty()),
        ("dispatch succeeded", outcome.dispatched()),
        ("independent verification", verification.confirmed),
        (
            "takeover recorded only",
            matches!(takeover_outcome, DispatchOutcome::RecordedOnly { .. })
                && takeover_executor.physical_executions() == 0,
        ),
        (
            "degradation fell back",
            matches!(degraded_outcome, DispatchOutcome::Fallback { .. })
                && degraded_executor.physical_executions() == 0,
        ),
        ("prediction inside window", checks[0].inside_window == Some(true)),
        ("prediction outside window", checks[1].inside_window == Some(false)),
        ("prediction censored", checks[2].censored),
    ];
    let failures = assertions.iter().filter(|(_, passed)| !passed).count();
    writeln!(report, "\n## 7. 断言\n").unwrap();
    for (name, passed) in &assertions {
        writeln!(report, "- {}：{}", name, if *passed { "PASS" } else { "FAIL" }).unwrap();
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
