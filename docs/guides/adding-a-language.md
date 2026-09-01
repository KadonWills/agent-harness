# Adding a language parser

Goal: a new language lands in **under 100 lines** of glue. If your PR is
bigger than that, the framework is failing you — open an issue.

## What you implement

The `LanguageParser` trait in `crates/harness-ast/src/lib.rs`:

1. Bind the Tree-sitter grammar crate for your language (one dependency
   line in `harness-ast/Cargo.toml`, feature-gated).
2. Provide the node-kind mapping: which Tree-sitter node kinds correspond
   to functions, types, and import/dependency declarations.
3. Implement `diff()` — most of the walking logic is shared; you supply
   the mapping table.

## Checklist for a parser PR

- [ ] Feature flag: `lang-<name>` in harness-ast's Cargo.toml (off by default
      until stabilized, on in `full`)
- [ ] Mapping table + trait impl
- [ ] Fixture pair: `tests/fixtures/<lang>/before.<ext>` / `after.<ext>` plus
      the expected `AstChange` list
- [ ] Entry in the supported-languages table in README.md
- [ ] Conventional commit: `feat(ast): add <language> parser`

## Review criteria

Parsers are judged on the fixtures: do renames, signature changes, and
dependency additions produce the right `AstChange` events on realistic
code (including slightly broken code — agents produce that).

Note: while harness-ast is pre-v0.2, this guide describes the target
shape; the shared walking logic is still being built. Early parser
contributors get direct maintainer pairing — say hi in Discussions.
