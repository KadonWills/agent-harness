# Adding a language parser

Goal: a new language lands in **under 100 lines** of glue. Python is 29
lines and JavaScript 37 (`crates/harness-ast/src/lang/`). If your PR is much
bigger, the framework is failing you — open an issue.

## What you write

One `LanguageSpec` constant: a mapping table from your grammar's Tree-sitter
node kinds to the concepts the shared walker understands. You write no tree
walking and no diffing. `crates/harness-ast/src/walker.rs` does that for
every language.

```rust
// crates/harness-ast/src/lang/go.rs
use crate::walker::{LanguageSpec, Rule};

pub static GO: LanguageSpec = LanguageSpec {
    name: "go",
    extensions: &["go"],
    grammar: || tree_sitter_go::LANGUAGE.into(),
    functions: &[
        Rule::new("function_declaration", "name"),
        Rule::new("method_declaration", "name"),
    ],
    types: &[Rule::new("type_spec", "name")],
    members: &[Rule::new("field_declaration", "name")],
    receiver: None,
    imports: &[("import_spec", "path")],
    identifiers: &["identifier", "field_identifier", "type_identifier"],
    signature_fields: &["parameters", "result"],
};
```

| Field | Meaning |
|---|---|
| `functions` | Declarations that become `FunctionAdded/Removed/Changed/Renamed`. `Rule::with_value(&[..])` restricts to nodes whose `value` field has one of the given kinds (e.g. `const f = () => {}`). |
| `types` | Classes, interfaces, structs, enums. Symbols inside them are qualified (`User.save`). |
| `members` | Fields and methods declared in a type body. Removing one emits `MemberRemoved`. `Rule::with_name_kinds(&[..])` filters by the name node's kind. |
| `receiver` | Optional `self.x = …` / `this.x = …` rule, so attributes assigned in methods count as members. |
| `imports` | `(node kind, field)` pairs whose text names a module. |
| `identifiers` | Node kinds whose text is a referenced name. These feed conflict detection. |
| `signature_fields` | Function fields whose change means `SignatureChanged`. |

Find node kinds and field names in your grammar crate's
`src/node-types.json`, or with `tree-sitter parse file.ext`.

## Checklist for a parser PR

- [ ] Grammar dependency, optional, in `crates/harness-ast/Cargo.toml`, plus a
      `lang-<name>` feature (off by default) added to `full`
- [ ] `src/lang/<name>.rs` with the spec, registered in `src/lang/mod.rs`
- [ ] Fixture pair `tests/fixtures/<name>/before.<ext>` / `after.<ext>` plus
      `expected.txt` (one `<Variant>:<symbol>` per line), and a test in
      `tests/fixtures.rs`
- [ ] Entry in the supported-languages table in README.md
- [ ] Conventional commit: `feat(ast): add <language> parser`

## Review criteria

Parsers are judged on the fixtures. Renames, signature changes, removed
members and dependency additions must produce the right `AstChange` events
on realistic code, including slightly broken code (agents produce that).
Tree-sitter's error recovery handles most of it. The TypeScript fixture ends
in an unterminated function on purpose.
