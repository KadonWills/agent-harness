# RFC-0002: Local terminal and web UIs

- Status: Accepted
- Author(s): @KadonWills (agent-assisted, see PR)
- Tracking issue: —

## Summary

Add two optional front ends over the existing commands: `harness tui`, a
terminal UI, and `harness ui`, a web UI served on localhost. Both are
views over the same library functions the CLI uses, so they cannot reach a
different verdict than `harness run`. Supporting changes:

- `harness-cli` becomes a library plus a binary.
- `SandboxBackend` gains a provided method that reports step progress.
- `harness run` keeps a bounded history of manifests under `.harness/`.

## Motivation

Everything harness knows is already printed as text or JSON. But a person
supervising several agents wants to see it together: whether the repo is
ready, what the last runs decided and why, which findings blocked them,
which agent branches collide, and what the config allows. Today that means
re-running four commands and reading `.harness/AgentManifest.json` by hand,
and that file is overwritten by every run.

Two audiences:

- **Developers at a terminal** want a dashboard that needs no browser and
  works over SSH.
- **Leads reviewing agent output** want a richer view: side-by-side
  structural changes, history, and conflict lists.

## Design

### 1. Principle: views, not deciders

The UIs call the same functions as the CLI commands (`check::inspect`,
`scan::inspect`, `conflicts::analyze`, `run::execute`) and display their
serializable results. They never compute a verdict of their own.
ADR-0004 (deterministic core) is unaffected: the UIs add no model calls
and no new inputs to the gate.

### 2. `harness-cli` becomes library + binary

`crates/harness-cli/src/lib.rs` exposes the command modules. `main.rs`
shrinks to argument parsing and exit-code mapping. The library API is
**internal**: it exists so the `tui` and `web` modules share one code path
with the CLI, and it is not a semver contract. Published contracts remain
the CLI surface and the manifest.

### 3. `SandboxBackend` progress (additive)

```rust
pub enum Progress<'a> {
    StepStarted { phase: Phase, command: &'a str },
    StepFinished { outcome: &'a StepOutcome },
}

pub trait SandboxBackend: Send + Sync {
    // existing: name, check_available, execute
    /// Like `execute`, reporting each step as it starts and finishes.
    fn execute_observed(
        &self,
        plan: &ExecutionPlan,
        observer: &mut dyn FnMut(Progress<'_>),
    ) -> Result<SessionReport, Error> {
        // default: run `execute`, then replay its steps to the observer
    }
}
```

The provided default keeps every existing implementation (including the
out-of-tree cloud backend) compiling and correct. It only loses liveness.
The Docker and Wasmtime backends override it to report each step as it
happens, and implement `execute` as `execute_observed` with a no-op
observer.

### 4. Run history (`.harness/history/`)

Every `harness run` also writes its manifest to
`.harness/history/<session_id>.json`.

- **Retention:** the 50 newest are kept and older ones are deleted. Session
  ids begin with the hex Unix time, so name order is chronological.
- **Failure handling:** if a history write fails, the run gets a note, not
  an error. History is a convenience and must never change a verdict or an
  exit code.
- **Content:** no new manifest fields. History files are ordinary
  `AgentManifest.json` documents.
- **Not configurable** in this RFC.

### 5. CLI surface (additive)

| Command | Feature | Behaviour |
|---|---|---|
| `harness tui` | `tui` | Full-screen terminal UI for the repository. |
| `harness ui [--port N] [--open]` | `web` | Serves the web UI on `127.0.0.1:N` (0 = pick a free port, the default) and prints the URL. `--open` launches the browser. |

Both subcommands exist in every build. In a build without the feature they
exit 2 with `fix: rebuild with --features tui` (or `web`), mirroring how
`sandbox.provider = "wasmtime"` behaves without the `wasmtime` feature.
Exit codes:

- 0 on a normal quit.
- 2 if the UI cannot start.

The UIs never exit 1. Blocking is the job of `run` and `scan`.

Neither feature is in `default`. Release binaries should enable both (see
Unresolved questions).

### 6. Screens (both UIs)

| Screen | Source | Actions |
|---|---|---|
| Status | `check::inspect` | Re-check |
| Run | `run::execute` with an observer | Start a run (agent id optional); live steps |
| History | `.harness/history/*.json` | Open a manifest; see its blocking reasons, steps, findings, `ast_changes` |
| Findings | `scan::inspect` (working tree vs base) | Re-scan |
| Conflicts | `conflicts::analyze` over `[conflicts].branches` | Apply Level 1 fixes to the checked-out branch |
| Config | `harness.toml` + `validate()` | TUI: open `$EDITOR`, then re-validate. Web: edit in a text area, save only when valid |

### 7. Web UI security model

The server can start sandbox runs and rewrite `harness.toml`, so it is
treated as a privileged local endpoint.

- **Loopback only.** It binds `127.0.0.1` and there is no flag to change
  that. Remote use goes through SSH port forwarding.
- **Token.** A 256-bit random token is generated per process from the OS
  RNG. The printed URL carries it once (`/?token=…`). The server swaps it
  for an `HttpOnly; SameSite=Strict` cookie and redirects to a URL without
  the token. Any request without a valid cookie or
  `Authorization: Bearer <token>` gets 401.
- **DNS rebinding.** Requests whose `Host` is not `127.0.0.1:<port>` or
  `localhost:<port>` get 403.
- **CSRF.** State-changing requests are `POST` only. They must carry
  `X-Harness-Token: <token>` (a custom header, which forces a CORS
  preflight that the server never approves). If an `Origin` header is
  present, it must match the server's own origin.
- **Loosening config needs explicit confirmation.** A save that loosens
  security is refused unless the request sets `confirm_loosen`, and the
  response lists each loosened setting. Loosening means any of:
  - turning on `sandbox.allow_egress` or `sandbox.allow_setup_egress`;
  - turning off `security.scan_secrets`,
    `security.dependencies.typosquat_check` or `agents.strict_mode`;
  - emptying a non-empty `agents.allowed`.
- **Valid configs only.** A config that fails to parse or `validate()`
  is never written.
- **No third-party assets.** HTML is rendered on the server with escaping.
  It includes one small inline script for `fetch` and polling. No CDN, no
  frontend build step, and the UI works offline.
- **One run at a time.** A second start returns 409. Live progress is
  delivered by polling `GET /api/run` every 500 ms. That avoids a streaming
  dependency and is plenty for step-level updates.

The TUI has no network surface. It shows the same loosening warnings after
`$EDITOR` closes, but cannot stop a save, because the file is already
written. `harness run` refuses invalid configs as before.

### 8. Scope boundary with the commercial cloud

PRD §5 lists "review UI" and "org dashboards" as non-goals of this
repository. This RFC narrows the first and keeps the second:

- **In scope:** UIs for *one repository on one machine*, launched by the
  person who owns the checkout, with no accounts and no state beyond
  `.harness/`.
- **Out of scope:** anything spanning many repositories, many users,
  remote hosts or persistent servers. That remains the cloud.

The open-source UI is deliberately not a review tool: it has no comments,
no approvals and no merging.

### 9. Dependencies

All are MIT or MIT/Apache-2.0, as allowed by `deny.toml`, and none is
compiled unless its feature is on.

- `tui`: `ratatui`, `crossterm`.
- `web`: `axum`, `tokio` (`rt-multi-thread`, `net`, `macros`), `getrandom`.

The MSRV policy from RFC-0001 §8 applies: default features build on 1.90.
`tui` and `web` must also build on 1.90 unless a dependency forces a bump,
in which case they become exempt like `wasmtime`.

## Alternatives considered

- **A React/Vite single-page app embedded in the binary.** It would give
  the richest UI, but adds a Node toolchain to the contributor setup and
  to CI, and an npm supply chain to a security product. Rejected for now.
  The JSON endpoints in §7 allow one later without changing the server.
- **Server-sent events or WebSockets for live runs.** They would need a
  streaming dependency (`tokio-stream`/`futures`) and more server code.
  Step-level progress at 500 ms polling is indistinguishable in practice.
- **Separate `harness-tui` and `harness-web` crates.** That is cleaner, but
  each needs a new commit scope and a crates.io name, and the crate-naming
  question (ADR-0005) is still open. Feature-gated modules in
  `harness-cli` can be split later without user-visible change.
- **Binding to `0.0.0.0` behind a flag.** Rejected. A one-flag mistake
  would expose run and config-write endpoints to the LAN. Use SSH port
  forwarding instead.

## Compatibility

- `harness.toml`: unchanged.
- `AgentManifest.json`: unchanged. History files use the same schema.
- `SandboxBackend`: additive provided method. Existing implementations
  compile unchanged.
- `LanguageParser`: unchanged.
- CLI: two new subcommands. Existing commands, flags and exit codes are
  unchanged. `harness run` now also writes `.harness/history/`, which
  `harness init` already ignores via `.harness/` in `.gitignore`.

## Unresolved questions

- Should release binaries enable `tui` and `web` by default? Proposed
  answer: yes for GitHub Release binaries, no for `cargo install` unless
  asked, which keeps tokio out of the default build.
- History retention is fixed at 50. Make it configurable only if someone
  asks.
