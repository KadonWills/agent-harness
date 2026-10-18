//! `harness run` — the gate. Snapshot → sandbox → findings → manifest.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use harness_ast::conflict::{ChangeSet, FileChanges};
use harness_ast::AstChange;
use harness_core::config::HarnessConfig;
use harness_core::manifest::{AgentManifest, ImpactedFile, SandboxStatus, Verdict};
use harness_runtime::{ExecutionPlan, Progress, SandboxBackend, SessionReport, Snapshot};

use crate::git::{ChangeKind, FileChange, Git};
use crate::init::STATE_DIR;
use crate::{conflicts, history, intent, scan, settings, Outcome};

#[derive(clap::Args, Clone, Default)]
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
    /// The working tree's structural changes, for conflict detection.
    pub working_set: ChangeSet,
    /// Merge base with the base ref; `None` in a repository with no commits.
    pub base: Option<String>,
    /// Non-blocking notes shown in the human report.
    pub notes: Vec<String>,
}

/// A finished gate run.
#[derive(Debug)]
pub struct Completed {
    pub manifest: AgentManifest,
    /// Step output; `None` when the sandbox was skipped.
    pub report: Option<SessionReport>,
    /// Non-blocking notes shown in the human report.
    pub notes: Vec<String>,
    pub manifest_path: PathBuf,
}

impl Completed {
    pub fn outcome(&self) -> Outcome {
        match self.manifest.verdict {
            Verdict::Pass => Outcome::Pass,
            _ => Outcome::Blocked,
        }
    }
}

pub fn run(dir: &Path, args: &Args) -> Result<Outcome> {
    let backend = backend(dir)?;
    run_with(dir, args, backend.as_ref())
}

/// The sandbox backend `sandbox.provider` selects for the repository at `dir`.
pub fn backend(dir: &Path) -> Result<Box<dyn SandboxBackend>> {
    let git = Git::discover(dir)?;
    let provider = settings::load_valid(git.root())?.sandbox.provider;
    Ok(harness_runtime::backend_for(&provider)?)
}

/// The gate with an injectable backend (tests use a fake one).
pub fn run_with(dir: &Path, args: &Args, backend: &dyn SandboxBackend) -> Result<Outcome> {
    let done = execute(dir, args, backend, &mut |_| {})?;
    if args.json {
        println!("{}", done.manifest.to_json()?);
    }
    print_human(
        &done.manifest,
        done.report.as_ref(),
        &done.notes,
        &done.manifest_path,
    );
    Ok(done.outcome())
}

/// Run every stage, write the manifest and record it in the history.
/// Prints nothing; `observer` sees sandbox steps as they run.
pub fn execute(
    dir: &Path,
    args: &Args,
    backend: &dyn SandboxBackend,
    observer: &mut dyn FnMut(Progress<'_>),
) -> Result<Completed> {
    let mut session = open(dir, args)?;
    agent_policy(&mut session);
    conflict_stage(&mut session)?;
    security_stage(&mut session)?;
    let report = sandbox_stage(&mut session, backend, observer)?;

    let Session {
        git,
        mut manifest,
        mut notes,
        ..
    } = session;
    manifest.finalize();
    let manifest_path = args
        .manifest
        .clone()
        .unwrap_or_else(|| git.root().join(STATE_DIR).join("AgentManifest.json"));
    write_manifest(&manifest_path, &manifest)?;
    if let Err(e) = history::record(git.root(), &manifest) {
        notes.push(format!("run history not updated: {e:#}"));
    }
    Ok(Completed {
        manifest,
        report,
        notes,
        manifest_path,
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
    let mut notes = vec![summarize(&changes)];
    let ast = structural_diff(&git, base.as_deref(), None, &changes, &mut notes)?;
    let working_set = ChangeSet {
        name: git.current_branch().unwrap_or_else(|| "HEAD".to_string()),
        files: changes
            .iter()
            .map(|c| FileChanges {
                path: c.path.clone(),
                changes: ast.get(&c.path).cloned().unwrap_or_default(),
            })
            .collect(),
    };
    manifest.impacted_files = changes
        .iter()
        .map(|c| ImpactedFile {
            path: c.path.clone(),
            ast_changes: ast
                .get(&c.path)
                .map(|changes| {
                    changes
                        .iter()
                        .filter(|a| a.is_reportable())
                        .map(ToString::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect();

    Ok(Session {
        git,
        config,
        manifest,
        working_set,
        base,
        notes,
    })
}

/// Structural changes per file, for every file a parser claims. `head` is
/// a revision, or `None` for the working tree.
pub fn structural_diff(
    git: &Git,
    base: Option<&str>,
    head: Option<&str>,
    changes: &[FileChange],
    notes: &mut Vec<String>,
) -> Result<BTreeMap<String, Vec<AstChange>>> {
    let mut out = BTreeMap::new();
    for change in changes {
        let Some(parser) = harness_ast::parser_for_path(&change.path) else {
            continue;
        };
        let before = match (base, change.kind) {
            (Some(base), ChangeKind::Modified | ChangeKind::Deleted) => {
                git.show(base, &change.path)?.unwrap_or_default()
            }
            _ => String::new(),
        };
        let after = match (change.kind, head) {
            (ChangeKind::Deleted, _) => String::new(),
            (_, Some(head)) => git.show(head, &change.path)?.unwrap_or_default(),
            (_, None) => git.read_worktree(&change.path).unwrap_or_default(),
        };
        match parser.diff(&before, &after) {
            Ok(ast) => {
                out.insert(change.path.clone(), ast);
            }
            Err(e) => notes.push(format!("{}: structural diff skipped ({e})", change.path)),
        }
    }
    Ok(out)
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

/// Secrets in added lines and policy for new dependencies.
fn security_stage(session: &mut Session) -> Result<()> {
    let target = scan::Target::WorkingTree {
        base: session.base.as_deref(),
    };
    let findings = scan::inspect(&session.git, &session.config, &target)?;
    for reason in findings.blocking_reasons() {
        session.manifest.block(reason);
    }
    let audit = &mut session.manifest.security_audit;
    audit.secret_findings = findings.secrets;
    audit.dependency_findings = findings.dependencies;
    Ok(())
}

/// Compare the working tree with every sibling branch named by
/// `[conflicts].branches`. Level 2 conflicts block.
fn conflict_stage(session: &mut Session) -> Result<()> {
    let patterns = &session.config.conflicts.branches;
    if patterns.is_empty() {
        return Ok(());
    }
    let current = session.working_set.name.clone();
    let siblings: Vec<String> = session
        .git
        .branches()?
        .into_iter()
        .filter(|b| *b != current && harness_core::glob::any(patterns, b))
        .collect();
    let mut sets = vec![session.working_set.clone()];
    for sibling in &siblings {
        sets.push(conflicts::change_set(
            &session.git,
            &session.manifest.base_ref,
            sibling,
            false,
            &mut session.notes,
        )?);
    }
    let found: Vec<_> = harness_ast::conflict::detect(&sets)
        .into_iter()
        .filter(|c| c.branches.contains(&current))
        .collect();
    for c in &found {
        if c.level >= 2 {
            let other = c.branches.iter().find(|b| **b != current).cloned();
            session.manifest.block(format!(
                "semantic conflict with `{}` ({}): {}; fix: coordinate the two changes, or \
                 rebase onto `{}` and adapt",
                other.as_deref().unwrap_or("?"),
                c.kind,
                c.explanation,
                other.as_deref().unwrap_or("?"),
            ));
        }
    }
    session
        .manifest
        .conflicts
        .extend(found.iter().map(conflicts::report));
    session.notes.push(format!(
        "conflicts: compared with {} sibling branch(es)",
        siblings.len()
    ));
    Ok(())
}

fn sandbox_stage(
    session: &mut Session,
    backend: &dyn SandboxBackend,
    observer: &mut dyn FnMut(Progress<'_>),
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
    let report = backend.execute_observed(&plan, observer)?;

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
        assert!(
            m.impacted_files[0].ast_changes.is_empty(),
            "x = 2 declares nothing"
        );
        assert_eq!(m.base_ref, "main");
        assert_eq!(m.head.len(), 40);

        let recorded = history::list(dir.path()).expect("history");
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].manifest.session_id, m.session_id);
    }

    #[test]
    fn execute_reports_progress_and_returns_the_manifest() {
        let dir = repo(&[("harness.toml", CONFIG)]);
        let mut seen = Vec::new();
        let done = execute(
            dir.path(),
            &Args::default(),
            &FakeBackend::new(&[0, 0]),
            &mut |p| {
                if let Progress::StepStarted { command, .. } = p {
                    seen.push(command.to_string());
                }
            },
        )
        .expect("run");
        assert_eq!(seen, ["install", "test"]);
        assert_eq!(done.outcome(), Outcome::Pass);
        assert_eq!(done.report.expect("sandbox ran").steps.len(), 2);
        assert!(done.manifest_path.ends_with(".harness/AgentManifest.json"));
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
    fn structural_changes_are_recorded() {
        let dir = repo(&[
            ("harness.toml", ""),
            (
                "svc.ts",
                "export function getUser(id: string) { return id; }\n",
            ),
            ("old.py", "def gone():\n    pass\n"),
        ]);
        write(
            dir.path(),
            &[(
                "svc.ts",
                "export function getUser(id: string, deep: boolean) { return id; }\n",
            )],
        );
        std::fs::remove_file(dir.path().join("old.py")).expect("rm");
        run_with(dir.path(), &Args::default(), &FakeBackend::new(&[])).expect("run");
        let m = manifest_of(dir.path());
        let by_path = |p: &str| {
            m.impacted_files
                .iter()
                .find(|f| f.path == p)
                .map(|f| f.ast_changes.clone())
                .expect("impacted")
        };
        assert_eq!(
            by_path("svc.ts"),
            ["FunctionChanged:getUser", "SignatureChanged:getUser"]
        );
        assert_eq!(by_path("old.py"), ["FunctionRemoved:gone"]);
    }

    #[test]
    fn sibling_branch_conflicts_block_the_run() {
        let dir = repo(&[
            ("harness.toml", "[conflicts]\nbranches = [\"agent/*\"]\n"),
            ("lib.ts", "export function total(a: number) { return a; }\n"),
        ]);
        let p = dir.path();
        crate::git::testing::git(p, &["checkout", "-q", "-b", "agent/other"]);
        write(
            p,
            &[(
                "lib.ts",
                "export function sum(a: number) { return a + 0; }\n",
            )],
        );
        crate::git::testing::git(p, &["commit", "-qam", "drop total"]);
        crate::git::testing::git(p, &["checkout", "-q", "main"]);
        write(
            p,
            &[("use.ts", "import { total } from \"./lib\";\ntotal(1);\n")],
        );

        let outcome = run_with(p, &Args::default(), &FakeBackend::new(&[])).expect("run");
        assert_eq!(outcome, Outcome::Blocked);
        let m = manifest_of(p);
        assert_eq!(m.conflicts.len(), 1, "{:?}", m.conflicts);
        assert_eq!(m.conflicts[0].kind, "removed-symbol");
        assert_eq!(m.conflicts[0].branches, ["agent/other", "main"]);
        assert!(m.blocking_reasons[0].contains("agent/other"));
    }

    #[test]
    fn secrets_and_risky_dependencies_block_the_run() {
        let dir = repo(&[("harness.toml", ""), ("requirements.txt", "requests\n")]);
        let key = ["ghp_", "8vKq2LmN4pR6sT0uW3xY5zA7bC9dE1fG2hJ4"].concat();
        write(
            dir.path(),
            &[
                ("deploy.sh", &format!("TOKEN={key}\n")),
                ("requirements.txt", "requests\nreqeusts\n"),
            ],
        );
        let outcome = run_with(dir.path(), &Args::default(), &FakeBackend::new(&[])).expect("run");
        assert_eq!(outcome, Outcome::Blocked);
        let m = manifest_of(dir.path());
        assert_eq!(m.security_audit.secrets_blocked, 1);
        assert_eq!(m.security_audit.secret_findings[0].rule_id, "github-pat");
        assert_eq!(m.security_audit.dependency_findings[0].name, "reqeusts");
        assert_eq!(m.blocking_reasons.len(), 2);
        let json = std::fs::read_to_string(dir.path().join(".harness/AgentManifest.json"))
            .expect("manifest");
        assert!(!json.contains(&key), "the secret itself is never written");
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
