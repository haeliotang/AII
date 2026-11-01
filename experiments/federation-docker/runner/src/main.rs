//! Two-container network-partition federation preview (M12.6 AI preview).
//!
//! `node` runs inside a container: it retries a bidirectional TCP exchange with
//! its peer every 250 ms, writes the conflicting commitments when the
//! orchestrator drops `/tmp/poie-go`, and keeps a JSON status snapshot in
//! `/tmp/poie-status.json`. `run.sh` partitions the network with
//! `docker network disconnect` and saves snapshots to `raw/`; `report` judges
//! those snapshots against PREREG.md.
//!
//! Both containers share one host kernel and clock: this narrows, but does not
//! close, the M12.6 gap.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use poie_federation::federation::{LeaseClaim, ReservationClaim};
use poie_federation::Federation;
use poie_state::action::{Action, Authorization, BudgetItem, Precondition, Reversibility, VerificationSpec};
use poie_state::record::{Envelope, Hlc, Namespace, Payload, Record, RecordId, RecordKind, Source, Timestamp};

const GO_FILE: &str = "/tmp/poie-go";
const STATUS_FILE: &str = "/tmp/poie-status.json";
const SKEW_BOUND_MS: i64 = 2_000;
const EXPECTED_CONFLICTS: [&str; 3] = ["double_ownership", "duplicate_idempotency_key", "over_reservation"];

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct Message {
    site: String,
    records: Vec<Record>,
    reservations: Vec<ReservationClaim>,
    leases: Vec<LeaseClaim>,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct Status {
    site: String,
    attempts: u64,
    successes: u64,
    failures: u64,
    written: bool,
    records: usize,
    reservations: usize,
    leases: usize,
    conflicts: Vec<String>,
    conflict_count: usize,
}

struct Node {
    site: String,
    peer_site: String,
    local: Message,
    federation: Federation,
    seen_reservations: BTreeSet<String>,
    seen_leases: BTreeSet<String>,
    status: Status,
}

fn action_record(site: &str, key: &str) -> Record {
    let action = Action {
        action_id: format!("{site}-action"),
        idempotency_key: key.to_string(),
        target: "pump-p1".to_string(),
        command: "replace-sleeve-p1".to_string(),
        plan_ref: Some("plan-docker".to_string()),
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
            basis: "docker partition preview".to_string(),
            scope: "dispatch".to_string(),
            valid_until: Timestamp(120_000),
        }),
    };
    let envelope = Envelope::new(
        RecordId::new(site, 1),
        RecordKind::Action,
        "pump-p1",
        Source::Person(format!("operator-{site}")),
        Timestamp(1_000),
        Timestamp(1_000),
        Hlc::new(1_000, 0),
        Namespace::live(),
    );
    Record::new(envelope, Payload::Action(Box::new(action))).unwrap()
}

impl Node {
    fn new(site: &str, peer_site: &str) -> Node {
        let mut federation = Federation::new();
        federation.add_site(site, 0).expect("local site");
        federation.add_site(peer_site, 0).expect("peer site");
        Node {
            site: site.to_string(),
            peer_site: peer_site.to_string(),
            local: Message { site: site.to_string(), ..Message::default() },
            federation,
            seen_reservations: BTreeSet::new(),
            seen_leases: BTreeSet::new(),
            status: Status { site: site.to_string(), ..Status::default() },
        }
    }

    /// Writes the conflicting commitments locally; this must succeed whether or
    /// not the peer is reachable.
    fn write_local(&mut self) {
        let site = self.site.clone();
        let record = action_record(&site, "shared-key");
        self.federation.append(&site, record.clone()).expect("local append");
        self.local.records.push(record);
        let reservation = ReservationClaim {
            site: site.clone(),
            batch: "lot-bronze".to_string(),
            action_id: format!("{site}-action"),
            amount_si: if site == "site-a" { 4.0 } else { 3.0 },
        };
        let lease = LeaseClaim {
            site: site.clone(),
            executor: "robot-m1".to_string(),
            from_ms: if site == "site-a" { 0 } else { 5_000 },
            until_ms: if site == "site-a" { 10_000 } else { 20_000 },
        };
        self.local.reservations.push(reservation.clone());
        self.local.leases.push(lease.clone());
        self.add_claims(&[reservation], &[lease]);
        self.status.written = true;
        self.refresh();
    }

    /// Claims are deduplicated so that repeated exchanges do not grow them.
    fn add_claims(&mut self, reservations: &[ReservationClaim], leases: &[LeaseClaim]) {
        for claim in reservations {
            if self.seen_reservations.insert(format!("{}|{}|{}", claim.site, claim.batch, claim.action_id)) {
                self.federation.reserve(claim.clone());
            }
        }
        for claim in leases {
            if self
                .seen_leases
                .insert(format!("{}|{}|{}|{}", claim.site, claim.executor, claim.from_ms, claim.until_ms))
            {
                self.federation.grant_lease(claim.clone());
            }
        }
    }

    fn merge_remote(&mut self, remote: &Message) {
        if remote.site != self.peer_site {
            eprintln!("{}: ignoring message from unexpected site {:?}", self.site, remote.site);
            return;
        }
        if let Ok(site) = self.federation.site_mut(&remote.site) {
            site.merge(&remote.records);
        }
        self.add_claims(&remote.reservations, &remote.leases);
        self.refresh();
    }

    fn refresh(&mut self) {
        let report = self.federation.reconcile(&BTreeMap::from([("lot-bronze".to_string(), 5.0)]), SKEW_BOUND_MS);
        let mut kinds: Vec<String> = report.conflicts.iter().map(|c| c.kind.as_str().to_string()).collect();
        kinds.sort();
        kinds.dedup();
        self.status.records = self.federation.merged().len();
        self.status.reservations = self.seen_reservations.len();
        self.status.leases = self.seen_leases.len();
        self.status.conflict_count = report.conflicts.len();
        self.status.conflicts = kinds;
    }

    fn save_status(&self) {
        let json = serde_json::to_string(&self.status).expect("status serializes");
        let tmp = format!("{STATUS_FILE}.tmp");
        if std::fs::write(&tmp, json).and_then(|_| std::fs::rename(&tmp, STATUS_FILE)).is_err() {
            eprintln!("{}: cannot write status", self.site);
        }
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

fn try_exchange(node: &Arc<Mutex<Node>>, peer: &str) -> std::io::Result<()> {
    let addr = peer.to_socket_addrs()?.next().ok_or_else(|| std::io::Error::other("peer name did not resolve"))?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(300))?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    let local = node.lock().unwrap().local.clone();
    write_message(&mut stream, &local)?;
    let remote = read_message(&mut stream)?;
    node.lock().unwrap().merge_remote(&remote);
    Ok(())
}

fn serve(node: Arc<Mutex<Node>>, listener: TcpListener) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
        let Ok(remote) = read_message(&mut stream) else { continue };
        let local = {
            let mut guard = node.lock().unwrap();
            guard.merge_remote(&remote);
            guard.local.clone()
        };
        let _ = write_message(&mut stream, &local);
    }
}

fn run_node(site: &str, peer_site: &str, listen: u16, peer: &str) -> ExitCode {
    let listener = match TcpListener::bind(("0.0.0.0", listen)) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("{site}: cannot bind {listen}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let node = Arc::new(Mutex::new(Node::new(site, peer_site)));
    node.lock().unwrap().save_status();
    let server = Arc::clone(&node);
    std::thread::spawn(move || serve(server, listener));
    loop {
        if Path::new(GO_FILE).exists() && !node.lock().unwrap().status.written {
            node.lock().unwrap().write_local();
        }
        let result = try_exchange(&node, peer);
        {
            let mut guard = node.lock().unwrap();
            guard.status.attempts += 1;
            match result {
                Ok(()) => guard.status.successes += 1,
                Err(_) => guard.status.failures += 1,
            }
            guard.save_status();
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn load(raw: &Path, name: &str) -> Result<Vec<Status>, String> {
    let path = raw.join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut statuses: Vec<Status> = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    statuses.sort_by(|left, right| left.site.cmp(&right.site));
    if statuses.len() != 2 {
        return Err(format!("{}: expected 2 site snapshots", path.display()));
    }
    Ok(statuses)
}

fn converged(status: &Status, expected: &[String], conflict_count: Option<usize>) -> bool {
    status.records == 2
        && status.reservations == 2
        && status.leases == 2
        && status.conflicts == expected
        && conflict_count.is_none_or(|count| status.conflict_count == count)
}

fn report(raw: &Path, write_dir: Option<PathBuf>) -> ExitCode {
    let expected: Vec<String> = EXPECTED_CONFLICTS.iter().map(|kind| kind.to_string()).collect();
    let mut rows: Vec<(String, String, String, bool)> = Vec::new();
    let mut load_errors = Vec::new();
    let mut get = |name: &str| match load(raw, name) {
        Ok(statuses) => Some(statuses),
        Err(error) => {
            load_errors.push(error);
            None
        }
    };
    let d0 = get("D0");
    let d1_before = get("D1-before");
    let d1_after = get("D1-after");
    let d2 = get("D2");
    let d3 = get("D3");
    let d4: Vec<Option<Vec<Status>>> = (1..=3).map(|round| get(&format!("D4-r{round}"))).collect();

    let describe = |s: &Status| {
        format!(
            "成功 {} / 失败 {}；记录 {}；预留 {}；租约 {}；冲突 {} 条 [{}]",
            s.successes,
            s.failures,
            s.records,
            s.reservations,
            s.leases,
            s.conflict_count,
            s.conflicts.join(", ")
        )
    };
    if let Some(d0) = &d0 {
        for s in d0 {
            let ok = s.successes >= 1 && s.records == 0 && s.conflict_count == 0;
            rows.push(("D0 联通基线".into(), s.site.clone(), describe(s), ok));
        }
    }
    if let (Some(before), Some(after)) = (&d1_before, &d1_after) {
        for (b, a) in before.iter().zip(after) {
            let ok = a.successes == b.successes && a.failures >= b.failures + 3;
            let detail = format!(
                "窗口内成功 +{}，失败 +{}",
                a.successes.saturating_sub(b.successes),
                a.failures.saturating_sub(b.failures)
            );
            rows.push(("D1 分区检测".into(), a.site.clone(), detail, ok));
        }
    }
    if let (Some(after), Some(d2)) = (&d1_after, &d2) {
        for (b, s) in after.iter().zip(d2) {
            let ok = s.written
                && s.successes == b.successes
                && s.records == 1
                && s.reservations == 1
                && s.leases == 1
                && s.conflict_count == 0;
            rows.push(("D2 分区期间写入".into(), s.site.clone(), describe(s), ok));
        }
    }
    let d3_count = d3.as_ref().map(|d3| d3[0].conflict_count);
    if let Some(d3) = &d3 {
        let same_count = d3[0].conflict_count == d3[1].conflict_count;
        for s in d3 {
            let ok = same_count && converged(s, &expected, None);
            rows.push(("D3 重连对账".into(), s.site.clone(), describe(s), ok));
        }
    }
    for (index, round) in d4.iter().enumerate() {
        if let Some(round) = round {
            for s in round {
                let ok = converged(s, &expected, d3_count);
                rows.push((format!("D4 反复分区 第 {} 轮", index + 1), s.site.clone(), describe(s), ok));
            }
        }
    }

    let failures = rows.iter().filter(|row| !row.3).count() + load_errors.len();
    let mut out = String::new();
    writeln!(out, "# 联邦：双容器网络分区预演（AI 预演 · 本机 Docker · 非真实多节点）\n").unwrap();
    writeln!(
        out,
        "**两个站点各运行在一个 Docker 容器中，用 `docker network disconnect` 制造真实的网络层分区。两个容器共享宿主机内核与时钟，不是真实多节点；本结果不计入 M12.6 退出条件。** 预登记见 [PREREG.md](PREREG.md)。\n"
    )
    .unwrap();
    writeln!(out, "| 场景 | 站点 | 观测 | 判定 |").unwrap();
    writeln!(out, "| --- | --- | --- | --- |").unwrap();
    for (scenario, site, detail, ok) in &rows {
        writeln!(out, "| {scenario} | {site} | {detail} | {} |", if *ok { "PASS" } else { "FAIL" }).unwrap();
    }
    writeln!(out, "\n## 判定\n").unwrap();
    writeln!(out, "- 期望冲突类别：{}", expected.join(", ")).unwrap();
    for error in &load_errors {
        writeln!(out, "- 缺失快照：{error}").unwrap();
    }
    writeln!(out, "- 失败数：{failures}").unwrap();
    writeln!(
        out,
        "- 结论：{}",
        if failures == 0 {
            "双容器网络分区预演全部符合预登记期望"
        } else {
            "存在不符合预登记期望的项，见上表"
        }
    )
    .unwrap();
    writeln!(
        out,
        "\n## 不覆盖\n\n时钟漂移与偏差、非对称分区、丢包与乱序、崩溃重启与持久化、认证与加密、真实路由与广域网。见 PREREG §5。"
    )
    .unwrap();
    print!("{out}");
    if let Some(dir) = write_dir {
        if let Err(error) = std::fs::write(dir.join("REPORT.md"), &out) {
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

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("node") => {
            let (Some(site), Some(peer_site), Some(peer)) =
                (arg(&args, "--site"), arg(&args, "--peer-site"), arg(&args, "--peer"))
            else {
                eprintln!("usage: node --site S --peer-site P --peer HOST:PORT [--listen PORT]");
                return ExitCode::from(2);
            };
            let listen = arg(&args, "--listen").and_then(|v| v.parse().ok()).unwrap_or(7000);
            run_node(&site, &peer_site, listen, &peer)
        }
        Some("report") => {
            let Some(raw) = arg(&args, "--raw") else {
                eprintln!("usage: report --raw DIR [--write DIR]");
                return ExitCode::from(2);
            };
            report(Path::new(&raw), arg(&args, "--write").map(PathBuf::from))
        }
        _ => {
            eprintln!("usage: poie-federation-docker node|report ...");
            ExitCode::from(2)
        }
    }
}
