# AgentHarness

[![CI](https://github.com/KadonWills/agent-harness/actions/workflows/ci.yml/badge.svg)](https://github.com/KadonWills/agent-harness/actions/workflows/ci.yml)
[![Security](https://github.com/KadonWills/agent-harness/actions/workflows/security.yml/badge.svg)](https://github.com/KadonWills/agent-harness/actions/workflows/security.yml)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/KadonWills/agent-harness/badge)](https://scorecard.dev/viewer/?uri=github.com/KadonWills/agent-harness)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

**Guardrails for autonomous coding agents.** AgentHarness sits between AI
coding agents (Claude Code, Cline, Cursor, Copilot Workspace, …) and your
git repository: it sandboxes and executes agent-generated changes, diffs
them structurally at the AST level, detects semantic conflicts between
concurrent agents, and blocks secrets and unvetted dependencies — all
**before** the code reaches human review.

> Status: **pre-alpha.** Milestones v0.1–v0.4 (gate, structural diff,
> multi-agent conflicts, security) are implemented per
> [RFC-0001](docs/rfcs/0001-v0.1-v0.4-contracts.md). Nothing is published
> yet. See [docs/ROADMAP.md](docs/ROADMAP.md).

## Why

Agent fleets write code faster than humans can vet it. Today the first
execution of an agent's change often happens in *your* review, or worse,
in CI after merge. AgentHarness makes "was this ever run, is it
structurally sane, and is it safe?" an automatic, deterministic gate.

## How it works

```
agent edits ──► harness run ──► sandbox (no network) ──► AST diff ──► gate
                                                                       │
                              AgentManifest.json ◄─────────── pass / block
```

- **Deterministic core** — the gate never calls an LLM; same diff, same verdict.
- **Zero-trust sandbox** — egress denied by default; attempts are audited.
- **Structural diffing** — Tree-sitter-based, language-neutral change events.
- **Multi-agent aware** — catches jointly-broken changes plain merge can't see.
- **Agent-legible** — config, errors, and reports designed for LLMs to self-correct against.

## Quick start

Nothing is published yet. `harness-cli` on crates.io is an unrelated tool,
and crate names are still undecided (see [ADR-0005](docs/adr/0005-crate-naming.md)).
Build from source:

```bash
cargo install --locked --git https://github.com/KadonWills/agent-harness harness-cli
# add `--features wasmtime` for the WASI sandbox backend,
# `--features tui,web` for the terminal and web UIs

cd your-repo
harness init --hooks   # writes harness.toml; pre-commit = scan, pre-push = run
harness check          # validates config + sandbox availability
harness run            # snapshot → sandbox → AST diff → conflicts → security → AgentManifest.json
```

| Command | What it does |
|---|---|
| `harness init [--force] [--hooks]` | Detects Node/Python/Rust/Go and writes `harness.toml` with your install and test commands |
| `harness check [--json]` | Reports config problems, a missing base ref, or an unavailable sandbox, each with a fix |
| `harness run [--agent] [--intent\|--intent-file] [--base] [--manifest] [--json]` | The full gate; writes `.harness/AgentManifest.json` |
| `harness scan [--staged\|--base] [--json]` | Secrets and new-dependency policy only (the pre-commit hook) |
| `harness conflicts [--base] [--fix] [--json] [BRANCH...]` | Semantic conflicts between agent branches; `--fix` applies Level 1 codemods |
| `harness tui` | Terminal UI over all of the above, plus run history (`--features tui`) |
| `harness ui [--port] [--open]` | The same as a local web UI on 127.0.0.1, token-protected (`--features web`) |

Exit codes: **0** pass, **1** the gate blocked, **2** harness could not run.
In CI, use the [GitHub Action](docs/guides/github-action.md).

Configuration lives in [`harness.toml`](examples/harness.toml):

```toml
[sandbox]
provider = "docker"
image = "node:22-bookworm-slim"
allow_egress = false        # validation commands get no network
allow_setup_egress = true   # dependency install may

[validation]
setup = ["npm ci"]
commands = ["npm test"]

[agents]
allowed = ["claude-code", "cline", "github-copilot"]
strict_mode = true

[conflicts]
branches = ["agent/*"]
```

## Supported languages (structural diff)

| Language | Extensions | Feature |
|---|---|---|
| TypeScript / TSX | `.ts` `.mts` `.cts` `.tsx` | `lang-typescript` |
| JavaScript / JSX | `.js` `.jsx` `.mjs` `.cjs` | `lang-javascript` |
| Python | `.py` `.pyi` | `lang-python` |

Adding one is a mapping table of under 100 lines — see
[the guide](docs/guides/adding-a-language.md).

## Repository layout

```
crates/harness-cli       the `harness` binary and library (TUI, web UI)
crates/harness-core      config + AgentManifest contracts (dependency-light)
crates/harness-ast       Tree-sitter structural diff engine
crates/harness-runtime   sandbox backends (Docker, Wasmtime)
crates/harness-security  secret scanning + dependency policy
docs/                    PRD, architecture, ADRs, RFCs, guides
.github/                 the automated contribution gate
```

## Documentation

The [documentation map](docs/README.md) routes you by task. Highlights:

- [Product requirements](docs/PRD.md) · [Roadmap](docs/ROADMAP.md)
- [Architecture](docs/ARCHITECTURE.md) · [Decision records](docs/adr/README.md) · [RFCs](docs/rfcs/README.md)
- [GitHub Action](docs/guides/github-action.md) · [Terminal and web UIs](docs/guides/local-uis.md)
- [Contributing](CONTRIBUTING.md) — including the
  [< 100-line language parser path](docs/guides/adding-a-language.md)
- [Security policy](SECURITY.md) · [Governance](GOVERNANCE.md)

## Contributing

PRs — from humans and agents — go through the same automated gate this
project exists to promote: format, lints, tests on three OSes, coverage,
conventional commits, DCO, license/advisory audit, secret scan, and semver
checks. If the gate is green and a maintainer approves, it merges. Start
with [CONTRIBUTING.md](CONTRIBUTING.md) or a
[`good first issue`](https://github.com/KadonWills/agent-harness/labels/good%20first%20issue).

## Community

- Questions and ideas: [Discussions](https://github.com/KadonWills/agent-harness/discussions) — see [SUPPORT.md](SUPPORT.md)
- Vulnerabilities: report privately per [SECURITY.md](SECURITY.md)
- [Code of Conduct](CODE_OF_CONDUCT.md) · [Governance](GOVERNANCE.md) · [Maintainers](MAINTAINERS.md)

## License

MIT OR Apache-2.0, at your option. See [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE).
