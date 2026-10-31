//! Parser robustness (fuzz-lite): arbitrary mutations of the Base-01 model and
//! cases TOML must produce `Ok` or `Err`, never a panic, and the unmodified
//! originals must still load.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use poie_ontology::model::Model;
use poie_ontology::scenario::Scenario;

static COUNTER: AtomicU64 = AtomicU64::new(0);
const SEED: u64 = 0x706f_6965_7061_7273; // "poiepars"

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

fn temp_path(label: &str) -> PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("poie-parse-{label}-{}-{id}-{nanos}.toml", std::process::id()))
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
            let clone = lines[index].clone();
            lines.insert(index, clone);
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
        out.push_str("\n[broken");
    }
    out
}

#[test]
fn mutated_models_never_panic_the_parser() {
    let original = fs::read_to_string(workspace("scenarios/base-01/model.toml")).expect("model text");
    Model::load(&workspace("scenarios/base-01/model.toml")).expect("original model loads");
    let mut rng = SplitMix64::new(SEED);
    for trial in 0..120 {
        let mutated = mutate(&original, &mut rng);
        let path = temp_path("model");
        fs::write(&path, mutated.as_bytes()).expect("write mutated model");
        // The assertion is simply that this call returns (Ok or Err) without
        // panicking; the parser must never treat untrusted text as a panic path.
        let _ = Model::load(&path);
        let _ = fs::remove_file(&path);
        if trial % 20 == 0 {
            assert!(mutated.len() <= original.len() + 16);
        }
    }
    // Empty input and pure garbage are errors, not panics.
    let empty = temp_path("empty");
    fs::write(&empty, b"").unwrap();
    assert!(Model::load(&empty).is_err());
    let garbage = temp_path("garbage");
    fs::write(&garbage, b"\x00\x01\x02 not toml").unwrap();
    assert!(Model::load(&garbage).is_err());
    let _ = fs::remove_file(&empty);
    let _ = fs::remove_file(&garbage);
}

#[test]
fn mutated_cases_never_panic_the_parser() {
    let model_path = workspace("scenarios/base-01/model.toml");
    let original = fs::read_to_string(workspace("scenarios/base-01/cases.toml")).expect("cases text");
    Scenario::load(&workspace("scenarios/base-01/cases.toml")).expect("original cases load");
    let mut rng = SplitMix64::new(SEED ^ 0xcafe);
    for _ in 0..120 {
        let mutated = mutate(&original, &mut rng);
        let directory = std::env::temp_dir().join(format!(
            "poie-cases-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&directory).unwrap();
        fs::copy(&model_path, directory.join("model.toml")).unwrap();
        let cases_path = directory.join("cases.toml");
        fs::write(&cases_path, mutated.as_bytes()).unwrap();
        let _ = Scenario::load(&cases_path);
        let _ = fs::remove_dir_all(&directory);
    }
}
