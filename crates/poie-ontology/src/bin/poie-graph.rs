//! Runs the cases of a scenario and reports each capability-graph result.
//!
//! Usage: poie-graph <cases.toml> [--routes N] [--external-deps N] [--timings] [CASE_ID ...]
//! Exits with status 1 if any selected case does not match its expectation.

use std::path::PathBuf;
use std::process::ExitCode;

use poie_ontology::graph;
use poie_ontology::model::MassBalance;
use poie_ontology::scenario::Scenario;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: poie-graph <cases.toml> [--routes N] [CASE_ID ...]");
        return ExitCode::from(2);
    };
    let mut selected: Vec<String> = Vec::new();
    let mut routes_limit: Option<usize> = None;
    let mut external_limit: Option<usize> = None;
    let mut timings = false;
    while let Some(arg) = args.next() {
        if arg == "--timings" {
            timings = true;
        } else if arg == "--routes" || arg == "--external-deps" {
            match args.next().and_then(|value| value.parse().ok()) {
                Some(n) if arg == "--routes" => routes_limit = Some(n),
                Some(n) => external_limit = Some(n),
                None => {
                    eprintln!("{arg} needs a number");
                    return ExitCode::from(2);
                }
            }
        } else {
            selected.push(arg);
        }
    }
    let scenario = match Scenario::load(&PathBuf::from(&path)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    let m = &scenario.model;
    println!("model: {}", m.name);
    println!(
        "  {} resources, {} assets, {} lots, {} processes, {} services",
        m.resources.len(),
        m.assets.len(),
        m.lots.len(),
        m.processes.len(),
        m.services.len()
    );
    let balanced = m.mass_balance.values().filter(|b| matches!(b, MassBalance::Balanced { .. })).count();
    let unchecked: Vec<&str> = m
        .mass_balance
        .iter()
        .filter(|(_, b)| matches!(b, MassBalance::Unchecked { .. }))
        .map(|(p, _)| p.as_str())
        .collect();
    println!("  mass balance: {balanced} balanced, {} unchecked {:?}", unchecked.len(), unchecked);
    println!("  external dependencies (no local producer): {}", m.external_resources().join(", "));
    // PO §3.1 (issue #111): statuses without `status_evidence` are synthetic
    // observations by the model author and must be reported as such.
    println!("  declared statuses (synthetic, no status_evidence): {}", m.declared_status_assets().join(", "));
    println!();

    let mut failed = 0;
    let mut ran = 0;
    for case in &scenario.cases {
        if !selected.is_empty() && !selected.contains(&case.id) {
            continue;
        }
        ran += 1;
        let started = std::time::Instant::now();
        let run = match scenario.run(case) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        };
        let status = if run.mismatches.is_empty() { "PASS" } else { "FAIL" };
        println!("[{status}] {} — {} (min_verification = {})", case.id, case.title, case.min_verification);
        if timings {
            println!("  solve time: {:.3} ms", started.elapsed().as_secs_f64() * 1_000.0);
        }
        print!("{}", graph::render(&run.model, &run.result));
        if let Some(limit) = routes_limit {
            if run.result.verdict == graph::Verdict::Candidate {
                match scenario.enumerate(case, limit) {
                    Ok(routes) => {
                        println!("  enumerated routes ({}):", routes.len());
                        print!("{}", graph::render_routes(&run.model, &routes));
                    }
                    Err(e) => {
                        eprintln!("error: {e}");
                        return ExitCode::from(2);
                    }
                }
            }
        }
        if let Some(limit) = external_limit {
            match scenario.external_dependency_report(case, limit) {
                Ok(report) => print!("{}", graph::render_external_dependencies(&run.model, &report)),
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::from(2);
                }
            }
        }
        for msg in &run.mismatches {
            println!("  !! {msg}");
        }
        if !run.mismatches.is_empty() {
            failed += 1;
        }
        println!();
    }
    if ran == 0 {
        eprintln!("no case matched {selected:?}");
        return ExitCode::from(2);
    }
    println!("{} case(s), {} passed, {} failed", ran, ran - failed, failed);
    if failed > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
