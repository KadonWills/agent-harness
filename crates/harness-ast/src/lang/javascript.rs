//! JavaScript (including JSX).

use crate::walker::{LanguageSpec, ReceiverRule, Rule};

pub static JAVASCRIPT: LanguageSpec = LanguageSpec {
    name: "javascript",
    extensions: &["js", "jsx", "mjs", "cjs"],
    grammar: || tree_sitter_javascript::LANGUAGE.into(),
    functions: &[
        Rule::new("function_declaration", "name"),
        Rule::new("generator_function_declaration", "name"),
        Rule::new("method_definition", "name"),
        Rule::new("variable_declarator", "name").with_value(&[
            "arrow_function",
            "function_expression",
            "function",
        ]),
    ],
    types: &[Rule::new("class_declaration", "name")],
    members: &[
        Rule::new("method_definition", "name"),
        Rule::new("field_definition", "property"),
    ],
    receiver: Some(ReceiverRule {
        assignment: "assignment_expression",
        left_field: "left",
        access: "member_expression",
        object_field: "object",
        property_field: "property",
        receiver: "this",
    }),
    imports: &[
        ("import_statement", "source"),
        ("export_statement", "source"),
    ],
    identifiers: &[
        "identifier",
        "property_identifier",
        "shorthand_property_identifier",
        "shorthand_property_identifier_pattern",
    ],
    signature_fields: &["parameters"],
};
