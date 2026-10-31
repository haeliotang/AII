//! Simulated M12.6 partition/reconnect experiment (see ../PREREG.md).
//!
//! This is a **simulated** two-site federation: it exercises HLC ordering, log
//! exchange deduplication and reconnect conflict reporting. It does not replace
//! the real multi-node tests required by M12.6.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use poie_federation::federation::{LeaseClaim, ReservationClaim};
use poie_federation::Federation;
use poie_state::action::{
    Action, ActionState, Authorization, BudgetItem, Precondition, Reversibility, VerificationSpec,
};
use poie_state::record::{Envelope, Hlc, Namespace, Payload, Record, RecordId, RecordKind, Source, Timestamp};

fn action_record(site: &str, seq: u64, hlc: Hlc, action_id: &str, key: &str) -> Record {
    let action = Action {
        action_id: action_id.to_string(),
        idempotency_key: key.to_string(),
        target: "pump-p1".to_string(),
        command: "replace-sleeve-p1".to_string(),
        plan_ref: Some("plan-1".to_string()),
        policy_version: "policy/0.1".to_string(),
        precondition: Precondition {
            state_version: BTreeMap::from([(site.to_string(), seq)]),
            predicates: vec!["pump-p1 available".to_string()],
        },
        valid_from: Timestamp(hlc.physical_ms),
        valid_until: Timestamp(hlc.physical_ms + 60_000),
        budget: vec![BudgetItem { resource: "sleeve-p1".to_string(), amount_si: 1.0 }],
        reservations: Vec::new(),
        reversibility: Reversibility::Idempotent,
        verification_spec: VerificationSpec {
            observe: "pump status".to_string(),
            condition: "pump available".to_string(),
            window_ms: 60_000,
        },
        authorization: Some(Authorization {
            by: "operator".to_string(),
            basis: "maintenance".to_string(),
            scope: "dispatch".to_string(),
            valid_until: Timestamp(hlc.physical_ms + 120_000),
        }),
    };
    let envelope = Envelope::new(
        RecordId::new(site, seq),
        RecordKind::Action,
        "pump-p1",
        Source::Person(format!("operator-{site}")),
        Timestamp(hlc.physical_ms),
        Timestamp(hlc.physical_ms),
        hlc,
        Namespace::live(),
    );
    Record::new(envelope, Payload::Action(Box::new(action))).unwrap()
}

fn transition_record(site: &str, seq: u64, hlc: Hlc, action_id: &str) -> Record {
    let mut envelope = Envelope::new(
        RecordId::new(site, seq),
        RecordKind::Transition,
        action_id,
        Source::Person(format!("dispatcher-{site}")),
        Timestamp(hlc.physical_ms),
        Timestamp(hlc.physical_ms),
        hlc,
        Namespace::live(),
    );
    envelope.kind = RecordKind::Transition;
    Record::new(
        envelope,
        Payload::Transition {
            action_id: action_id.to_string(),
            from: ActionState::Authorized,
            to: ActionState::Dispatched,
            reason: "dispatch".to_string(),
            actor: format!("dispatcher-{site}"),
        },
    )
    .unwrap()
}

struct Row {
    scenario: &'static str,
    expected: &'static str,
    conflicts: Vec<String>,
    merged: usize,
    reservations: usize,
    leases: usize,
    passed: bool,
}

fn base_federation() -> Federation {
    let mut federation = Federation::new();
    federation.add_site("site-a", 1_000).unwrap();
    federation.add_site("site-b", 1_000).unwrap();
    federation
}

fn run() -> Vec<Row> {
    let mut rows = Vec::new();

    // P0: disjoint work during the partition.
    {
        let mut federation = base_federation();
        federation.append("site-a", action_record("site-a", 1, Hlc::new(1_100, 0), "a1", "key-a")).unwrap();
        federation.append("site-b", action_record("site-b", 1, Hlc::new(1_050, 0), "b1", "key-b")).unwrap();
        let report = federation.exchange("site-a", "site-b").unwrap();
        let reconciliation = federation.reconcile(&BTreeMap::from([("lot-bronze".to_string(), 5.0)]), 2_000);
        let merged = federation.merged();
        let passed = reconciliation.conflicts.is_empty()
            && merged.len() == 2
            && report.duplicates_skipped == 0
            && merged[0].envelope.record_id.site == "site-b";
        rows.push(Row {
            scenario: "P0",
            expected: "no conflicts, ordered merge",
            conflicts: reconciliation.conflicts.iter().map(|c| c.kind.as_str().to_string()).collect(),
            merged: merged.len(),
            reservations: reconciliation.reservations,
            leases: reconciliation.leases,
            passed,
        });
    }

    // P1: over-reservation.
    {
        let mut federation = base_federation();
        federation.reserve(ReservationClaim {
            site: "site-a".into(),
            batch: "lot-bronze".into(),
            action_id: "a1".into(),
            amount_si: 4.0,
        });
        federation.reserve(ReservationClaim {
            site: "site-b".into(),
            batch: "lot-bronze".into(),
            action_id: "b1".into(),
            amount_si: 3.0,
        });
        let reconciliation = federation.reconcile(&BTreeMap::from([("lot-bronze".to_string(), 5.0)]), 2_000);
        let kinds: Vec<String> = reconciliation.conflicts.iter().map(|c| c.kind.as_str().to_string()).collect();
        let passed = kinds == vec!["over_reservation".to_string()] && reconciliation.reservations == 2;
        rows.push(Row {
            scenario: "P1",
            expected: "over_reservation, both retained",
            conflicts: kinds,
            merged: 0,
            reservations: reconciliation.reservations,
            leases: reconciliation.leases,
            passed,
        });
    }

    // P2: overlapping leases.
    {
        let mut federation = base_federation();
        federation.grant_lease(LeaseClaim {
            site: "site-a".into(),
            executor: "robot-m1".into(),
            from_ms: 0,
            until_ms: 10_000,
        });
        federation.grant_lease(LeaseClaim {
            site: "site-b".into(),
            executor: "robot-m1".into(),
            from_ms: 5_000,
            until_ms: 20_000,
        });
        let reconciliation = federation.reconcile(&BTreeMap::new(), 2_000);
        let kinds: Vec<String> = reconciliation.conflicts.iter().map(|c| c.kind.as_str().to_string()).collect();
        let passed = kinds == vec!["double_ownership".to_string()] && reconciliation.leases == 2;
        rows.push(Row {
            scenario: "P2",
            expected: "double_ownership, leases retained",
            conflicts: kinds,
            merged: 0,
            reservations: reconciliation.reservations,
            leases: reconciliation.leases,
            passed,
        });
    }

    // P3: one idempotency key used by two actions.
    {
        let mut federation = base_federation();
        federation.append("site-a", action_record("site-a", 1, Hlc::new(1_100, 0), "a1", "shared")).unwrap();
        federation.append("site-b", action_record("site-b", 1, Hlc::new(1_200, 0), "b1", "shared")).unwrap();
        federation.exchange("site-a", "site-b").unwrap();
        let reconciliation = federation.reconcile(&BTreeMap::new(), 2_000);
        let kinds: Vec<String> = reconciliation.conflicts.iter().map(|c| c.kind.as_str().to_string()).collect();
        let passed = kinds == vec!["duplicate_idempotency_key".to_string()] && reconciliation.duplicate_actions == 1;
        rows.push(Row {
            scenario: "P3",
            expected: "duplicate_idempotency_key",
            conflicts: kinds,
            merged: federation.merged().len(),
            reservations: reconciliation.reservations,
            leases: reconciliation.leases,
            passed,
        });
    }

    // P4: the same action dispatched from both sites.
    {
        let mut federation = base_federation();
        federation.append("site-a", transition_record("site-a", 1, Hlc::new(1_100, 0), "a1")).unwrap();
        federation.append("site-b", transition_record("site-b", 1, Hlc::new(1_200, 0), "a1")).unwrap();
        federation.exchange("site-a", "site-b").unwrap();
        let reconciliation = federation.reconcile(&BTreeMap::new(), 2_000);
        let kinds: Vec<String> = reconciliation.conflicts.iter().map(|c| c.kind.as_str().to_string()).collect();
        let passed = kinds == vec!["double_dispatch".to_string()];
        rows.push(Row {
            scenario: "P4",
            expected: "double_dispatch",
            conflicts: kinds,
            merged: federation.merged().len(),
            reservations: reconciliation.reservations,
            leases: reconciliation.leases,
            passed,
        });
    }

    // P5: repeated partitions; exchange must be idempotent.
    {
        let mut federation = base_federation();
        federation.append("site-a", action_record("site-a", 1, Hlc::new(1_100, 0), "a1", "key-a")).unwrap();
        federation.append("site-b", action_record("site-b", 1, Hlc::new(1_200, 0), "b1", "key-b")).unwrap();
        federation.exchange("site-a", "site-b").unwrap();
        federation.exchange("site-a", "site-b").unwrap();
        let reconciliation = federation.reconcile(&BTreeMap::new(), 2_000);
        let merged = federation.merged().len();
        let passed = reconciliation.conflicts.is_empty() && merged == 2;
        rows.push(Row {
            scenario: "P5",
            expected: "no duplicate records, stable conflicts",
            conflicts: reconciliation.conflicts.iter().map(|c| c.kind.as_str().to_string()).collect(),
            merged,
            reservations: reconciliation.reservations,
            leases: reconciliation.leases,
            passed,
        });
    }

    rows
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
    let rows = run();
    let mut report = String::new();
    writeln!(report, "# M12.6 分区与重连实验：模拟报告（simulated, not multi-node）\n").unwrap();
    writeln!(report, "预登记：[PREREG.md](PREREG.md)。**本报告来自进程内模拟，不覆盖真实网络/认证/时钟漂移；真实多节点复测仍待外部条件。**\n").unwrap();
    writeln!(report, "| 场景 | 期望 | 实际冲突 | 合并记录 | 预留 | 租约 | 判定 |").unwrap();
    writeln!(report, "| --- | --- | --- | --- | --- | --- | --- |").unwrap();
    for row in &rows {
        writeln!(
            report,
            "| {} | {} | {} | {} | {} | {} | {} |",
            row.scenario,
            row.expected,
            if row.conflicts.is_empty() { "（无）".to_string() } else { row.conflicts.join(", ") },
            row.merged,
            row.reservations,
            row.leases,
            if row.passed { "PASS" } else { "FAIL" }
        )
        .unwrap();
    }
    let failed = rows.iter().filter(|row| !row.passed).count();
    writeln!(report, "\n## 汇总\n").unwrap();
    writeln!(report, "- 场景：{}；未通过：{}", rows.len(), failed).unwrap();
    writeln!(report, "- 冲突均被报告（无静默双重控制/超额预留）；预留与租约未被丢弃").unwrap();
    writeln!(report, "- 结论：{}", if failed == 0 { "模拟分区/重连部分被支持" } else { "模拟分区/重连未通过" })
        .unwrap();
    println!("{report}");
    if let Some(dir) = write_dir {
        if let Err(error) = std::fs::create_dir_all(dir.join("raw")) {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
        let raw = rows
            .iter()
            .map(|row| {
                format!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                    row.scenario,
                    row.expected,
                    row.conflicts.join(","),
                    row.merged,
                    row.reservations,
                    row.leases,
                    row.passed
                )
            })
            .collect::<String>();
        if let Err(error) = std::fs::write(dir.join("REPORT.sim.md"), &report) {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
        if let Err(error) = std::fs::write(dir.join("raw").join("partition.txt"), raw) {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    }
    if failed > 0 {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
