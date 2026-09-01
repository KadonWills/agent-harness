//! `harness` — the AgentHarness CLI.
//!
//! Command surface (v0.1): `init`, `check`, `run`.
//! See docs/PRD.md §4 for the full specification.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "harness",
    version,
    about = "Guardrails for autonomous coding agents"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a harness.toml in the current repository.
    Init,
    /// Validate harness.toml and report environment readiness.
    Check,
    /// Run the validation pipeline against pending agent changes.
    Run,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    match cli.command {
        Command::Init => {
            println!("harness init: not yet implemented (v0.1 milestone — see docs/ROADMAP.md)");
        }
        Command::Check => {
            println!("harness check: not yet implemented (v0.1 milestone — see docs/ROADMAP.md)");
        }
        Command::Run => {
            println!("harness run: not yet implemented (v0.1 milestone — see docs/ROADMAP.md)");
        }
    }
    Ok(())
}
