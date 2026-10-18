//! The Wasmtime backend: runs WASI (preview 1) modules.
//!
//! Each command is `<module path> [args…]`, the module path relative to the
//! snapshot (a `.wasm` binary or `.wat` text). The snapshot is the only
//! preopened directory and WASI p1 has no sockets, so egress is impossible
//! rather than merely denied. Memory is capped per store; the session's
//! wall-clock deadline interrupts execution via epochs. A WASI module is
//! single-threaded, so `max_cpu_time` is enforced through that deadline.

use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use harness_core::manifest::StepReport;
use wasmtime::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder, Trap};
use wasmtime_wasi::p1::{self, WasiP1Ctx};
use wasmtime_wasi::p2::pipe::MemoryOutputPipe;
use wasmtime_wasi::{FsPerms, I32Exit, WasiCtxBuilder};

use crate::docker::TIMEOUT_EXIT_CODE;
use crate::{Error, ExecutionPlan, Progress, SandboxBackend, SessionReport, StepOutcome};

/// Exit code recorded when a module traps (e.g. out of memory).
pub const TRAP_EXIT_CODE: i32 = 134;

const OUTPUT_CAPACITY: usize = 1 << 20;

#[derive(Debug, Clone, Copy, Default)]
pub struct WasmtimeBackend;

struct State {
    wasi: WasiP1Ctx,
    limits: StoreLimits,
}

impl SandboxBackend for WasmtimeBackend {
    fn name(&self) -> &'static str {
        "wasmtime"
    }

    fn check_available(&self) -> Result<(), Error> {
        engine().map(|_| ())
    }

    fn execute(&self, plan: &ExecutionPlan) -> Result<SessionReport, Error> {
        self.execute_observed(plan, &mut |_| {})
    }

    fn execute_observed(
        &self,
        plan: &ExecutionPlan,
        observer: &mut dyn FnMut(Progress<'_>),
    ) -> Result<SessionReport, Error> {
        let deadline = Instant::now() + plan.limits.timeout;
        let engine = engine()?;
        let mut report = SessionReport::default();
        for (phase, command) in plan.steps() {
            observer(Progress::StepStarted { phase, command });
            let start = Instant::now();
            let (exit_code, timed_out, stdout, stderr) =
                run_step(&engine, plan, command, deadline)?;
            let outcome = StepOutcome {
                report: StepReport {
                    phase,
                    command: command.to_string(),
                    exit_code,
                    duration_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
                    timed_out,
                },
                stdout,
                stderr,
            };
            let ok = outcome.succeeded();
            observer(Progress::StepFinished { outcome: &outcome });
            report.steps.push(outcome);
            if !ok {
                break;
            }
        }
        Ok(report)
    }
}

fn engine() -> Result<Engine, Error> {
    let mut config = Config::new();
    config.epoch_interruption(true);
    Engine::new(&config).map_err(|e| {
        Error::BackendUnavailable(format!(
            "cannot start the Wasmtime engine: {e}\nfix: use sandbox.provider = \"docker\" \
             on this platform"
        ))
    })
}

type StepResult = (i32, bool, String, String);

fn run_step(
    engine: &Engine,
    plan: &ExecutionPlan,
    command: &str,
    deadline: Instant,
) -> Result<StepResult, Error> {
    let argv: Vec<&str> = command.split_whitespace().collect();
    let Some(module_path) = argv.first() else {
        return Err(Error::Execution(
            "empty command\nfix: remove empty strings from [validation]".to_string(),
        ));
    };
    let invalid = |what: &str, e: &dyn std::fmt::Display| {
        Error::Execution(format!(
            "{what} for `{command}`: {e}\nfix: with the wasmtime provider every command is \
             `<path/to/module.wasm> [args]`, relative to the repository root"
        ))
    };
    let module_file = plan.workspace.join(module_path);
    if !is_inside(&plan.workspace, &module_file) {
        return Err(invalid("module path escapes the workspace", &module_path));
    }
    let module =
        Module::from_file(engine, &module_file).map_err(|e| invalid("cannot load module", &e))?;

    let stdout = MemoryOutputPipe::new(OUTPUT_CAPACITY);
    let stderr = MemoryOutputPipe::new(OUTPUT_CAPACITY);
    let mut builder = WasiCtxBuilder::new();
    builder
        .args(&argv)
        .stdout(stdout.clone())
        .stderr(stderr.clone())
        .preopened_dir(&plan.workspace, ".", FsPerms::ReadWrite)
        .map_err(|e| invalid("cannot preopen the workspace", &e))?;
    let limits = StoreLimitsBuilder::new()
        .memory_size(usize::try_from(plan.limits.memory_bytes).unwrap_or(usize::MAX))
        .build();
    let mut store = Store::new(
        engine,
        State {
            wasi: builder.build_p1(),
            limits,
        },
    );
    store.limiter(|s| &mut s.limits);
    store.set_epoch_deadline(1);

    let mut linker: Linker<State> = Linker::new(engine);
    p1::add_to_linker_sync(&mut linker, |s| &mut s.wasi)
        .map_err(|e| invalid("cannot link WASI", &e))?;

    // Interrupt the guest at the deadline unless the step finishes first.
    let (done, wait) = mpsc::channel::<()>();
    let ticker = {
        let engine = engine.clone();
        let remaining = deadline.saturating_duration_since(Instant::now());
        thread::spawn(move || {
            if wait.recv_timeout(remaining).is_err() {
                engine.increment_epoch();
            }
        })
    };
    let result = linker
        .instantiate(&mut store, &module)
        .and_then(|instance| instance.get_typed_func::<(), ()>(&mut store, "_start"))
        .and_then(|start| start.call(&mut store, ()));
    let _ = done.send(());
    let _ = ticker.join();

    let read = |p: &MemoryOutputPipe| String::from_utf8_lossy(&p.contents()).into_owned();
    let (code, timed_out, trap) = match result {
        Ok(()) => (0, false, None),
        Err(e) => {
            if let Some(exit) = e.downcast_ref::<I32Exit>() {
                (exit.0, false, None)
            } else if e.downcast_ref::<Trap>() == Some(&Trap::Interrupt) {
                (TIMEOUT_EXIT_CODE, true, None)
            } else {
                (TRAP_EXIT_CODE, false, Some(format!("{e:#}")))
            }
        }
    };
    let mut err_text = read(&stderr);
    if let Some(trap) = trap {
        err_text.push_str(&format!("\nwasm trap: {trap}\n"));
    }
    Ok((code, timed_out, read(&stdout), err_text))
}

fn is_inside(root: &Path, path: &Path) -> bool {
    use std::path::Component;
    path.strip_prefix(root)
        .map(|rel| rel.components().all(|c| matches!(c, Component::Normal(_))))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::*;
    use crate::Limits;

    /// Prints "hello\n" via fd_write, then exits with code 3.
    const HELLO: &str = r#"(module
      (import "wasi_snapshot_preview1" "fd_write"
        (func $fd_write (param i32 i32 i32 i32) (result i32)))
      (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
      (memory (export "memory") 1)
      (data (i32.const 16) "hello\n")
      (func (export "_start")
        (i32.store (i32.const 0) (i32.const 16))
        (i32.store (i32.const 4) (i32.const 6))
        (drop (call $fd_write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)))
        (call $exit (i32.const 3))))"#;

    const SPIN: &str = r#"(module (func (export "_start") (loop br 0)))"#;

    /// Grows memory by 64 MiB; traps if the limiter refuses.
    const GREEDY: &str = r#"(module
      (memory 1)
      (func (export "_start")
        (if (i32.eq (memory.grow (i32.const 1024)) (i32.const -1))
          (then unreachable))))"#;

    const OK: &str = r#"(module (func (export "_start")))"#;

    fn plan(dir: &Path, commands: &[&str], timeout: Duration) -> ExecutionPlan {
        ExecutionPlan {
            session_id: "wasm".into(),
            workspace: dir.to_path_buf(),
            image: String::new(),
            setup: Vec::new(),
            commands: commands.iter().map(|s| s.to_string()).collect(),
            limits: Limits {
                memory_bytes: 16 << 20,
                cpu_time: Duration::from_secs(60),
                timeout,
            },
            allow_egress: false,
            allow_setup_egress: false,
        }
    }

    fn workspace(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        for (name, text) in files {
            std::fs::write(dir.path().join(name), text).expect("write");
        }
        dir
    }

    #[test]
    fn runs_modules_and_captures_output() {
        let dir = workspace(&[("hello.wat", HELLO), ("ok.wat", OK)]);
        let backend = WasmtimeBackend;
        backend.check_available().expect("engine");
        let p = plan(
            dir.path(),
            &["ok.wat", "hello.wat a b", "ok.wat"],
            Duration::from_secs(30),
        );
        let report = backend.execute(&p).expect("execute");
        assert_eq!(report.steps.len(), 2, "stops after the failing step");
        assert_eq!(report.steps[0].report.exit_code, 0);
        assert_eq!(report.steps[1].report.exit_code, 3);
        assert_eq!(report.steps[1].stdout, "hello\n");
        assert!(report.egress.is_empty());
    }

    #[test]
    fn progress_is_reported_live_and_stops_with_the_session() {
        let dir = workspace(&[("hello.wat", HELLO), ("ok.wat", OK)]);
        let p = plan(
            dir.path(),
            &["ok.wat", "hello.wat", "ok.wat"],
            Duration::from_secs(30),
        );
        let mut events = Vec::new();
        let report = WasmtimeBackend
            .execute_observed(&p, &mut |e| {
                events.push(match e {
                    Progress::StepStarted { command, .. } => format!("start {command}"),
                    Progress::StepFinished { outcome } => format!("end {}", outcome.succeeded()),
                })
            })
            .expect("execute");
        assert_eq!(report.steps.len(), 2);
        assert_eq!(
            events,
            ["start ok.wat", "end true", "start hello.wat", "end false"]
        );
    }

    #[test]
    fn deadline_interrupts_infinite_loops() {
        let dir = workspace(&[("spin.wat", SPIN)]);
        let start = Instant::now();
        let report = WasmtimeBackend
            .execute(&plan(dir.path(), &["spin.wat"], Duration::from_millis(300)))
            .expect("execute");
        assert!(report.steps[0].report.timed_out);
        assert_eq!(report.steps[0].report.exit_code, TIMEOUT_EXIT_CODE);
        assert!(start.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn memory_ceiling_is_enforced() {
        let dir = workspace(&[("greedy.wat", GREEDY)]);
        let report = WasmtimeBackend
            .execute(&plan(dir.path(), &["greedy.wat"], Duration::from_secs(30)))
            .expect("execute");
        assert_eq!(report.steps[0].report.exit_code, TRAP_EXIT_CODE);
        assert!(report.steps[0].stderr.contains("wasm trap"));
    }

    #[test]
    fn bad_commands_are_explained() {
        let dir = workspace(&[]);
        for cmd in ["missing.wasm", "../outside.wasm"] {
            let err = WasmtimeBackend
                .execute(&plan(dir.path(), &[cmd], Duration::from_secs(5)))
                .expect_err("must fail");
            assert!(err.to_string().contains("fix:"), "{cmd}: {err}");
        }
        assert!(!is_inside(Path::new("/w"), &PathBuf::from("/w/../x")));
    }
}
