//! Isolated copies of the working tree. Agent changes never execute in the
//! real checkout.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::Error;

/// A temporary copy of selected files; deleted on drop.
#[derive(Debug)]
pub struct Snapshot {
    dir: tempfile::TempDir,
    skipped: Vec<PathBuf>,
}

impl Snapshot {
    /// Copy `files` (paths relative to `root`) into a fresh temporary
    /// directory. Symlinks are skipped so a link cannot pull host files
    /// from outside the repository into the sandbox.
    pub fn create(root: &Path, files: &[PathBuf]) -> Result<Self, Error> {
        let dir = tempfile::Builder::new()
            .prefix("harness-snapshot-")
            .tempdir()
            .map_err(|e| {
                Error::Snapshot(format!(
                    "cannot create a temporary directory: {e}\nfix: check that the \
                     system temp directory exists and is writable (or set TMPDIR)"
                ))
            })?;
        let mut skipped = Vec::new();
        for rel in files {
            if !is_safe_relative(rel) {
                return Err(Error::Snapshot(format!(
                    "refusing to snapshot {}: path escapes the repository\nfix: \
                     pass paths relative to the repository root",
                    rel.display()
                )));
            }
            let src = root.join(rel);
            let meta = match fs::symlink_metadata(&src) {
                Ok(m) => m,
                // Listed by git but deleted in the working tree.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(copy_error(&src, &e)),
            };
            if !meta.is_file() {
                skipped.push(rel.clone());
                continue;
            }
            let dst = dir.path().join(rel);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent).map_err(|e| copy_error(&dst, &e))?;
            }
            fs::copy(&src, &dst).map_err(|e| copy_error(&src, &e))?;
        }
        Ok(Self { dir, skipped })
    }

    /// The snapshot directory.
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Symlinks and other non-regular files that were not copied.
    pub fn skipped(&self) -> &[PathBuf] {
        &self.skipped
    }
}

fn is_safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)))
}

fn copy_error(path: &Path, e: &std::io::Error) -> Error {
    Error::Snapshot(format!(
        "cannot copy {}: {e}\nfix: check file permissions in the repository",
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_listed_files_only() {
        let repo = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(repo.path().join("src")).expect("mkdir");
        fs::write(repo.path().join("src/a.txt"), "a").expect("write");
        fs::write(repo.path().join("secret.txt"), "not listed").expect("write");

        let files = [PathBuf::from("src/a.txt"), PathBuf::from("gone.txt")];
        let snap = Snapshot::create(repo.path(), &files).expect("snapshot");
        assert_eq!(
            fs::read_to_string(snap.path().join("src/a.txt")).expect("read"),
            "a"
        );
        assert!(!snap.path().join("secret.txt").exists());
        assert!(!snap.path().join("gone.txt").exists());

        let dir = snap.path().to_path_buf();
        drop(snap);
        assert!(!dir.exists(), "snapshot must be deleted on drop");
    }

    #[test]
    fn rejects_paths_that_escape() {
        let repo = tempfile::tempdir().expect("tempdir");
        for bad in ["../x", "/etc/passwd", "a/../../b", ""] {
            let err = Snapshot::create(repo.path(), &[PathBuf::from(bad)])
                .expect_err("escaping path must fail");
            assert!(err.to_string().contains("fix:"), "{bad}: {err}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn skips_symlinks() {
        let repo = tempfile::tempdir().expect("tempdir");
        std::os::unix::fs::symlink("/etc/hosts", repo.path().join("link")).expect("symlink");
        let snap = Snapshot::create(repo.path(), &[PathBuf::from("link")]).expect("snapshot");
        assert!(!snap.path().join("link").exists());
        assert_eq!(snap.skipped(), [PathBuf::from("link")]);
    }
}
