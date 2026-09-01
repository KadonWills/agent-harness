# ADR-0003: Tree-sitter for parsing

Status: Accepted

## Context
Structural diffing needs incremental, error-tolerant parsing across many
languages, on possibly-broken agent-generated code.

## Decision
harness-ast builds on Tree-sitter grammars. Language support is a thin
mapping from Tree-sitter node kinds to our language-neutral `AstChange`
events, behind the `LanguageParser` trait.

## Consequences
+ Dozens of maintained grammars for free; parses incomplete code.
+ New-language contributions are mapping tables, not parsers (< 100 line goal).
- Semantic depth is limited (no type inference); cross-file semantic
  checks are built above the AST layer, per-language, as needed.
