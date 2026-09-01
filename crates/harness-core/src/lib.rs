//! # harness-core
//!
//! Shared foundation for AgentHarness: the `harness.toml` configuration
//! schema, the `AgentManifest` file contract, and common error types.
//!
//! This crate is intentionally dependency-light — every other crate in the
//! workspace depends on it, and external plugins link against it.

pub mod config;
pub mod manifest;

/// Crate-level error type.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("manifest error: {0}")]
    Manifest(String),
}
