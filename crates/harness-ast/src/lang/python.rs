//! Python.

use crate::walker::{LanguageSpec, ReceiverRule, Rule};

pub static PYTHON: LanguageSpec = LanguageSpec {
    name: "python",
    extensions: &["py", "pyi"],
    grammar: || tree_sitter_python::LANGUAGE.into(),
    functions: &[Rule::new("function_definition", "name")],
    types: &[Rule::new("class_definition", "name")],
    members: &[
        Rule::new("function_definition", "name"),
        Rule::new("assignment", "left").with_name_kinds(&["identifier"]),
    ],
    receiver: Some(ReceiverRule {
        assignment: "assignment",
        left_field: "left",
        access: "attribute",
        object_field: "object",
        property_field: "attribute",
        receiver: "self",
    }),
    imports: &[
        ("import_statement", "name"),
        ("import_from_statement", "module_name"),
    ],
    identifiers: &["identifier"],
    signature_fields: &["parameters", "return_type"],
};
