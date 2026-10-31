//! H1 structural experiment harness: compares the capability-graph solver with
//! the per-device baseline on the pre-registered Base-01 cases.
//!
//! Usage: poie-h1-baseline <cases.toml> [CASE_ID ...]
//!
//! This prints one line per case (truth, Poieweave verdict, baseline verdict,
//! classification, root-cause match) and a summary. It does not change any
//! case or contract; see experiments/h1-structural/PREREG.md for the protocol.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;

use poie_h1_baseline::baseline;
use poie_ontology::graph::Verdict;
use poie_ontology::scenario::Scenario;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Classification {
    Agree,
    MissedBlocking,
    FalseBlock,
}

impl Classification {
    fn as_str(self) -> &'static str {
        match self {
            Classification::Agree => "agree",
            Classification::MissedBlocking => "missed_blocking",
            Classification::FalseBlock => "false_block",
        }
    }
}

fn classify(truth: Verdict, baseline: Verdict) -> Classification {
    let truth_blocked = matches!(truth, Verdict::Blocked | Verdict::Unknown);
    let baseline_blocked = matches!(baseline, Verdict::Blocked | Verdict::Unknown);
    if truth == baseline {
        Classification::Agree
    } else if truth_blocked && !baseline_blocked {
        Classification::MissedBlocking
    } else if !truth_blocked && baseline_blocked {
        Classification::FalseBlock
    } else {
        // search_limit vs blocked/unknown differences do not occur in the baseline.
        Classification::Agree
    }
}

fn root_match(expected: &[String], actual: &BTreeSet<String>) -> (usize, usize) {
    let matched = expected.iter().filter(|subject| actual.contains(subject.as_str())).count();
    (matched, expected.len())
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: poie-h1-baseline <cases.toml> [CASE_ID ...]");
        return ExitCode::from(2);
    };
    let selected: Vec<String> = args.collect();
    let scenario = match Scenario::load(&PathBuf::from(&path)) {
        Ok(scenario) => scenario,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    println!("case\ttruth\tpoieweave\tbaseline\tclass\texpected_roots\tmatched_roots\troot_rate");
    let mut missed = 0usize;
    let mut false_block = 0usize;
    let mut agree = 0usize;
    let mut truth_blocked = 0usize;
    let mut truth_candidate = 0usize;
    let mut root_expected = 0usize;
    let mut root_matched = 0usize;
    let mut ran = 0usize;

    for case in &scenario.cases {
        if !selected.is_empty() && !selected.contains(&case.id) {
            continue;
        }
        ran += 1;
        let run = match scenario.run(case) {
            Ok(run) => run,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        };
        let (model, query) = match scenario.prepared(case) {
            Ok(prepared) => prepared,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        };
        let truth = case.expect.verdict;
        let poie = run.result.verdict;
        let base = baseline::check(&model, &query);
        let class = classify(truth, base.verdict);

        if matches!(truth, Verdict::Blocked | Verdict::Unknown) {
            truth_blocked += 1;
        } else {
            truth_candidate += 1;
        }
        match class {
            Classification::Agree => agree += 1,
            Classification::MissedBlocking => missed += 1,
            Classification::FalseBlock => false_block += 1,
        }

        let expected_roots: Vec<String> = case.expect.root_causes.iter().map(|b| b.subject.clone()).collect();
        let baseline_roots: BTreeSet<String> = base.blockers.iter().map(|b| b.subject.clone()).collect();
        let (matched, total) = root_match(&expected_roots, &baseline_roots);
        root_expected += total;
        root_matched += matched;
        let rate = if total == 0 { String::from("-") } else { format!("{matched}/{total}") };

        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}/{}\t{}",
            case.id,
            truth.as_str(),
            poie.as_str(),
            base.verdict.as_str(),
            class.as_str(),
            expected_roots.join(","),
            matched,
            total,
            rate
        );
    }

    if ran == 0 {
        eprintln!("no case matched {selected:?}");
        return ExitCode::from(2);
    }

    let non_candidate = truth_blocked;
    let miss_rate = if non_candidate == 0 { 0.0 } else { missed as f64 / non_candidate as f64 };
    let false_rate = if truth_candidate == 0 { 0.0 } else { false_block as f64 / truth_candidate as f64 };
    let root_rate = if root_expected == 0 { 0.0 } else { root_matched as f64 / root_expected as f64 };
    println!();
    println!("cases: {ran}");
    println!("agreement: {agree}/{ran}");
    println!("missed_blocking: {missed}/{non_candidate} (rate {miss_rate:.3})");
    println!("false_block: {false_block}/{truth_candidate} (rate {false_rate:.3})");
    println!("baseline root-subject match: {root_matched}/{root_expected} (rate {root_rate:.3})");
    ExitCode::SUCCESS
}
