//! Language mapping tables. Each language is one `LanguageSpec` constant.

use crate::walker::TreeSitterParser;

#[cfg(feature = "lang-javascript")]
pub mod javascript;
#[cfg(feature = "lang-python")]
pub mod python;
#[cfg(feature = "lang-typescript")]
pub mod typescript;

/// Every parser enabled by this build's features.
pub static ALL: &[TreeSitterParser] = &[
    #[cfg(feature = "lang-typescript")]
    TreeSitterParser::new(&typescript::TYPESCRIPT),
    #[cfg(feature = "lang-typescript")]
    TreeSitterParser::new(&typescript::TSX),
    #[cfg(feature = "lang-javascript")]
    TreeSitterParser::new(&javascript::JAVASCRIPT),
    #[cfg(feature = "lang-python")]
    TreeSitterParser::new(&python::PYTHON),
];
