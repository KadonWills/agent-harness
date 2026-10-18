//! TypeScript and TSX.

use crate::walker::{LanguageSpec, ReceiverRule, Rule};

const FUNCTION_VALUES: &[&str] = &["arrow_function", "function_expression", "function"];

const FUNCTIONS: &[Rule] = &[
    Rule::new("function_declaration", "name"),
    Rule::new("generator_function_declaration", "name"),
    Rule::new("method_definition", "name"),
    Rule::new("variable_declarator", "name").with_value(FUNCTION_VALUES),
];

const TYPES: &[Rule] = &[
    Rule::new("class_declaration", "name"),
    Rule::new("abstract_class_declaration", "name"),
    Rule::new("interface_declaration", "name"),
    Rule::new("type_alias_declaration", "name"),
    Rule::new("enum_declaration", "name"),
];

const MEMBERS: &[Rule] = &[
    Rule::new("property_signature", "name"),
    Rule::new("method_signature", "name"),
    Rule::new("abstract_method_signature", "name"),
    Rule::new("public_field_definition", "name"),
    Rule::new("method_definition", "name"),
    Rule::new("enum_assignment", "name"),
];

const RECEIVER: ReceiverRule = ReceiverRule {
    assignment: "assignment_expression",
    left_field: "left",
    access: "member_expression",
    object_field: "object",
    property_field: "property",
    receiver: "this",
};

const IMPORTS: &[(&str, &str)] = &[
    ("import_statement", "source"),
    ("export_statement", "source"),
];

const IDENTIFIERS: &[&str] = &[
    "identifier",
    "property_identifier",
    "type_identifier",
    "shorthand_property_identifier",
    "shorthand_property_identifier_pattern",
];

const SIGNATURE: &[&str] = &["parameters", "return_type"];

pub static TYPESCRIPT: LanguageSpec = LanguageSpec {
    name: "typescript",
    extensions: &["ts", "mts", "cts"],
    grammar: || tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
    functions: FUNCTIONS,
    types: TYPES,
    members: MEMBERS,
    receiver: Some(RECEIVER),
    imports: IMPORTS,
    identifiers: IDENTIFIERS,
    signature_fields: SIGNATURE,
};

pub static TSX: LanguageSpec = LanguageSpec {
    name: "tsx",
    extensions: &["tsx"],
    grammar: || tree_sitter_typescript::LANGUAGE_TSX.into(),
    ..TYPESCRIPT
};
