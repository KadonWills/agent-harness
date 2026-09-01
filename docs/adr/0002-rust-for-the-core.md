# ADR-0002: Rust for the core

Status: Accepted

## Context
The harness sits in the hot path of every agent commit: it must start
fast (sub-100ms CLI overhead), run untrusted-adjacent workloads safely,
and ship as a single static binary across platforms.

## Decision
The entire open core is Rust (workspace of four crates). Distribution is
a prebuilt binary; the npm package is a thin binary-download wrapper,
not a JS implementation.

## Consequences
+ Single binary, no runtime dependency for users; memory safety in the
  layer that handles untrusted code.
- Smaller contributor pool than TypeScript; we compensate by keeping
  language parsers declarative (see docs/guides/adding-a-language.md)
  so most contributions don't require deep Rust.
