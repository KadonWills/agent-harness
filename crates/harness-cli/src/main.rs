//! `harness` — the AgentHarness CLI.
//!
//! Command surface and exit codes are specified in
//! docs/rfcs/0001-v0.1-v0.4-contracts.md §5: 0 = pass, 1 = the gate
//! blocked, 2 = harness could not run.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use harness_cli::{check, conflicts, init, run, scan, Outcome};

#[derive(Parser)]
#[command(
    name = "harness",
    version,
    about = "Guardrails for autonomous coding agents",
    after_help = "Exit codes: 0 = pass, 1 = the gate blocked, 2 = harness could not run."
)]
struct Cli {
    /// Run as if started in this directory.
    #[arg(short = 'C', long, global = true, value_name = "DIR")]
    directory: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a harness.toml for the detected project type.
    Init(init::Args),
    /// Validate harness.toml and report environment readiness.
    Check(check::Args),
    /// Run the validation pipeline against pending agent changes.
    Run(run::Args),
    /// Detect semantic conflicts between concurrent agent branches.
    Conflicts(conflicts::Args),
    /// Scan for secrets and policy-violating new dependencies (no sandbox).
    Scan(scan::Args),
    /// Open the terminal UI (needs the `tui` feature).
    Tui,
}

#[cfg(feature = "tui")]
fn tui(dir: &std::path::Path) -> anyhow::Result<Outcome> {
    harness_cli::tui::run(dir)
}

#[cfg(not(feature = "tui"))]
fn tui(_: &std::path::Path) -> anyhow::Result<Outcome> {
    anyhow::bail!(
        "this build has no terminal UI\nfix: reinstall with `cargo install harness-cli --features tui`"
    )
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("HARNESS_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();
    let cli = Cli::parse();
    let dir = match cli.directory {
        Some(dir) => dir,
        None => match std::env::current_dir() {
            Ok(dir) => dir,
            Err(e) => {
                eprintln!("error: cannot read the current directory: {e}");
                return ExitCode::from(2);
            }
        },
    };
    let result = match cli.command {
        Command::Init(args) => init::run(&dir, &args),
        Command::Check(args) => check::run(&dir, &args),
        Command::Run(args) => run::run(&dir, &args),
        Command::Conflicts(args) => conflicts::run(&dir, &args),
        Command::Scan(args) => scan::run(&dir, &args),
        Command::Tui => tui(&dir),
    };
    match result {
        Ok(Outcome::Pass) => ExitCode::SUCCESS,
        Ok(Outcome::Blocked) => ExitCode::from(1),
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}
