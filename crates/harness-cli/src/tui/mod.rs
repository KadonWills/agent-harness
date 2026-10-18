//! `harness tui` — a terminal UI over the same functions as the CLI
//! (RFC-0002). Long work runs on worker threads and reports back over a
//! channel, so the screen stays responsive during a sandbox run.

use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use anyhow::{Context, Result};
use harness_runtime::Progress;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::DefaultTerminal;

use crate::{check, conflicts, run, scan, Outcome};

pub mod app;
pub mod view;

use app::{App, Effect, Job, Key, Msg, RunResult};

pub fn run(dir: &Path) -> Result<Outcome> {
    let mut app = App::new(dir)?;
    let (tx, rx) = mpsc::channel();
    let mut terminal = ratatui::try_init().context(
        "cannot start the terminal UI\nfix: run `harness tui` in an interactive terminal",
    )?;
    let result = event_loop(&mut terminal, &mut app, &tx, &rx);
    ratatui::restore();
    result.map(|()| Outcome::Pass)
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    tx: &Sender<Msg>,
    rx: &Receiver<Msg>,
) -> Result<()> {
    while !app.quit {
        for job in app.pending.drain(..) {
            spawn(app.root.clone(), job, tx.clone());
        }
        terminal.draw(|f| view::draw(f, app))?;
        while let Ok(msg) = rx.try_recv() {
            app.apply(msg);
        }
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if let Some(Effect::EditConfig(path)) = app.on_key(translate(key.code)) {
            ratatui::restore();
            let edited = edit(&path);
            *terminal = ratatui::try_init()?;
            match edited {
                Ok(()) => app.reload_config_after_edit(),
                Err(e) => app.message = format!("{e:#}"),
            }
        }
    }
    Ok(())
}

fn translate(code: KeyCode) -> Key {
    match code {
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Esc => Key::Esc,
        _ => Key::Other,
    }
}

/// Open `path` in `$VISUAL`/`$EDITOR` (default `vi`) and wait for it.
fn edit(path: &Path) -> Result<()> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    let mut parts = editor.split_whitespace();
    let program = parts.next().unwrap_or("vi");
    let status = std::process::Command::new(program)
        .args(parts)
        .arg(path)
        .status()
        .with_context(|| {
            format!("cannot start editor `{editor}`\nfix: set $EDITOR to an installed editor")
        })?;
    if !status.success() {
        anyhow::bail!("editor `{editor}` exited with {status}");
    }
    Ok(())
}

fn spawn(root: std::path::PathBuf, job: Job, tx: Sender<Msg>) {
    std::thread::spawn(move || {
        let msg = match job {
            Job::Status => Msg::Status(check::status(&root)),
            Job::Scan => Msg::Findings(scan::analyze(&root, false, None).map_err(text)),
            Job::Conflicts { fix } => {
                Msg::Conflicts(conflicts::analyze(&root, None, &[], fix).map_err(text))
            }
            Job::Run { agent } => Msg::RunDone(Box::new(gate(&root, agent, &tx).map_err(text))),
        };
        // The receiver is gone only when the UI has quit.
        let _ = tx.send(msg);
    });
}

fn gate(root: &Path, agent: Option<String>, tx: &Sender<Msg>) -> Result<RunResult> {
    let backend = run::backend(root)?;
    let args = run::Args {
        agent,
        ..run::Args::default()
    };
    let done = run::execute(root, &args, backend.as_ref(), &mut |p| {
        let msg = match p {
            Progress::StepStarted { phase, command } => Msg::StepStarted {
                phase,
                command: command.to_string(),
            },
            Progress::StepFinished { outcome } => Msg::StepFinished {
                report: outcome.report.clone(),
                output: format!("{}{}", outcome.stdout, outcome.stderr),
            },
        };
        let _ = tx.send(msg);
    })?;
    Ok(RunResult {
        manifest: done.manifest,
        notes: done.notes,
        manifest_path: done.manifest_path,
    })
}

fn text(e: anyhow::Error) -> String {
    format!("{e:#}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testing::{repo, write};

    fn wait(rx: &Receiver<Msg>) -> Msg {
        rx.recv_timeout(Duration::from_secs(60))
            .expect("worker reports")
    }

    #[test]
    fn workers_report_scan_and_conflict_results() {
        let dir = repo(&[("harness.toml", ""), ("a.txt", "x\n")]);
        let key = ["AKIA", "Z7VRSQ5TJS2GVAXR"].concat();
        write(dir.path(), &[("a.txt", &format!("x\n{key}\n"))]);
        let (tx, rx) = mpsc::channel();
        spawn(dir.path().to_path_buf(), Job::Scan, tx.clone());
        match wait(&rx) {
            Msg::Findings(Ok(f)) => assert_eq!(f.secrets.len(), 1),
            other => panic!("unexpected {other:?}"),
        }
        spawn(dir.path().to_path_buf(), Job::Conflicts { fix: false }, tx);
        match wait(&rx) {
            Msg::Conflicts(Err(e)) => assert!(e.contains("fix:"), "{e}"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_run_without_commands_completes_with_a_manifest() {
        let dir = repo(&[("harness.toml", "")]);
        let (tx, rx) = mpsc::channel();
        spawn(dir.path().to_path_buf(), Job::Run { agent: None }, tx);
        match wait(&rx) {
            Msg::RunDone(r) => {
                let r = r.expect("run completes");
                assert!(r.notes.iter().any(|n| n.contains("sandbox skipped")));
                assert!(r.manifest_path.is_file());
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn keys_translate() {
        assert_eq!(translate(KeyCode::Char('q')), Key::Char('q'));
        assert_eq!(translate(KeyCode::BackTab), Key::BackTab);
        assert_eq!(translate(KeyCode::F(1)), Key::Other);
    }
}
