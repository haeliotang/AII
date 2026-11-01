//! Localhost TCP two-process federation exchange (M12.6 gap-narrowing).
//!
//! The parent binds two loopback ports, spawns two child processes (same binary,
//! `--site ...`), and each child exchanges a JSON message over a real TCP socket:
//! records, reservation claims and lease claims. Each child then reconciles the
//! merged view and prints a summary; the parent checks that both children report
//! the same expected conflicts (over-reservation, double ownership, duplicate
//! idempotency key).
//!
//! This is **localhost TCP between two processes**, not real multi-site
//! infrastructure; it narrows, but does not close, the M12.6 gap.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use std::time::Duration;

use poie_federation::federation::{LeaseClaim, ReservationClaim};
use poie_federation::Federation;
use poie_state::action::{Action, Authorization, BudgetItem, Precondition, Reversibility, VerificationSpec};
use poie_state::record::{Envelope, Hlc, Namespace, Payload, Record, RecordId, RecordKind, Source, Timestamp};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Message {
    site: String,
    records: Vec<Record>,
    reservations: Vec<ReservationClaim>,
    leases: Vec<LeaseClaim>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Summary {
    site: String,
    records: usize,
    conflicts: Vec<String>,
}

fn action_record(site: &str, key: &str) -> Record {
    let hlc = Hlc::new(1_000, 0);
    let action = Action {
        action_id: format!("{site}-action"),
        idempotency_key: key.to_string(),
        target: "pump-p1".to_string(),
        command: "replace-sleeve-p1".to_string(),
        plan_ref: Some("plan-tcp".to_string()),
        policy_version: "policy/0.1".to_string(),
        precondition: Precondition {
            state_version: BTreeMap::from([(site.to_string(), 1)]),
            predicates: vec!["pump-p1 available".to_string()],
        },
        valid_from: Timestamp(0),
        valid_until: Timestamp(60_000),
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
            basis: "tcp demo".to_string(),
            scope: "dispatch".to_string(),
            valid_until: Timestamp(120_000),
        }),
        redispatch_of: None,
    };
    let envelope = Envelope::new(
        RecordId::new(site, 1),
        RecordKind::Action,
        "pump-p1",
        Source::Person(format!("operator-{site}")),
        Timestamp(1_000),
        Timestamp(1_000),
        hlc,
        Namespace::live(),
    );
    Record::new(envelope, Payload::Action(Box::new(action))).unwrap()
}

fn local_message(site: &str) -> Message {
    Message {
        site: site.to_string(),
        records: vec![action_record(site, "shared-key")],
        reservations: vec![ReservationClaim {
            site: site.to_string(),
            batch: "lot-bronze".to_string(),
            action_id: format!("{site}-action"),
            amount_si: if site == "site-a" { 4.0 } else { 3.0 },
        }],
        leases: vec![LeaseClaim {
            site: site.to_string(),
            executor: "robot-m1".to_string(),
            from_ms: if site == "site-a" { 0 } else { 5_000 },
            until_ms: if site == "site-a" { 10_000 } else { 20_000 },
        }],
    }
}

fn write_message(stream: &mut TcpStream, message: &Message) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(message).expect("message serializes");
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(&bytes)?;
    stream.flush()
}

fn read_message(stream: &mut TcpStream) -> std::io::Result<Message> {
    let mut len = [0u8; 4];
    stream.read_exact(&mut len)?;
    let mut bytes = vec![0u8; u32::from_be_bytes(len) as usize];
    stream.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes).map_err(std::io::Error::other)
}

fn serve(site: &str, listen: u16, peer: u16) -> ExitCode {
    let listener = match TcpListener::bind(("127.0.0.1", listen)) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("{site}: cannot bind loopback port: {error}");
            return ExitCode::FAILURE;
        }
    };
    // Connect to the peer (retry while it binds), and accept the peer's connection.
    let mut outgoing = None;
    for _ in 0..100 {
        if let Ok(stream) = TcpStream::connect(("127.0.0.1", peer)) {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            outgoing = Some(stream);
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let Some(mut outgoing) = outgoing else {
        eprintln!("{site}: could not connect to peer port {peer}");
        return ExitCode::FAILURE;
    };
    let Ok((mut incoming, _)) = listener.accept() else {
        eprintln!("{site}: no incoming connection");
        return ExitCode::FAILURE;
    };
    let _ = incoming.set_read_timeout(Some(Duration::from_secs(5)));
    let local = local_message(site);
    if let Err(error) = write_message(&mut outgoing, &local) {
        eprintln!("{site}: send failed: {error}");
        return ExitCode::FAILURE;
    }
    let remote = match read_message(&mut incoming) {
        Ok(message) => message,
        Err(error) => {
            eprintln!("{site}: receive failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut federation = Federation::new();
    if federation.add_site(site, 0).is_err() || federation.add_site(&remote.site, 0).is_err() {
        eprintln!("{site}: cannot create sites");
        return ExitCode::FAILURE;
    }
    for record in &local.records {
        if let Err(error) = federation.append(site, record.clone()) {
            eprintln!("{site}: append failed: {error}");
            return ExitCode::FAILURE;
        }
    }
    for record in &remote.records {
        if let Err(error) = federation.append(&remote.site, record.clone()) {
            eprintln!("{site}: peer append failed: {error}");
            return ExitCode::FAILURE;
        }
    }
    for claim in local.reservations.iter().chain(remote.reservations.iter()) {
        federation.reserve(claim.clone());
    }
    for claim in local.leases.iter().chain(remote.leases.iter()) {
        federation.grant_lease(claim.clone());
    }
    let report = federation.reconcile(&BTreeMap::from([("lot-bronze".to_string(), 5.0)]), 2_000);
    let mut conflicts: Vec<String> =
        report.conflicts.iter().map(|conflict| conflict.kind.as_str().to_string()).collect();
    conflicts.sort();
    conflicts.dedup();
    let summary = Summary { site: site.to_string(), records: federation.merged().len(), conflicts };
    println!("{}", serde_json::to_string(&summary).expect("summary serializes"));
    ExitCode::SUCCESS
}

fn parent(write_dir: Option<PathBuf>) -> ExitCode {
    let exe = std::env::current_exe().expect("current executable");
    // Bind two loopback ports, then release them for the children.
    let ports: Vec<u16> = (0..2)
        .map(|_| {
            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind ephemeral port");
            listener.local_addr().expect("local addr").port()
        })
        .collect();
    let mut children = Vec::new();
    for (index, site) in ["site-a", "site-b"].iter().enumerate() {
        let peer = ports[1 - index];
        let child = Command::new(&exe)
            .arg("--site")
            .arg(site)
            .arg("--listen")
            .arg(ports[index].to_string())
            .arg("--peer")
            .arg(peer.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn child");
        children.push((site.to_string(), child));
    }
    let expected: Vec<String> = vec!["double_ownership", "duplicate_idempotency_key", "over_reservation"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let mut summaries = Vec::new();
    let mut failures = 0;
    let outputs: Vec<(String, std::process::Output)> =
        children.into_iter().map(|(site, child)| (site, child.wait_with_output().expect("wait child"))).collect();
    for (site, output) in &outputs {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout.lines().next().unwrap_or_default();
        match serde_json::from_str::<Summary>(line) {
            Ok(summary) => {
                if summary.conflicts != expected {
                    failures += 1;
                }
                summaries.push(summary);
            }
            Err(error) => {
                failures += 1;
                eprintln!(
                    "{site}: cannot parse summary {line:?}: {error}; stderr={}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
    summaries.sort_by(|left, right| left.site.cmp(&right.site));
    let mut report = String::new();
    writeln!(report, "# 联邦：本机 TCP 双进程交换演示（localhost, two processes）\n").unwrap();
    writeln!(report, "**本演示是两个本地进程通过真实 TCP socket 交换 JSON 消息；它不是真实多站点部署，只用于缩小 M12.6 的网络差距。**\n").unwrap();
    writeln!(
        report,
        "两个站点各自产生：一条使用同一幂等键的 action、一条预留（4 kg + 3 kg，超过 5 kg）、一条重叠租约。\n"
    )
    .unwrap();
    writeln!(report, "| 站点 | 合并记录数 | 对账冲突 | 判定 |").unwrap();
    writeln!(report, "| --- | --- | --- | --- |").unwrap();
    for summary in &summaries {
        writeln!(
            report,
            "| {} | {} | {} | {} |",
            summary.site,
            summary.records,
            summary.conflicts.join(", "),
            if summary.conflicts == expected { "PASS" } else { "FAIL" }
        )
        .unwrap();
    }
    writeln!(report, "\n## 判定\n").unwrap();
    writeln!(report, "- 期望冲突：{}", expected.join(", ")).unwrap();
    writeln!(report, "- 失败数：{failures}").unwrap();
    writeln!(report, "- 结论：{}", if failures == 0 { "本机 TCP 交换与重连对账符合预期" } else { "存在未达预期站点" })
        .unwrap();
    println!("{report}");
    if let Some(dir) = write_dir {
        let raw: String = summaries
            .iter()
            .map(|summary| format!("{}\t{}\t{}\n", summary.site, summary.records, summary.conflicts.join(",")))
            .collect();
        if let Err(error) = std::fs::create_dir_all(dir.join("raw"))
            .and_then(|_| std::fs::write(dir.join("REPORT.md"), &report))
            .and_then(|_| std::fs::write(dir.join("raw").join("exchange.txt"), raw))
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

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--site") {
        let site = args.get(1).cloned().unwrap_or_default();
        let listen = args.get(3).and_then(|value| value.parse().ok()).unwrap_or(0);
        let peer = args.get(5).and_then(|value| value.parse().ok()).unwrap_or(0);
        return serve(&site, listen, peer);
    }
    let mut write_dir = None;
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--write" {
            write_dir = args.get(index + 1).map(PathBuf::from);
            index += 2;
        } else {
            eprintln!("unknown argument {:?}", args[index]);
            return ExitCode::from(2);
        }
    }
    parent(write_dir)
}
