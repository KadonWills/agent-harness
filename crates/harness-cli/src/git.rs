//! Thin wrapper over the `git` CLI. Interception is boring on purpose: we
//! read what git already knows instead of watching the filesystem.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

#[derive(Debug, Clone)]
pub struct Git {
    root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
}

/// One file that differs between the base and the working tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
}

impl Git {
    /// Find the repository containing `dir`.
    pub fn discover(dir: &Path) -> Result<Self> {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .context("cannot run `git`\nfix: install git and make sure it is on PATH")?;
        if !out.status.success() {
            bail!(
                "{} is not inside a git repository\nfix: run harness from a git checkout \
                 (or `git init` first)",
                dir.display()
            );
        }
        let root = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(Self {
            root: PathBuf::from(root),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn raw(&self, args: &[&str]) -> Result<std::process::Output> {
        Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(args)
            .output()
            .with_context(|| format!("cannot run `git {}`", args.join(" ")))
    }

    fn run(&self, args: &[&str]) -> Result<String> {
        let out = self.raw(args)?;
        if !out.status.success() {
            bail!(
                "`git {}` failed: {}\nfix: check the repository state with `git status`",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Resolve a revision to a commit SHA, or `None` if it does not exist.
    pub fn resolve(&self, rev: &str) -> Option<String> {
        let spec = format!("{rev}^{{commit}}");
        let out = self
            .raw(&["rev-parse", "--verify", "--quiet", &spec])
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// The HEAD commit, or `None` in a repository with no commits.
    pub fn head(&self) -> Option<String> {
        self.resolve("HEAD")
    }

    /// The checked-out branch name, or `None` when detached.
    pub fn current_branch(&self) -> Option<String> {
        let name = self
            .run(&["symbolic-ref", "--quiet", "--short", "HEAD"])
            .ok()?;
        Some(name.trim().to_string()).filter(|n| !n.is_empty())
    }

    /// The commit to diff against: the merge base of `base` and `head`.
    /// `None` when the repository has no commits yet (everything is new).
    pub fn merge_base(&self, base: &str, head: &str) -> Result<Option<String>> {
        if self.head().is_none() {
            return Ok(None);
        }
        if self.resolve(base).is_none() {
            bail!(
                "base ref `{base}` does not exist\nfix: set project.base_ref in harness.toml \
                 or pass --base <ref> (e.g. --base origin/main)"
            );
        }
        let out = self.run(&["merge-base", base, head])?;
        Ok(Some(out.trim().to_string()))
    }

    /// Files that differ between `base` (a commit) and `head`. When `head`
    /// is `None` the working tree is compared, untracked files included.
    pub fn changed_files(&self, base: Option<&str>, head: Option<&str>) -> Result<Vec<FileChange>> {
        let mut changes = Vec::new();
        match (base, head) {
            (Some(base), head) => {
                let mut args = vec!["diff", "--name-status", "--no-renames", "-z", base];
                args.extend(head);
                let out = self.run(&args)?;
                let mut fields = out.split('\0').filter(|s| !s.is_empty());
                while let (Some(status), Some(path)) = (fields.next(), fields.next()) {
                    let kind = match status.chars().next() {
                        Some('A') => ChangeKind::Added,
                        Some('D') => ChangeKind::Deleted,
                        _ => ChangeKind::Modified,
                    };
                    changes.push(FileChange {
                        path: path.to_string(),
                        kind,
                    });
                }
            }
            (None, _) => {
                for path in self.split_z(&self.run(&["ls-files", "-z"])?) {
                    changes.push(FileChange {
                        path,
                        kind: ChangeKind::Added,
                    });
                }
            }
        }
        if head.is_none() {
            for path in self.untracked()? {
                changes.push(FileChange {
                    path,
                    kind: ChangeKind::Added,
                });
            }
        }
        changes.sort_by(|a, b| a.path.cmp(&b.path));
        changes.dedup_by(|a, b| a.path == b.path);
        Ok(changes)
    }

    fn untracked(&self) -> Result<Vec<String>> {
        let out = self.run(&["ls-files", "--others", "--exclude-standard", "-z"])?;
        Ok(self.split_z(&out))
    }

    fn split_z(&self, out: &str) -> Vec<String> {
        out.split('\0')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// Every file a snapshot should contain: tracked and untracked, minus
    /// ignored files and tracked files deleted from the working tree.
    pub fn workspace_files(&self) -> Result<Vec<PathBuf>> {
        let out = self.run(&[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])?;
        let mut files: Vec<PathBuf> = self
            .split_z(&out)
            .into_iter()
            .map(PathBuf::from)
            .filter(|p| self.root.join(p).exists())
            .collect();
        files.sort();
        files.dedup();
        Ok(files)
    }

    /// File contents at `rev`, or `None` if the file does not exist there.
    pub fn show(&self, rev: &str, path: &str) -> Result<Option<String>> {
        let spec = format!("{rev}:{path}");
        let out = self.raw(&["show", &spec])?;
        Ok(out
            .status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned()))
    }

    /// File contents in the working tree, or `None` if absent or binary.
    pub fn read_worktree(&self, path: &str) -> Option<String> {
        let bytes = std::fs::read(self.root.join(path)).ok()?;
        String::from_utf8(bytes).ok()
    }

    /// Path of the hooks directory (honours `core.hooksPath`).
    pub fn hooks_dir(&self) -> Result<PathBuf> {
        let out = self.run(&["rev-parse", "--git-path", "hooks"])?;
        let path = PathBuf::from(out.trim());
        Ok(if path.is_absolute() {
            path
        } else {
            self.root.join(path)
        })
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use std::path::Path;
    use std::process::Command;

    /// Run git in `dir` with an isolated identity; panics on failure.
    pub fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=.git/hooks",
            ])
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// A fresh repository on branch `main` with one commit of `files`.
    pub fn repo(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        git(dir.path(), &["init", "-q", "-b", "main"]);
        // Keep hook tests inside the temp repo even if the user has a
        // global core.hooksPath.
        git(dir.path(), &["config", "core.hooksPath", ".git/hooks"]);
        write(dir.path(), files);
        git(dir.path(), &["add", "-A"]);
        git(dir.path(), &["commit", "-q", "--allow-empty", "-m", "init"]);
        dir
    }

    pub fn write(dir: &Path, files: &[(&str, &str)]) {
        for (path, content) in files {
            let full = dir.join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).expect("mkdir");
            }
            std::fs::write(full, content).expect("write");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{git, repo, write};
    use super::*;

    #[test]
    fn reports_working_tree_changes_against_base() {
        let dir = repo(&[("a.txt", "1\n"), ("b.txt", "2\n"), ("c.txt", "3\n")]);
        write(dir.path(), &[("a.txt", "changed\n"), ("new.txt", "n\n")]);
        std::fs::remove_file(dir.path().join("b.txt")).expect("rm");
        let g = Git::discover(dir.path()).expect("repo");
        let head = g.head().expect("head");
        let base = g.merge_base("main", "HEAD").expect("merge base");
        assert_eq!(base.as_deref(), Some(head.as_str()));
        assert_eq!(
            g.show(&head, "a.txt").expect("show").as_deref(),
            Some("1\n")
        );
        assert_eq!(g.show(&head, "missing").expect("show"), None);
        assert_eq!(g.read_worktree("a.txt").as_deref(), Some("changed\n"));

        let changes = g.changed_files(base.as_deref(), None).expect("changes");
        let summary: Vec<_> = changes.iter().map(|c| (c.path.as_str(), c.kind)).collect();
        assert_eq!(
            summary,
            [
                ("a.txt", ChangeKind::Modified),
                ("b.txt", ChangeKind::Deleted),
                ("new.txt", ChangeKind::Added),
            ]
        );
        let files = g.workspace_files().expect("files");
        assert_eq!(
            files,
            ["a.txt", "c.txt", "new.txt"].map(PathBuf::from).to_vec()
        );
    }

    #[test]
    fn missing_base_is_explained() {
        let dir = repo(&[("a", "")]);
        let g = Git::discover(dir.path()).expect("repo");
        let err = g.merge_base("nope", "HEAD").expect_err("must fail");
        assert!(format!("{err:#}").contains("fix:"));
    }

    #[test]
    fn unborn_repository_treats_everything_as_added() {
        let dir = tempfile::tempdir().expect("tempdir");
        git(dir.path(), &["init", "-q", "-b", "main"]);
        write(dir.path(), &[("x.py", "print(1)\n")]);
        let g = Git::discover(dir.path()).expect("repo");
        assert_eq!(g.merge_base("main", "HEAD").expect("ok"), None);
        let changes = g.changed_files(None, None).expect("changes");
        assert_eq!(changes[0].kind, ChangeKind::Added);
    }

    #[test]
    fn branch_diffs_and_listing() {
        let dir = repo(&[("a", "1\n")]);
        git(dir.path(), &["checkout", "-q", "-b", "agent/x"]);
        write(dir.path(), &[("a", "2\n")]);
        git(dir.path(), &["commit", "-qam", "x"]);
        let g = Git::discover(dir.path()).expect("repo");
        assert_eq!(g.current_branch().as_deref(), Some("agent/x"));
        let base = g.merge_base("main", "agent/x").expect("mb").expect("some");
        let changes = g
            .changed_files(Some(&base), Some("agent/x"))
            .expect("changes");
        assert_eq!(changes.len(), 1);
    }

    #[test]
    fn outside_a_repository_is_explained() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = Git::discover(dir.path()).expect_err("not a repo");
        assert!(format!("{err:#}").contains("fix:"));
    }
}
