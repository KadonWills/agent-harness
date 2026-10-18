//! `harness ui` — a local web UI over the same functions as the CLI
//! (RFC-0002). Loopback only, token-authenticated, with Host and CSRF
//! checks, because it can start sandbox runs and rewrite `harness.toml`.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{Context, Result};
use axum::extract::{Path as UrlPath, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use harness_core::config::HarnessConfig;
use harness_core::manifest::{Phase, StepReport, Verdict};
use harness_runtime::Progress;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::git::Git;
use crate::{check, conflicts, history, loosen, run, scan, settings, Outcome};

pub mod html;

const COOKIE: &str = "harness_token";

struct AppState {
    root: PathBuf,
    token: String,
    port: u16,
    run: Mutex<RunState>,
}

impl AppState {
    fn run(&self) -> MutexGuard<'_, RunState> {
        // A panicking run thread must not take the UI down with it.
        self.run.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The live state of the run started from this UI (`GET /api/run`).
#[derive(Debug, Clone, Default, Serialize)]
struct RunState {
    running: bool,
    steps: Vec<StepView>,
    /// Output of the last failed step, last 30 lines.
    output_tail: String,
    verdict: Option<Verdict>,
    blocking_reasons: Vec<String>,
    notes: Vec<String>,
    session_id: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct StepView {
    phase: Phase,
    command: String,
    done: Option<StepReport>,
}

/// A running server; dropped or [`Server::stop`]ped to shut down.
pub struct Server {
    pub port: u16,
    pub token: String,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<Result<()>>>,
}

impl Server {
    /// Bind `127.0.0.1:port` (0 = any free port) for the repository
    /// containing `dir`, and serve on a background thread.
    pub fn start(dir: &Path, port: u16) -> Result<Self> {
        let root = Git::discover(dir)?.root().to_path_buf();
        let listener = std::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
            .with_context(|| {
            format!(
                "cannot listen on 127.0.0.1:{port}\nfix: pick another port with --port, \
                     or omit it to use a free one"
            )
        })?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let token = new_token()?;
        let state = Arc::new(AppState {
            root,
            token: token.clone(),
            port,
            run: Mutex::new(RunState::default()),
        });
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let thread = std::thread::spawn(move || -> Result<()> {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()?;
            rt.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener)?;
                axum::serve(listener, router(state))
                    .with_graceful_shutdown(async {
                        let _ = stopped.await;
                    })
                    .await?;
                Ok(())
            })
        });
        Ok(Self {
            port,
            token,
            stop: Some(stop),
            thread: Some(thread),
        })
    }

    /// The one-time login URL.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/?token={}", self.port, self.token)
    }

    pub fn stop(mut self) -> Result<()> {
        self.shutdown()
    }

    fn shutdown(&mut self) -> Result<()> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        match self.thread.take().map(std::thread::JoinHandle::join) {
            Some(Ok(result)) => result,
            Some(Err(_)) => anyhow::bail!("the web server thread panicked"),
            None => Ok(()),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

/// `harness ui`: serve until Ctrl-C.
pub fn serve(dir: &Path, port: u16, open: bool) -> Result<Outcome> {
    let server = Server::start(dir, port)?;
    let url = server.url();
    eprintln!("harness ui: {url}");
    eprintln!("  loopback only; the token in this URL is the only credential. Ctrl-C stops.");
    if open {
        open_browser(&url);
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(tokio::signal::ctrl_c())
        .context("cannot wait for Ctrl-C")?;
    server.stop()?;
    Ok(Outcome::Pass)
}

fn open_browser(url: &str) {
    let (program, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("open", &[])
    } else if cfg!(windows) {
        ("cmd", &["/C", "start", ""])
    } else {
        ("xdg-open", &[])
    };
    if std::process::Command::new(program)
        .args(args)
        .arg(url)
        .spawn()
        .is_err()
    {
        eprintln!("  could not open a browser; open the URL above yourself");
    }
}

fn new_token() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("no OS randomness: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(status_page))
        .route("/run", get(run_page))
        .route("/history", get(history_page))
        .route("/history/{id}", get(manifest_page))
        .route("/findings", get(findings_page))
        .route("/conflicts", get(conflicts_page))
        .route("/config", get(config_page))
        .route("/api/status", get(api_status))
        .route("/api/run", get(api_run_state).post(api_run_start))
        .route("/api/history", get(api_history))
        .route("/api/history/{id}", get(api_manifest))
        .route("/api/findings", get(api_findings))
        .route("/api/conflicts", get(api_conflicts))
        .route("/api/conflicts/fix", post(api_conflicts_fix))
        .route("/api/config", get(api_config).post(api_config_save))
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state)
}

/// Equal-time comparison, so response timing does not leak the token.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn header_str(headers: &HeaderMap, name: header::HeaderName) -> Option<&str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

fn deny(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

/// Host check, token check, and CSRF check for every request (RFC-0002 §7).
async fn guard(State(s): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    let headers = req.headers();
    let host = header_str(headers, header::HOST).unwrap_or_default();
    let allowed_hosts = [
        format!("127.0.0.1:{}", s.port),
        format!("localhost:{}", s.port),
    ];
    if !allowed_hosts.iter().any(|h| h == host) {
        return deny(
            StatusCode::FORBIDDEN,
            "unexpected Host header; fix: open the URL printed by `harness ui`",
        );
    }

    // First visit: trade the URL token for a cookie, then drop it from the URL.
    if req.uri().path() == "/" {
        if let Some(t) = req.uri().query().and_then(|q| {
            q.split('&')
                .find_map(|kv| kv.strip_prefix("token="))
                .map(str::to_string)
        }) {
            if !same(&t, &s.token) {
                return deny(StatusCode::UNAUTHORIZED, "wrong token");
            }
            let cookie = format!("{COOKIE}={}; HttpOnly; SameSite=Strict; Path=/", s.token);
            return ([(header::SET_COOKIE, cookie)], Redirect::to("/")).into_response();
        }
    }

    let from_cookie = header_str(headers, header::COOKIE).and_then(|c| {
        c.split(';')
            .find_map(|kv| kv.trim().strip_prefix(&format!("{COOKIE}=")))
    });
    let from_bearer =
        header_str(headers, header::AUTHORIZATION).and_then(|a| a.strip_prefix("Bearer "));
    let authed = [from_cookie, from_bearer]
        .into_iter()
        .flatten()
        .any(|t| same(t, &s.token));
    if !authed {
        return deny(
            StatusCode::UNAUTHORIZED,
            "not signed in; fix: open the URL printed by `harness ui` (it carries the token)",
        );
    }

    if req.method() != axum::http::Method::GET && req.method() != axum::http::Method::HEAD {
        let csrf = req
            .headers()
            .get("x-harness-token")
            .and_then(|v| v.to_str().ok());
        if !csrf.is_some_and(|t| same(t, &s.token)) {
            return deny(StatusCode::FORBIDDEN, "missing X-Harness-Token header");
        }
        if let Some(origin) = header_str(req.headers(), header::ORIGIN) {
            if origin != format!("http://{host}") {
                return deny(StatusCode::FORBIDDEN, "cross-origin request refused");
            }
        }
    }
    next.run(req).await
}

/// Run blocking repository work off the async workers.
async fn blocking<T: Send + 'static>(
    s: &Arc<AppState>,
    f: impl FnOnce(&Path) -> T + Send + 'static,
) -> T {
    let root = s.root.clone();
    match tokio::task::spawn_blocking(move || f(&root)).await {
        Ok(v) => v,
        Err(e) => std::panic::resume_unwind(e.into_panic()),
    }
}

fn text(e: anyhow::Error) -> String {
    format!("{e:#}")
}

fn page(s: &AppState, active: &str, body: &str) -> Html<String> {
    Html(html::layout(
        active,
        &s.token,
        &s.root.display().to_string(),
        body,
    ))
}

async fn status_page(State(s): State<Arc<AppState>>) -> Html<String> {
    let report = blocking(&s, check::status).await;
    page(&s, "/", &html::status(&report))
}

async fn run_page(State(s): State<Arc<AppState>>) -> Html<String> {
    page(&s, "/run", &html::run())
}

async fn history_page(State(s): State<Arc<AppState>>) -> Html<String> {
    let body = match blocking(&s, history::list).await {
        Ok(entries) => html::history(&entries),
        Err(e) => html::error("History", &text(e)),
    };
    page(&s, "/history", &body)
}

async fn manifest_page(State(s): State<Arc<AppState>>, UrlPath(id): UrlPath<String>) -> Response {
    match blocking(&s, move |root| history::load(root, &id)).await {
        Ok(m) => page(&s, "/history", &html::manifest(&m)).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            page(&s, "/history", &html::error("Run not found", &text(e))),
        )
            .into_response(),
    }
}

async fn findings_page(State(s): State<Arc<AppState>>) -> Html<String> {
    let body = match blocking(&s, |root| scan::analyze(root, false, None)).await {
        Ok(f) => html::findings(&f),
        Err(e) => html::error("Findings", &text(e)),
    };
    page(&s, "/findings", &body)
}

async fn conflicts_page(State(s): State<Arc<AppState>>) -> Html<String> {
    let body = match blocking(&s, |root| conflicts::analyze(root, None, &[], false)).await {
        Ok(a) => html::conflicts(&a),
        Err(e) => html::error("Conflicts", &text(e)),
    };
    page(&s, "/conflicts", &body)
}

struct ConfigFile {
    path: PathBuf,
    text: String,
    error: Option<String>,
    problems: Vec<harness_core::config::Problem>,
}

fn read_config(root: &Path) -> ConfigFile {
    let path = settings::config_path(root);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let (error, problems) = if !path.is_file() {
        (
            Some("no harness.toml; fix: run `harness init`".to_string()),
            Vec::new(),
        )
    } else {
        match HarnessConfig::from_toml(&text) {
            Ok(c) => (None, c.validate()),
            Err(e) => (Some(e.to_string()), Vec::new()),
        }
    };
    ConfigFile {
        path,
        text,
        error,
        problems,
    }
}

async fn config_page(State(s): State<Arc<AppState>>) -> Html<String> {
    let c = blocking(&s, read_config).await;
    let body = html::config(
        &c.path.display().to_string(),
        &c.text,
        c.error.as_deref(),
        &c.problems,
    );
    page(&s, "/config", &body)
}

fn json_result<T: Serialize>(r: Result<T, String>) -> Response {
    match r {
        Ok(v) => Json(v).into_response(),
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": e })),
        )
            .into_response(),
    }
}

async fn api_status(State(s): State<Arc<AppState>>) -> Json<check::Report> {
    Json(blocking(&s, check::status).await)
}

async fn api_history(State(s): State<Arc<AppState>>) -> Response {
    let r = blocking(&s, history::list).await;
    json_result(
        r.map(|entries| entries.into_iter().map(|e| e.manifest).collect::<Vec<_>>())
            .map_err(text),
    )
}

async fn api_manifest(State(s): State<Arc<AppState>>, UrlPath(id): UrlPath<String>) -> Response {
    match blocking(&s, move |root| history::load(root, &id)).await {
        Ok(m) => Json(m).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(json!({ "error": text(e) }))).into_response(),
    }
}

async fn api_findings(State(s): State<Arc<AppState>>) -> Response {
    json_result(
        blocking(&s, |root| scan::analyze(root, false, None))
            .await
            .map_err(text),
    )
}

async fn api_conflicts(State(s): State<Arc<AppState>>) -> Response {
    json_result(
        blocking(&s, |root| conflicts::analyze(root, None, &[], false))
            .await
            .map_err(text),
    )
}

async fn api_conflicts_fix(State(s): State<Arc<AppState>>) -> Response {
    json_result(
        blocking(&s, |root| conflicts::analyze(root, None, &[], true))
            .await
            .map_err(text),
    )
}

async fn api_config(State(s): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let c = blocking(&s, read_config).await;
    Json(json!({
        "path": c.path,
        "text": c.text,
        "error": c.error,
        "problems": c.problems,
    }))
}

#[derive(Deserialize)]
struct ConfigSave {
    text: String,
    #[serde(default)]
    confirm_loosen: bool,
}

/// Save `harness.toml` only when it parses and validates; loosening needs
/// `confirm_loosen` (RFC-0002 §7).
async fn api_config_save(State(s): State<Arc<AppState>>, Json(body): Json<ConfigSave>) -> Response {
    blocking(&s, move |root| {
        let after = match HarnessConfig::from_toml(&body.text) {
            Ok(c) => c,
            Err(e) => {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({ "error": e.to_string() })),
                )
                    .into_response()
            }
        };
        let problems = after.validate();
        if !problems.is_empty() {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "problems": problems })),
            )
                .into_response();
        }
        let path = settings::config_path(root);
        let before = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| HarnessConfig::from_toml(&t).ok())
            .unwrap_or_default();
        let loosened = loosen::loosened(&before, &after);
        if !loosened.is_empty() && !body.confirm_loosen {
            return (
                StatusCode::CONFLICT,
                Json(json!({ "loosened": loosened, "needs_confirmation": true })),
            )
                .into_response();
        }
        match std::fs::write(&path, &body.text) {
            Ok(()) => Json(json!({ "saved": true, "loosened": loosened })).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("cannot write {}: {e}", path.display()) })),
            )
                .into_response(),
        }
    })
    .await
}

async fn api_run_state(State(s): State<Arc<AppState>>) -> Json<RunState> {
    Json(s.run().clone())
}

#[derive(Deserialize)]
struct RunStart {
    agent: Option<String>,
}

async fn api_run_start(State(s): State<Arc<AppState>>, body: Option<Json<RunStart>>) -> Response {
    {
        let mut state = s.run();
        if state.running {
            return deny(StatusCode::CONFLICT, "a run is already in progress");
        }
        *state = RunState {
            running: true,
            ..RunState::default()
        };
    }
    let agent = body.and_then(|Json(b)| b.agent).filter(|a| !a.is_empty());
    let worker = s.clone();
    std::thread::spawn(move || gate(&worker, agent));
    (StatusCode::ACCEPTED, Json(json!({ "started": true }))).into_response()
}

/// Run the gate, mirroring progress into the shared run state.
fn gate(s: &AppState, agent: Option<String>) {
    let result = run::backend(&s.root).and_then(|backend| {
        let args = run::Args {
            agent,
            ..run::Args::default()
        };
        run::execute(&s.root, &args, backend.as_ref(), &mut |p| {
            let mut state = s.run();
            match p {
                Progress::StepStarted { phase, command } => state.steps.push(StepView {
                    phase,
                    command: command.to_string(),
                    done: None,
                }),
                Progress::StepFinished { outcome } => {
                    if !outcome.succeeded() {
                        let output = format!("{}{}", outcome.stdout, outcome.stderr);
                        let lines: Vec<&str> = output.lines().collect();
                        state.output_tail = lines[lines.len().saturating_sub(30)..].join("\n");
                    }
                    if let Some(last) = state.steps.last_mut() {
                        last.done = Some(outcome.report.clone());
                    }
                }
            }
        })
    });
    let mut state = s.run();
    state.running = false;
    match result {
        Ok(done) => {
            state.verdict = Some(done.manifest.verdict);
            state.blocking_reasons = done.manifest.blocking_reasons;
            state.notes = done.notes;
            state.session_id = Some(done.manifest.session_id);
        }
        Err(e) => state.error = Some(text(e)),
    }
}

#[cfg(test)]
mod tests;
