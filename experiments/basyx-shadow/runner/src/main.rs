//! BaSyx AAS read-only shadow rehearsal (M9.2, AI rehearsal on a local container).
//!
//! The runner keeps one `LocalRuntime` alive across phases. `run.sh` drives the
//! container (start/stop/restart/inject) and sends one phase command per line
//! on stdin; after each phase the runner appends `<phase> done` to the ack file.
//!
//! Commands: `read <ObservedAt>`, `disconnected`, `restarted`,
//! `reinjected <ObservedAt>`, `finish`.

mod aas;
mod adapter;
mod http;

use std::cell::RefCell;
use std::fmt::Write as _;
use std::io::{BufRead, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;

use poie_ontology::graph::Goal;
use poie_ontology::model::{AssetStatus, Model, Verification};
use poie_plan::plan::PlanningGoal;
use poie_runtime::adapter::AdapterError;
use poie_runtime::runtime::{LocalRuntime, PollReport};
use poie_runtime::shadow::shadow_plan;
use poie_state::projection::EvidencePolicy;

use crate::adapter::{wall_clock_ms, BasyxAdapter, Diagnostics};

const ASSET: &str = "pump-p1";
const SUBMODEL_ID: &str = "urn:poieweave:base-01:pump-p1:operational-data";
const EVIDENCE_MAX_AGE_MS: u64 = 60_000;

struct Check {
    id: &'static str,
    item: &'static str,
    expected: &'static str,
    actual: String,
    pass: bool,
}

struct Args {
    base_url: String,
    out: PathBuf,
    ack: PathBuf,
    image: String,
}

fn parse_args() -> Result<Args, String> {
    let mut base_url = None;
    let mut out = None;
    let mut ack = None;
    let mut image = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--base-url" => base_url = Some(value()?),
            "--out" => out = Some(PathBuf::from(value()?)),
            "--ack" => ack = Some(PathBuf::from(value()?)),
            "--image" => image = Some(value()?),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(Args {
        base_url: base_url.ok_or("--base-url is required")?,
        out: out.ok_or("--out is required")?,
        ack: ack.ok_or("--ack is required")?,
        image: image.ok_or("--image is required")?,
    })
}

fn base01() -> Model {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../scenarios/base-01/model.toml");
    Model::load(&path).expect("Base-01 loads")
}

fn error_class(error: &Option<AdapterError>) -> String {
    match error {
        None => "none".to_string(),
        Some(AdapterError::Disconnected(detail)) => format!("Disconnected({detail})"),
        Some(AdapterError::Protocol(detail)) => format!("Protocol({detail})"),
        Some(AdapterError::Invalid(detail)) => format!("Invalid({detail})"),
    }
}

/// One runtime poll plus the adapter's view of it, recorded as raw text.
struct PhaseOutcome {
    report: PollReport,
    diagnostics: Diagnostics,
    now_ms: i64,
}

fn run_phase(runtime: &mut LocalRuntime, diagnostics: &Rc<RefCell<Diagnostics>>) -> Result<PhaseOutcome, String> {
    let now_ms = wall_clock_ms();
    let report = runtime.poll(now_ms).map_err(|error| error.to_string())?;
    Ok(PhaseOutcome { report, diagnostics: diagnostics.borrow().clone(), now_ms })
}

fn raw_phase(name: &str, outcome: &PhaseOutcome) -> String {
    let mut raw = String::new();
    let d = &outcome.diagnostics;
    writeln!(raw, "phase={name}").unwrap();
    writeln!(raw, "now_ms={}", outcome.now_ms).unwrap();
    writeln!(raw, "http_status={:?}", d.last_http_status).unwrap();
    writeln!(raw, "adapter_error={}", error_class(&d.last_error)).unwrap();
    writeln!(raw, "observations={:?}", d.last_observations).unwrap();
    writeln!(raw, "reading={:?}", d.last_reading).unwrap();
    writeln!(raw, "accepted_observations={}", outcome.report.accepted_observations).unwrap();
    writeln!(raw, "disconnected_adapters={:?}", outcome.report.disconnected_adapters).unwrap();
    writeln!(raw, "projected_{ASSET}={:?}", outcome.report.projected_assets.get(ASSET)).unwrap();
    writeln!(raw, "health={:?}", outcome.report.health).unwrap();
    writeln!(raw, "--- body ---\n{}", d.last_body.clone().unwrap_or_default()).unwrap();
    raw
}

fn projected(outcome: &PhaseOutcome) -> Option<AssetStatus> {
    outcome.report.projected_assets.get(ASSET).copied()
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
    };
    match run(&args) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(3)
        }
    }
}

fn ack(path: &Path, line: &str) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("ack file: {error}"))?;
    writeln!(file, "{line}").map_err(|error| format!("ack file: {error}"))
}

fn run(args: &Args) -> Result<usize, String> {
    let raw_dir = args.out.join("raw");
    std::fs::create_dir_all(&raw_dir).map_err(|error| error.to_string())?;
    let model = base01();
    if !model.assets.contains_key(ASSET) {
        return Err(format!("Base-01 has no asset {ASSET}"));
    }
    let compatibility = format!("{} (AI rehearsal, local container)", args.image);
    let adapter = BasyxAdapter::new(&args.base_url, SUBMODEL_ID, ASSET, &compatibility)?;
    let path = adapter.path().to_string();
    let diagnostics = adapter.diagnostics();
    let log_path = raw_dir.join("adapter-log.jsonl");
    let _ = std::fs::remove_file(&log_path);
    let mut runtime =
        LocalRuntime::new("site-a", model, vec![Box::new(adapter)], EvidencePolicy::new(EVIDENCE_MAX_AGE_MS))
            .map_err(|error| error.to_string())?
            .with_log(&log_path);

    let mut checks: Vec<Check> = Vec::new();
    let mut ran: Vec<String> = Vec::new();
    let mut expected_observed_at: Option<(String, i64)> = None;
    let mut shadow_summary = String::new();

    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line.map_err(|error| error.to_string())?;
        let mut words = line.split_whitespace();
        let Some(command) = words.next() else { continue };
        let argument = words.next().map(str::to_string);
        if command == "finish" {
            break;
        }
        if let Some(text) = &argument {
            let ms = aas::parse_datetime_ms(text).map_err(|error| format!("{command}: {error}"))?;
            expected_observed_at = Some((text.clone(), ms));
        }
        let outcome = run_phase(&mut runtime, &diagnostics)?;
        std::fs::write(raw_dir.join(format!("phase-{command}.txt")), raw_phase(command, &outcome))
            .map_err(|error| error.to_string())?;
        match command {
            "read" => {
                checks.extend(read_checks(&outcome, expected_observed_at.as_ref()));
                // C4: read-only shadow run on the projected model.
                let model = runtime.projected_model(outcome.now_ms).map_err(|error| error.to_string())?;
                let goals = vec![PlanningGoal {
                    goal: Goal::Service("potable-water".into()),
                    deadline_s: Some(4.0 * 3600.0),
                    priority: 10,
                }];
                let shadow = shadow_plan(&model, 0.0, 86_400.0, goals, Verification::Demonstrated, &[]);
                let (actual, pass) = match &shadow {
                    Ok(shadow) => {
                        for action in &shadow.plan.actions {
                            writeln!(
                                shadow_summary,
                                "- {} → {} @ {:.0}s",
                                action.process, action.target, action.start_s
                            )
                            .unwrap();
                        }
                        if shadow.plan.actions.is_empty() {
                            shadow_summary.push_str(
                                "- （无计划动作）\n\n注：计划动作为 0 时，C4 的 `dispatched = false` 是平凡满足的；\
                                 本次只证明了「真实适配器观测 → 投影 → 影子规划」路径可运行，\
                                 没有证明非空计划在影子模式下不派发（该性质由 `experiments/shadow-run` 的模拟演示覆盖）。\n",
                            );
                        }
                        (
                            format!(
                                "投影 {ASSET} = {:?}；计划动作 {} 个；dispatched = {}",
                                model.assets[ASSET].status,
                                shadow.plan.actions.len(),
                                shadow.dispatched
                            ),
                            model.assets[ASSET].status == AssetStatus::Degraded && !shadow.dispatched,
                        )
                    }
                    Err(error) => (format!("shadow_plan 失败：{error:?}"), false),
                };
                checks.push(Check {
                    id: "C4",
                    item: "只读影子运行",
                    expected: "投影 pump-p1 = Degraded；dispatched = false",
                    actual,
                    pass,
                });
            }
            "disconnected" => {
                let d = &outcome.diagnostics;
                let disconnected = matches!(d.last_error, Some(AdapterError::Disconnected(_)));
                let status = projected(&outcome);
                let healthy = outcome.report.health.healthy;
                let accepted = outcome.report.accepted_observations;
                checks.push(Check {
                    id: "C5",
                    item: "容器停止后的断连",
                    expected: "Disconnected；pump-p1 = Unknown；healthy = false；接受观测 0",
                    actual: format!(
                        "错误 = {}；pump-p1 = {status:?}；healthy = {healthy}；接受观测 {accepted}",
                        error_class(&d.last_error)
                    ),
                    pass: disconnected && status == Some(AssetStatus::Unknown) && !healthy && accepted == 0,
                });
            }
            "restarted" => {
                let d = &outcome.diagnostics;
                let protocol = matches!(d.last_error, Some(AdapterError::Protocol(_)));
                let status = projected(&outcome);
                let healthy = outcome.report.health.healthy;
                checks.push(Check {
                    id: "C6",
                    item: "容器重启后（尚未重注入）",
                    expected: "HTTP 404；Protocol 错误；pump-p1 = Unknown；healthy = false",
                    actual: format!(
                        "HTTP {:?}；错误 = {}；pump-p1 = {status:?}；healthy = {healthy}",
                        d.last_http_status,
                        error_class(&d.last_error)
                    ),
                    pass: d.last_http_status == Some(404)
                        && protocol
                        && status == Some(AssetStatus::Unknown)
                        && !healthy,
                });
            }
            "reinjected" => {
                let d = &outcome.diagnostics;
                let status = projected(&outcome);
                let healthy = outcome.report.health.healthy;
                let observed = d.last_observations.first().map(|o| o.observed_at_ms);
                let expected_ms = expected_observed_at.as_ref().map(|(_, ms)| *ms);
                checks.push(Check {
                    id: "C7",
                    item: "重注入后重连",
                    expected: "轮询成功；pump-p1 = Degraded；healthy = true；observed_at = 新 ObservedAt",
                    actual: format!(
                        "错误 = {}；pump-p1 = {status:?}；healthy = {healthy}；observed_at_ms = {observed:?}（期望 {expected_ms:?}）",
                        error_class(&d.last_error)
                    ),
                    pass: d.last_error.is_none()
                        && status == Some(AssetStatus::Degraded)
                        && healthy
                        && observed.is_some()
                        && observed == expected_ms,
                });
            }
            other => return Err(format!("unknown phase command {other:?}")),
        }
        ran.push(command.to_string());
        ack(&args.ack, &format!("{command} done"))?;
    }

    // Phases the script never reached are reported as not run (FAIL), not skipped.
    for (phase, id, item) in [
        ("read", "C1–C4", "读取/单位/时间戳/影子运行"),
        ("disconnected", "C5", "容器停止后的断连"),
        ("restarted", "C6", "容器重启后（尚未重注入）"),
        ("reinjected", "C7", "重注入后重连"),
    ] {
        if !ran.iter().any(|done| done == phase) {
            checks.push(Check {
                id,
                item,
                expected: "见 PREREG",
                actual: format!("阶段 `{phase}` 未运行"),
                pass: false,
            });
        }
    }

    let failures = checks.iter().filter(|check| !check.pass).count();
    let report = render_report(args, &path, &checks, &shadow_summary, failures);
    std::fs::write(args.out.join("REPORT.md"), &report).map_err(|error| error.to_string())?;
    let mut summary = String::new();
    for check in &checks {
        writeln!(summary, "{}\t{}\t{}", check.id, if check.pass { "PASS" } else { "FAIL" }, check.actual).unwrap();
    }
    writeln!(summary, "failures={failures}").unwrap();
    std::fs::write(raw_dir.join("checks.tsv"), &summary).map_err(|error| error.to_string())?;
    print!("{summary}");
    Ok(failures)
}

fn read_checks(outcome: &PhaseOutcome, expected_observed_at: Option<&(String, i64)>) -> Vec<Check> {
    let d = &outcome.diagnostics;
    let reading = d.last_reading.as_ref();
    let status = reading.map(|r| r.status);
    let flow = reading.and_then(|r| r.units.iter().find(|u| u.id_short == "FlowRate"));
    let export = reading.and_then(|r| r.units.iter().find(|u| u.id_short == "FlowRateExport"));
    let mut checks = Vec::new();

    checks.push(Check {
        id: "C1",
        item: "读取 Submodel 属性值",
        expected: "HTTP 200；OperatingState → pump-p1 = Degraded；FlowRate 原值 12.5",
        actual: format!(
            "HTTP {:?}；错误 = {}；状态 = {status:?}；FlowRate 原值 = {:?}",
            d.last_http_status,
            error_class(&d.last_error),
            flow.map(|f| f.raw_value.clone())
        ),
        pass: d.last_error.is_none()
            && status == Some(AssetStatus::Degraded)
            && flow.and_then(|f| f.raw_value.parse::<f64>().ok()) == Some(12.5),
    });

    let expected_si = 12.5 / 3600.0;
    let flow_si = flow.and_then(|f| f.si.clone().ok());
    checks.push(Check {
        id: "C2a",
        item: "单位映射（已知单位）",
        expected: "unit = m3/h；SI = 0.003472222… m3/s（相对误差 < 1e-9）",
        actual: format!("unit = {:?}；SI = {flow_si:?}", flow.and_then(|f| f.unit.clone())),
        pass: flow.and_then(|f| f.unit.as_deref()) == Some("m3/h")
            && flow_si.map(|si| ((si - expected_si) / expected_si).abs() < 1e-9) == Some(true),
    });

    checks.push(Check {
        id: "C2b",
        item: "单位映射（未知单位）",
        expected: "bbl/d 被拒绝：无 SI 值、有原因、轮询不失败",
        actual: format!(
            "unit = {:?}；结果 = {:?}；轮询错误 = {}",
            export.and_then(|f| f.unit.clone()),
            export.map(|f| f.si.clone()),
            error_class(&d.last_error)
        ),
        pass: export.and_then(|f| f.unit.as_deref()) == Some("bbl/d")
            && matches!(export.map(|f| &f.si), Some(Err(reason)) if !reason.is_empty())
            && d.last_error.is_none(),
    });

    let observation = d.last_observations.first();
    let expected_ms = expected_observed_at.map(|(_, ms)| *ms);
    checks.push(Check {
        id: "C3a",
        item: "时间戳语义（事件时间）",
        expected: "observed_at_ms = 注入的 ObservedAt；ingested_at_ms ≥ observed_at_ms",
        actual: format!(
            "ObservedAt = {:?}；observed_at_ms = {:?}（期望 {expected_ms:?}）；ingested_at_ms = {:?}；quality = {:?}",
            reading.and_then(|r| r.observed_at_text.clone()),
            observation.map(|o| o.observed_at_ms),
            observation.map(|o| o.ingested_at_ms),
            observation.map(|o| o.quality)
        ),
        pass: observation
            .is_some_and(|o| Some(o.observed_at_ms) == expected_ms && o.ingested_at_ms >= o.observed_at_ms),
    });

    let suspicious = ["time", "stamp", "updated", "modified", "date", "created"];
    let timestamp_keys: Vec<String> = reading
        .map(|r| {
            r.property_keys
                .iter()
                .flat_map(|(id, keys)| keys.iter().map(move |key| (id, key)))
                .filter(|(_, key)| suspicious.iter().any(|s| key.to_ascii_lowercase().contains(s)))
                .map(|(id, key)| format!("{id}.{key}"))
                .collect()
        })
        .unwrap_or_default();
    let all_keys: std::collections::BTreeSet<String> = reading
        .map(|r| r.property_keys.iter().flat_map(|(_, keys)| keys.iter().cloned()).collect())
        .unwrap_or_default();
    checks.push(Check {
        id: "C3b",
        item: "时间戳语义（BaSyx 原生）",
        expected: "Property JSON 无服务端时间戳字段",
        actual: format!("Property 键集合 = {all_keys:?}；时间戳类键 = {timestamp_keys:?}"),
        pass: reading.is_some() && !all_keys.is_empty() && timestamp_keys.is_empty(),
    });
    checks
}

fn render_report(args: &Args, path: &str, checks: &[Check], shadow_summary: &str, failures: usize) -> String {
    let mut report = String::new();
    writeln!(report, "# BaSyx AAS 只读适配器兼容性预演报告（M9.2）\n").unwrap();
    writeln!(report, "> **AI 预演 · 本机容器 · 非现场设备** —— 不计入 M9.2 退出条件。\n").unwrap();
    writeln!(report, "- 预登记：[PREREG.md](PREREG.md)").unwrap();
    writeln!(report, "- 镜像：`{}`", args.image).unwrap();
    writeln!(report, "- 端点：`{}`；读取路径：`GET {path}`", args.base_url).unwrap();
    writeln!(report, "- 适配器：`basyx-aas-readonly`（std `TcpStream` 手写 HTTP/1.1 GET；只读；`MarkUnknown`）")
        .unwrap();
    writeln!(report, "- 证据过期窗口：{EVIDENCE_MAX_AGE_MS} ms").unwrap();
    writeln!(report, "- 原始输出：`raw/`（每阶段 `phase-*.txt`、`adapter-log.jsonl`、`checks.tsv`、`run.log`）\n")
        .unwrap();
    writeln!(report, "## 兼容性项\n").unwrap();
    writeln!(report, "| 编号 | 兼容性项 | 期望 | 实际 | 结果 |").unwrap();
    writeln!(report, "| --- | --- | --- | --- | --- |").unwrap();
    for check in checks {
        writeln!(
            report,
            "| {} | {} | {} | {} | {} |",
            check.id,
            check.item,
            check.expected,
            check.actual.replace('|', "\\|").replace('\n', " "),
            if check.pass { "PASS" } else { "**FAIL**" }
        )
        .unwrap();
    }
    writeln!(report, "\n失败数：{failures} / {}\n", checks.len()).unwrap();
    if !shadow_summary.is_empty() {
        writeln!(report, "## 影子计划（仅记录，未派发）\n").unwrap();
        report.push_str(shadow_summary);
        report.push('\n');
    }
    writeln!(
        report,
        "## 结论\n\n{}\n",
        if failures == 0 {
            "C1–C7 全部 PASS：本机预演支持该只读适配器与此 BaSyx 版本的兼容性。这仍只是 AI 在本机单容器上的预演，M9.2 需要现场/测试实例与兼容性 ADR。"
        } else {
            "存在 FAIL：预演不通过；失败项如实保留。逐项判断是实现、模型还是预期错，并把理由写入 PREREG.md §7 变更记录；不得只改期望使结果变绿。"
        }
    )
    .unwrap();
    writeln!(report, "## 局限与注记\n").unwrap();
    for note in [
        "单容器、本机回环网络、无认证、镜像默认（InMemory）后端；不覆盖 TLS、权限、MongoDB 持久化后端、订阅/事件（MQTT）与规模。",
        "数据为 AI 编写的合成值，不是现场设备读数；`ObservedAt` 由注入脚本写入，不是设备时钟。",
        "BaSyx 的 AAS v3 Property 不带服务端时间戳（C3b）：只有显式提供 `ObservedAt` 一类属性的 Submodel 才能声明 `EventTime`；否则适配器把观测降为 `uncertain` 并以接入时间计。",
        "C6 表明重启后数据丢失（InMemory）：HTTP 可达不等于数据可用；适配器把 404 视为错误而不是「无变化」，因此不会伪造状态。",
        "断连与重连之间的间隔短于证据过期窗口，证据过期路径未在本次真实运行中被触发（由 `poie-runtime` 单元测试覆盖）。",
        "仅测 BaSyx；Eclipse Ditto 与 ROS 2 未测。M9.2 退出条件仍需现场/测试实例与兼容性 ADR。",
    ] {
        writeln!(report, "- {note}").unwrap();
    }
    report
}
