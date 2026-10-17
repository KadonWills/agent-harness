# Roadmap

Phases are scoped so a lean team ships each one in weeks, not quarters.
Each phase ends with something a real user can run. Dates are intentionally
absent — sequence and exit criteria are the commitment.

## v0.1 — "The Gate" (MVP)

The smallest thing that is honestly useful: *never let an unexecuted agent
change reach review.*

- [x] `harness init` / `check` / `run` (crate: `harness-cli`)
- [x] `harness.toml` parsing with safe defaults (`harness-core`)
- [x] Docker sandbox backend: tree snapshot, egress denied, memory/time limits
- [x] Run the project's own test command inside the sandbox
- [x] Emit `AgentManifest.json`
- [x] Git pre-push hook installer + reference GitHub Action

**Exit criteria:** on a fresh TypeScript or Python repo, `harness init &&
harness run` completes in under 5 minutes total and blocks a deliberately
broken change. *Met:* a fresh Python repo passes in about 57s and a broken
`add()` is blocked; the action smoke workflow repeats this in CI.

## v0.2 — "The Eyes" (structural awareness)

- [x] `harness-ast` Tree-sitter integration (trait stabilizes at v1.0)
- [x] TypeScript/JavaScript + Python parsers
- [x] `ast_changes` populated in the manifest
- [ ] `docs/guides/adding-a-language.md` proven by an external contribution

**Exit criteria:** a community-contributed language parser merges at < 100
lines of glue code. *Pending:* the in-tree specs are 29–70 lines; the
external contribution has not happened yet.

## v0.3 — "The Referee" (multi-agent)

- [x] Cross-branch `AstChange` comparison (worktrees: OQ-1, still open)
- [x] Semantic conflict detection + Level 2 halt-and-explain
- [x] Level 1 deterministic codemod auto-fixes (narrow, provably safe set)
- [x] Wasmtime sandbox backend (no-Docker environments, Windows story)

**Exit criteria:** the GraphQL-schema-drift scenario from the PRD is caught
on a demo repo with two concurrent agent branches. *Met:* see
`schema_drift_between_agent_branches_blocks` in `harness-cli`.

## v0.4 — "The Shield" (zero-trust security)

- [x] Secret scanning on staged changes (gitleaks-compatible rules)
- [x] Egress attempt audit trail in the manifest
- [x] Dependency policy: allow/deny lists + advisory DB check on new deps

**Exit criteria:** a planted credential and a typosquatted dependency are
both blocked before commit on the demo repo. *Met:* the `pre-commit` hook
from `harness init --hooks` blocks both.

## v0.5 — "The Judge" (behavioral)

- [ ] Performance baselines (time + peak memory) with regression thresholds
- [ ] Plugin API for LLM-backed extensions (bring-your-own-key)
- [ ] `harness-plugin-intent-tests` reference plugin (AI-generated E2E)

## v1.0 — Stability

- [ ] Manifest schema v1 frozen (RFC process for changes)
- [ ] `LanguageParser` + `SandboxBackend` trait APIs frozen
- [ ] Six supported languages, three sandbox backends (Docker, Wasmtime, cloud)
- [ ] Security audit of the sandbox isolation model

## Deferred / explicitly out

- LLM-mediated agent-to-agent refinement loops → plugin territory, post-1.0
- Real-time fsnotify terminal overlay → revisit post-1.0 if hooks+CI fall short
- The commercial cloud layer → separate repository, consumes the manifest contract
