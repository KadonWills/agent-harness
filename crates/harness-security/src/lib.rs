//! # harness-security
//!
//! The zero-trust gate (PRD §4.5): secret scanning on added lines and a
//! policy for newly introduced dependencies. Deterministic and offline —
//! rule files and advisory snapshots are inputs, never network fetches.

pub mod diff;
pub mod secrets;

pub use diff::{added_lines, AddedLine};
pub use secrets::Scanner;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid secret rules: {0}")]
    Rules(String),
}
