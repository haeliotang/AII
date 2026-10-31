//! Fuzz-lite robustness for expansion proposals: arbitrary mutations of the
//! proposal TOML must be accepted or rejected, never panic, and a successfully
//! loaded proposal must expand without panicking.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use poie_expansion::proposal::ExpansionProposal;
use poie_ontology::model::Model;

static COUNTER: AtomicU64 = AtomicU64::new(0);
const SEED: u64 = 0x706f_6965_6578_7072; // "poieexpr"

#[derive(Clone, Debug)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> SplitMix64 {
        SplitMix64 { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn index(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }
}

fn workspace(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(path)
}

fn temp_path() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "poie-proposal-{}-{}-{}.toml",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst),
        nanos
    ))
}

fn mutate(text: &str, rng: &mut SplitMix64) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    if lines.is_empty() {
        return String::new();
    }
    match rng.index(5) {
        0 => {
            lines.remove(rng.index(lines.len()));
        }
        1 => {
            let index = rng.index(lines.len());
            lines.insert(index, lines[index].clone());
        }
        2 => {
            let index = rng.index(lines.len());
            lines[index].clear();
        }
        3 => {
            let index = rng.index(lines.len());
            let line = &mut lines[index];
            if !line.is_empty() {
                let char_index = rng.index(line.len());
                line.replace_range(char_index..char_index + 1, "\u{0}");
            }
        }
        _ => {
            let index = rng.index(lines.len());
            lines.truncate(index.max(1));
        }
    }
    let mut out = lines.join("\n");
    if rng.index(3) == 0 {
        out.push_str("\n[incomplete");
    }
    out
}

#[test]
fn mutated_proposals_never_panic() {
    let original = fs::read_to_string(workspace("scenarios/base-01-expansion/proposal.toml")).expect("proposal text");
    let base = Model::load(&workspace("scenarios/base-01/model.toml")).expect("base model");
    ExpansionProposal::load(&workspace("scenarios/base-01-expansion/proposal.toml")).expect("original proposal loads");
    let mut rng = SplitMix64::new(SEED);
    for _ in 0..120 {
        let mutated = mutate(&original, &mut rng);
        let path = temp_path();
        fs::write(&path, mutated.as_bytes()).expect("write mutated proposal");
        // No panic on untrusted text; a loaded proposal must also expand safely.
        if let Ok(proposal) = ExpansionProposal::load(&path) {
            let _ = proposal.expand(&base);
        }
        let _ = fs::remove_file(&path);
    }
}

#[test]
fn empty_and_garbage_proposals_are_errors() {
    let empty = temp_path();
    fs::write(&empty, b"").unwrap();
    assert!(ExpansionProposal::load(&empty).is_err());
    let garbage = temp_path();
    fs::write(&garbage, b"\x00\x01not toml").unwrap();
    assert!(ExpansionProposal::load(&garbage).is_err());
    let missing = temp_path();
    let _ = fs::remove_file(&missing);
    assert!(ExpansionProposal::load(&missing).is_err());
    let _ = fs::remove_file(&empty);
    let _ = fs::remove_file(&garbage);
}
