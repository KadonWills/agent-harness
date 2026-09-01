//! # harness-runtime
//!
//! Deterministic, network-denied sandbox execution for agent-generated
//! changes. Backends are pluggable: Docker (default, v0.1) and Wasmtime
//! (v0.3+). The cloud Firecracker backend lives outside this repository.
//!
//! ## Status: v0.1 milestone — interfaces only

use harness_core::config::SandboxConfig;

/// Outcome of executing a validation step inside the sandbox.
#[derive(Debug, Clone)]
pub struct ExecutionReport {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    /// Egress attempts observed while the sandbox denied network access.
    pub egress_attempts: u32,
}

/// A sandbox backend (Docker, Wasmtime, …).
pub trait SandboxBackend: Send + Sync {
    fn name(&self) -> &'static str;
    /// True when the backend's host dependencies are available.
    fn is_available(&self) -> bool;
    /// Run a command inside an isolated workspace copy.
    fn run(&self, config: &SandboxConfig, command: &[String]) -> Result<ExecutionReport, Error>;
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("backend unavailable: {0}")]
    BackendUnavailable(String),
    #[error("execution failed: {0}")]
    Execution(String),
}
