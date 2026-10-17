//! The shared Tree-sitter walker. A language supplies only a
//! [`LanguageSpec`] mapping table; everything else lives here.

use std::collections::{BTreeMap, BTreeSet};

use tree_sitter::{Node, Parser};

use crate::{AstChange, Error, LanguageParser};

/// Matches declarations of one node kind.
#[derive(Debug, Clone, Copy)]
pub struct Rule {
    pub kind: &'static str,
    /// Field holding the declared name.
    pub name_field: &'static str,
    /// When non-empty, the node's `value` field must have one of these
    /// kinds (e.g. `const f = () => {}` is a function; `const n = 1` is not).
    pub value_kinds: &'static [&'static str],
    /// When non-empty, the name node must have one of these kinds.
    pub name_kinds: &'static [&'static str],
}

impl Rule {
    pub const fn new(kind: &'static str, name_field: &'static str) -> Self {
        Self {
            kind,
            name_field,
            value_kinds: &[],
            name_kinds: &[],
        }
    }

    pub const fn with_value(mut self, kinds: &'static [&'static str]) -> Self {
        self.value_kinds = kinds;
        self
    }

    pub const fn with_name_kinds(mut self, kinds: &'static [&'static str]) -> Self {
        self.name_kinds = kinds;
        self
    }
}

/// `self.x = …` / `this.x = …` inside a type's methods declares member `x`.
#[derive(Debug, Clone, Copy)]
pub struct ReceiverRule {
    /// The assignment node kind and its target field.
    pub assignment: &'static str,
    pub left_field: &'static str,
    /// The attribute-access node kind and its fields.
    pub access: &'static str,
    pub object_field: &'static str,
    pub property_field: &'static str,
    /// The receiver's spelling: "self" or "this".
    pub receiver: &'static str,
}

/// Everything a language contributes. See docs/guides/adding-a-language.md.
#[derive(Debug, Clone, Copy)]
pub struct LanguageSpec {
    pub name: &'static str,
    pub extensions: &'static [&'static str],
    pub grammar: fn() -> tree_sitter::Language,
    pub functions: &'static [Rule],
    pub types: &'static [Rule],
    /// Declarations inside a type body (fields, methods, enum members).
    pub members: &'static [Rule],
    pub receiver: Option<ReceiverRule>,
    /// Node kind and the field naming the imported module.
    pub imports: &'static [(&'static str, &'static str)],
    /// Node kinds whose text is a referenced name.
    pub identifiers: &'static [&'static str],
    /// Fields of a function node that form its signature.
    pub signature_fields: &'static [&'static str],
}

/// A [`LanguageParser`] driven by a [`LanguageSpec`].
#[derive(Debug, Clone, Copy)]
pub struct TreeSitterParser {
    spec: &'static LanguageSpec,
}

impl TreeSitterParser {
    pub const fn new(spec: &'static LanguageSpec) -> Self {
        Self { spec }
    }

    pub fn spec(&self) -> &'static LanguageSpec {
        self.spec
    }

    /// Parse `source` into its declared and referenced symbols.
    pub fn symbols(&self, source: &str) -> Result<Symbols, Error> {
        let mut parser = Parser::new();
        parser
            .set_language(&(self.spec.grammar)())
            .map_err(|e| Error::Parse(format!("{}: {e}", self.spec.name)))?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| Error::Parse(format!("{}: parser gave up", self.spec.name)))?;
        let mut symbols = Symbols::default();
        Extractor {
            spec: self.spec,
            src: source.as_bytes(),
            out: &mut symbols,
        }
        .visit(tree.root_node(), &mut Vec::new());
        Ok(symbols)
    }

    /// Byte ranges of plain `identifier` nodes spelled `name` (property
    /// accesses and strings never match).
    pub fn identifier_ranges(
        &self,
        source: &str,
        name: &str,
    ) -> Result<Vec<std::ops::Range<usize>>, Error> {
        let mut parser = Parser::new();
        parser
            .set_language(&(self.spec.grammar)())
            .map_err(|e| Error::Parse(format!("{}: {e}", self.spec.name)))?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| Error::Parse(format!("{}: parser gave up", self.spec.name)))?;
        let mut ranges = Vec::new();
        let mut stack = vec![tree.root_node()];
        while let Some(node) = stack.pop() {
            if node.kind() == "identifier" && node.utf8_text(source.as_bytes()).ok() == Some(name) {
                ranges.push(node.byte_range());
            }
            let mut cursor = node.walk();
            stack.extend(node.children(&mut cursor));
        }
        ranges.sort_by_key(|r| r.start);
        Ok(ranges)
    }
}

impl LanguageParser for TreeSitterParser {
    fn language(&self) -> &'static str {
        self.spec.name
    }

    fn extensions(&self) -> &'static [&'static str] {
        self.spec.extensions
    }

    fn diff(&self, before: &str, after: &str) -> Result<Vec<AstChange>, Error> {
        Ok(diff_symbols(&self.symbols(before)?, &self.symbols(after)?))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionInfo {
    /// Whitespace-normalized source of the whole declaration.
    pub text: String,
    /// The declaration with its name cut out, to detect pure renames.
    pub body_key: String,
    pub signature: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeInfo {
    pub text: String,
    pub members: BTreeSet<String>,
}

/// Declared and referenced symbols of one file.
#[derive(Debug, Clone, Default)]
pub struct Symbols {
    pub functions: BTreeMap<String, FunctionInfo>,
    pub types: BTreeMap<String, TypeInfo>,
    pub imports: BTreeSet<String>,
    pub identifiers: BTreeSet<String>,
    /// Unqualified names this file declares (functions, types, members).
    pub declared: BTreeSet<String>,
}

fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn find(rules: &[Rule], node: Node<'_>) -> Option<Rule> {
    rules.iter().copied().find(|r| {
        r.kind == node.kind()
            && (r.value_kinds.is_empty()
                || node
                    .child_by_field_name("value")
                    .is_some_and(|v| r.value_kinds.contains(&v.kind())))
            && (r.name_kinds.is_empty()
                || node
                    .child_by_field_name(r.name_field)
                    .is_some_and(|n| r.name_kinds.contains(&n.kind())))
    })
}

struct Extractor<'a> {
    spec: &'static LanguageSpec,
    src: &'a [u8],
    out: &'a mut Symbols,
}

impl Extractor<'_> {
    fn text(&self, node: Node<'_>) -> &str {
        node.utf8_text(self.src).unwrap_or_default()
    }

    fn name_of<'t>(&self, node: Node<'t>, rule: Rule) -> Option<(Node<'t>, String)> {
        let name = node.child_by_field_name(rule.name_field)?;
        let text = self.text(name).trim_matches(|c| c == '"' || c == '\'');
        (!text.is_empty()).then(|| (name, text.to_string()))
    }

    fn qualify(scope: &[String], name: &str) -> String {
        if scope.is_empty() {
            name.to_string()
        } else {
            format!("{}.{name}", scope.join("."))
        }
    }

    fn visit(&mut self, node: Node<'_>, scope: &mut Vec<String>) {
        let kind = node.kind();
        if self.spec.identifiers.contains(&kind) {
            let text = self.text(node).to_string();
            self.out.identifiers.insert(text);
        }
        for (import_kind, field) in self.spec.imports {
            if *import_kind == kind {
                let mut cursor = node.walk();
                for module in node.children_by_field_name(field, &mut cursor) {
                    let text = self.text(module);
                    let text = text.split(" as ").next().unwrap_or(text);
                    let text = text
                        .trim()
                        .trim_matches(|c| c == '"' || c == '\'' || c == '`');
                    if !text.is_empty() {
                        self.out.imports.insert(text.to_string());
                    }
                }
            }
        }

        if let Some(rule) = find(self.spec.types, node) {
            if let Some((_, name)) = self.name_of(node, rule) {
                let mut members = BTreeSet::new();
                self.collect_members(node, &mut members, true);
                self.out.declared.extend(members.iter().cloned());
                self.out.declared.insert(name.clone());
                let qualified = Self::qualify(scope, &name);
                let text = normalize(self.text(node));
                self.out
                    .types
                    .entry(qualified)
                    .or_insert(TypeInfo { text, members });
                scope.push(name);
                self.visit_children(node, scope);
                scope.pop();
                return;
            }
        }

        if let Some(rule) = find(self.spec.functions, node) {
            if let Some((name_node, name)) = self.name_of(node, rule) {
                let range = node.byte_range();
                let cut = name_node.byte_range();
                let src = self.src;
                let slice = |a: usize, b: usize| String::from_utf8_lossy(&src[a..b]).into_owned();
                let body_key = format!(
                    "{}\u{0}{}",
                    normalize(&slice(range.start, cut.start)),
                    normalize(&slice(cut.end, range.end))
                );
                let signature: Vec<String> = self
                    .spec
                    .signature_fields
                    .iter()
                    .filter_map(|f| node.child_by_field_name(f))
                    .map(|n| normalize(self.text(n)))
                    .collect();
                self.out.declared.insert(name.clone());
                let qualified = Self::qualify(scope, &name);
                let text = normalize(self.text(node));
                self.out.functions.entry(qualified).or_insert(FunctionInfo {
                    text,
                    body_key,
                    signature: signature.join(" "),
                });
            }
        }
        self.visit_children(node, scope);
    }

    fn visit_children(&mut self, node: Node<'_>, scope: &mut Vec<String>) {
        let mut cursor = node.walk();
        let children: Vec<Node<'_>> = node.children(&mut cursor).collect();
        for child in children {
            self.visit(child, scope);
        }
    }

    /// Members of the type at `node`. Does not descend into nested types,
    /// and inside function bodies only receiver assignments count.
    fn collect_members(&self, node: Node<'_>, out: &mut BTreeSet<String>, top: bool) {
        if !top && find(self.spec.types, node).is_some() {
            return;
        }
        let in_function = !top && find(self.spec.functions, node).is_some();
        if let Some(rule) = find(self.spec.members, node) {
            if let Some((_, name)) = self.name_of(node, rule) {
                out.insert(name);
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if in_function {
                self.collect_receiver_members(child, out);
            } else {
                self.collect_members(child, out, false);
            }
        }
    }

    fn collect_receiver_members(&self, node: Node<'_>, out: &mut BTreeSet<String>) {
        if find(self.spec.types, node).is_some() {
            return;
        }
        if let Some(r) = self.spec.receiver {
            if node.kind() == r.assignment {
                let target = node
                    .child_by_field_name(r.left_field)
                    .filter(|l| l.kind() == r.access);
                if let Some(target) = target {
                    let object = target.child_by_field_name(r.object_field);
                    let property = target.child_by_field_name(r.property_field);
                    if let (Some(o), Some(p)) = (object, property) {
                        if self.text(o) == r.receiver {
                            out.insert(self.text(p).to_string());
                        }
                    }
                }
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_receiver_members(child, out);
        }
    }
}

/// Compare two symbol tables. Output order is deterministic.
pub fn diff_symbols(before: &Symbols, after: &Symbols) -> Vec<AstChange> {
    let mut changes = Vec::new();

    let mut removed: Vec<&String> = before
        .functions
        .keys()
        .filter(|k| !after.functions.contains_key(*k))
        .collect();
    let mut added: Vec<&String> = after
        .functions
        .keys()
        .filter(|k| !before.functions.contains_key(*k))
        .collect();
    let mut renames = Vec::new();
    removed.retain(|old| {
        let key = &before.functions[*old].body_key;
        match added
            .iter()
            .position(|new| &after.functions[*new].body_key == key)
        {
            Some(i) => {
                renames.push(AstChange::FunctionRenamed {
                    from: (*old).clone(),
                    to: added.remove(i).clone(),
                });
                false
            }
            None => true,
        }
    });
    changes.extend(added.into_iter().cloned().map(AstChange::FunctionAdded));
    changes.extend(removed.into_iter().cloned().map(AstChange::FunctionRemoved));
    changes.extend(renames);
    for (name, old) in &before.functions {
        if let Some(new) = after.functions.get(name) {
            if old.text != new.text {
                changes.push(AstChange::FunctionChanged(name.clone()));
                if old.signature != new.signature {
                    changes.push(AstChange::SignatureChanged(name.clone()));
                }
            }
        }
    }

    for (name, new) in &after.types {
        match before.types.get(name) {
            None => changes.push(AstChange::TypeAdded(name.clone())),
            Some(old) if old.text != new.text => {
                changes.push(AstChange::TypeChanged(name.clone()));
                for member in old.members.difference(&new.members) {
                    changes.push(AstChange::MemberRemoved {
                        owner: name.clone(),
                        member: member.clone(),
                    });
                }
            }
            Some(_) => {}
        }
    }
    for name in before.types.keys() {
        if !after.types.contains_key(name) {
            changes.push(AstChange::TypeRemoved(name.clone()));
        }
    }

    for dep in after.imports.difference(&before.imports) {
        changes.push(AstChange::DependencyAdded(dep.clone()));
    }
    for dep in before.imports.difference(&after.imports) {
        changes.push(AstChange::DependencyRemoved(dep.clone()));
    }
    // Declarations count too: `const email = (u) => u.email` both declares
    // and reads `email`. Rules that care discount definitions themselves.
    for ident in after.identifiers.difference(&before.identifiers) {
        changes.push(AstChange::ReferenceAdded(ident.clone()));
    }
    changes
}

#[cfg(all(test, feature = "full"))]
mod tests {
    use super::*;
    use crate::lang::{python::PYTHON, typescript::TYPESCRIPT};

    fn diff(spec: &'static LanguageSpec, before: &str, after: &str) -> Vec<String> {
        TreeSitterParser::new(spec)
            .diff(before, after)
            .expect("diff")
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn unchanged_and_reformatted_files_have_no_changes() {
        let src = "export function f(a: number) { return a + 1; }\n";
        assert!(diff(&TYPESCRIPT, src, src).is_empty());
        let reflowed = "export function f(a: number) {\n  return a + 1;\n}\n";
        assert!(diff(&TYPESCRIPT, src, reflowed).is_empty());
    }

    #[test]
    fn added_and_removed_files() {
        let src = "import os\n\nclass A:\n    pass\n\ndef f():\n    return os.sep\n";
        assert_eq!(
            diff(&PYTHON, "", src),
            [
                "FunctionAdded:f",
                "TypeAdded:A",
                "DependencyAdded:os",
                "ReferenceAdded:A",
                "ReferenceAdded:f",
                "ReferenceAdded:os",
                "ReferenceAdded:sep",
            ]
        );
        assert_eq!(
            diff(&PYTHON, src, ""),
            ["FunctionRemoved:f", "TypeRemoved:A", "DependencyRemoved:os"]
        );
    }

    #[test]
    fn new_references_are_reported() {
        let before = "export function a() { return 1; }\n";
        let after = "export function a() { return helper(user.email); }\n\
                     function helper(x: string) { return x; }\n";
        let changes = diff(&TYPESCRIPT, before, after);
        for name in ["email", "user", "helper"] {
            let want = format!("ReferenceAdded:{name}");
            assert!(changes.contains(&want), "{want} missing from {changes:?}");
        }
        assert!(
            !changes.contains(&"ReferenceAdded:a".to_string()),
            "a existed before"
        );
    }

    #[test]
    fn python_receiver_and_class_members() {
        let t = TreeSitterParser::new(&PYTHON);
        let symbols = t
            .symbols(
                "class S:\n    limit = 3\n    def __init__(self):\n        self.a = 1\n        local = 2\n    def run(self):\n        pass\n",
            )
            .expect("parse");
        let members: Vec<_> = symbols.types["S"].members.iter().cloned().collect();
        assert_eq!(members, ["__init__", "a", "limit", "run"]);
        assert!(symbols.functions.contains_key("S.run"));
    }

    #[test]
    fn nested_types_are_qualified_and_kept_separate() {
        let t = TreeSitterParser::new(&PYTHON);
        let s = t
            .symbols("class Outer:\n    class Inner:\n        x = 1\n    y = 2\n")
            .expect("parse");
        assert!(s.types.contains_key("Outer.Inner"));
        let outer: Vec<_> = s.types["Outer"].members.iter().cloned().collect();
        assert_eq!(outer, ["y"]);
    }

    #[test]
    fn non_function_variables_are_not_functions() {
        let s = TreeSitterParser::new(&TYPESCRIPT)
            .symbols("const n = 1;\nconst f = function () {};\nconst g = () => 2;\n")
            .expect("parse");
        let names: Vec<_> = s.functions.keys().cloned().collect();
        assert_eq!(names, ["f", "g"]);
    }
}
