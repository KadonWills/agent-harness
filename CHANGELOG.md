# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[SemVer](https://semver.org/). Entries are generated from conventional
commits at release time and curated by hand.

## [Unreleased]

## [0.1.0] - 2026-10-20

First release. Binaries for Linux, macOS, and Windows are attached to the
GitHub release, built with the `tui` and `web` features, with SHA-256
checksums and build provenance
(`gh attestation verify <file> --repo KadonWills/agent-harness`). The crates
are not on crates.io yet (see ADR-0005); install from git or a release
binary.

### Added
- `harness init`: detects Node, Python, Rust, and Go projects and writes
  `harness.toml`; `--hooks` installs git hooks.
- `harness check`: validates `harness.toml`, rejecting unknown fields with a
  suggested fix; `--json` for agents.
- `harness run`: runs an agent's change in a sandbox, diffs it structurally,
  applies the security gate, and writes an `AgentManifest` verdict.
- `harness scan`: secret scanning with gitleaks-compatible rules, and a
  dependency policy with typosquat and advisory checks.
- `harness conflicts`: detects semantic conflicts across agent branches.
- `harness tui`: a terminal UI for runs, history, findings, and conflicts.
- `harness ui`: a loopback-only web UI with token login, CSRF protection,
  and confirmation before loosening the sandbox.
- Sandbox backends: Docker, and Wasmtime behind the `wasmtime` feature.
- Tree-sitter structural diff for TypeScript, JavaScript, and Python.
- A library API for `harness-cli` and recorded run history.
- A reference GitHub Action (`action.yml`).
- `harness.toml` and `AgentManifest` contracts per RFC 0001; local UIs per
  RFC 0002.

### Security
- Release builds carry signed build-provenance attestations.
- All workflow actions are pinned to commit SHAs; CodeQL, cargo-deny,
  gitleaks, and dependency review run on every pull request.
- The Wasmtime backend uses Wasmtime 49.0.4, which fixes RUSTSEC-2026-0378
  and RUSTSEC-2026-0379. Building with `--features wasmtime` needs Rust 1.96;
  the default build supports Rust 1.90.

[Unreleased]: https://github.com/KadonWills/agent-harness/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/KadonWills/agent-harness/releases/tag/v0.1.0
