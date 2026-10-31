//! Two-site shadow integration (simulated, in-process federation).
//!
//! Site A observes the field, projects state, plans a service action. Site B
//! shares inventory and owns some command leases through the federation layer.
//! Dispatch is gated on reconciliation: an over-reservation or overlapping
//! ownership blocks the action; a clean reservation and sequential lease allow
//! it through the SAP-validated execution gate.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use poie_exec::gate::ExecutionGate;
use poie_exec::sim::SimulatedWritableAdapter;
use poie_exec::verify::{verify_action, FixedSignal, VerificationSignal};
use poie_federation::federation::{LeaseClaim, ReservationClaim};
use poie_federation::{ConflictKind, Federation};
use poie_ontology::graph::Goal;
use poie_ontology::model::{AssetStatus, Model, Verification};
use poie_plan::plan::{plan, PlanningGoal, PlanningRequest};
use poie_runtime::adapter::SimulatedAdapter;
use poie_runtime::runtime::LocalRuntime;
use poie_state::action::{Action, Authorization, BudgetItem, Precondition, Reversibility, VerificationSpec};
use poie_state::projection::EvidencePolicy;
use poie_state::record::Timestamp;

fn base01() -> Model {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../scenarios/base-01/model.toml");
    Model::load(&path).expect("Base-01 loads")
}

fn action_from_plan(plan: &poie_plan::plan::Plan) -> Action {
    let planned = &plan.actions[0];
    let start_ms = (planned.start_s * 1000.0) as i64;
    let end_ms = (planned.end_s * 1000.0) as i64;
    Action {
        action_id: "two-site-a1".to_string(),
        idempotency_key: "two-site-a1-key".to_string(),
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
            basis: "two-site shadow".to_string(),
            scope: "dispatch".to_string(),
            valid_until: Timestamp(end_ms + 3_600_000),
        }),
    }
}

fn federation() -> Federation {
    let mut federation = Federation::new();
    federation.add_site("site-a", 1_000).unwrap();
    federation.add_site("site-b", 1_050).unwrap();
    federation
}

fn conflicts(federation: &Federation, available: &BTreeMap<String, f64>) -> Vec<&'static str> {
    let mut kinds: Vec<&'static str> =
        federation.reconcile(available, 2_000).conflicts.iter().map(|conflict| conflict.kind.as_str()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    kinds
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
    writeln!(report, "# 两站点影子集成演示（simulated, in-process federation）\n").unwrap();
    writeln!(report, "站点 A：现场观测 → 投影 → 规划；站点 B：共享库存与租约。派发以联邦对账结果为门禁。\n").unwrap();
    writeln!(report, "**本演示为模拟/进程内联邦，不含真实多站点网络；真实多节点仍待 M12.6 外部环境。**\n").unwrap();

    // 1. Site A field observation and projection.
    let base = base01();
    let observations: Vec<_> =
        base.assets.values().map(|asset| SimulatedAdapter::observations(&asset.id, asset.status, 1_000)).collect();
    let adapter = SimulatedAdapter::new(SimulatedAdapter::declarations("site-a-sensor", "simulated-field"))
        .with_observations(observations);
    let mut runtime = LocalRuntime::new("site-a", base, vec![Box::new(adapter)], EvidencePolicy::new(300_000)).unwrap();
    let poll = runtime.poll(1_000).unwrap();
    let projected = runtime.projected_model(1_000).unwrap();
    writeln!(report, "## 1. 站点 A：观测与投影\n").unwrap();
    writeln!(report, "- 观测 {} 条；pump-p1 = {:?}", poll.accepted_observations, projected.assets["pump-p1"].status)
        .unwrap();
    raw.push_str(&format!(
        "step1\tobservations={}\tpump={:?}\n",
        poll.accepted_observations, projected.assets["pump-p1"].status
    ));

    // 2. Plan.
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
    writeln!(report, "- 动作数 {}，完成 {:.0}s", plan.actions.len(), plan.completion_s).unwrap();
    raw.push_str(&format!("step2\tactions={}\tcompletion_s={:.0}\n", plan.actions.len(), plan.completion_s));

    // 3. Conflicting reservations block dispatch.
    let available = BTreeMap::from([("lot-sleeve".to_string(), 1.0)]);
    let mut conflicting = federation();
    conflicting.reserve(ReservationClaim {
        site: "site-a".into(),
        batch: "lot-sleeve".into(),
        action_id: "a1".into(),
        amount_si: 1.0,
    });
    conflicting.reserve(ReservationClaim {
        site: "site-b".into(),
        batch: "lot-sleeve".into(),
        action_id: "b1".into(),
        amount_si: 1.0,
    });
    let conflict_kinds = conflicts(&conflicting, &available);
    let reservation_blocked = conflict_kinds.contains(&ConflictKind::OverReservation.as_str());
    writeln!(report, "\n## 3. 联邦对账：超额预留\n").unwrap();
    writeln!(report, "- 冲突：{:?}", conflict_kinds).unwrap();
    writeln!(report, "- 是否阻断派发：{}", if reservation_blocked { "是（over_reservation）" } else { "否" }).unwrap();
    raw.push_str(&format!("step3\tconflicts={:?}\tblocked={reservation_blocked}\n", conflict_kinds));

    // 4. Clean reservation and sequential lease allow dispatch.
    let mut clean = federation();
    clean.reserve(ReservationClaim {
        site: "site-a".into(),
        batch: "lot-sleeve".into(),
        action_id: "a1".into(),
        amount_si: 1.0,
    });
    clean.grant_lease(LeaseClaim { site: "site-a".into(), executor: "robot-m1".into(), from_ms: 0, until_ms: 10_000 });
    clean.grant_lease(LeaseClaim {
        site: "site-b".into(),
        executor: "robot-m1".into(),
        from_ms: 15_000,
        until_ms: 25_000,
    });
    let clean_conflicts = conflicts(&clean, &available);
    let allowed = clean_conflicts.is_empty();
    writeln!(report, "\n## 4. 联邦对账：干净预留与顺序租约\n").unwrap();
    writeln!(report, "- 冲突：{:?}；允许派发：{}", clean_conflicts, allowed).unwrap();
    raw.push_str(&format!("step4\tconflicts={clean_conflicts:?}\tallowed={allowed}\n"));

    // 5. Overlapping lease blocks dispatch.
    let mut overlap = federation();
    overlap.reserve(ReservationClaim {
        site: "site-a".into(),
        batch: "lot-sleeve".into(),
        action_id: "a1".into(),
        amount_si: 1.0,
    });
    overlap.grant_lease(LeaseClaim {
        site: "site-a".into(),
        executor: "robot-m1".into(),
        from_ms: 0,
        until_ms: 10_000,
    });
    overlap.grant_lease(LeaseClaim {
        site: "site-b".into(),
        executor: "robot-m1".into(),
        from_ms: 5_000,
        until_ms: 20_000,
    });
    let overlap_conflicts = conflicts(&overlap, &available);
    let lease_blocked = overlap_conflicts.contains(&ConflictKind::DoubleOwnership.as_str());
    writeln!(report, "\n## 5. 联邦对账：重叠租约\n").unwrap();
    writeln!(report, "- 冲突：{:?}；阻断派发：{}", overlap_conflicts, lease_blocked).unwrap();
    raw.push_str(&format!("step5\tconflicts={overlap_conflicts:?}\tblocked={lease_blocked}\n"));

    // 6. Dispatch through the SAP-validated gate with independent verification.
    let action = action_from_plan(&plan);
    let mut gate = ExecutionGate::new();
    gate.register_authorized(&action).expect("registers");
    let mut executor = SimulatedWritableAdapter::new(SimulatedWritableAdapter::declaration(true));
    let outcome = gate.dispatch(&action, &mut executor, 0).expect("gate");
    let state = gate.ledger().state("two-site-a1").map(|state| state.as_str().to_string()).unwrap_or_default();
    let mut signals: Vec<Box<dyn VerificationSignal>> = vec![
        Box::new(FixedSignal { source: "site-a-pump-status".to_string(), result: Some(true) }),
        Box::new(FixedSignal { source: "site-a-pressure".to_string(), result: Some(true) }),
    ];
    let verification = verify_action("two-site-a1", "sim-executor", &mut signals).expect("verification");
    writeln!(report, "\n## 6. 派发与独立验证\n").unwrap();
    writeln!(report, "- 派发：{}；SAP 状态：{state}；验证来源：{}", outcome.dispatched(), verification.source).unwrap();
    raw.push_str(&format!(
        "step6\tdispatched={}\tstate={state}\tverified={}\n",
        outcome.dispatched(),
        verification.source
    ));

    // 7. Assertions.
    let assertions = [
        ("site A projection applied", projected.assets["pump-p1"].status == AssetStatus::Degraded),
        ("site A planned an action", !plan.actions.is_empty()),
        ("over-reservation blocks dispatch", reservation_blocked),
        ("clean federation allows dispatch", allowed),
        ("overlapping lease blocks dispatch", lease_blocked),
        ("dispatch succeeded", outcome.dispatched()),
        ("independent verification", verification.confirmed),
    ];
    let failures = assertions.iter().filter(|(_, passed)| !passed).count();
    writeln!(report, "\n## 7. 断言\n").unwrap();
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
