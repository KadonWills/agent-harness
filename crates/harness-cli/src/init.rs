//! `harness init` — write a `harness.toml` for the detected project.

use std::io::Write;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::git::Git;
use crate::settings::{config_path, CONFIG_FILE};
use crate::{detect, hooks, Outcome};

/// Where `harness run` writes manifests; kept out of git.
pub const STATE_DIR: &str = ".harness";

#[derive(clap::Args)]
pub struct Args {
    /// Overwrite an existing harness.toml (and replace unmanaged hooks).
    #[arg(long)]
    pub force: bool,
    /// Install the pre-commit (scan) and pre-push (run) hooks. Works on an
    /// existing config, too.
    #[arg(long)]
    pub hooks: bool,
}

pub fn run(dir: &Path, args: &Args) -> Result<Outcome> {
    let git = Git::discover(dir)?;
    let root = git.root();
    let path = config_path(root);
    if path.exists() && !args.force && args.hooks {
        install_hooks(&git, false)?;
        return Ok(Outcome::Pass);
    }
    if path.exists() && !args.force {
        bail!(
            "{} already exists\nfix: edit it, or pass --force to regenerate it",
            path.display()
        );
    }

    let detected = detect::detect(root);
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let text = detect::render(&name, &default_base_ref(&git), &detected);
    std::fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))?;
    ignore_state_dir(root)?;

    eprintln!("Detected a {} project.", detected.ecosystem);
    eprintln!("Wrote {CONFIG_FILE} (image {}).", detected.image);
    if detected.commands.is_empty() {
        eprintln!(
            "No test command found: add one under [validation] commands before `harness run`."
        );
    }
    if args.hooks {
        install_hooks(&git, args.force)?;
    } else {
        eprintln!("Tip: `harness init --hooks` runs the gate before every push.");
    }
    eprintln!("Next: `harness check`, then `harness run`.");
    Ok(Outcome::Pass)
}

fn install_hooks(git: &Git, force: bool) -> Result<()> {
    for path in hooks::install(git, force)? {
        eprintln!("Installed {}", path.display());
    }
    Ok(())
}

/// Prefer `main`, then `master`, then the current branch.
fn default_base_ref(git: &Git) -> String {
    ["main", "master"]
        .into_iter()
        .find(|b| git.resolve(b).is_some())
        .map(str::to_string)
        .or_else(|| git.current_branch())
        .unwrap_or_else(|| "main".to_string())
}

/// Append `.harness/` to `.gitignore` unless it is already listed.
fn ignore_state_dir(root: &Path) -> Result<()> {
    let path = root.join(".gitignore");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let entry = format!("{STATE_DIR}/");
    if existing
        .lines()
        .any(|l| l.trim() == entry || l.trim() == STATE_DIR)
    {
        return Ok(());
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("cannot update {}", path.display()))?;
    let sep = if existing.is_empty() || existing.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    writeln!(file, "{sep}{entry}").with_context(|| format!("cannot update {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testing::repo;
    use harness_core::config::HarnessConfig;

    #[test]
    fn writes_a_valid_config_and_ignores_state() {
        let dir = repo(&[
            ("package.json", r#"{"scripts":{"test":"jest"}}"#),
            (".gitignore", "node_modules"),
        ]);
        let outcome = run(
            dir.path(),
            &Args {
                force: false,
                hooks: false,
            },
        )
        .expect("init");
        assert_eq!(outcome, Outcome::Pass);
        let text = std::fs::read_to_string(dir.path().join(CONFIG_FILE)).expect("config");
        let cfg = HarnessConfig::from_toml(&text).expect("valid");
        assert_eq!(cfg.validation.commands, ["npm run test"]);
        assert_eq!(cfg.project.base_ref, "main");
        let ignore = std::fs::read_to_string(dir.path().join(".gitignore")).expect("ignore");
        assert_eq!(ignore, "node_modules\n.harness/\n");

        // Idempotent ignore entry; refuses to clobber without --force.
        let err = run(
            dir.path(),
            &Args {
                force: false,
                hooks: false,
            },
        )
        .expect_err("exists");
        assert!(format!("{err:#}").contains("--force"));
        run(
            dir.path(),
            &Args {
                force: true,
                hooks: false,
            },
        )
        .expect("forced");
        let ignore = std::fs::read_to_string(dir.path().join(".gitignore")).expect("ignore");
        assert_eq!(ignore.matches(".harness/").count(), 1);

        // --hooks on an existing config installs hooks and keeps the config.
        let before = std::fs::read_to_string(dir.path().join(CONFIG_FILE)).expect("config");
        let args = Args {
            force: false,
            hooks: true,
        };
        run(dir.path(), &args).expect("hooks only");
        assert!(dir.path().join(".git/hooks/pre-push").is_file());
        let after = std::fs::read_to_string(dir.path().join(CONFIG_FILE)).expect("config");
        assert_eq!(before, after);
    }
}
