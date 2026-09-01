# AgentHarness

**Guardrails for autonomous coding agents.** AgentHarness sits between AI
coding agents (Claude Code, Cline, Cursor, Copilot Workspace, …) and your
git repository: it sandboxes and executes agent-generated changes, diffs
them structurally at the AST level, detects semantic conflicts between
concurrent agents, and blocks secrets and unvetted dependencies — all
**before** the code reaches human review.

> Status: **pre-alpha scaffold.** The architecture, contracts, and
> contribution paths are defined; v0.1 ("The Gate") is under active
> construction. See [docs/ROADMAP.md](docs/ROADMAP.md).

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

## Quick start (target UX — v0.1)

```bash
cargo install harness-cli    # npm i -g @agentharness/cli also planned
harness init                 # writes harness.toml, offers git hooks
harness check                # validates config + sandbox availability
harness run                  # snapshot → sandbox → gate → AgentManifest.json
```

Configuration lives in [`harness.toml`](examples/harness.toml):

```toml
[sandbox]
provider = "docker"
allow_egress = false
max_memory = "512MB"

[agents]
allowed = ["claude-code", "cline", "github-copilot"]
strict_mode = true
```

## Repository layout

```
crates/harness-cli       the `harness` binary
crates/harness-core      config + AgentManifest contracts (dependency-light)
crates/harness-ast       Tree-sitter structural diff engine
crates/harness-runtime   sandbox backends (Docker, Wasmtime)
docs/                    PRD, architecture, ADRs, RFCs, guides
.github/                 the automated contribution gate
```

## Documentation

- [Product requirements](docs/PRD.md) · [Roadmap](docs/ROADMAP.md)
- [Architecture](docs/ARCHITECTURE.md) · [Decision records](docs/adr/)
- [Contributing](CONTRIBUTING.md) — including the
  [< 100-line language parser path](docs/guides/adding-a-language.md)
- [Security policy](SECURITY.md) · [Governance](GOVERNANCE.md)

## Contributing

PRs — from humans and agents — go through the same automated gate this
project exists to promote: format, lints, tests on three OSes, coverage,
conventional commits, DCO, license/advisory audit, secret scan, and semver
checks. If the gate is green and a maintainer approves, it merges. Start
with [CONTRIBUTING.md](CONTRIBUTING.md) or a
[`good-first-issue`](https://github.com/KadonWills/agent-harness/labels/good-first-issue).

## License

MIT OR Apache-2.0, at your option. See [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE).
