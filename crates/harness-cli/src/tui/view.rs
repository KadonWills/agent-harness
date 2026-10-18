//! Rendering. Pure function of [`App`]; tested against ratatui's TestBackend.

use harness_core::manifest::{AgentManifest, Verdict};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Tabs, Wrap};
use ratatui::Frame;

use super::app::{App, Load, Tab};

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(2),
    ])
    .areas(frame.area());

    let titles = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, t)| format!("{} {}", i + 1, t.title()));
    let selected = Tab::ALL.iter().position(|t| *t == app.tab).unwrap_or(0);
    frame.render_widget(
        Tabs::new(titles)
            .select(selected)
            .highlight_style(Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::bordered().title(format!(" harness · {} ", app.root.display()))),
        header,
    );

    match app.tab {
        Tab::Status => status(frame, body, app),
        Tab::Run => run(frame, body, app),
        Tab::History => history(frame, body, app),
        Tab::Findings => findings(frame, body, app),
        Tab::Conflicts => conflicts(frame, body, app),
        Tab::Config => config(frame, body, app),
    }

    let keys = match app.tab {
        Tab::Run => "s start run",
        Tab::Conflicts => "r re-scan · f apply Level 1 fixes",
        Tab::Config => "e edit in $EDITOR · r reload",
        Tab::History => "↑↓ select · PgUp/PgDn scroll · r reload",
        _ => "r refresh",
    };
    let help = format!("{keys} · Tab/1-6 switch · q quit");
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(app.message.clone()).yellow(),
            Line::from(help).dark_gray(),
        ]),
        footer,
    );
}

fn pane<'a>(title: &str, lines: Vec<Line<'a>>, scroll: u16) -> Paragraph<'a> {
    Paragraph::new(lines)
        .block(Block::bordered().title(format!(" {title} ")))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
}

fn loading<T>(load: &Load<T>) -> Option<Vec<Line<'static>>> {
    match load {
        Load::NotLoaded => Some(vec![Line::from("press r to load")]),
        Load::Loading => Some(vec![Line::from("working…").dark_gray()]),
        Load::Failed(e) => Some(e.lines().map(|l| Line::from(l.to_string()).red()).collect()),
        Load::Ready(_) => None,
    }
}

fn ok_line(text: impl Into<String>) -> Line<'static> {
    Line::from(vec![Span::from("ok   ").green(), Span::from(text.into())])
}

fn fail_line(tag: &str, text: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::from(format!("{tag:<5}")).red().bold(),
        Span::from(text.into()),
    ])
}

fn status(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let lines = loading(&app.status).unwrap_or_else(|| {
        let Load::Ready(report) = &app.status else {
            return Vec::new();
        };
        let mut lines = Vec::new();
        for p in &report.problems {
            lines.push(fail_line("FAIL", format!("{}: {}", p.field, p.message)));
            lines.push(Line::from(format!("     fix: {}", p.fix)).dark_gray());
        }
        for w in &report.warnings {
            lines.push(Line::from(vec![
                Span::from("warn ").yellow(),
                Span::from(w.clone()),
            ]));
        }
        lines.push(if report.ok {
            ok_line("harness is ready")
        } else {
            fail_line("", format!("{} problem(s) found", report.problems.len()))
        });
        lines
    });
    frame.render_widget(pane("Status · harness check", lines, app.scroll), area);
}

fn run(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let r = &app.run;
    let mut lines = Vec::new();
    if !r.running && r.result.is_none() && r.steps.is_empty() {
        lines.push(Line::from(
            "press s to run the gate on the working tree (same as `harness run`)",
        ));
    }
    for (phase, command, done) in &r.steps {
        let phase = format!("{phase:?}").to_lowercase();
        lines.push(match done {
            None => Line::from(format!("…    {phase:<8} {command}")).cyan(),
            Some(s) if s.exit_code == 0 && !s.timed_out => ok_line(format!(
                "{phase:<8} {command} ({:.1}s)",
                s.duration_ms as f64 / 1000.0
            )),
            Some(s) => fail_line(
                if s.timed_out { "TIME" } else { "FAIL" },
                format!("{phase:<8} {command} (exit {})", s.exit_code),
            ),
        });
    }
    if !r.failure_output.is_empty() {
        let out: Vec<&str> = r.failure_output.lines().collect();
        for l in &out[out.len().saturating_sub(30)..] {
            lines.push(Line::from(format!("       | {l}")).dark_gray());
        }
    }
    if r.running {
        lines.push(Line::from("running…").dark_gray());
    }
    match &r.result {
        Some(Ok(done)) => {
            for n in &done.notes {
                lines.push(Line::from(format!("note: {n}")).dark_gray());
            }
            lines.extend(verdict(&done.manifest));
            lines.push(Line::from(format!(
                "manifest: {}",
                done.manifest_path.display()
            )));
        }
        Some(Err(e)) => lines.extend(e.lines().map(|l| Line::from(l.to_string()).red())),
        None => {}
    }
    frame.render_widget(pane("Run · harness run", lines, app.scroll), area);
}

fn verdict(m: &AgentManifest) -> Vec<Line<'static>> {
    let mut lines = vec![match m.verdict {
        Verdict::Pass => Line::from("PASS").green().bold(),
        Verdict::Blocked => Line::from("BLOCKED").red().bold(),
        Verdict::Unknown => Line::from("UNKNOWN").yellow().bold(),
    }];
    for reason in &m.blocking_reasons {
        lines.push(Line::from(format!("  - {reason}")));
    }
    lines
}

/// Everything a manifest says, for the History detail pane.
pub fn manifest_lines(m: &AgentManifest) -> Vec<Line<'static>> {
    let mut lines = verdict(m);
    let head = m.head.get(..7).unwrap_or("(none)");
    lines.push(Line::from(format!(
        "agent {} · base {} · head {head}",
        m.target_agent, m.base_ref
    )));
    lines.push(Line::from(format!("session {}", m.session_id)).dark_gray());
    if !m.steps.is_empty() {
        lines.push(Line::from("steps").bold());
        for s in &m.steps {
            let text = format!(
                "{:<8} {} (exit {}, {:.1}s)",
                format!("{:?}", s.phase).to_lowercase(),
                s.command,
                s.exit_code,
                s.duration_ms as f64 / 1000.0
            );
            lines.push(if s.exit_code == 0 && !s.timed_out {
                ok_line(text)
            } else {
                fail_line("FAIL", text)
            });
        }
    }
    if !m.impacted_files.is_empty() {
        lines.push(Line::from(format!("impacted files ({})", m.impacted_files.len())).bold());
        for f in &m.impacted_files {
            lines.push(Line::from(format!("  {}", f.path)));
            for c in &f.ast_changes {
                lines.push(Line::from(format!("      {c}")).dark_gray());
            }
        }
    }
    let audit = &m.security_audit;
    for s in &audit.secret_findings {
        lines.push(fail_line(
            "SECRET",
            format!("{}:{} {}", s.path, s.line, s.rule_id),
        ));
    }
    for d in &audit.dependency_findings {
        lines.push(fail_line(
            "DEP",
            format!("{} {}: {}", d.ecosystem, d.name, d.reason),
        ));
    }
    for e in &audit.egress_events {
        lines.push(fail_line(
            "NET",
            format!("{} during `{}`", e.destination, e.step),
        ));
    }
    for c in &m.conflicts {
        lines.push(fail_line(
            &format!("L{}", c.level),
            format!("{} `{}`: {}", c.kind, c.symbol, c.explanation),
        ));
    }
    lines
}

fn history(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let [list_area, detail] =
        Layout::horizontal([Constraint::Length(36), Constraint::Min(20)]).areas(area);
    let items: Vec<ListItem<'_>> = app
        .history
        .iter()
        .map(|e| {
            let m = &e.manifest;
            let tag = match m.verdict {
                Verdict::Pass => Span::from("PASS ").green(),
                Verdict::Blocked => Span::from("BLOCK").red(),
                Verdict::Unknown => Span::from("?    ").yellow(),
            };
            ListItem::new(Line::from(vec![
                tag,
                Span::from(format!(
                    " {} {}",
                    m.target_agent,
                    m.head.get(..7).unwrap_or("")
                )),
            ]))
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(app.selected));
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::bordered().title(format!(" Runs ({}) ", app.history.len())))
            .highlight_style(Style::new().reversed()),
        list_area,
        &mut state,
    );
    let lines = app.selected_manifest().map_or_else(
        || {
            vec![Line::from(
                "no recorded runs yet; start one from the Run tab",
            )]
        },
        manifest_lines,
    );
    frame.render_widget(pane("Manifest", lines, app.scroll), detail);
}

fn findings(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let lines = loading(&app.findings).unwrap_or_else(|| {
        let Load::Ready(f) = &app.findings else {
            return Vec::new();
        };
        let mut lines = Vec::new();
        for s in &f.secrets {
            lines.push(fail_line(
                "SECRET",
                format!("{}:{} {} ({})", s.path, s.line, s.rule_id, s.description),
            ));
        }
        for d in &f.dependencies {
            lines.push(fail_line(
                "DEP",
                format!("{} {} in {}: {}", d.ecosystem, d.name, d.file, d.reason),
            ));
        }
        if lines.is_empty() {
            lines.push(ok_line("no secrets or policy-violating dependencies"));
        }
        lines
    });
    frame.render_widget(pane("Findings · harness scan", lines, app.scroll), area);
}

fn conflicts(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let lines = loading(&app.conflicts).unwrap_or_else(|| {
        let Load::Ready(a) = &app.conflicts else {
            return Vec::new();
        };
        let mut lines = vec![Line::from(format!("branches: {}", a.branches.join(" · "))).bold()];
        for f in &a.fixed {
            lines.push(ok_line(format!("fixed {f}")));
        }
        for c in &a.conflicts {
            lines.push(fail_line(
                &format!("L{}", c.level),
                format!(
                    "{} `{}` [{} -> {}]",
                    c.kind, c.symbol, c.branches[0], c.branches[1]
                ),
            ));
            lines.push(Line::from(format!("     {}", c.explanation)));
            if let Some(fix) = &c.fix {
                lines.push(Line::from(format!("     fix: {fix}")).dark_gray());
            }
        }
        if a.conflicts.is_empty() {
            lines.push(ok_line("no semantic conflicts"));
        }
        for n in &a.notes {
            lines.push(Line::from(format!("note: {n}")).dark_gray());
        }
        lines
    });
    frame.render_widget(
        pane("Conflicts · harness conflicts", lines, app.scroll),
        area,
    );
}

fn config(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let c = &app.config;
    let mut lines: Vec<Line<'_>> = c
        .text
        .lines()
        .enumerate()
        .map(|(i, l)| Line::from(format!("{:>3} {l}", i + 1)))
        .collect();
    lines.push(Line::from(""));
    if let Some(e) = &c.error {
        lines.extend(e.lines().map(|l| Line::from(l.to_string()).red()));
    }
    for p in &c.problems {
        lines.push(fail_line("FAIL", format!("{}: {}", p.field, p.message)));
        lines.push(Line::from(format!("     fix: {}", p.fix)).dark_gray());
    }
    if c.error.is_none() && c.problems.is_empty() {
        lines.push(ok_line("valid"));
    }
    for l in &c.loosened {
        lines.push(Line::from(vec![
            Span::from("LOOSENED ").yellow().bold(),
            Span::from(l.clone()),
        ]));
    }
    frame.render_widget(
        pane(&format!("Config · {}", c.path.display()), lines, app.scroll),
        area,
    );
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use super::*;
    use crate::check::Report;
    use crate::git::testing::repo;
    use crate::tui::app::{Key, Msg};

    fn screen(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(110, 30)).expect("terminal");
        terminal.draw(|f| draw(f, app)).expect("draw");
        let buffer = terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn every_tab_renders() {
        let dir = repo(&[("harness.toml", "[sandbox]\ntimeout = \"x\"\n")]);
        let mut app = App::new(dir.path()).expect("app");
        app.apply(Msg::Status(Report {
            ok: false,
            problems: vec![harness_core::config::Problem {
                field: "sandbox.timeout".into(),
                message: "bad".into(),
                fix: "use 10m".into(),
            }],
            warnings: vec![],
        }));
        let s = screen(&app);
        assert!(s.contains("1 Status"), "{s}");
        assert!(s.contains("fix: use 10m"), "{s}");

        let expectations = [
            ('2', "press s to run"),
            ('3', "no recorded runs yet"),
            ('4', "working"),
            ('5', "working"),
            ('6', "sandbox.timeout"),
        ];
        for (key, text) in expectations {
            app.on_key(Key::Char(key));
            let s = screen(&app);
            assert!(s.contains(text), "tab {key}: {s}");
        }
    }

    #[test]
    fn manifest_detail_lists_reasons_and_findings() {
        let mut m = AgentManifest::new("s", "claude-code");
        m.block("`npm test` exited with code 1");
        m.security_audit
            .secret_findings
            .push(harness_core::manifest::SecretFinding {
                rule_id: "aws-access-key".into(),
                description: "AWS".into(),
                path: "a.ts".into(),
                line: 3,
            });
        m.finalize();
        let text: Vec<String> = manifest_lines(&m).iter().map(ToString::to_string).collect();
        assert_eq!(text[0], "BLOCKED");
        assert!(text.iter().any(|l| l.contains("npm test")));
        assert!(text.iter().any(|l| l.contains("a.ts:3 aws-access-key")));
    }
}
