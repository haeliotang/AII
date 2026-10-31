//! H1 temporal re-test: compares the time-blind per-device baseline with the
//! timed capability-graph evaluator on the pre-registered timed Base-01 cases.
//!
//! Usage: poie-h1-temporal <cases.toml>
//!
//! Prints one line per case plus summary counters. See ../PREREG.md.

use std::path::PathBuf;
use std::process::ExitCode;

use poie_h1_baseline::baseline;
use poie_ontology::graph::Verdict;
use poie_sim::timed::{TimedCases, TimedRunner, TimedScenario, TimedVerdict};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Classification {
    Agree,
    TemporalMiss,
    JointMiss,
    FalseBlock,
}

impl Classification {
    fn as_str(self) -> &'static str {
        match self {
            Classification::Agree => "agree",
            Classification::TemporalMiss => "temporal_miss",
            Classification::JointMiss => "joint_miss",
            Classification::FalseBlock => "false_block",
        }
    }
}

fn baseline_as_timed(verdict: Verdict) -> TimedVerdict {
    match verdict {
        Verdict::Candidate => TimedVerdict::Feasible,
        Verdict::Blocked => TimedVerdict::Blocked,
        Verdict::Unknown => TimedVerdict::Unknown,
        Verdict::SearchLimit => TimedVerdict::SearchLimit,
    }
}

fn is_temporal(verdict: TimedVerdict) -> bool {
    matches!(verdict, TimedVerdict::TooLate | TimedVerdict::InsufficientResource | TimedVerdict::CapacityConflict)
}

fn classify(truth: TimedVerdict, baseline: TimedVerdict) -> Classification {
    let truth_feasible = truth == TimedVerdict::Feasible;
    let baseline_feasible = baseline == TimedVerdict::Feasible;
    if truth == baseline {
        Classification::Agree
    } else if !truth_feasible && baseline_feasible && is_temporal(truth) {
        Classification::TemporalMiss
    } else if !truth_feasible && baseline_feasible {
        Classification::JointMiss
    } else if truth_feasible && !baseline_feasible {
        Classification::FalseBlock
    } else {
        Classification::Agree
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(cases_path) = args.next() else {
        eprintln!("usage: poie-h1-temporal <cases.toml>");
        return ExitCode::from(2);
    };
    let cases_path = PathBuf::from(cases_path);
    let (cases, scenario_path) = match TimedCases::load(&cases_path) {
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

    println!("case\ttruth\tpoieweave\tbaseline\tclass\ttags");
    let mut agree = 0usize;
    let mut temporal_miss = 0usize;
    let mut joint_miss = 0usize;
    let mut false_block = 0usize;
    let mut poie_mismatch = 0usize;
    let mut temporal_truths = 0usize;
    let mut joint_truths = 0usize;
    let mut feasible_truths = 0usize;

    for case in &cases.cases {
        let truth = case.expect.verdict;
        let outcome = match runner.run(case) {
            Ok(outcome) => outcome,
            Err(error) => {
                eprintln!("error: {error}");
                return ExitCode::from(2);
            }
        };
        let (model, query) = match runner.prepared_at(case) {
            Ok(prepared) => prepared,
            Err(error) => {
                eprintln!("error: {error}");
                return ExitCode::from(2);
            }
        };
        let baseline = baseline_as_timed(baseline::check(&model, &query).verdict);
        let class = classify(truth, baseline);
        match class {
            Classification::Agree => agree += 1,
            Classification::TemporalMiss => temporal_miss += 1,
            Classification::JointMiss => joint_miss += 1,
            Classification::FalseBlock => false_block += 1,
        }
        if is_temporal(truth) {
            temporal_truths += 1;
        }
        if matches!(truth, TimedVerdict::Blocked | TimedVerdict::Unknown) {
            joint_truths += 1;
        }
        if truth == TimedVerdict::Feasible {
            feasible_truths += 1;
        }
        if outcome.verdict != truth {
            poie_mismatch += 1;
        }
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            case.id,
            truth.as_str(),
            outcome.verdict.as_str(),
            baseline.as_str(),
            class.as_str(),
            case.description.lines().next().unwrap_or("")
        );
    }

    let total = cases.cases.len();
    println!();
    println!("cases: {total}");
    println!("agreement: {agree}/{total}");
    println!("temporal_miss: {temporal_miss}/{temporal_truths}");
    println!("joint_miss: {joint_miss}/{joint_truths}");
    println!("false_block: {false_block}/{feasible_truths}");
    println!("poieweave_vs_truth_mismatch: {poie_mismatch}/{total}");
    ExitCode::SUCCESS
}
