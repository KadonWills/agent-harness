//! TUI state and key handling, kept free of terminal I/O so it is testable.

use std::path::{Path, PathBuf};

use harness_core::config::{HarnessConfig, Problem};
use harness_core::manifest::{AgentManifest, Phase, StepReport};

use crate::check::Report;
use crate::conflicts::Analysis;
use crate::scan::Findings;
use crate::{git::Git, history, loosen, settings};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Status,
    Run,
    History,
    Findings,
    Conflicts,
    Config,
}

impl Tab {
    pub const ALL: [Tab; 6] = [
        Tab::Status,
        Tab::Run,
        Tab::History,
        Tab::Findings,
        Tab::Conflicts,
        Tab::Config,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Status => "Status",
            Tab::Run => "Run",
            Tab::History => "History",
            Tab::Findings => "Findings",
            Tab::Conflicts => "Conflicts",
            Tab::Config => "Config",
        }
    }

    fn index(self) -> usize {
        Tab::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

/// Work done off the UI thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    Status,
    Scan,
    Conflicts { fix: bool },
    Run { agent: Option<String> },
}

/// Results reported back by workers.
#[derive(Debug)]
pub enum Msg {
    Status(Report),
    Findings(Result<Findings, String>),
    Conflicts(Result<Analysis, String>),
    StepStarted { phase: Phase, command: String },
    StepFinished { report: StepReport, output: String },
    RunDone(Box<Result<RunResult, String>>),
}

#[derive(Debug)]
pub struct RunResult {
    pub manifest: AgentManifest,
    pub notes: Vec<String>,
    pub manifest_path: PathBuf,
}

/// Something only the terminal loop can do.
#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    EditConfig(PathBuf),
}

#[derive(Debug, Default)]
pub enum Load<T> {
    #[default]
    NotLoaded,
    Loading,
    Ready(T),
    Failed(String),
}

#[derive(Debug, Default)]
pub struct RunView {
    pub running: bool,
    /// Steps in order; `None` while still running.
    pub steps: Vec<(Phase, String, Option<StepReport>)>,
    /// Output tail of the last failed step.
    pub failure_output: String,
    pub result: Option<Result<RunResult, String>>,
}

#[derive(Debug, Default)]
pub struct ConfigView {
    pub path: PathBuf,
    pub text: String,
    /// Parse error, if the file does not parse.
    pub error: Option<String>,
    pub problems: Vec<Problem>,
    /// Loosening caused by the last edit made from the TUI.
    pub loosened: Vec<String>,
}

pub struct App {
    pub root: PathBuf,
    pub tab: Tab,
    pub status: Load<Report>,
    pub findings: Load<Findings>,
    pub conflicts: Load<Analysis>,
    pub history: Vec<history::Entry>,
    pub selected: usize,
    pub run: RunView,
    pub config: ConfigView,
    /// Scroll offset of the active detail pane.
    pub scroll: u16,
    /// One-line feedback in the footer.
    pub message: String,
    /// Jobs requested but not yet started by the event loop.
    pub pending: Vec<Job>,
    pub quit: bool,
}

impl App {
    /// State for the repository containing `dir`. Starts the status check.
    pub fn new(dir: &Path) -> anyhow::Result<Self> {
        let root = Git::discover(dir)?.root().to_path_buf();
        let mut app = Self {
            config: ConfigView {
                path: settings::config_path(&root),
                ..ConfigView::default()
            },
            root,
            tab: Tab::Status,
            status: Load::NotLoaded,
            findings: Load::NotLoaded,
            conflicts: Load::NotLoaded,
            history: Vec::new(),
            selected: 0,
            run: RunView::default(),
            scroll: 0,
            message: String::new(),
            pending: Vec::new(),
            quit: false,
        };
        app.reload_config();
        app.reload_history();
        app.request(Job::Status);
        Ok(app)
    }

    pub fn request(&mut self, job: Job) {
        match &job {
            Job::Status => self.status = Load::Loading,
            Job::Scan => self.findings = Load::Loading,
            Job::Conflicts { .. } => self.conflicts = Load::Loading,
            Job::Run { .. } => {
                if self.run.running {
                    self.message = "a run is already in progress".into();
                    return;
                }
                self.run = RunView {
                    running: true,
                    ..RunView::default()
                };
            }
        }
        self.pending.push(job);
    }

    pub fn select_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.scroll = 0;
        match tab {
            Tab::Findings if matches!(self.findings, Load::NotLoaded) => self.request(Job::Scan),
            Tab::Conflicts if matches!(self.conflicts, Load::NotLoaded) => {
                self.request(Job::Conflicts { fix: false })
            }
            Tab::History => self.reload_history(),
            _ => {}
        }
    }

    pub fn on_key(&mut self, key: Key) -> Option<Effect> {
        self.message.clear();
        match key {
            Key::Char('q') | Key::Esc => self.quit = true,
            Key::Tab => self.select_tab(Tab::ALL[(self.tab.index() + 1) % Tab::ALL.len()]),
            Key::BackTab => {
                self.select_tab(Tab::ALL[(self.tab.index() + Tab::ALL.len() - 1) % Tab::ALL.len()])
            }
            Key::Char(c @ '1'..='6') => {
                let i = c as usize - '1' as usize;
                self.select_tab(Tab::ALL[i]);
            }
            Key::Char('r') => self.refresh(),
            Key::Char('s') if self.tab == Tab::Run => self.request(Job::Run {
                agent: std::env::var("HARNESS_AGENT").ok(),
            }),
            Key::Char('f') if self.tab == Tab::Conflicts => {
                self.request(Job::Conflicts { fix: true })
            }
            Key::Char('e') if self.tab == Tab::Config => {
                return Some(Effect::EditConfig(self.config.path.clone()));
            }
            Key::Up | Key::Char('k') if self.tab == Tab::History => {
                self.selected = self.selected.saturating_sub(1);
                self.scroll = 0;
            }
            Key::Down | Key::Char('j') if self.tab == Tab::History => {
                if self.selected + 1 < self.history.len() {
                    self.selected += 1;
                }
                self.scroll = 0;
            }
            Key::Up | Key::Char('k') | Key::PageUp => {
                let step = if key == Key::PageUp { 10 } else { 1 };
                self.scroll = self.scroll.saturating_sub(step);
            }
            Key::Down | Key::Char('j') | Key::PageDown => {
                let step = if key == Key::PageDown { 10 } else { 1 };
                self.scroll = self.scroll.saturating_add(step);
            }
            _ => {}
        }
        None
    }

    fn refresh(&mut self) {
        match self.tab {
            Tab::Status => self.request(Job::Status),
            Tab::Findings => self.request(Job::Scan),
            Tab::Conflicts => self.request(Job::Conflicts { fix: false }),
            Tab::History => self.reload_history(),
            Tab::Config => self.reload_config(),
            Tab::Run => {}
        }
    }

    pub fn apply(&mut self, msg: Msg) {
        match msg {
            Msg::Status(report) => self.status = Load::Ready(report),
            Msg::Findings(r) => self.findings = r.map_or_else(Load::Failed, Load::Ready),
            Msg::Conflicts(r) => {
                if let Ok(a) = &r {
                    if !a.fixed.is_empty() {
                        self.message = format!("applied {} Level 1 fix(es)", a.fixed.len());
                    }
                }
                self.conflicts = r.map_or_else(Load::Failed, Load::Ready);
            }
            Msg::StepStarted { phase, command } => self.run.steps.push((phase, command, None)),
            Msg::StepFinished { report, output } => {
                if report.exit_code != 0 || report.timed_out {
                    self.run.failure_output = output;
                }
                if let Some(last) = self.run.steps.last_mut() {
                    last.2 = Some(report);
                }
            }
            Msg::RunDone(r) => {
                self.run.running = false;
                self.run.result = Some(*r);
                self.reload_history();
            }
        }
    }

    pub fn reload_history(&mut self) {
        match history::list(&self.root) {
            Ok(h) => self.history = h,
            Err(e) => self.message = format!("cannot read history: {e:#}"),
        }
        self.selected = self.selected.min(self.history.len().saturating_sub(1));
    }

    /// Re-read `harness.toml`; report what an edit loosened since `before`.
    pub fn reload_config_after_edit(&mut self) {
        let before = HarnessConfig::from_toml(&self.config.text).ok();
        self.reload_config();
        let after = HarnessConfig::from_toml(&self.config.text).ok();
        self.config.loosened = match (before, after) {
            (Some(b), Some(a)) => loosen::loosened(&b, &a),
            _ => Vec::new(),
        };
        self.message = if self.config.loosened.is_empty() {
            "harness.toml reloaded".into()
        } else {
            "harness.toml reloaded — this edit loosens the gate (see below)".into()
        };
    }

    pub fn reload_config(&mut self) {
        let path = &self.config.path;
        self.config.text = std::fs::read_to_string(path).unwrap_or_default();
        self.config.problems.clear();
        self.config.error = None;
        if !path.is_file() {
            self.config.error = Some("no harness.toml; fix: run `harness init`".into());
            return;
        }
        match HarnessConfig::from_toml(&self.config.text) {
            Ok(c) => self.config.problems = c.validate(),
            Err(e) => self.config.error = Some(e.to_string()),
        }
    }

    pub fn selected_manifest(&self) -> Option<&AgentManifest> {
        self.history.get(self.selected).map(|e| &e.manifest)
    }
}

/// Terminal-independent key events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Up,
    Down,
    PageUp,
    PageDown,
    Tab,
    BackTab,
    Esc,
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testing::{repo, write};

    fn app() -> (tempfile::TempDir, App) {
        let dir = repo(&[("harness.toml", "[agents]\nstrict_mode = true\n")]);
        let app = App::new(dir.path()).expect("app");
        (dir, app)
    }

    #[test]
    fn starts_on_status_and_loads_tabs_lazily() {
        let (_dir, mut app) = app();
        assert_eq!(app.pending, [Job::Status]);
        assert!(matches!(app.status, Load::Loading));
        app.pending.clear();

        app.on_key(Key::Char('4'));
        assert_eq!(app.tab, Tab::Findings);
        app.on_key(Key::Tab);
        assert_eq!(app.tab, Tab::Conflicts);
        assert_eq!(app.pending, [Job::Scan, Job::Conflicts { fix: false }]);

        app.pending.clear();
        app.on_key(Key::BackTab);
        assert!(app.pending.is_empty(), "findings already loading");
        app.on_key(Key::Char('q'));
        assert!(app.quit);
    }

    #[test]
    fn a_run_streams_steps_and_refuses_to_overlap() {
        let (_dir, mut app) = app();
        app.select_tab(Tab::Run);
        app.on_key(Key::Char('s'));
        app.on_key(Key::Char('s'));
        assert_eq!(
            app.pending
                .iter()
                .filter(|j| matches!(j, Job::Run { .. }))
                .count(),
            1
        );
        assert!(app.message.contains("already"));

        app.apply(Msg::StepStarted {
            phase: Phase::Validate,
            command: "npm test".into(),
        });
        assert!(app.run.steps[0].2.is_none());
        app.apply(Msg::StepFinished {
            report: StepReport {
                phase: Phase::Validate,
                command: "npm test".into(),
                exit_code: 1,
                duration_ms: 10,
                timed_out: false,
            },
            output: "1 failing".into(),
        });
        assert_eq!(app.run.failure_output, "1 failing");
        app.apply(Msg::RunDone(Box::new(Err("boom".into()))));
        assert!(!app.run.running);
    }

    #[test]
    fn config_edits_report_loosening() {
        let (dir, mut app) = app();
        assert!(app.config.error.is_none());
        app.select_tab(Tab::Config);
        assert_eq!(
            app.on_key(Key::Char('e')),
            Some(Effect::EditConfig(app.root.join("harness.toml")))
        );
        write(
            dir.path(),
            &[(
                "harness.toml",
                "[sandbox]\nallow_egress = true\ntimeout = \"x\"\n",
            )],
        );
        app.reload_config_after_edit();
        assert_eq!(app.config.loosened.len(), 2, "{:?}", app.config.loosened);
        assert_eq!(app.config.problems[0].field, "sandbox.timeout");

        write(dir.path(), &[("harness.toml", "[nope]\n")]);
        app.reload_config();
        assert!(app.config.error.is_some());
    }

    #[test]
    fn history_selection_is_bounded() {
        let (dir, mut app) = app();
        for i in 0..3 {
            let mut m = AgentManifest::new(format!("harness_session_{i}"), "a");
            m.finalize();
            history::record(dir.path(), &m).expect("record");
        }
        app.select_tab(Tab::History);
        assert_eq!(app.history.len(), 3);
        for _ in 0..5 {
            app.on_key(Key::Down);
        }
        assert_eq!(app.selected, 2);
        assert_eq!(
            app.selected_manifest().map(|m| m.session_id.as_str()),
            Some("harness_session_0")
        );
        app.on_key(Key::Up);
        assert_eq!(app.selected, 1);
    }
}
