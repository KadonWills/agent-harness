# AGENTS.md

Operating instructions for AI coding agents working in this repository.
Humans should read [CONTRIBUTING.md](CONTRIBUTING.md) — this file is the
condensed, machine-readable form of the same rules. Where the two appear to
disagree, CONTRIBUTING.md wins.

## What this project is

AgentHarness is a deterministic guardrail layer between autonomous coding
agents and a git repository: sandboxed execution, AST-level structural
diffing, cross-agent conflict detection, and zero-trust security checks,
all before code reaches human review. Rust workspace, pre-alpha.

Read [docs/PRD.md](docs/PRD.md) before proposing anything architectural.

## Build and verify

```bash
cargo build --workspace
cargo test --workspace --all-features
./scripts/check.sh            # exact mirror of the CI gate
./scripts/check.sh --fix      # autofix fmt + clippy
```

`./scripts/check.sh` passing locally means the CI gate passes. Run it before
you claim a change is done. Do not push and let CI find what the script
would have told you in 30 seconds.

## Hard rules

- **Never call an LLM from the core.** The gate is deterministic: same diff,
  same verdict. Model-backed features go in plugins with bring-your-own-key
  (see [docs/adr/0004-deterministic-core.md](docs/adr/0004-deterministic-core.md)).
- **`unsafe` is denied workspace-wide.** Needing it is an RFC, not a PR.
- **Unknown config fields are hard errors,** not warnings — agents must get
  immediate feedback on hallucinated `harness.toml` keys.
- **Contract changes require an accepted RFC first.** Contracts are:
  `harness.toml` schema (`crates/harness-core/src/config.rs`),
  `AgentManifest.json` (`crates/harness-core/src/manifest.rs`), the
  `LanguageParser` and `SandboxBackend` traits, and CLI commands, flags, and
  exit codes. Copy `docs/rfcs/0000-template.md` to the next number.
- **Errors must say what is wrong and what to do about it.** This codebase's
  audience is partly other agents; a message without a fix suggestion is a bug.

## Commits and PRs

```bash
git commit -s -m "feat(ast): add go parser"
```

- Conventional Commits, enforced on both commits and the **PR title** (the PR
  title becomes the squash commit).
  Types: `feat` `fix` `docs` `test` `refactor` `perf` `chore` `ci` `build`.
  Scopes: `cli` `core` `ast` `runtime` `docs` `ci` `deps`.
- Subject must not start with a capital letter.
- **DCO sign-off is required on every commit** (`-s`). CI rejects the PR
  otherwise, and unsigned history cannot be fixed by a maintainer for you.
- One logical change per PR. Over 600 changed lines gets `size/XL` and a
  request to split.
- Tests are required. Patch coverage below 70% fails the gate.
- Disclose in the PR body that the change was agent-authored, and name a
  human author of record.
- **Push to the same branch to fix CI. Do not open a new PR per attempt.**

## Layout

| Path | What lives there |
|---|---|
| `crates/harness-cli` | the `harness` binary — `init`, `check`, `run` |
| `crates/harness-core` | config + `AgentManifest` contracts; keep dependency-light |
| `crates/harness-ast` | Tree-sitter structural diff engine |
| `crates/harness-runtime` | sandbox backends (Docker now, Wasmtime v0.3) |
| `docs/` | PRD, architecture, ADRs, RFCs, guides |
| `examples/harness.toml` | parsed by `harness-core`'s tests — keep it valid |

Adding a language parser is the best-scoped first contribution: see
[docs/guides/adding-a-language.md](docs/guides/adding-a-language.md).
Target is under 100 lines of glue.

## Do not

- Commit secrets or credentials — gitleaks runs on every PR and on history.
- Add a dependency without checking `deny.toml`'s license allowlist.
- Edit `CHANGELOG.md` by hand for unreleased work; entries are generated
  from conventional commits at release time.
- Bump versions or publish; releases are tag-driven and maintainer-run.
