//! H1 structural experiment harness: compares the capability-graph solver with
//! the per-device baseline on the pre-registered Base-01 cases.
//!
//! Usage: poie-h1-baseline <cases.toml> [CASE_ID ...]
//!
//! Prints one line per case (truth, Poieweave verdict, baseline verdict,
//! classification, root-cause matching, tags) and a summary overall and per
//! disturbance stratum. See experiments/h1-structural/PREREG.md.

use std::collections::{BTreeMap, BTreeSet};
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
        Classification::Agree
    }
}

fn root_match(expected: &[String], actual: &BTreeSet<String>) -> (usize, usize) {
    let matched = expected.iter().filter(|subject| actual.contains(subject.as_str())).count();
    (matched, expected.len())
}

#[derive(Clone, Default)]
struct Stats {
    cases: usize,
    agree: usize,
    missed: usize,
    false_block: usize,
    non_candidate: usize,
    candidate: usize,
    baseline_root_matched: usize,
    poie_root_matched: usize,
    root_expected: usize,
}

impl Stats {
    fn add(
        &mut self,
        truth: Verdict,
        class: Classification,
        baseline_roots: (usize, usize),
        poie_roots: (usize, usize),
    ) {
        self.cases += 1;
        match class {
            Classification::Agree => self.agree += 1,
            Classification::MissedBlocking => self.missed += 1,
            Classification::FalseBlock => self.false_block += 1,
        }
        if matches!(truth, Verdict::Blocked | Verdict::Unknown) {
            self.non_candidate += 1;
        } else {
            self.candidate += 1;
        }
        self.baseline_root_matched += baseline_roots.0;
        self.poie_root_matched += poie_roots.0;
        self.root_expected += baseline_roots.1;
    }

    fn line(&self, label: &str) -> String {
        let miss_rate = if self.non_candidate == 0 { 0.0 } else { self.missed as f64 / self.non_candidate as f64 };
        let false_rate = if self.candidate == 0 { 0.0 } else { self.false_block as f64 / self.candidate as f64 };
        let base_root = if self.root_expected == 0 {
            "-".to_string()
        } else {
            format!("{}/{}", self.baseline_root_matched, self.root_expected)
        };
        let poie_root = if self.root_expected == 0 {
            "-".to_string()
        } else {
            format!("{}/{}", self.poie_root_matched, self.root_expected)
        };
        format!(
            "{label:<22} n={cases:<3} agree={agree:<3} missed={missed:<3}/{non_candidate:<3} ({miss_rate:.3}) false={false_block:<3}/{candidate:<3} ({false_rate:.3}) roots base={base_root} poie={poie_root}",
            label = label,
            cases = self.cases,
            agree = self.agree,
            missed = self.missed,
            non_candidate = self.non_candidate,
            false_block = self.false_block,
            candidate = self.candidate,
            base_root = base_root,
            poie_root = poie_root,
        )
    }
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

    println!(
        "case\ttruth\tpoieweave\tbaseline\tclass\ttags\texpected_roots\tbaseline_roots\tpoie_roots\tbaseline_root_match\tpoie_root_match"
    );
    let mut overall = Stats::default();
    let mut by_tag: BTreeMap<String, Stats> = BTreeMap::new();
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

        let expected_roots: Vec<String> = case.expect.root_causes.iter().map(|b| b.subject.clone()).collect();
        let baseline_roots: BTreeSet<String> = base.blockers.iter().map(|b| b.subject.clone()).collect();
        let poie_roots: BTreeSet<String> = run.result.root_causes().map(|b| b.subject.clone()).collect();
        let baseline_match = root_match(&expected_roots, &baseline_roots);
        let poie_match = root_match(&expected_roots, &poie_roots);

        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}/{}\t{}/{}",
            case.id,
            truth.as_str(),
            poie.as_str(),
            base.verdict.as_str(),
            class.as_str(),
            case.tags.join(","),
            expected_roots.join(","),
            sorted(&baseline_roots),
            sorted(&poie_roots),
            baseline_match.0,
            baseline_match.1,
            poie_match.0,
            poie_match.1
        );

        overall.add(truth, class, baseline_match, poie_match);
        for tag in &case.tags {
            by_tag.entry(tag.clone()).or_default().add(truth, class, baseline_match, poie_match);
        }
    }

    if ran == 0 {
        eprintln!("no case matched {selected:?}");
        return ExitCode::from(2);
    }
    println!();
    println!("{}", overall.line("overall"));
    for (tag, stats) in &by_tag {
        println!("{}", stats.line(tag));
    }
    ExitCode::SUCCESS
}

fn sorted(set: &BTreeSet<String>) -> String {
    set.iter().cloned().collect::<Vec<_>>().join(",")
}
