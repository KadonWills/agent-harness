# Architecture

## System position

```
 AI coding agents (Claude Code, Cline, Cursor, Copilot Workspace, ...)
        │  edits working tree / opens PR
        ▼
┌─────────────────────────────────────────────────────────────────┐
│ AGENTHARNESS OPEN CORE (this repository, MIT OR Apache-2.0)     │
│                                                                 │
│  harness-cli ──► harness-core ◄── harness-ast                   │
│      │            (config,          (Tree-sitter               │
│      │             manifest,         structural diff)          │
│      │             shared types)                               │
│      └──────────► harness-runtime                              │
│                    (Docker / Wasmtime sandbox)                 │
│                                                                 │
│  Interception points: git hooks · `harness run` · GitHub Action │
│  Output contract:     AgentManifest.json                        │
└───────────────────────────────┬─────────────────────────────────┘
                                │ manifest (stable JSON contract)
                                ▼
        Human review · CI dashboards · commercial cloud layer
                     (cloud = separate repository)
```

## Crate graph

```
harness-cli ──► harness-ast ──► harness-core
     │                              ▲
     └────────► harness-runtime ────┘
```

Rules:
- `harness-core` depends on nothing in the workspace and stays
  dependency-light. It is the contract crate: config schema, manifest
  schema, shared errors.
- `harness-ast` and `harness-runtime` depend only on `harness-core`.
  They never depend on each other.
- `harness-cli` is the only binary and the only crate allowed to wire
  everything together.

## Data flow of `harness run`

1. **Load** `harness.toml` (harness-core). Invalid/unknown fields fail fast
   with agent-legible messages.
2. **Snapshot** the working tree into an isolated workspace copy.
3. **Diff** structurally against the base ref (harness-ast) → `AstChange` set.
4. **Execute** the project's validation commands in the sandbox
   (harness-runtime): network denied, resource-limited, egress attempts
   counted.
5. **Gate**: security checks (secrets, dependency policy) + conflict checks
   against sibling agent branches.
6. **Emit** `AgentManifest.json`; exit 0 (pass) or 1 (blocked) with a
   human- and agent-readable report.

## Extension points

| Trait | Crate | Who implements it |
|-------|-------|-------------------|
| `LanguageParser` | harness-ast | language parser contributions (< 100 lines goal) |
| `SandboxBackend` | harness-runtime | Docker (in-tree), Wasmtime (in-tree, v0.3), Firecracker (cloud, out-of-tree) |
| Plugin API (v0.5) | harness-cli | LLM-backed extensions, notifiers |

## Key decisions

Recorded as ADRs in `docs/adr/`. Start with:
- ADR-0001: Record architecture decisions
- ADR-0002: Rust for the core
- ADR-0003: Tree-sitter for parsing
- ADR-0004: Deterministic core — no LLM calls in the gate
