//! `harness check` — validate config and environment readiness.

use std::path::Path;

use anyhow::Result;
use harness_core::config::Problem;
use serde::Serialize;

use crate::git::Git;
use crate::{settings, Outcome};

#[derive(clap::Args)]
pub struct Args {
    /// Print a machine-readable report on stdout.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub ok: bool,
    pub problems: Vec<Problem>,
    pub warnings: Vec<String>,
}

pub fn run(dir: &Path, args: &Args) -> Result<Outcome> {
    let report = inspect(dir, &|provider| {
        harness_runtime::backend_for(provider).and_then(|b| b.check_available())
    });
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_human(&report);
    }
    Ok(if report.ok {
        Outcome::Pass
    } else {
        Outcome::Blocked
    })
}

fn problem(field: &str, message: impl Into<String>, fix: impl Into<String>) -> Problem {
    Problem {
        field: field.to_string(),
        message: message.into(),
        fix: fix.into(),
    }
}

/// Every check, with the sandbox probe injected so tests need no Docker.
pub fn inspect(dir: &Path, probe: &dyn Fn(&str) -> Result<(), harness_runtime::Error>) -> Report {
    let mut report = Report::default();
    let git = match Git::discover(dir) {
        Ok(git) => git,
        Err(e) => {
            report.problems.push(problem(
                "git",
                format!("{e:#}"),
                "run inside a git repository",
            ));
            return report;
        }
    };
    let config = match settings::load(git.root()) {
        Ok(config) => config,
        Err(e) => {
            report.problems.push(problem(
                settings::CONFIG_FILE,
                format!("{e:#}"),
                "fix the file, or run `harness init --force` to regenerate it",
            ));
            return report;
        }
    };
    report.problems.extend(config.validate());

    if git.head().is_some() && git.resolve(&config.project.base_ref).is_none() {
        report.problems.push(problem(
            "project.base_ref",
            format!("ref `{}` does not exist", config.project.base_ref),
            "set project.base_ref to an existing branch (e.g. \"main\" or \"origin/main\")",
        ));
    }
    if let Err(e) = probe(&config.sandbox.provider) {
        report.problems.push(problem(
            "sandbox.provider",
            e.to_string(),
            "see the fix above, then rerun `harness check`",
        ));
    }
    if config.validation.commands.is_empty() {
        report.warnings.push(
            "no [validation] commands: `harness run` will not execute anything \
             (add e.g. commands = [\"npm test\"])"
                .to_string(),
        );
    }
    if config.sandbox.allow_egress {
        report.warnings.push(
            "sandbox.allow_egress = true: validation commands can reach the network".to_string(),
        );
    }
    report.ok = report.problems.is_empty();
    report
}

fn print_human(report: &Report) {
    for p in &report.problems {
        eprintln!("FAIL {}: {}", p.field, p.message);
        eprintln!("     fix: {}", p.fix);
    }
    for w in &report.warnings {
        eprintln!("warn {w}");
    }
    if report.ok {
        eprintln!("ok   harness is ready");
    } else {
        eprintln!("{} problem(s) found", report.problems.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testing::repo;

    fn ok_probe(_: &str) -> Result<(), harness_runtime::Error> {
        Ok(())
    }

    #[test]
    fn ready_repository_passes_with_warnings() {
        let dir = repo(&[("harness.toml", "")]);
        let report = inspect(dir.path(), &ok_probe);
        assert!(report.ok, "{:?}", report.problems);
        assert_eq!(report.warnings.len(), 1, "no commands warning");
    }

    #[test]
    fn every_problem_is_reported_with_a_fix() {
        let dir = repo(&[(
            "harness.toml",
            "[project]\nbase_ref = \"develop\"\n[sandbox]\nmax_memory = \"lots\"\n",
        )]);
        let report = inspect(dir.path(), &|_| {
            Err(harness_runtime::Error::BackendUnavailable(
                "daemon down".into(),
            ))
        });
        assert!(!report.ok);
        let fields: Vec<_> = report.problems.iter().map(|p| p.field.as_str()).collect();
        assert_eq!(
            fields,
            ["sandbox.max_memory", "project.base_ref", "sandbox.provider"]
        );
        assert!(report.problems.iter().all(|p| !p.fix.is_empty()));
    }

    #[test]
    fn missing_or_invalid_config() {
        let dir = repo(&[]);
        let report = inspect(dir.path(), &ok_probe);
        assert!(report.problems[0].message.contains("harness init"));

        let dir = repo(&[("harness.toml", "[sandbox]\nbogus = 1\n")]);
        let report = inspect(dir.path(), &ok_probe);
        assert!(report.problems[0].message.contains("bogus"));
    }

    #[test]
    fn outside_git() {
        let dir = tempfile::tempdir().expect("tempdir");
        let report = inspect(dir.path(), &ok_probe);
        assert_eq!(report.problems[0].field, "git");
    }
}
