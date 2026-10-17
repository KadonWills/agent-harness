//! Git hook installation: the boring interception point (PRD principle 4).

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::git::Git;

/// First-line marker identifying hooks this tool owns.
const MARKER: &str = "# managed by agentharness";

/// The hooks `harness init --hooks` installs, with the command each runs.
pub const HOOKS: &[(&str, &str)] = &[("pre-push", "run")];

/// A POSIX sh hook (git runs hooks through sh on every platform).
pub fn script(subcommand: &str) -> String {
    format!(
        r#"#!/bin/sh
{MARKER} — regenerate with `harness init --hooks`; delete this file to opt out.
if command -v harness >/dev/null 2>&1; then
  exec harness {subcommand}
fi
echo "agentharness: \`harness\` is not on PATH; install it, or delete $0 to skip this check" >&2
exit 1
"#
    )
}

/// Install every hook. An existing hook that this tool does not manage is
/// left alone unless `force` is set.
pub fn install(git: &Git, force: bool) -> Result<Vec<PathBuf>> {
    let dir = git.hooks_dir()?;
    std::fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let mut written = Vec::new();
    for (name, subcommand) in HOOKS {
        let path = dir.join(name);
        if let Ok(existing) = std::fs::read_to_string(&path) {
            if !existing.contains(MARKER) && !force {
                bail!(
                    "{} already exists and is not managed by agentharness\nfix: add `harness \
                     {subcommand}` to it yourself, or pass --force to replace it",
                    path.display()
                );
            }
        }
        std::fs::write(&path, script(subcommand))
            .with_context(|| format!("cannot write {}", path.display()))?;
        make_executable(&path)?;
        written.push(path);
    }
    Ok(written)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .with_context(|| format!("cannot make {} executable", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testing::repo;

    #[test]
    fn installs_and_reinstalls_managed_hooks() {
        let dir = repo(&[]);
        let git = Git::discover(dir.path()).expect("repo");
        let written = install(&git, false).expect("install");
        assert_eq!(written.len(), 1);
        let text = std::fs::read_to_string(&written[0]).expect("hook");
        assert!(text.starts_with("#!/bin/sh\n"));
        assert!(text.contains("exec harness run"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&written[0])
                .expect("meta")
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111);
        }
        install(&git, false).expect("managed hooks are replaced silently");
    }

    #[test]
    fn foreign_hooks_need_force() {
        let dir = repo(&[]);
        let git = Git::discover(dir.path()).expect("repo");
        let hook = git.hooks_dir().expect("dir").join("pre-push");
        std::fs::create_dir_all(hook.parent().expect("parent")).expect("mkdir");
        std::fs::write(&hook, "#!/bin/sh\nmy-own-check\n").expect("write");
        let err = install(&git, false).expect_err("must refuse");
        assert!(format!("{err:#}").contains("--force"));
        assert_eq!(
            std::fs::read_to_string(&hook).expect("read"),
            "#!/bin/sh\nmy-own-check\n"
        );
        install(&git, true).expect("forced");
        assert!(std::fs::read_to_string(&hook)
            .expect("read")
            .contains(MARKER));
    }

    #[cfg(unix)]
    #[test]
    fn hook_fails_closed_without_harness_on_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("hook");
        std::fs::write(&path, script("run")).expect("write");
        let out = std::process::Command::new("/bin/sh")
            .arg(&path)
            .env("PATH", "/nonexistent")
            .output()
            .expect("sh runs");
        assert_eq!(out.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&out.stderr).contains("not on PATH"));
    }
}
