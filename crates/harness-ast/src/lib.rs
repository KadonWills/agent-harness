//! # harness-ast
//!
//! Structural (AST-level) change tracking across agent working trees,
//! built on Tree-sitter grammars.
//!
//! A language is a [`walker::LanguageSpec`] — a mapping table from
//! Tree-sitter node kinds to functions, types, members, imports, and
//! identifiers. The shared [`walker`] turns any spec into a
//! [`LanguageParser`]. See docs/guides/adding-a-language.md.
//!
//! Languages are behind features (`lang-typescript`, `lang-javascript`,
//! `lang-python`; `full` enables all).

use std::fmt;

pub mod codemod;
pub mod conflict;
pub mod lang;
pub mod walker;

/// A structural change detected in one file, expressed language-neutrally.
///
/// Symbols nested in a type are qualified (`User.save`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum AstChange {
    FunctionAdded(String),
    FunctionRemoved(String),
    FunctionChanged(String),
    /// A function removed under one name and added under another with an
    /// identical body.
    FunctionRenamed {
        from: String,
        to: String,
    },
    /// Parameters or return type differ (reported alongside `FunctionChanged`).
    SignatureChanged(String),
    TypeAdded(String),
    TypeRemoved(String),
    TypeChanged(String),
    /// A field or method no longer declared on a type that still exists.
    MemberRemoved {
        owner: String,
        member: String,
    },
    DependencyAdded(String),
    DependencyRemoved(String),
    /// An identifier the file did not reference before. Conflict-engine
    /// input; not written to the manifest.
    ReferenceAdded(String),
}

impl AstChange {
    /// Whether this change belongs in `AgentManifest.json`.
    pub fn is_reportable(&self) -> bool {
        !matches!(self, AstChange::ReferenceAdded(_))
    }
}

/// The manifest format: `<Variant>:<symbol>`.
impl fmt::Display for AstChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AstChange::FunctionAdded(s) => write!(f, "FunctionAdded:{s}"),
            AstChange::FunctionRemoved(s) => write!(f, "FunctionRemoved:{s}"),
            AstChange::FunctionChanged(s) => write!(f, "FunctionChanged:{s}"),
            AstChange::FunctionRenamed { from, to } => write!(f, "FunctionRenamed:{from}->{to}"),
            AstChange::SignatureChanged(s) => write!(f, "SignatureChanged:{s}"),
            AstChange::TypeAdded(s) => write!(f, "TypeAdded:{s}"),
            AstChange::TypeRemoved(s) => write!(f, "TypeRemoved:{s}"),
            AstChange::TypeChanged(s) => write!(f, "TypeChanged:{s}"),
            AstChange::MemberRemoved { owner, member } => {
                write!(f, "MemberRemoved:{owner}.{member}")
            }
            AstChange::DependencyAdded(s) => write!(f, "DependencyAdded:{s}"),
            AstChange::DependencyRemoved(s) => write!(f, "DependencyRemoved:{s}"),
            AstChange::ReferenceAdded(s) => write!(f, "ReferenceAdded:{s}"),
        }
    }
}

/// Implemented once per supported language (TypeScript, Python, Rust, Go, …).
pub trait LanguageParser: Send + Sync {
    /// Language identifier, e.g. "typescript".
    fn language(&self) -> &'static str;
    /// File extensions this parser claims, e.g. ["ts", "tsx"].
    fn extensions(&self) -> &'static [&'static str];
    /// Diff two versions of a file into structural changes. An empty
    /// `before` means the file was added; an empty `after`, removed.
    fn diff(&self, before: &str, after: &str) -> Result<Vec<AstChange>, Error>;
}

/// Every parser compiled into this build.
pub fn parsers() -> Vec<&'static dyn LanguageParser> {
    lang::ALL
        .iter()
        .map(|p| p as &'static dyn LanguageParser)
        .collect()
}

/// The parser that claims `path`'s extension, if any.
pub fn parser_for_path(path: &str) -> Option<&'static dyn LanguageParser> {
    let ext = std::path::Path::new(path).extension()?.to_str()?;
    parsers()
        .into_iter()
        .find(|p| p.extensions().iter().any(|e| e.eq_ignore_ascii_case(ext)))
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("parse error: {0}")]
    Parse(String),
    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_the_manifest_format() {
        let rename = AstChange::FunctionRenamed {
            from: "a".into(),
            to: "b".into(),
        };
        assert_eq!(rename.to_string(), "FunctionRenamed:a->b");
        let member = AstChange::MemberRemoved {
            owner: "User".into(),
            member: "email".into(),
        };
        assert_eq!(member.to_string(), "MemberRemoved:User.email");
        assert!(!AstChange::ReferenceAdded("x".into()).is_reportable());
        assert!(AstChange::TypeAdded("T".into()).is_reportable());
    }

    #[cfg(feature = "full")]
    #[test]
    fn parsers_are_found_by_extension() {
        let lang = |p: &str| parser_for_path(p).map(|p| p.language());
        assert_eq!(lang("src/a.ts"), Some("typescript"));
        assert_eq!(lang("src/a.TSX"), Some("tsx"));
        assert_eq!(lang("a.mjs"), Some("javascript"));
        assert_eq!(lang("pkg/mod.py"), Some("python"));
        assert_eq!(lang("README.md"), None);
        assert_eq!(lang("Makefile"), None);
    }
}
