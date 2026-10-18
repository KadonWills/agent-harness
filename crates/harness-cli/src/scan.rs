//! `harness scan` and the security stage of `harness run`: secrets in
//! added lines and policy for newly introduced dependencies.

use std::path::Path;

use anyhow::{Context, Result};
use harness_core::config::HarnessConfig;
use harness_core::manifest::{DependencyFinding, SecretFinding};
use harness_security::{added_lines, deps, Policy, Scanner};
use serde::Serialize;

use crate::git::{ChangeKind, Git};
use crate::{settings, Outcome};

#[derive(clap::Args, Default)]
pub struct Args {
    /// Scan the staged changes (what the pre-commit hook uses).
    #[arg(long, conflicts_with = "base")]
    pub staged: bool,
    /// Ref to diff the working tree against (default: project.base_ref).
    #[arg(long, value_name = "REF")]
    pub base: Option<String>,
    /// Print a machine-readable report on stdout.
    #[arg(long)]
    pub json: bool,
}

/// What the security gate found.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Findings {
    pub secrets: Vec<SecretFinding>,
    pub dependencies: Vec<DependencyFinding>,
}

impl Findings {
    /// One agent-legible blocking reason per problem group.
    pub fn blocking_reasons(&self) -> Vec<String> {
        let mut reasons = Vec::new();
        if let Some(first) = self.secrets.first() {
            reasons.push(format!(
                "{} secret(s) in added lines (first: {} at {}:{}); fix: remove the secret and \
                 rotate it, or mark a false positive with a `harness:allow-secret` comment",
                self.secrets.len(),
                first.rule_id,
                first.path,
                first.line
            ));
        }
        for d in &self.dependencies {
            reasons.push(format!(
                "new {} dependency `{}` in {}: {}",
                d.ecosystem, d.name, d.file, d.reason
            ));
        }
        reasons
    }
}

/// What to compare: the working tree against a base commit, or the index.
pub enum Target<'a> {
    WorkingTree { base: Option<&'a str> },
    Staged,
}

/// Scan the repository at `dir`: the index when `staged`, otherwise the
/// working tree against its merge base with `base` (default:
/// `project.base_ref`). Prints nothing.
pub fn analyze(dir: &Path, staged: bool, base: Option<&str>) -> Result<Findings> {
    let git = Git::discover(dir)?;
    let config = settings::load_valid(git.root())?;
    if staged {
        return inspect(&git, &config, &Target::Staged);
    }
    let base_ref = base.unwrap_or(&config.project.base_ref);
    let base = git.merge_base(base_ref, "HEAD")?;
    inspect(
        &git,
        &config,
        &Target::WorkingTree {
            base: base.as_deref(),
        },
    )
}

/// Run both checks. Errors only for unusable configuration.
pub fn inspect(git: &Git, config: &HarnessConfig, target: &Target<'_>) -> Result<Findings> {
    let mut findings = Findings::default();
    let security = &config.security;

    if security.scan_secrets {
        let mut scanner = Scanner::builtin();
        if !security.secret_rules.trim().is_empty() {
            let path = git.root().join(&security.secret_rules);
            let text = std::fs::read_to_string(&path).with_context(|| {
                format!(
                    "cannot read security.secret_rules {}\nfix: point it at a gitleaks rules \
                     file, or set it to \"\"",
                    path.display()
                )
            })?;
            scanner.extend(Scanner::from_gitleaks_toml(&text)?);
        }
        let diff = match target {
            Target::WorkingTree { base } => git.unified_diff(*base, false)?,
            Target::Staged => git.unified_diff(None, true)?,
        };
        findings.secrets = scanner.scan(&added_lines(&diff));
    }

    let policy = Policy::from_config(&security.dependencies, git.root())?;
    let changes = match target {
        Target::WorkingTree { base } => git.changed_files(*base, None)?,
        Target::Staged => git.staged_files()?,
    };
    for change in changes {
        if change.kind == ChangeKind::Deleted || deps::manifest_ecosystem(&change.path).is_none() {
            continue;
        }
        let (before, after) = match target {
            Target::WorkingTree { base } => (
                match base {
                    Some(b) => git.show(b, &change.path)?.unwrap_or_default(),
                    None => String::new(),
                },
                git.read_worktree(&change.path).unwrap_or_default(),
            ),
            Target::Staged => (
                match git.head() {
                    Some(_) => git.show("HEAD", &change.path)?.unwrap_or_default(),
                    None => String::new(),
                },
                git.show("", &change.path)?.unwrap_or_default(),
            ),
        };
        findings
            .dependencies
            .extend(policy.evaluate(&change.path, &before, &after));
    }
    Ok(findings)
}

pub fn run(dir: &Path, args: &Args) -> Result<Outcome> {
    let findings = analyze(dir, args.staged, args.base.as_deref())?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&findings)?);
    }
    for s in &findings.secrets {
        eprintln!(
            "SECRET {}:{} {} ({})",
            s.path, s.line, s.rule_id, s.description
        );
    }
    for d in &findings.dependencies {
        eprintln!(
            "DEP    {} {} in {}: {}",
            d.ecosystem, d.name, d.file, d.reason
        );
    }
    let reasons = findings.blocking_reasons();
    if reasons.is_empty() {
        eprintln!("harness scan: clean");
        Ok(Outcome::Pass)
    } else {
        eprintln!("BLOCKED");
        for r in reasons {
            eprintln!("  - {r}");
        }
        Ok(Outcome::Blocked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testing::{git, repo, write};

    fn aws_key() -> String {
        ["AKIA", "Z7VRSQ5TJS2GVAXR"].concat()
    }

    #[test]
    fn planted_credential_and_typosquat_are_blocked() {
        let dir = repo(&[
            ("harness.toml", ""),
            ("package.json", r#"{"dependencies":{"express":"4"}}"#),
        ]);
        let p = dir.path();
        write(
            p,
            &[
                (
                    "src/aws.ts",
                    &format!("export const key = \"{}\";\n", aws_key()),
                ),
                (
                    "package.json",
                    r#"{"dependencies":{"express":"4","lodahs":"1"}}"#,
                ),
            ],
        );
        assert_eq!(run(p, &Args::default()).expect("scan"), Outcome::Blocked);

        let g = Git::discover(p).expect("repo");
        let config = settings::load_valid(p).expect("config");
        let head = g.head().expect("head");
        let f = inspect(&g, &config, &Target::WorkingTree { base: Some(&head) }).expect("ok");
        assert_eq!(f.secrets.len(), 1);
        assert_eq!(
            (f.secrets[0].path.as_str(), f.secrets[0].line),
            ("src/aws.ts", 1)
        );
        assert_eq!(f.dependencies.len(), 1);
        assert_eq!(f.dependencies[0].name, "lodahs");
        assert_eq!(f.blocking_reasons().len(), 2);
        let same = analyze(p, false, Some("HEAD")).expect("analyze");
        assert_eq!((same.secrets.len(), same.dependencies.len()), (1, 1));
    }

    #[test]
    fn staged_scan_only_sees_the_index() {
        let dir = repo(&[("harness.toml", "")]);
        let p = dir.path();
        write(p, &[("a.py", &format!("KEY = \"{}\"\n", aws_key()))]);
        let args = Args {
            staged: true,
            ..Args::default()
        };
        assert_eq!(
            run(p, &args).expect("scan"),
            Outcome::Pass,
            "not staged yet"
        );
        git(p, &["add", "a.py"]);
        assert_eq!(run(p, &args).expect("scan"), Outcome::Blocked);
    }

    #[test]
    fn custom_rules_and_disabled_scanning() {
        let dir = repo(&[
            (
                "harness.toml",
                "[security]\nsecret_rules = \"rules.toml\"\n",
            ),
            (
                "rules.toml",
                "[[rules]]\nid = \"acme\"\nregex = '''acme_[a-z]{8}'''\n",
            ),
        ]);
        let p = dir.path();
        write(p, &[("x.txt", "token acme_abcdefgh\n")]);
        assert_eq!(run(p, &Args::default()).expect("scan"), Outcome::Blocked);

        write(p, &[("harness.toml", "[security]\nscan_secrets = false\n")]);
        assert_eq!(run(p, &Args::default()).expect("scan"), Outcome::Pass);

        write(
            p,
            &[("harness.toml", "[security]\nsecret_rules = \"nope.toml\"\n")],
        );
        let err = run(p, &Args::default()).expect_err("missing rules file");
        assert!(format!("{err:#}").contains("fix:"));
    }
}
