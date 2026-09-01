# Roadmap

Phases are scoped so a lean team ships each one in weeks, not quarters.
Each phase ends with something a real user can run. Dates are intentionally
absent — sequence and exit criteria are the commitment.

## v0.1 — "The Gate" (MVP)

The smallest thing that is honestly useful: *never let an unexecuted agent
change reach review.*

- [ ] `harness init` / `check` / `run` (crate: `harness-cli`)
- [ ] `harness.toml` parsing with safe defaults (`harness-core`) — done in scaffold
- [ ] Docker sandbox backend: tree snapshot, egress denied, memory/time limits
- [ ] Run the project's own test command inside the sandbox
- [ ] Emit `AgentManifest.json`
- [ ] Git pre-push hook installer + reference GitHub Action

**Exit criteria:** on a fresh TypeScript or Python repo, `harness init &&
harness run` completes in under 5 minutes total and blocks a deliberately
broken change.

## v0.2 — "The Eyes" (structural awareness)

- [ ] `harness-ast` Tree-sitter integration, `LanguageParser` trait stabilized
- [ ] TypeScript/JavaScript + Python parsers
- [ ] `ast_changes` populated in the manifest
- [ ] `docs/guides/adding-a-language.md` proven by an external contribution

**Exit criteria:** a community-contributed language parser merges at < 100
lines of glue code.

## v0.3 — "The Referee" (multi-agent)

- [ ] Cross-branch/worktree `AstChange` comparison
- [ ] Semantic conflict detection + Level 2 halt-and-explain
- [ ] Level 1 deterministic codemod auto-fixes (narrow, provably safe set)
- [ ] Wasmtime sandbox backend (no-Docker environments, Windows story)

**Exit criteria:** the GraphQL-schema-drift scenario from the PRD is caught
on a demo repo with two concurrent agent branches.

## v0.4 — "The Shield" (zero-trust security)

- [ ] Secret scanning on staged changes (gitleaks-compatible rules)
- [ ] Egress attempt audit trail in the manifest
- [ ] Dependency policy: allow/deny lists + advisory DB check on new deps

**Exit criteria:** a planted credential and a typosquatted dependency are
both blocked before commit on the demo repo.

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
