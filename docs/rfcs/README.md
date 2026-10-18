# RFCs

An RFC is required before changing any public contract:

- the `harness.toml` schema (`crates/harness-core/src/config.rs`)
- `AgentManifest.json` (`crates/harness-core/src/manifest.rs`)
- the `LanguageParser` and `SandboxBackend` traits
- CLI commands, flags, and exit codes

Everything else — bug fixes, new language parsers, internal refactors,
docs — goes straight to a PR.

## Process

1. Open a [Discussion in Ideas](https://github.com/KadonWills/agent-harness/discussions/categories/ideas)
   first if you are unsure the change is wanted. It is cheaper than an RFC.
2. Copy [`0000-template.md`](0000-template.md) to the next free number and
   open a PR titled `docs(docs): add rfc NNNN for <topic>`, with
   `Status: In review`.
3. Discussion happens on that PR. Expect at least a week for anything
   non-trivial so people in other time zones can weigh in.
4. Maintainers accept by consensus ([GOVERNANCE](../../GOVERNANCE.md)).
   The RFC is set to `Accepted` and merged **before** its implementation
   PRs, which link it.
5. A rejected RFC is merged as `Rejected` with the reasoning, so the same
   idea is not re-litigated without new information.

An accepted RFC can be changed later only by a new RFC that supersedes it.

## Index

| RFC | Title | Status |
|---|---|---|
| [0001](0001-v0.1-v0.4-contracts.md) | Contracts for v0.1–v0.4 (gate, eyes, referee, shield) | Accepted |
| [0002](0002-local-uis.md) | Local terminal and web UIs | Accepted |
