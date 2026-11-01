//! Runs the pre-registered timed cases of a scenario and checks replay digests.
//!
//! Usage: `poie-timed <cases.toml> [--write-digests <path>] [--check-digests <path>]`
//!
//! Exits with status 1 if any case does not match its expectation or a digest
//! differs from the committed replay file (M5.5, M5.6).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use poie_sim::timed::{read_digests, write_digests, DigestEntry, TimedCases, TimedRunner, TimedScenario};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(cases_path) = args.next() else {
        eprintln!("usage: poie-timed <cases.toml> [--write-digests <path>] [--check-digests <path>]");
        return ExitCode::from(2);
    };
    let mut write_path: Option<String> = None;
    let mut check_path: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--write-digests" => {
                let Some(path) = args.next() else {
                    eprintln!("--write-digests needs a path");
                    return ExitCode::from(2);
                };
                write_path = Some(path);
            }
            "--check-digests" => {
                let Some(path) = args.next() else {
                    eprintln!("--check-digests needs a path");
                    return ExitCode::from(2);
                };
                check_path = Some(path);
            }
            other => {
                eprintln!("unknown argument {other:?}");
                return ExitCode::from(2);
            }
        }
    }

    let (cases, scenario_path) = match TimedCases::load(&PathBuf::from(&cases_path)) {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
    };
    let scenario = match TimedScenario::load(&scenario_path) {
        Ok(scenario) => scenario,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
    };
    let runner = TimedRunner::new(scenario);

    let mut failures = 0usize;
    let mut digests = Vec::new();
    for case in &cases.cases {
        let (outcome, mismatches) = match runner.check(case) {
            Ok(result) => result,
            Err(error) => {
                eprintln!("error: {error}");
                return ExitCode::from(2);
            }
        };
        let status = if mismatches.is_empty() { "PASS" } else { "FAIL" };
        let completion =
            outcome.completion_s.map(|completion| format!(", completion {:.0}s", completion)).unwrap_or_default();
        println!(
            "[{status}] {} — {} (min_verification = {}) -> {}{}",
            case.id,
            case.title,
            case.min_verification,
            outcome.verdict.as_str(),
            completion
        );
        if !outcome.detail.is_empty() {
            println!("  detail: {}", outcome.detail);
        }
        for warning in &outcome.warnings {
            println!("  warning: {warning}");
        }
        for mismatch in &mismatches {
            println!("  !! {mismatch}");
        }
        if !mismatches.is_empty() {
            failures += 1;
        }
        digests.push(DigestEntry { id: case.id.clone(), digest: outcome.digest() });
    }

    if let Some(path) = write_path {
        if let Err(error) = write_digests(Path::new(&path), digests.clone()) {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
        println!("wrote {} replay digests to {path}", digests.len());
    }
    if let Some(path) = check_path {
        let expected = match read_digests(Path::new(&path)) {
            Ok(expected) => expected,
            Err(error) => {
                eprintln!("error: {error}");
                return ExitCode::from(2);
            }
        };
        let mut digest_failures = 0usize;
        for entry in &expected {
            match digests.iter().find(|computed| computed.id == entry.id) {
                Some(computed) if computed.digest == entry.digest => {}
                Some(computed) => {
                    digest_failures += 1;
                    println!(
                        "  !! replay digest mismatch for {}: expected {:?}, got {:?}",
                        entry.id, entry.digest, computed.digest
                    );
                }
                None => {
                    digest_failures += 1;
                    println!("  !! replay digest file names unknown case {}", entry.id);
                }
            }
        }
        if digest_failures > 0 {
            println!("{} replay digest mismatch(es)", digest_failures);
            failures += digest_failures;
        } else {
            println!("replay digests match for {} case(s)", expected.len());
        }
    }

    println!(
        "{} case(s), {} passed, {} failed",
        cases.cases.len(),
        cases.cases.len() - failures.min(cases.cases.len()),
        failures
    );
    if failures > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
