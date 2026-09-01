# AgentHarness — Product Requirements Document

**Status:** Living document · v2 (restructured from the original draft)
**License:** Core is MIT OR Apache-2.0 · Cloud layer is out of scope for this repository

---

## 1. Vision

AgentHarness is an open-source guardrail layer that sits between autonomous
coding agents (Claude Code, Cline, Cursor, Copilot Workspace, …) and the git
repository. It deterministically validates agent-generated changes — sandboxed
execution, structural (AST-level) diff analysis, cross-agent conflict
detection, and zero-trust security checks — before code reaches human review.

As agent fleets generate more of the code, the bottleneck shifts from writing
code to *trusting* code. AgentHarness makes that trust checkable.

## 2. Design principles

1. **Deterministic core, probabilistic edges.** The core never calls an LLM.
   Every gate decision (pass/fail/halt) is reproducible from the same inputs.
   Anything requiring model calls (test generation, auto-fix suggestions)
   lives in optional plugins with bring-your-own-key.
2. **Zero-trust by default.** Sandboxes deny network egress unless explicitly
   allowed. Unknown agents are rejected in strict mode. Secrets never reach a
   commit.
3. **Agent-legible.** `harness.toml`, CLI output, and `AgentManifest.json` are
   designed to be read, generated, and self-corrected by LLMs: flat schemas,
   explicit error messages with fix suggestions, stable field names.
4. **Boring interception.** We hook the paths that already exist — git hooks,
   CI, an explicit `harness run` — before inventing new ones. Real-time
   filesystem watching is an optimization, not the foundation.
5. **Open-core with a clean seam.** This repository is complete and useful on
   its own. The commercial cloud (Firecracker fleets, org dashboards,
   compliance walls) consumes the same `AgentManifest.json` contract but
   lives elsewhere.

## 3. Problems solved (in priority order)

| # | Problem | Component |
|---|---------|-----------|
| P1 | Agent changes land without ever being executed — broken builds reach humans | `harness-runtime` |
| P2 | Agents leak secrets / add unvetted dependencies / attempt network calls during "tests" | security gate |
| P3 | Two agents make individually-valid changes that are jointly broken (schema drift, contract mismatch) with no textual merge conflict | `harness-ast` conflict engine |
| P4 | No audit trail links a change back to the instruction that produced it | `AgentManifest.json` |
| P5 | Agent code passes tests but regresses performance or behavior | behavioral gate (later phase) |

## 4. Functional requirements

### 4.1 CLI (`harness`) — v0.1

- `harness init` — create `harness.toml` with safe defaults; detect project
  language/engine; install git hooks on request.
- `harness check` — validate config, verify sandbox backend availability
  (Docker present, etc.), report environment readiness. Exit non-zero on
  problems; machine-readable `--json` output.
- `harness run` — the pipeline: snapshot working tree → execute the project's
  validation commands inside the sandbox → collect results → emit
  `AgentManifest.json` → exit 0/1. This single command is what git hooks and
  CI call.
- Config: `harness.toml` (see `examples/harness.toml`). Every field defaults
  safely; an empty file is valid. Unknown fields are hard errors (agents get
  immediate feedback on hallucinated keys).

### 4.2 Sandbox (`harness-runtime`) — v0.1 Docker, v0.3 Wasmtime

- Copy-on-write snapshot of the working tree; agent changes never execute in
  the real checkout.
- Network egress denied by default (`allow_egress = false`); attempts are
  counted and reported in the manifest, not silently swallowed.
- Resource ceilings: memory (`max_memory`), CPU time, wall-clock timeout.
- Backend trait is pluggable; the cloud Firecracker backend implements the
  same trait outside this repo.

### 4.3 Structural diff engine (`harness-ast`) — v0.2

- Tree-sitter based, language plugins behind a `LanguageParser` trait.
- Launch languages: TypeScript/JavaScript, Python. Then Rust, Go, Java, C#.
- Output: language-neutral `AstChange` events (FunctionChanged, TypeChanged,
  DependencyAdded, …) recorded per impacted file in the manifest.
- Contribution goal: a new language lands in **< 100 lines** of glue
  (grammar binding + node-kind mapping table). See
  `docs/guides/adding-a-language.md`.

### 4.4 Cross-agent conflict detection — v0.3

- Compare `AstChange` sets across concurrent agent branches/worktrees.
- Flag *semantic* conflicts that textual merge cannot see (e.g. Agent A
  renames a schema field; Agent B adds a resolver reading the old name).
- Resolution matrix:
  - **Level 1 — Auto-fixable:** mechanical, provably-safe alignments
    (re-exported symbol renames, lockfile regeneration). Applied by
    deterministic codemods only. *LLM-mediated "refinement prompt loops"
    between agents are explicitly deferred to a plugin — they are
    non-deterministic and require agent-API integrations we don't control.*
  - **Level 2 — Halt & escalate:** structural contradiction. Block the merge,
    emit a human-readable explanation of both change sets and the collision
    point, notify via exit code + manifest (Slack/GitHub notifiers are thin
    adapters over the manifest).

### 4.5 Zero-trust security gate — v0.4

- **Secret scanning** on staged changes (entropy + pattern rules, gitleaks-
  compatible rule format). Block the commit, name the file and line.
- **Egress audit**: every denied network attempt inside the sandbox is
  logged with destination and originating process.
- **Dependency policy**: new dependencies introduced by an agent are checked
  against an allow/deny policy and known-advisory databases before the gate
  passes.

### 4.6 Behavioral gate — v0.5 (deliberately last)

- **Performance baseline:** compare sandbox execution time and peak memory
  of the project's test/bench suite against the main-branch baseline. The
  gate fails when the new code **regresses** beyond a configurable threshold
  (default: +15% time or memory). *(The original draft had this inverted —
  failing when CPU "drops 15%". Faster code is a win; the guard exists to
  catch runaway loops and pathological slowdowns. Anomalous >50% drops are
  flagged as warnings — sometimes a sign tests silently stopped running.)*
- **Intent-aware E2E generation** (reading the agent's original prompt and
  generating Playwright/Vitest checks) requires LLM calls, so it ships as an
  optional plugin (`harness-plugin-intent-tests`), never in the core gate.

### 4.7 The manifest contract (`AgentManifest.json`)

Emitted for every session; the stable public interface for CI, dashboards,
and the cloud layer. Schema lives in `harness-core::manifest` and is
semver-governed: breaking changes require an RFC. Manifest version is a
simple integer ("1"), not a date string.

## 5. Non-goals

- Not an agent framework or orchestrator — we validate outputs, we don't run
  agents.
- No LLM calls in the core, ever (see principle 1).
- No code hosting, review UI, or merge queue — we integrate with GitHub/GitLab.
- The commercial cloud (multi-tenant microVMs, org dashboards, egress
  firewalls at fleet scale) is a separate codebase consuming our contracts.
- Real-time terminal/fs interception overlay: revisit after v1.0 if git-hook
  + CI interception proves insufficient in practice.

## 6. Distribution

Primary binary is Rust. Channels, in order of rollout:
1. `cargo install harness-cli`
2. GitHub Releases (prebuilt binaries, checksummed) + `cargo-binstall`
3. `npm install -g @agentharness/cli` — a thin wrapper that downloads the
   platform binary (npm reach matters for the JS-first agent ecosystem)
4. Homebrew tap; `curl | sh` installer last, and always checksum-verified.

## 7. Success metrics

**Community (leading):** time-to-first-green-run < 5 minutes from install on
a real repo; a language parser PR mergeable at < 100 lines; ≥ 3 non-founder
maintainers within 12 months; adoption in agent starter templates.

**Product (lagging):** "chaos deflection" — count of broken builds, semantic
conflicts, and secret leaks intercepted per repo per month (measured from
manifests, opt-in telemetry only, off by default).

**Honest note on the "10k stars in 6 months" goal from the draft:** that is
an outcome, not a plan. The plan is: be genuinely useful at v0.1, publish
the manifest contract early so tooling grows around it, and make the parser
contribution path trivially easy.

## 8. Open questions (tracked as RFCs)

- OQ-1: Worktree layout for concurrent agent sessions — one worktree per
  session vs. branch-diff only. (RFC needed before v0.3.)
- OQ-2: Baseline storage for the behavioral gate — in-repo file vs.
  `.harness/` cache vs. CI artifact. (Before v0.5.)
- OQ-3: Windows sandbox story — Docker Desktop dependency vs. WSL2 vs.
  Wasmtime-only on Windows.
