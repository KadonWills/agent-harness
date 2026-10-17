//! `harness run` — the gate. Snapshot → sandbox → findings → manifest.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use harness_core::config::HarnessConfig;
use harness_core::manifest::{AgentManifest, ImpactedFile, SandboxStatus, Verdict};
use harness_runtime::{ExecutionPlan, SandboxBackend, SessionReport, Snapshot};

use crate::git::{ChangeKind, FileChange, Git};
use crate::init::STATE_DIR;
use crate::{intent, settings, Outcome};

#[derive(clap::Args, Default)]
pub struct Args {
    /// Identifier of the agent that made the change (checked against agents.allowed).
    #[arg(long, env = "HARNESS_AGENT")]
    pub agent: Option<String>,
    /// The instruction the agent was given; only its SHA-256 is recorded.
    #[arg(long, env = "HARNESS_INTENT", conflicts_with = "intent_file")]
    pub intent: Option<String>,
    /// Read the instruction from a file instead.
    #[arg(long, value_name = "PATH")]
    pub intent_file: Option<PathBuf>,
    /// Ref to diff against (default: project.base_ref).
    #[arg(long, value_name = "REF")]
    pub base: Option<String>,
    /// Where to write the manifest (default: .harness/AgentManifest.json).
    #[arg(long, value_name = "PATH")]
    pub manifest: Option<PathBuf>,
    /// Print the manifest on stdout.
    #[arg(long)]
    pub json: bool,
}

/// State shared by the gate's stages.
pub struct Session {
    pub git: Git,
    pub config: HarnessConfig,
    pub manifest: AgentManifest,
    /// Non-blocking notes shown in the human report.
    pub notes: Vec<String>,
}

pub fn run(dir: &Path, args: &Args) -> Result<Outcome> {
    let config_provider = {
        let git = Git::discover(dir)?;
        settings::load_valid(git.root())?.sandbox.provider
    };
    let backend = harness_runtime::backend_for(&config_provider)?;
    run_with(dir, args, backend.as_ref())
}

/// The gate with an injectable backend (tests use a fake one).
pub fn run_with(dir: &Path, args: &Args, backend: &dyn SandboxBackend) -> Result<Outcome> {
    let mut session = open(dir, args)?;
    agent_policy(&mut session);
    let report = sandbox_stage(&mut session, backend)?;

    let Session {
        git,
        mut manifest,
        notes,
        ..
    } = session;
    manifest.finalize();
    let path = args
        .manifest
        .clone()
        .unwrap_or_else(|| git.root().join(STATE_DIR).join("AgentManifest.json"));
    write_manifest(&path, &manifest)?;
    if args.json {
        println!("{}", manifest.to_json()?);
    }
    print_human(&manifest, report.as_ref(), &notes, &path);
    Ok(match manifest.verdict {
        Verdict::Pass => Outcome::Pass,
        _ => Outcome::Blocked,
    })
}

fn open(dir: &Path, args: &Args) -> Result<Session> {
    let git = Git::discover(dir)?;
    let config = settings::load_valid(git.root())?;
    let agent = args.agent.clone().unwrap_or_else(|| "unknown".to_string());
    let mut manifest = AgentManifest::new(intent::session_id(), agent);

    let intent_text = match &args.intent_file {
        Some(path) => Some(
            std::fs::read_to_string(path)
                .with_context(|| format!("cannot read --intent-file {}", path.display()))?,
        ),
        None => args.intent.clone(),
    };
    manifest.intent_hash = intent::hash(intent_text.as_deref());

    let base_ref = args
        .base
        .clone()
        .unwrap_or_else(|| config.project.base_ref.clone());
    let base = git.merge_base(&base_ref, "HEAD")?;
    let changes = git.changed_files(base.as_deref(), None)?;
    manifest.base_ref = base_ref;
    manifest.head = git.head().unwrap_or_default();
    let notes = vec![summarize(&changes)];
    manifest.impacted_files = changes
        .iter()
        .map(|c| ImpactedFile {
            path: c.path.clone(),
            ast_changes: Vec::new(),
        })
        .collect();

    Ok(Session {
        git,
        config,
        manifest,
        notes,
    })
}

fn summarize(changes: &[FileChange]) -> String {
    let count = |kind| changes.iter().filter(|c| c.kind == kind).count();
    format!(
        "changes: {} added, {} modified, {} deleted",
        count(ChangeKind::Added),
        count(ChangeKind::Modified),
        count(ChangeKind::Deleted)
    )
}

fn agent_policy(session: &mut Session) {
    let agents = &session.config.agents;
    let agent = session.manifest.target_agent.clone();
    if agents.allowed.is_empty() || agents.allowed.contains(&agent) {
        return;
    }
    let msg = format!(
        "agent `{agent}` is not in agents.allowed ({}); fix: add it to agents.allowed, \
         or pass --agent <id> / set HARNESS_AGENT to a listed agent",
        agents.allowed.join(", ")
    );
    if agents.strict_mode {
        session.manifest.block(msg);
    } else {
        session.notes.push(msg);
    }
}

fn sandbox_stage(
    session: &mut Session,
    backend: &dyn SandboxBackend,
) -> Result<Option<SessionReport>> {
    let validation = &session.config.validation;
    if validation.setup.is_empty() && validation.commands.is_empty() {
        session.manifest.sandbox_status = SandboxStatus::Skipped;
        session.notes.push(
            "sandbox skipped: no [validation] commands configured; fix: add the project's \
             test command, e.g. commands = [\"npm test\"]"
                .to_string(),
        );
        return Ok(None);
    }
    backend.check_available()?;
    let files = session.git.workspace_files()?;
    let snapshot = Snapshot::create(session.git.root(), &files)?;
    for skipped in snapshot.skipped() {
        session.notes.push(format!(
            "not copied into the sandbox (symlink or special file): {}",
            skipped.display()
        ));
    }
    let plan = ExecutionPlan::from_config(
        &session.config,
        &session.manifest.session_id,
        snapshot.path().to_path_buf(),
    )?;
    let report = backend.execute(&plan)?;

    let manifest = &mut session.manifest;
    manifest.steps = report.steps.iter().map(|s| s.report.clone()).collect();
    manifest.sandbox_status = if report.passed() {
        SandboxStatus::Passed
    } else {
        SandboxStatus::Failed
    };
    if let Some(failed) = report.steps.iter().find(|s| !s.succeeded()) {
        let r = &failed.report;
        manifest.block(if r.timed_out {
            format!(
                "`{}` was killed when the session hit sandbox.timeout; fix: make it faster \
                 or raise sandbox.timeout",
                r.command
            )
        } else {
            format!(
                "`{}` exited with code {} in the sandbox; fix: make it pass locally, then rerun \
                 `harness run`",
                r.command, r.exit_code
            )
        });
    }
    if let Some(first) = report.egress.first() {
        manifest.block(format!(
            "{} denied network attempt(s) in the sandbox (first: {} during `{}`); fix: \
             vendor or mock that dependency, or set sandbox.allow_egress = true if the \
             access is intended",
            report.egress.len(),
            first.destination,
            first.step
        ));
    }
    manifest.security_audit.egress_events = report.egress.clone();
    Ok(Some(report))
}

fn write_manifest(path: &Path, manifest: &AgentManifest) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    std::fs::write(path, manifest.to_json()? + "\n")
        .with_context(|| format!("cannot write the manifest to {}", path.display()))
}

fn print_human(
    manifest: &AgentManifest,
    report: Option<&SessionReport>,
    notes: &[String],
    path: &Path,
) {
    let head = manifest.head.get(..7).unwrap_or("(none)");
    eprintln!(
        "harness run · agent {} · base {} · head {head}",
        manifest.target_agent, manifest.base_ref
    );
    eprintln!("  impacted files: {}", manifest.impacted_files.len());
    if let Some(report) = report {
        for step in &report.steps {
            let r = &step.report;
            let status = if step.succeeded() {
                "ok  ".to_string()
            } else if r.timed_out {
                "TIME".to_string()
            } else {
                "FAIL".to_string()
            };
            eprintln!(
                "  {status} {:<8} {} ({:.1}s)",
                format!("{:?}", r.phase).to_lowercase(),
                r.command,
                r.duration_ms as f64 / 1000.0
            );
            if !step.succeeded() {
                let output = format!("{}{}", step.stdout, step.stderr);
                let lines: Vec<&str> = output.lines().collect();
                for line in &lines[lines.len().saturating_sub(30)..] {
                    eprintln!("       | {line}");
                }
            }
        }
    }
    for note in notes {
        eprintln!("  note: {note}");
    }
    match manifest.verdict {
        Verdict::Pass => eprintln!("PASS"),
        _ => {
            eprintln!("BLOCKED");
            for reason in &manifest.blocking_reasons {
                eprintln!("  - {reason}");
            }
        }
    }
    eprintln!("manifest: {}", path.display());
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use harness_core::manifest::{EgressEvent, Phase, StepReport};
    use harness_runtime::{Error, StepOutcome};

    use super::*;
    use crate::git::testing::{repo, write};

    /// Records the plan it was given and replays canned step results.
    struct FakeBackend {
        exit_codes: Vec<i32>,
        egress: Vec<EgressEvent>,
        seen: Mutex<Option<(ExecutionPlan, Vec<String>)>>,
    }

    impl FakeBackend {
        fn new(exit_codes: &[i32]) -> Self {
            Self {
                exit_codes: exit_codes.to_vec(),
                egress: Vec::new(),
                seen: Mutex::new(None),
            }
        }
    }

    impl SandboxBackend for FakeBackend {
        fn name(&self) -> &'static str {
            "fake"
        }
        fn check_available(&self) -> Result<(), Error> {
            Ok(())
        }
        fn execute(&self, plan: &ExecutionPlan) -> Result<SessionReport, Error> {
            let mut files: Vec<String> = walk(&plan.workspace);
            files.sort();
            *self.seen.lock().expect("lock") = Some((plan.clone(), files));
            let mut report = SessionReport {
                egress: self.egress.clone(),
                ..SessionReport::default()
            };
            for ((phase, command), code) in plan.steps().zip(&self.exit_codes) {
                report.steps.push(StepOutcome {
                    report: StepReport {
                        phase,
                        command: command.to_string(),
                        exit_code: *code,
                        duration_ms: 5,
                        timed_out: false,
                    },
                    stdout: String::new(),
                    stderr: format!("stderr of {command}"),
                });
                if *code != 0 {
                    break;
                }
            }
            Ok(report)
        }
    }

    fn walk(dir: &Path) -> Vec<String> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).expect("read_dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                for sub in walk(&path) {
                    out.push(format!("{}/{sub}", entry.file_name().to_string_lossy()));
                }
            } else {
                out.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
        out
    }

    const CONFIG: &str = "[validation]\nsetup = [\"install\"]\ncommands = [\"test\"]\n";

    fn manifest_of(dir: &Path) -> AgentManifest {
        let text = std::fs::read_to_string(dir.join(".harness/AgentManifest.json"))
            .expect("manifest written");
        AgentManifest::from_json(&text).expect("manifest parses")
    }

    #[test]
    fn passing_change_passes_and_snapshots_the_working_tree() {
        let dir = repo(&[
            ("harness.toml", CONFIG),
            ("src/a.py", "x = 1\n"),
            (".gitignore", ".harness/\nignored.log\n"),
        ]);
        write(
            dir.path(),
            &[
                ("src/a.py", "x = 2\n"),
                ("src/new.py", ""),
                ("ignored.log", "noise"),
            ],
        );
        let backend = FakeBackend::new(&[0, 0]);
        let args = Args {
            agent: Some("claude-code".into()),
            intent: Some("bump x".into()),
            ..Args::default()
        };
        assert_eq!(
            run_with(dir.path(), &args, &backend).expect("run"),
            Outcome::Pass
        );

        let (plan, files) = backend
            .seen
            .lock()
            .expect("lock")
            .clone()
            .expect("executed");
        assert_eq!(plan.setup, ["install"]);
        assert_eq!(
            files,
            [".gitignore", "harness.toml", "src/a.py", "src/new.py"]
        );

        let m = manifest_of(dir.path());
        assert_eq!(m.verdict, Verdict::Pass);
        assert_eq!(m.target_agent, "claude-code");
        assert_eq!(m.intent_hash, intent::hash(Some("bump x")));
        assert_eq!(m.sandbox_status, SandboxStatus::Passed);
        assert_eq!(m.steps.len(), 2);
        assert_eq!(m.steps[0].phase, Phase::Setup);
        let paths: Vec<_> = m.impacted_files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["src/a.py", "src/new.py"]);
        assert_eq!(m.base_ref, "main");
        assert_eq!(m.head.len(), 40);
    }

    #[test]
    fn failing_step_blocks() {
        let dir = repo(&[("harness.toml", CONFIG)]);
        let backend = FakeBackend::new(&[0, 1]);
        let outcome = run_with(dir.path(), &Args::default(), &backend).expect("run");
        assert_eq!(outcome, Outcome::Blocked);
        let m = manifest_of(dir.path());
        assert_eq!(m.sandbox_status, SandboxStatus::Failed);
        assert!(m.blocking_reasons[0].contains("`test` exited with code 1"));
        assert!(m.blocking_reasons[0].contains("fix:"));
    }

    #[test]
    fn egress_attempts_block_and_are_audited() {
        let dir = repo(&[("harness.toml", CONFIG)]);
        let mut backend = FakeBackend::new(&[0, 0]);
        backend.egress.push(EgressEvent {
            step: "test".into(),
            destination: "evil.example".into(),
            evidence: "Could not resolve host: evil.example".into(),
        });
        let outcome = run_with(dir.path(), &Args::default(), &backend).expect("run");
        assert_eq!(outcome, Outcome::Blocked);
        let m = manifest_of(dir.path());
        assert_eq!(m.security_audit.unauthorized_egress_attempts, 1);
        assert!(m.blocking_reasons[0].contains("evil.example"));
    }

    #[test]
    fn strict_mode_rejects_unlisted_agents() {
        let config = format!("{CONFIG}[agents]\nallowed = [\"cline\"]\nstrict_mode = true\n");
        let dir = repo(&[("harness.toml", &config)]);
        let backend = FakeBackend::new(&[0, 0]);
        let args = Args {
            agent: Some("rogue".into()),
            ..Args::default()
        };
        assert_eq!(
            run_with(dir.path(), &args, &backend).expect("run"),
            Outcome::Blocked
        );
        assert!(manifest_of(dir.path()).blocking_reasons[0].contains("`rogue`"));

        // Without strict mode it is only a note.
        let lax = format!("{CONFIG}[agents]\nallowed = [\"cline\"]\n");
        let dir = repo(&[("harness.toml", &lax)]);
        assert_eq!(
            run_with(dir.path(), &args, &backend).expect("run"),
            Outcome::Pass
        );
    }

    #[test]
    fn no_commands_skips_the_sandbox() {
        let dir = repo(&[("harness.toml", "")]);
        let backend = FakeBackend::new(&[]);
        let custom = dir.path().join("out/m.json");
        let args = Args {
            manifest: Some(custom.clone()),
            ..Args::default()
        };
        assert_eq!(
            run_with(dir.path(), &args, &backend).expect("run"),
            Outcome::Pass
        );
        assert!(backend.seen.lock().expect("lock").is_none());
        let m = AgentManifest::from_json(&std::fs::read_to_string(custom).expect("written"))
            .expect("parses");
        assert_eq!(m.sandbox_status, SandboxStatus::Skipped);
    }

    #[test]
    fn invalid_config_and_bad_base_are_errors() {
        let dir = repo(&[("harness.toml", "[sandbox]\ntimeout = \"x\"\n")]);
        let err = run_with(dir.path(), &Args::default(), &FakeBackend::new(&[]))
            .expect_err("invalid config");
        assert!(format!("{err:#}").contains("sandbox.timeout"));

        let dir = repo(&[("harness.toml", "")]);
        let args = Args {
            base: Some("nope".into()),
            ..Args::default()
        };
        let err = run_with(dir.path(), &args, &FakeBackend::new(&[])).expect_err("unknown base");
        assert!(format!("{err:#}").contains("--base"));
    }

    #[test]
    fn intent_file_is_hashed() {
        let dir = repo(&[("harness.toml", "")]);
        write(dir.path(), &[("prompt.txt", "  do the thing \n")]);
        let args = Args {
            intent_file: Some(dir.path().join("prompt.txt")),
            ..Args::default()
        };
        run_with(dir.path(), &args, &FakeBackend::new(&[])).expect("run");
        assert_eq!(
            manifest_of(dir.path()).intent_hash,
            intent::hash(Some("do the thing"))
        );
    }
}
