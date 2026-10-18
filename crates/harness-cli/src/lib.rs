//! The commands behind the `harness` binary, as a library so the terminal
//! and web UIs share one code path with the CLI (RFC-0002 §2).
//!
//! This API is internal: the published contracts are the CLI surface and
//! `AgentManifest.json`, not these functions.

pub mod check;
pub mod conflicts;
pub mod detect;
pub mod git;
pub mod history;
pub mod hooks;
pub mod init;
pub mod intent;
pub mod loosen;
pub mod run;
pub mod scan;
pub mod settings;
#[cfg(feature = "tui")]
pub mod tui;

/// What a command decided, mapped to exit codes 0 and 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Blocked,
}
