# Architecture decision records

Each ADR records one decision, the context it was made in, and its
consequences. ADRs are not edited once accepted; a changed decision gets a
new ADR that supersedes the old one. See [ADR-0001](0001-record-architecture-decisions.md)
for the format.

| ADR | Decision | Status |
|---|---|---|
| [0001](0001-record-architecture-decisions.md) | Record architecture decisions | Accepted |
| [0002](0002-rust-for-the-core.md) | Rust for the core | Accepted |
| [0003](0003-tree-sitter-for-parsing.md) | Tree-sitter for parsing | Accepted |
| [0004](0004-deterministic-core.md) | Deterministic core — no LLM calls in the gate | Accepted |
| [0005](0005-crate-naming.md) | Crate naming is unresolved; the semver gate is disabled until it is | Open |

ADR vs RFC: an [RFC](../rfcs/README.md) proposes a change to a public
contract and is debated before it lands. An ADR captures a decision,
whether or not it went through an RFC.
