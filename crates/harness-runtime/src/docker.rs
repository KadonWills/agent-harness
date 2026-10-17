//! The Docker backend: drives the `docker` CLI.
//!
//! One long-lived container per session holds the snapshot at `/workspace`.
//! Setup steps run first; if only setup was granted network, the container
//! is disconnected from it before any validation step runs. Every step is
//! a `docker exec` sharing one wall-clock deadline.

use std::ffi::OsString;
use std::process::Command;
use std::time::Instant;

use harness_core::manifest::{Phase, StepReport};

use crate::{egress, process, Error, ExecutionPlan, SandboxBackend, SessionReport, StepOutcome};

/// Exit code recorded for a step killed by the session deadline.
pub const TIMEOUT_EXIT_CODE: i32 = 124;

const WORKDIR: &str = "/workspace";

#[derive(Debug, Clone)]
pub struct DockerBackend {
    program: OsString,
}

impl Default for DockerBackend {
    fn default() -> Self {
        Self::new("docker")
    }
}

impl DockerBackend {
    /// Use `program` as the docker CLI (e.g. a full path).
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
        }
    }

    fn command(&self, args: &[String]) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(args);
        cmd
    }

    fn docker(&self, args: &[String], deadline: Instant) -> Result<process::Output, Error> {
        process::run(&mut self.command(args), deadline).map_err(|e| {
            Error::BackendUnavailable(format!(
                "cannot run `docker`: {e}\nfix: install Docker and make sure `docker` is on PATH"
            ))
        })
    }
}

/// `docker run` arguments that start the idle session container.
pub fn container_args(plan: &ExecutionPlan, name: &str) -> Vec<String> {
    let network_at_start = if starts_networked(plan) {
        "bridge"
    } else {
        "none"
    };
    let memory = format!("{}b", plan.limits.memory_bytes);
    let cpu = plan.limits.cpu_time.as_secs().max(1);
    let mount = format!("{}:{WORKDIR}", plan.workspace.display());
    // The container outlives the session budget only briefly, then exits
    // and `--rm` deletes it, even if harness itself was killed.
    let lifetime = format!("sleep {}", plan.limits.timeout.as_secs() + 60);
    [
        "run",
        "--detach",
        "--rm",
        "--name",
        name,
        "--network",
        network_at_start,
        "--memory",
        &memory,
        "--memory-swap",
        &memory,
        "--pids-limit",
        "1024",
        "--ulimit",
        &format!("cpu={cpu}:{cpu}"),
        "--security-opt",
        "no-new-privileges",
        "--cap-drop",
        "NET_RAW",
        "--volume",
        &mount,
        "--workdir",
        WORKDIR,
        "--entrypoint",
        "sh",
        &plan.image,
        "-c",
        &lifetime,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// The container needs network at start for full egress, or for setup
/// steps that were granted it.
fn starts_networked(plan: &ExecutionPlan) -> bool {
    plan.allow_egress || (plan.allow_setup_egress && !plan.setup.is_empty())
}

/// `docker exec` arguments that run one step.
pub fn exec_args(name: &str, command: &str) -> Vec<String> {
    ["exec", "--workdir", WORKDIR, name, "sh", "-c", command]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Container names allow `[a-zA-Z0-9][a-zA-Z0-9_.-]*`.
pub fn container_name(session_id: &str) -> String {
    let clean: String = session_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "_.-".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("harness-{clean}")
}

/// Removes the container (and loosens snapshot permissions so the host can
/// delete files the container created as root) when the session ends.
struct ContainerGuard<'a> {
    backend: &'a DockerBackend,
    name: String,
}

impl Drop for ContainerGuard<'_> {
    fn drop(&mut self) {
        let soon = Instant::now() + std::time::Duration::from_secs(30);
        let chmod = exec_args(&self.name, &format!("chmod -R a+rwX {WORKDIR} 2>/dev/null"));
        let _ = self.backend.docker(&chmod, soon);
        let rm = vec!["rm".to_string(), "--force".to_string(), self.name.clone()];
        let _ = self.backend.docker(&rm, soon);
    }
}

impl SandboxBackend for DockerBackend {
    fn name(&self) -> &'static str {
        "docker"
    }

    fn check_available(&self) -> Result<(), Error> {
        let args = ["info", "--format", "{{.ServerVersion}}"].map(String::from);
        let out = self.docker(&args, Instant::now() + std::time::Duration::from_secs(20))?;
        if out.code == Some(0) {
            Ok(())
        } else {
            Err(Error::BackendUnavailable(format!(
                "the Docker daemon is not reachable: {}\nfix: start Docker (Docker Desktop, \
                 colima, or `sudo systemctl start docker`), or set sandbox.provider = \"wasmtime\"",
                out.stderr.trim()
            )))
        }
    }

    fn execute(&self, plan: &ExecutionPlan) -> Result<SessionReport, Error> {
        let deadline = Instant::now() + plan.limits.timeout;
        let name = container_name(&plan.session_id);
        let mut report = SessionReport::default();
        if plan.setup.is_empty() && plan.commands.is_empty() {
            return Ok(report);
        }

        let started = self.docker(&container_args(plan, &name), deadline)?;
        if started.code != Some(0) {
            return Err(Error::Execution(format!(
                "could not start the sandbox container from image {:?}: {}\nfix: check that \
                 sandbox.image exists (`docker pull {}`) and that Docker can mount {}",
                plan.image,
                started.stderr.trim(),
                plan.image,
                plan.workspace.display()
            )));
        }
        let _guard = ContainerGuard {
            backend: self,
            name: name.clone(),
        };

        let mut network_open = starts_networked(plan);
        for (phase, command) in plan.steps() {
            if phase == Phase::Validate && network_open && plan.egress_denied(Phase::Validate) {
                let args = ["network", "disconnect", "bridge", &name].map(String::from);
                let out = self.docker(&args, deadline)?;
                if out.code != Some(0) {
                    return Err(Error::Execution(format!(
                        "could not cut network access after setup: {}\nfix: set \
                         sandbox.allow_setup_egress = false and vendor dependencies instead",
                        out.stderr.trim()
                    )));
                }
                network_open = false;
            }

            let out = self.docker(&exec_args(&name, command), deadline)?;
            if plan.egress_denied(phase) {
                let combined = format!("{}\n{}", out.stdout, out.stderr);
                report.egress.extend(egress::detect(command, &combined));
            }
            let outcome = StepOutcome {
                report: StepReport {
                    phase,
                    command: command.to_string(),
                    exit_code: if out.timed_out {
                        TIMEOUT_EXIT_CODE
                    } else {
                        out.code.unwrap_or(-1)
                    },
                    duration_ms: u64::try_from(out.duration.as_millis()).unwrap_or(u64::MAX),
                    timed_out: out.timed_out,
                },
                stdout: out.stdout,
                stderr: out.stderr,
            };
            let ok = outcome.succeeded();
            report.steps.push(outcome);
            if !ok {
                break;
            }
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::*;
    use crate::Limits;

    fn plan(setup: &[&str], allow_setup_egress: bool, allow_egress: bool) -> ExecutionPlan {
        ExecutionPlan {
            session_id: "abc".into(),
            workspace: PathBuf::from("/tmp/snap"),
            image: "node:22".into(),
            setup: setup.iter().map(|s| s.to_string()).collect(),
            commands: vec!["npm test".into()],
            limits: Limits {
                memory_bytes: 512 << 20,
                cpu_time: Duration::from_secs(300),
                timeout: Duration::from_secs(600),
            },
            allow_egress,
            allow_setup_egress,
        }
    }

    fn arg_after(args: &[String], flag: &str) -> String {
        let i = args.iter().position(|a| a == flag).expect("flag present");
        args[i + 1].clone()
    }

    #[test]
    fn container_is_locked_down_by_default() {
        let args = container_args(&plan(&[], false, false), "harness-abc");
        assert_eq!(arg_after(&args, "--network"), "none");
        assert_eq!(arg_after(&args, "--memory"), "536870912b");
        assert_eq!(arg_after(&args, "--memory-swap"), "536870912b");
        assert_eq!(arg_after(&args, "--ulimit"), "cpu=300:300");
        assert_eq!(arg_after(&args, "--volume"), "/tmp/snap:/workspace");
        assert!(args.contains(&"no-new-privileges".to_string()));
        assert!(args.contains(&"--rm".to_string()));
        assert_eq!(args.last().map(String::as_str), Some("sleep 660"));
        assert_eq!(args.iter().filter(|a| *a == "node:22").count(), 1);
    }

    #[test]
    fn setup_egress_starts_on_bridge() {
        let args = container_args(&plan(&["npm ci"], true, false), "n");
        assert_eq!(arg_after(&args, "--network"), "bridge");
        // Setup egress without setup commands grants nothing.
        let args = container_args(&plan(&[], true, false), "n");
        assert_eq!(arg_after(&args, "--network"), "none");
        let args = container_args(&plan(&[], false, true), "n");
        assert_eq!(arg_after(&args, "--network"), "bridge");
    }

    #[test]
    fn exec_runs_through_a_shell_in_the_workspace() {
        assert_eq!(
            exec_args("n", "npm test && echo ok"),
            [
                "exec",
                "--workdir",
                "/workspace",
                "n",
                "sh",
                "-c",
                "npm test && echo ok"
            ]
        );
    }

    #[test]
    fn container_names_are_sanitized() {
        assert_eq!(container_name("s 1/2:x"), "harness-s-1-2-x");
    }

    #[test]
    fn missing_docker_binary_is_explained() {
        let backend = DockerBackend::new("definitely-not-docker-binary");
        let err = backend.check_available().expect_err("must fail");
        assert!(err.to_string().contains("fix:"), "{err}");
        let err = backend
            .execute(&plan(&[], false, false))
            .expect_err("must fail");
        assert!(err.to_string().contains("fix:"), "{err}");
    }

    /// Real end-to-end run. Needs a Docker daemon and network to pull
    /// `busybox`: `cargo test -p harness-runtime -- --ignored`.
    #[test]
    #[ignore = "needs a Docker daemon"]
    fn docker_end_to_end() {
        let backend = DockerBackend::default();
        backend.check_available().expect("docker available");
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("input.txt"), "hello").expect("write");
        let mut p = plan(&[], false, false);
        p.session_id = format!("test-{}", std::process::id());
        p.workspace = dir.path().to_path_buf();
        p.image = "busybox:1.36".into();
        p.commands = vec![
            "grep -q hello input.txt && echo made > out.txt".into(),
            "wget -q -T 3 http://example.com/ || true".into(),
            "exit 7".into(),
            "echo never-runs".into(),
        ];
        let report = backend.execute(&p).expect("execute");
        assert_eq!(report.steps.len(), 3, "stops at the first failure");
        assert_eq!(report.steps[2].report.exit_code, 7);
        assert!(!report.passed());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("out.txt"))
                .expect("container wrote into the snapshot")
                .trim(),
            "made"
        );
        assert_eq!(report.egress.len(), 1, "{:?}", report.egress);
        assert_eq!(report.egress[0].destination, "example.com");

        // Setup gets the network; validation must not.
        p.session_id = format!("test-setup-{}", std::process::id());
        p.allow_setup_egress = true;
        p.setup = vec!["wget -q -T 10 -O /dev/null http://example.com/".into()];
        p.commands = vec!["wget -q -T 3 -O /dev/null http://example.com/ || true".into()];
        let report = backend.execute(&p).expect("execute");
        assert!(report.passed(), "{:?}", report.steps);
        assert_eq!(report.egress.len(), 1, "{:?}", report.egress);
        assert_eq!(report.egress[0].step, p.commands[0]);
        p.allow_setup_egress = false;
        p.setup.clear();

        p.session_id = format!("test-timeout-{}", std::process::id());
        p.limits.timeout = Duration::from_secs(5);
        p.commands = vec!["sleep 30".into()];
        let report = backend.execute(&p).expect("execute");
        assert!(report.steps[0].report.timed_out);
        assert_eq!(report.steps[0].report.exit_code, TIMEOUT_EXIT_CODE);
    }
}
