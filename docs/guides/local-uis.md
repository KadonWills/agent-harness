# Terminal and web UIs

**English** | [Français](../fr/guides/local-uis.md)

`harness tui` and `harness ui` show everything the CLI knows about one
repository on one screen:

- readiness;
- live and past runs;
- secret and dependency findings;
- conflicts between agent branches;
- the config.

Both call the same functions as `harness check`, `run`, `scan` and
`conflicts`, so they can never reach a different verdict than the CLI.
The design is in [RFC-0002](../rfcs/0002-local-uis.md).

## Install

Both are opt-in cargo features, so the default build stays small:

```bash
cargo install --locked --git https://github.com/KadonWills/agent-harness harness-cli \
  --features tui,web
```

In a build without a feature, the matching command exits 2 and prints a
`fix:` line telling you to rebuild with that feature.

## Screens

| Screen | Same as | Actions |
|---|---|---|
| Status | `harness check` | refresh |
| Run | `harness run` | start a run on the working tree; steps appear as they finish |
| History | `.harness/history/*.json` | open any of the last 50 manifests: reasons, steps, `ast_changes`, findings |
| Findings | `harness scan` | refresh |
| Conflicts | `harness conflicts` | apply Level 1 fixes to the checked-out branch |
| Config | `harness.toml` + validation | edit and re-validate |

Every `harness run` writes `.harness/history/<session>.json`. This
includes runs from the CLI, from git hooks and from the UIs, so History
shows them all. Only the newest 50 are kept.

## Terminal UI

```bash
harness tui            # or: harness -C path/to/repo tui
```

| Key | Action |
|---|---|
| `1`–`6`, `Tab`, `Shift-Tab` | switch screens |
| `r` | refresh the current screen |
| `s` | (Run) start a run; the agent id comes from `HARNESS_AGENT` |
| `↑` `↓`, `j` `k` | (History) select a run; elsewhere, scroll |
| `PgUp` `PgDn` | scroll |
| `f` | (Conflicts) apply Level 1 fixes |
| `e` | (Config) open `harness.toml` in `$VISUAL`/`$EDITOR`, then re-validate |
| `q`, `Esc` | quit |

After an edit, the Config screen lists every setting the edit loosened,
such as `sandbox.allow_egress turned on`.

## Web UI

```bash
harness ui --open      # picks a free port and opens the browser
harness ui --port 7800 # fixed port; prints the URL instead
```

The server can start sandbox runs and rewrite `harness.toml`, so it is
locked down:

- **Loopback only.** It binds `127.0.0.1` and there is no option to bind
  anything else. From another machine, use
  `ssh -L 7800:127.0.0.1:7800 host`.
- **The printed URL carries a one-time token.** Opening it sets an
  `HttpOnly`, `SameSite=Strict` cookie and removes the token from the
  address bar. Scripts can send `Authorization: Bearer <token>` instead.
  Restarting the server issues a new token.
- **Requests for any other `Host` are refused**, which blocks DNS
  rebinding.
- **Changes need a custom header.** Every request that changes something
  must carry an `X-Harness-Token` header, and a matching `Origin` if one
  is sent. Other websites cannot forge either.
- **Config saves are checked.** A save happens only if the text parses and
  validates. An edit that loosens the gate shows what it loosens and needs
  a second, explicit "Save anyway".
- **Nothing is loaded from the internet.** No CDN, no fonts, no analytics.

### JSON API

The pages are built on the same JSON the CLI's `--json` flags produce, so
scripts can use the API directly:

| Method | Path | Body / result |
|---|---|---|
| GET | `/api/status` | `harness check --json` |
| GET | `/api/findings` | `harness scan --json` |
| GET | `/api/conflicts` | `harness conflicts --json` |
| POST | `/api/conflicts/fix` | same, after applying Level 1 fixes |
| GET | `/api/history` | manifests, newest first |
| GET | `/api/history/{session_id}` | one `AgentManifest.json` |
| GET | `/api/run` | live state of the run started from the UI |
| POST | `/api/run` | `{"agent": "id"}` → 202; 409 if a run is in progress |
| GET | `/api/config` | `{path, text, error, problems}` |
| POST | `/api/config` | `{"text": "...", "confirm_loosen": false}` → 200 saved, 409 needs confirmation, 422 invalid |

The UIs never exit 1. Blocking is the job of `harness run` and
`harness scan`. They exit 0 when you quit, and 2 if they cannot start.
