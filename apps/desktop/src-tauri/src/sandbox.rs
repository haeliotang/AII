//! Path resolution for the app-owned workspace sandbox.
//!
//! The host, not the webview, decides where files may live. Every path coming
//! from the frontend is relative, rejects parent/root/prefix components, and is
//! resolved under a single app-data root. Reads and writes are size-limited.

use std::path::{Component, Path, PathBuf};

pub const MAX_FILE_BYTES: u64 = 1_048_576;

pub fn resolve(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative = relative.trim();
    if relative.is_empty() {
        return Err("empty sandbox path".into());
    }
    let mut output = root.to_path_buf();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => output.push(part),
            _ => return Err(format!("sandbox path must be relative and stay inside the root: {relative:?}")),
        }
    }
    Ok(output)
}

pub fn read(root: &Path, relative: &str) -> Result<String, String> {
    let path = resolve(root, relative)?;
    let metadata = std::fs::metadata(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(format!("{} is larger than the sandbox limit", path.display()));
    }
    std::fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn write(root: &Path, relative: &str, contents: &str) -> Result<String, String> {
    if contents.len() as u64 > MAX_FILE_BYTES {
        return Err("contents exceed the sandbox limit".into());
    }
    let path = resolve(root, relative)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    std::fs::write(&path, contents).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_resolve_under_the_root() {
        let root = Path::new("/tmp/poieweave-sandbox");
        assert_eq!(resolve(root, "notes/a.md").unwrap(), root.join("notes/a.md"));
        assert_eq!(resolve(root, "file.txt").unwrap(), root.join("file.txt"));
    }

    #[test]
    fn traversal_and_absolute_paths_are_rejected() {
        let root = Path::new("/tmp/poieweave-sandbox");
        for bad in ["", " ", "../secret", "notes/../../secret", "/etc/passwd", "./notes", "notes/../x"] {
            assert!(resolve(root, bad).is_err(), "{bad:?} should be rejected");
        }
    }
}
