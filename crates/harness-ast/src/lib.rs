//! # harness-ast
//!
//! Structural (AST-level) change tracking across parallel agent working
//! trees, built on Tree-sitter grammars.
//!
//! ## Status: v0.2 milestone — interfaces only
//!
//! The traits below define the contract that language parser plugins
//! implement. See docs/guides/adding-a-language.md for the contribution
//! guide; the goal is that a new language lands in < 100 lines.

/// A structural change detected in one file, expressed language-neutrally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AstChange {
    FunctionAdded(String),
    FunctionRemoved(String),
    FunctionChanged(String),
    TypeChanged(String),
    DependencyAdded(String),
    DependencyRemoved(String),
}

/// Implemented once per supported language (TypeScript, Python, Rust, Go, …).
pub trait LanguageParser: Send + Sync {
    /// Language identifier, e.g. "typescript".
    fn language(&self) -> &'static str;
    /// File extensions this parser claims, e.g. ["ts", "tsx"].
    fn extensions(&self) -> &'static [&'static str];
    /// Diff two versions of a file into structural changes.
    fn diff(&self, before: &str, after: &str) -> Result<Vec<AstChange>, Error>;
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("parse error: {0}")]
    Parse(String),
    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),
}
