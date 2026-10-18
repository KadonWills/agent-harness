//! Past `harness run` manifests, kept under `.harness/history/` (RFC-0002 §4).

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use harness_core::manifest::AgentManifest;

use crate::init::STATE_DIR;

/// How many manifests are kept; older ones are deleted on each record.
pub const KEEP: usize = 50;

pub fn dir(root: &Path) -> PathBuf {
    root.join(STATE_DIR).join("history")
}

/// One recorded run.
#[derive(Debug, Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub manifest: AgentManifest,
}

/// Write `manifest` to the history and prune it to [`KEEP`] entries.
pub fn record(root: &Path, manifest: &AgentManifest) -> Result<PathBuf> {
    let dir = dir(root);
    std::fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let path = dir.join(format!("{}.json", checked_id(&manifest.session_id)?));
    std::fs::write(&path, manifest.to_json()? + "\n")
        .with_context(|| format!("cannot write {}", path.display()))?;
    let names = names(&dir)?;
    for old in names.iter().take(names.len().saturating_sub(KEEP)) {
        // Best effort: a file we cannot delete only costs disk space.
        let _ = std::fs::remove_file(dir.join(old));
    }
    Ok(path)
}

/// Recorded runs, newest first. Files that do not parse are skipped.
pub fn list(root: &Path) -> Result<Vec<Entry>> {
    let dir = dir(root);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for name in names(&dir)?.into_iter().rev() {
        let path = dir.join(&name);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(manifest) = AgentManifest::from_json(&text) {
            out.push(Entry { path, manifest });
        }
    }
    Ok(out)
}

/// The recorded manifest of one session.
pub fn load(root: &Path, session_id: &str) -> Result<AgentManifest> {
    let path = dir(root).join(format!("{}.json", checked_id(session_id)?));
    let text = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "no recorded run {session_id}\nfix: pick a session from the history list; only \
             the newest {KEEP} runs are kept"
        )
    })?;
    AgentManifest::from_json(&text).with_context(|| format!("cannot parse {}", path.display()))
}

/// Session ids become file names, so only allow the characters
/// `intent::session_id` produces.
fn checked_id(id: &str) -> Result<&str> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        bail!("invalid session id {id:?}\nfix: use an id from the history list");
    }
    Ok(id)
}

/// `*.json` file names in `dir`, oldest first (ids start with hex time).
fn names(dir: &Path) -> Result<Vec<String>> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .with_context(|| format!("cannot read {}", dir.display()))?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json"))
        .collect();
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(id: &str) -> AgentManifest {
        let mut m = AgentManifest::new(id, "agent");
        m.finalize();
        m
    }

    #[test]
    fn records_lists_newest_first_and_prunes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        assert!(list(root).expect("empty").is_empty());
        for i in 0..KEEP + 3 {
            record(root, &manifest(&format!("harness_session_{i:04}"))).expect("record");
        }
        let entries = list(root).expect("list");
        assert_eq!(entries.len(), KEEP);
        assert_eq!(
            entries[0].manifest.session_id,
            format!("harness_session_{:04}", KEEP + 2)
        );
        assert_eq!(
            entries[KEEP - 1].manifest.session_id,
            "harness_session_0003"
        );
        let one = load(root, "harness_session_0010").expect("load");
        assert_eq!(one.target_agent, "agent");
    }

    #[test]
    fn ids_cannot_escape_the_history_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        for bad in ["../../etc/passwd", "", "a/b", "a.json"] {
            let err = load(dir.path(), bad).expect_err("rejected");
            assert!(format!("{err:#}").contains("fix:"), "{bad}");
        }
        let err = load(dir.path(), "harness_session_missing").expect_err("missing");
        assert!(format!("{err:#}").contains("fix:"));
    }

    #[test]
    fn unparsable_files_are_skipped() {
        let dir = tempfile::tempdir().expect("tempdir");
        record(dir.path(), &manifest("harness_session_1")).expect("record");
        std::fs::write(super::dir(dir.path()).join("junk.json"), "{").expect("write");
        assert_eq!(list(dir.path()).expect("list").len(), 1);
    }
}
