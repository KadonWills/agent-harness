//! `harness conflicts` — semantic conflicts between concurrent agent branches.

use std::path::Path;

use anyhow::{bail, Context, Result};
use harness_ast::codemod::rename_identifiers;
use harness_ast::conflict::{self, ChangeSet, Conflict, FileChanges, Fix};
use harness_core::manifest::ConflictReport;
use serde::Serialize;

use crate::git::Git;
use crate::run::structural_diff;
use crate::{settings, Outcome};

#[derive(clap::Args, Default)]
pub struct Args {
    /// Ref the branches started from (default: project.base_ref).
    #[arg(long, value_name = "REF")]
    pub base: Option<String>,
    /// Apply Level 1 codemods to the checked-out branch's working tree.
    #[arg(long)]
    pub fix: bool,
    /// Print a machine-readable report on stdout.
    #[arg(long)]
    pub json: bool,
    /// Branches to compare (default: those matching [conflicts].branches).
    /// The checked-out branch is read from the working tree.
    pub branches: Vec<String>,
}

#[derive(Serialize)]
struct Report<'a> {
    branches: &'a [String],
    conflicts: Vec<ConflictReport>,
    fixed: &'a [String],
}

/// The manifest form of a conflict.
pub fn report(c: &Conflict) -> ConflictReport {
    ConflictReport {
        level: c.level,
        kind: c.kind.to_string(),
        symbol: c.symbol.clone(),
        branches: c.branches.clone(),
        explanation: c.explanation.clone(),
        fix: c.fix.as_ref().map(ToString::to_string),
    }
}

/// Structural changes of `branch` since it left `base_ref`.
pub fn change_set(
    git: &Git,
    base_ref: &str,
    branch: &str,
    from_worktree: bool,
    notes: &mut Vec<String>,
) -> Result<ChangeSet> {
    let head = if from_worktree { "HEAD" } else { branch };
    let base = git.merge_base(base_ref, head)?;
    let rev = (!from_worktree).then_some(branch);
    let changes = git.changed_files(base.as_deref(), rev)?;
    let ast = structural_diff(git, base.as_deref(), rev, &changes, notes)?;
    Ok(ChangeSet {
        name: branch.to_string(),
        files: changes
            .into_iter()
            .map(|c| FileChanges {
                changes: ast.get(&c.path).cloned().unwrap_or_default(),
                path: c.path,
            })
            .collect(),
    })
}

pub fn run(dir: &Path, args: &Args) -> Result<Outcome> {
    let git = Git::discover(dir)?;
    let config = settings::load_valid(git.root())?;
    let base_ref = args
        .base
        .clone()
        .unwrap_or_else(|| config.project.base_ref.clone());
    let current = git.current_branch();

    let branches: Vec<String> = if args.branches.is_empty() {
        git.branches()?
            .into_iter()
            .filter(|b| harness_core::glob::any(&config.conflicts.branches, b))
            .collect()
    } else {
        for b in &args.branches {
            if git.resolve(b).is_none() {
                bail!("branch `{b}` does not exist\nfix: check `git branch --list`");
            }
        }
        args.branches.clone()
    };
    if branches.len() < 2 {
        bail!(
            "need at least two branches to compare, found {}: [{}]\nfix: pass branch names \
             (`harness conflicts agent/a agent/b`) or set [conflicts].branches = [\"agent/*\"]",
            branches.len(),
            branches.join(", ")
        );
    }

    let mut notes = Vec::new();
    let mut sets = Vec::new();
    for b in &branches {
        let worktree = current.as_deref() == Some(b.as_str());
        sets.push(change_set(&git, &base_ref, b, worktree, &mut notes)?);
    }
    let mut found = conflict::detect(&sets);

    let mut fixed = Vec::new();
    if args.fix {
        if let Some(current) = &current {
            fixed = apply_fixes(&git, current, &found)?;
            if !fixed.is_empty() {
                // Re-read the working tree so the report reflects the fix.
                let i = branches.iter().position(|b| b == current);
                if let Some(i) = i {
                    sets[i] = change_set(&git, &base_ref, current, true, &mut notes)?;
                }
                found = conflict::detect(&sets);
            }
        }
    }

    let reports: Vec<ConflictReport> = found.iter().map(report).collect();
    if args.json {
        let out = Report {
            branches: &branches,
            conflicts: reports.clone(),
            fixed: &fixed,
        };
        println!("{}", serde_json::to_string_pretty(&out)?);
    }
    print_human(&branches, &reports, &fixed, &notes);
    Ok(if reports.iter().any(|c| c.level >= 2) {
        Outcome::Blocked
    } else {
        Outcome::Pass
    })
}

/// Apply rename codemods whose target is the checked-out branch. Returns
/// the files rewritten.
fn apply_fixes(git: &Git, current: &str, found: &[Conflict]) -> Result<Vec<String>> {
    let mut fixed = Vec::new();
    for c in found {
        let Some(Fix::RenameReferences { from, to, files }) = &c.fix else {
            continue;
        };
        if c.branches[1] != current {
            continue;
        }
        for path in files {
            let Some(source) = git.read_worktree(path) else {
                continue;
            };
            if let Some(updated) = rename_identifiers(path, &source, from, to)? {
                let full = git.root().join(path);
                std::fs::write(&full, updated)
                    .with_context(|| format!("cannot write {}", full.display()))?;
                fixed.push(format!("{path}: `{from}` -> `{to}`"));
            }
        }
    }
    Ok(fixed)
}

fn print_human(
    branches: &[String],
    reports: &[ConflictReport],
    fixed: &[String],
    notes: &[String],
) {
    eprintln!("harness conflicts · {}", branches.join(" · "));
    for f in fixed {
        eprintln!("  fixed {f}");
    }
    for c in reports {
        eprintln!(
            "  L{} {} `{}` [{} -> {}]",
            c.level, c.kind, c.symbol, c.branches[0], c.branches[1]
        );
        eprintln!("     {}", c.explanation);
        if let Some(fix) = &c.fix {
            eprintln!("     fix: {fix}");
        }
    }
    for n in notes {
        eprintln!("  note: {n}");
    }
    let halts = reports.iter().filter(|c| c.level >= 2).count();
    match (reports.len(), halts) {
        (0, _) => eprintln!("no semantic conflicts"),
        (_, 0) => eprintln!("{} auto-fixable conflict(s); no halt", reports.len()),
        _ => eprintln!("BLOCKED: {halts} conflict(s) need a human"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testing::{git, repo, write};

    /// The PRD's scenario: agent A renames a schema field, agent B adds a
    /// resolver reading the old name. No textual conflict exists.
    fn schema_drift_repo() -> tempfile::TempDir {
        let dir = repo(&[
            ("harness.toml", "[conflicts]\nbranches = [\"agent/*\"]\n"),
            (
                "schema.ts",
                "export interface User {\n  id: string;\n  email: string;\n}\n",
            ),
            (
                "resolvers.ts",
                "import { User } from \"./schema\";\nexport const id = (u: User) => u.id;\n",
            ),
        ]);
        let p = dir.path();
        git(p, &["checkout", "-q", "-b", "agent/rename-field"]);
        write(
            p,
            &[(
                "schema.ts",
                "export interface User {\n  id: string;\n  emailAddress: string;\n}\n",
            )],
        );
        git(p, &["commit", "-qam", "rename email"]);
        git(p, &["checkout", "-q", "main"]);
        git(p, &["checkout", "-q", "-b", "agent/add-resolver"]);
        write(
            p,
            &[(
                "resolvers.ts",
                "import { User } from \"./schema\";\nexport const id = (u: User) => u.id;\n\
                 export const email = (u: User) => u.email.toLowerCase();\n",
            )],
        );
        git(p, &["commit", "-qam", "add resolver"]);
        git(p, &["checkout", "-q", "main"]);
        dir
    }

    #[test]
    fn schema_drift_between_agent_branches_blocks() {
        let dir = schema_drift_repo();
        let outcome = run(dir.path(), &Args::default()).expect("run");
        assert_eq!(outcome, Outcome::Blocked);

        let g = Git::discover(dir.path()).expect("repo");
        let mut notes = Vec::new();
        let sets: Vec<_> = ["agent/add-resolver", "agent/rename-field"]
            .iter()
            .map(|b| change_set(&g, "main", b, false, &mut notes).expect("set"))
            .collect();
        let found = conflict::detect(&sets);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].kind, "removed-member");
        assert_eq!(found[0].symbol, "User.email");
        assert_eq!(
            found[0].branches,
            ["agent/rename-field", "agent/add-resolver"]
        );
    }

    #[test]
    fn level_one_rename_is_fixed_in_the_working_tree() {
        let dir = repo(&[
            ("harness.toml", ""),
            ("lib.py", "def fmt(x):\n    return str(x)\n"),
            ("app.py", "print(1)\n"),
        ]);
        let p = dir.path();
        git(p, &["checkout", "-q", "-b", "agent/rename"]);
        write(
            p,
            &[("lib.py", "def format_value(x):\n    return str(x)\n")],
        );
        git(p, &["commit", "-qam", "rename"]);
        git(p, &["checkout", "-q", "main"]);
        git(p, &["checkout", "-q", "-b", "agent/use"]);
        write(p, &[("app.py", "from lib import fmt\nprint(fmt(1))\n")]);
        git(p, &["commit", "-qam", "use fmt"]);

        let branches = vec!["agent/rename".to_string(), "agent/use".to_string()];
        let args = Args {
            branches: branches.clone(),
            ..Args::default()
        };
        assert_eq!(
            run(p, &args).expect("run"),
            Outcome::Pass,
            "level 1 never halts"
        );

        let args = Args {
            branches,
            fix: true,
            ..Args::default()
        };
        assert_eq!(run(p, &args).expect("run"), Outcome::Pass);
        assert_eq!(
            std::fs::read_to_string(p.join("app.py")).expect("read"),
            "from lib import format_value\nprint(format_value(1))\n"
        );
    }

    #[test]
    fn needs_two_branches_and_real_names() {
        let dir = repo(&[("harness.toml", "")]);
        let err = run(dir.path(), &Args::default()).expect_err("no branches");
        assert!(format!("{err:#}").contains("fix:"));
        let args = Args {
            branches: vec!["main".into(), "ghost".into()],
            ..Args::default()
        };
        let err = run(dir.path(), &args).expect_err("missing branch");
        assert!(format!("{err:#}").contains("ghost"));
    }
}
