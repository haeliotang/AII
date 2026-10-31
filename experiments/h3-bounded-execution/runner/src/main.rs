//! Simulated H3 bounded-execution harness (M10.4). See ../PREREG.md.
//!
//! This is a **simulated** run: it exercises the execution gate, the simulated
//! writable adapter and the fault model. It does not replace a physical
//! testbed, which is required for the M10 exit conditions.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use poie_exec::gate::{DispatchOutcome, ExecutionGate};
use poie_exec::sim::{AdapterFault, SimulatedWritableAdapter};
use poie_exec::verify::{verify_action, FixedSignal, VerificationSignal};
use poie_state::action::{Action, Authorization, BudgetItem, Precondition, Reversibility, VerificationSpec};
use poie_state::record::Timestamp;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fault {
    None,
    DropAck,
    Crash,
    Duplicate,
    Network,
    Takeover,
    Degrade,
}

impl Fault {
    fn as_str(self) -> &'static str {
        match self {
            Fault::None => "none",
            Fault::DropAck => "ack_lost",
            Fault::Crash => "crash_after_execute",
            Fault::Duplicate => "duplicate_delivery",
            Fault::Network => "network_loss",
            Fault::Takeover => "human_takeover",
            Fault::Degrade => "runtime_degraded",
        }
    }

    fn adapter_fault(self) -> AdapterFault {
        match self {
            Fault::DropAck => AdapterFault::DropAck,
            Fault::Crash => AdapterFault::CrashAfterExecute,
            Fault::Duplicate => AdapterFault::DuplicateDelivery,
            Fault::Network => AdapterFault::Unreachable,
            Fault::None | Fault::Takeover | Fault::Degrade => AdapterFault::None,
        }
    }
}

fn action(id: &str, key: &str, process: &str, target: &str) -> Action {
    Action {
        action_id: id.to_string(),
        idempotency_key: key.to_string(),
        target: target.to_string(),
        command: process.to_string(),
        plan_ref: Some("plan-h3".to_string()),
        policy_version: "policy/0.1".to_string(),
        precondition: Precondition {
            state_version: std::collections::BTreeMap::from([("site-a".to_string(), 42)]),
            predicates: vec![format!("{target} available")],
        },
        valid_from: Timestamp(0),
        valid_until: Timestamp(60_000),
        budget: vec![BudgetItem { resource: "sleeve-p1".to_string(), amount_si: 1.0 }],
        reservations: Vec::new(),
        reversibility: Reversibility::Idempotent,
        verification_spec: VerificationSpec {
            observe: format!("{target} status"),
            condition: "effect observed".to_string(),
            window_ms: 60_000,
        },
        authorization: Some(Authorization {
            by: "operator-1".to_string(),
            basis: "maintenance window".to_string(),
            scope: "dispatch".to_string(),
            valid_until: Timestamp(3_600_000),
        }),
    }
}

#[derive(Debug)]
struct Row {
    scenario: String,
    process: String,
    fault: Fault,
    outcome: String,
    physical: usize,
    registered_duplicates: usize,
    unregistered_duplicates: usize,
    sap_state: String,
    verified: Option<String>,
    passed: bool,
}

fn run(fault: Fault, process: &str, index: usize) -> Row {
    let target = if process == "replace-sleeve-p1" { "pump-p1" } else { "lathe-1" };
    let id = format!("h3-{index}-{fault:?}");
    let key = format!("key-{index}");
    let action = action(&id, &key, process, target);
    let mut gate = ExecutionGate::new();
    gate.register_authorized(&action).expect("action registers");
    match fault {
        Fault::Takeover => gate.take_over(),
        Fault::Degrade => gate.set_degraded(true),
        _ => {}
    }
    let mut adapter = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
    adapter.fault = fault.adapter_fault();
    let outcome = gate.dispatch(&action, &mut adapter, 1_000).expect("gate dispatch");
    let state = gate.ledger().state(&id).map(|state| state.as_str().to_string()).unwrap_or_else(|| "unknown".into());
    let expected_physical = match fault {
        Fault::None | Fault::DropAck | Fault::Crash => 1,
        Fault::Duplicate => 2,
        Fault::Network | Fault::Takeover | Fault::Degrade => 0,
    };
    let registered_duplicates = adapter.duplicate_physical_actions();
    let unregistered_duplicates = adapter.physical_executions().saturating_sub(1 + registered_duplicates);
    let mut verified = None;
    if let DispatchOutcome::Dispatched { receipt } = &outcome {
        if receipt.accepted {
            let mut signals: Vec<Box<dyn VerificationSignal>> = vec![
                Box::new(FixedSignal { source: "pump-status-sensor".to_string(), result: Some(true) }),
                Box::new(FixedSignal { source: "pressure-sensor".to_string(), result: Some(true) }),
            ];
            verified = verify_action(&id, "sim-executor", &mut signals).ok().map(|outcome| outcome.source);
        }
    }
    let expected_outcome = match fault {
        Fault::None => "dispatched",
        Fault::DropAck | Fault::Crash => "dispatched_or_rejected",
        Fault::Duplicate => "dispatched",
        Fault::Network | Fault::Degrade => "fallback",
        Fault::Takeover => "recorded_only",
    };
    let outcome_str = match &outcome {
        DispatchOutcome::Dispatched { .. } => "dispatched",
        DispatchOutcome::RecordedOnly { .. } => "recorded_only",
        DispatchOutcome::Fallback { .. } => "fallback",
        DispatchOutcome::Rejected { .. } => "rejected",
    };
    let state_ok = match fault {
        Fault::None | Fault::Duplicate => state == "acknowledged",
        Fault::DropAck | Fault::Crash => state == "outcome_unknown",
        Fault::Network | Fault::Takeover | Fault::Degrade => state == "authorized",
    };
    let passed = adapter.physical_executions() == expected_physical
        && unregistered_duplicates == 0
        && state_ok
        && (fault == Fault::Duplicate || registered_duplicates == 0)
        && (expected_outcome == outcome_str || expected_outcome == "dispatched_or_rejected")
        && (fault != Fault::None || verified.is_some());
    Row {
        scenario: id,
        process: process.to_string(),
        fault,
        outcome: outcome_str.to_string(),
        physical: adapter.physical_executions(),
        registered_duplicates,
        unregistered_duplicates,
        sap_state: state,
        verified,
        passed,
    }
}

fn main() -> ExitCode {
    let mut write_dir: Option<PathBuf> = None;
    let mut check_dir: Option<PathBuf> = None;
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
            "--check" => {
                let Some(path) = args.next() else {
                    eprintln!("--check needs a directory");
                    return ExitCode::from(2);
                };
                check_dir = Some(PathBuf::from(path));
            }
            other => {
                eprintln!("unknown argument {other:?}");
                return ExitCode::from(2);
            }
        }
    }

    let faults =
        [Fault::None, Fault::DropAck, Fault::Crash, Fault::Duplicate, Fault::Network, Fault::Takeover, Fault::Degrade];
    let processes = ["replace-sleeve-p1", "turn-sleeve-p1"];
    let mut rows = Vec::new();
    for (fault_index, fault) in faults.iter().enumerate() {
        for (process_index, process) in processes.iter().enumerate() {
            let index = fault_index * processes.len() + process_index;
            rows.push(run(*fault, process, index));
        }
    }
    let mut report = String::new();
    writeln!(report, "# H3 有界执行：模拟运行报告（simulated, not physical）\n").unwrap();
    writeln!(report, "本报告由 `poie-h3-bounded-execution --write <dir>` 生成；预登记见 [PREREG.md](PREREG.md)。\n")
        .unwrap();
    writeln!(report, "**限制**：这是模拟执行端（`crates/poie-exec::sim`），不是物理试验台；M10 退出条件要求的真实故障与安全演练未覆盖。\n").unwrap();
    writeln!(report, "| 场景 | 动作 | 故障 | 结果 | 物理动作 | 已登记重复 | 未登记重复 | SAP 状态 | 独立验证 | 判定 |")
        .unwrap();
    writeln!(report, "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |").unwrap();
    for row in &rows {
        writeln!(
            report,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            row.scenario,
            row.process,
            row.fault.as_str(),
            row.outcome,
            row.physical,
            row.registered_duplicates,
            row.unregistered_duplicates,
            row.sap_state,
            row.verified.clone().unwrap_or_else(|| "—".to_string()),
            if row.passed { "PASS" } else { "FAIL" }
        )
        .unwrap();
    }
    let unregistered: usize = rows.iter().map(|row| row.unregistered_duplicates).sum();
    let registered: usize = rows.iter().map(|row| row.registered_duplicates).sum();
    let failed = rows.iter().filter(|row| !row.passed).count();
    let verified = rows.iter().filter(|row| row.verified.is_some()).count();
    writeln!(report, "\n## 汇总\n").unwrap();
    writeln!(report, "- 场景数：{}", rows.len()).unwrap();
    writeln!(report, "- 未登记重复物理动作：{unregistered}（要求 0）").unwrap();
    writeln!(report, "- 已登记重复物理动作：{registered}（F3 重复投递的显式记录）").unwrap();
    writeln!(report, "- 独立验证通过：{verified}").unwrap();
    writeln!(report, "- 未通过场景：{failed}").unwrap();
    writeln!(
        report,
        "- 结论：{}",
        if failed == 0 && unregistered == 0 {
            "模拟 H3 被支持：无未登记物理重复，状态可对账，接管/降级不派发"
        } else {
            "模拟 H3 未通过：存在未登记物理重复或状态不一致"
        }
    )
    .unwrap();

    if let Some(dir) = write_dir {
        if let Err(error) = write_outputs(&dir, &report, &rows) {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    }
    if let Some(dir) = check_dir {
        let expected_report = std::fs::read_to_string(dir.join("REPORT.sim.md")).unwrap_or_default();
        let expected_raw = std::fs::read_to_string(dir.join("raw").join("sim.txt")).unwrap_or_default();
        if expected_report != report || expected_raw != raw_table(&rows) {
            eprintln!("error: regenerated H3 output differs from the committed files");
            return ExitCode::FAILURE;
        }
        println!("H3 simulated report check: {} scenarios, replay matches", rows.len());
    }
    if failed > 0 || unregistered > 0 {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn raw_table(rows: &[Row]) -> String {
    let mut out = String::new();
    for row in rows {
        writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            row.scenario,
            row.process,
            row.fault.as_str(),
            row.outcome,
            row.physical,
            row.registered_duplicates,
            row.unregistered_duplicates,
            row.sap_state,
            row.verified.clone().unwrap_or_else(|| "-".to_string()),
            row.passed
        )
        .unwrap();
    }
    out
}

fn write_outputs(dir: &Path, report: &str, rows: &[Row]) -> Result<(), String> {
    std::fs::create_dir_all(dir.join("raw")).map_err(|error| error.to_string())?;
    std::fs::write(dir.join("REPORT.sim.md"), report).map_err(|error| error.to_string())?;
    std::fs::write(dir.join("raw").join("sim.txt"), raw_table(rows)).map_err(|error| error.to_string())
}
