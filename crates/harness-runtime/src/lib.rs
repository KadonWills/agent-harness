//! # harness-runtime
//!
//! Deterministic, network-denied sandbox execution for agent-generated
//! changes. Backends are pluggable: Docker (default) and Wasmtime (opt-in
//! `wasmtime` feature). The cloud Firecracker backend lives outside this
//! repository and implements the same [`SandboxBackend`] trait.
//!
//! The flow: [`Snapshot::create`] copies the working tree into an isolated
//! directory, then [`SandboxBackend::execute`] runs an [`ExecutionPlan`]
//! against it and returns a [`SessionReport`].

use std::path::PathBuf;
use std::time::Duration;

use harness_core::config::{parse_duration, parse_memory, HarnessConfig};
use harness_core::manifest::{EgressEvent, Phase, StepReport};

pub mod docker;
pub mod egress;
mod process;
pub mod snapshot;
#[cfg(feature = "wasmtime")]
pub mod wasm;

pub use docker::DockerBackend;
pub use snapshot::Snapshot;

/// Resource ceilings for one session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    pub memory_bytes: u64,
    /// Per-process CPU-time ceiling.
    pub cpu_time: Duration,
    /// Wall-clock budget shared by every step of the session.
    pub timeout: Duration,
}

/// Everything a backend needs to run one session.
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    /// Used to name sandbox resources; must be unique per concurrent session.
    pub session_id: String,
    /// The prepared snapshot directory (see [`Snapshot`]).
    pub workspace: PathBuf,
    /// Docker image; ignored by backends that do not use one.
    pub image: String,
    pub setup: Vec<String>,
    pub commands: Vec<String>,
    pub limits: Limits,
    pub allow_egress: bool,
    pub allow_setup_egress: bool,
}

impl ExecutionPlan {
    /// Build a plan from a validated config.
    pub fn from_config(
        config: &HarnessConfig,
        session_id: &str,
        workspace: PathBuf,
    ) -> Result<Self, Error> {
        let sandbox = &config.sandbox;
        let invalid = |field: &str, e: String| {
            Error::Execution(format!(
                "sandbox.{field}: {e}\nfix: run `harness check` and correct harness.toml"
            ))
        };
        Ok(Self {
            session_id: session_id.to_string(),
            workspace,
            image: sandbox.image.clone(),
            setup: config.validation.setup.clone(),
            commands: config.validation.commands.clone(),
            limits: Limits {
                memory_bytes: parse_memory(&sandbox.max_memory)
                    .map_err(|e| invalid("max_memory", e))?,
                cpu_time: parse_duration(&sandbox.max_cpu_time)
                    .map_err(|e| invalid("max_cpu_time", e))?,
                timeout: parse_duration(&sandbox.timeout).map_err(|e| invalid("timeout", e))?,
            },
            allow_egress: sandbox.allow_egress,
            allow_setup_egress: sandbox.allow_setup_egress,
        })
    }

    /// Whether `phase` runs with the network denied.
    pub fn egress_denied(&self, phase: Phase) -> bool {
        match phase {
            Phase::Setup => !(self.allow_egress || self.allow_setup_egress),
            Phase::Validate => !self.allow_egress,
        }
    }

    /// Setup steps followed by validation steps, in execution order.
    pub fn steps(&self) -> impl Iterator<Item = (Phase, &str)> {
        let setup = self.setup.iter().map(|c| (Phase::Setup, c.as_str()));
        let validate = self.commands.iter().map(|c| (Phase::Validate, c.as_str()));
        setup.chain(validate)
    }
}

/// Outcome of one step, including its captured output.
#[derive(Debug, Clone)]
pub struct StepOutcome {
    pub report: StepReport,
    pub stdout: String,
    pub stderr: String,
}

impl StepOutcome {
    pub fn succeeded(&self) -> bool {
        self.report.exit_code == 0 && !self.report.timed_out
    }
}

/// Result of executing a plan.
#[derive(Debug, Clone, Default)]
pub struct SessionReport {
    /// Steps that ran, in order. Execution stops at the first failure.
    pub steps: Vec<StepOutcome>,
    /// Network attempts observed while egress was denied.
    pub egress: Vec<EgressEvent>,
}

impl SessionReport {
    pub fn passed(&self) -> bool {
        self.steps.iter().all(StepOutcome::succeeded)
    }
}

/// A sandbox backend (Docker, Wasmtime, …).
pub trait SandboxBackend: Send + Sync {
    fn name(&self) -> &'static str;
    /// Ok when usable; the error says what is missing and how to fix it.
    fn check_available(&self) -> Result<(), Error>;
    /// Run setup then validation commands against a prepared snapshot.
    fn execute(&self, plan: &ExecutionPlan) -> Result<SessionReport, Error>;
}

/// The backend for a `sandbox.provider` value.
pub fn backend_for(provider: &str) -> Result<Box<dyn SandboxBackend>, Error> {
    match provider {
        "docker" => Ok(Box::new(DockerBackend::default())),
        #[cfg(feature = "wasmtime")]
        "wasmtime" => Ok(Box::new(wasm::WasmtimeBackend)),
        #[cfg(not(feature = "wasmtime"))]
        "wasmtime" => Err(Error::BackendUnavailable(
            "this build has no wasmtime backend\nfix: rebuild with `--features wasmtime`, \
             or set sandbox.provider = \"docker\""
                .to_string(),
        )),
        other => Err(Error::BackendUnavailable(format!(
            "unknown sandbox provider {other:?}\nfix: set sandbox.provider to \"docker\" or \"wasmtime\""
        ))),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("backend unavailable: {0}")]
    BackendUnavailable(String),
    #[error("execution failed: {0}")]
    Execution(String),
    #[error("snapshot failed: {0}")]
    Snapshot(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_from_default_config_denies_egress() {
        let cfg = HarnessConfig::default();
        let plan = ExecutionPlan::from_config(&cfg, "s1", PathBuf::from("/tmp/x")).expect("plan");
        assert_eq!(plan.limits.memory_bytes, 512 << 20);
        assert_eq!(plan.limits.timeout, Duration::from_secs(600));
        assert!(plan.egress_denied(Phase::Setup));
        assert!(plan.egress_denied(Phase::Validate));
    }

    #[test]
    fn setup_egress_only_opens_setup() {
        let cfg = HarnessConfig::from_toml(
            "[sandbox]\nallow_setup_egress = true\n[validation]\nsetup = [\"npm ci\"]\ncommands = [\"npm test\"]\n",
        )
        .expect("parses");
        let plan = ExecutionPlan::from_config(&cfg, "s", PathBuf::new()).expect("plan");
        assert!(!plan.egress_denied(Phase::Setup));
        assert!(plan.egress_denied(Phase::Validate));
        let steps: Vec<_> = plan.steps().collect();
        assert_eq!(
            steps,
            [(Phase::Setup, "npm ci"), (Phase::Validate, "npm test")]
        );
    }

    #[test]
    fn invalid_limits_are_reported_with_a_fix() {
        let cfg = HarnessConfig::from_toml("[sandbox]\ntimeout = \"soon\"\n").expect("parses");
        let err = ExecutionPlan::from_config(&cfg, "s", PathBuf::new()).expect_err("must fail");
        assert!(err.to_string().contains("sandbox.timeout"));
        assert!(err.to_string().contains("fix:"));
    }

    #[test]
    fn backend_lookup() {
        assert_eq!(backend_for("docker").expect("docker").name(), "docker");
        let err = backend_for("podman").err().expect("unknown provider");
        assert!(err.to_string().contains("fix:"));
    }
}
