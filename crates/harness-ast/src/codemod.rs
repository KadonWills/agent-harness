//! Level 1 auto-fixes: deterministic, narrow rewrites (RFC-0001 §6).

use crate::walker::TreeSitterParser;
use crate::{lang, Error};

fn parser_for(path: &str) -> Option<&'static TreeSitterParser> {
    let ext = std::path::Path::new(path).extension()?.to_str()?;
    lang::ALL.iter().find(|p| {
        p.spec()
            .extensions
            .iter()
            .any(|e| e.eq_ignore_ascii_case(ext))
    })
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// Rewrite plain identifier references `from` → `to` in `source`.
///
/// Returns `Ok(None)` when nothing was changed, including when the rewrite
/// is not provably safe: if the file itself declares `from` (a local
/// definition could be shadowing the renamed symbol), it is left alone.
pub fn rename_identifiers(
    path: &str,
    source: &str,
    from: &str,
    to: &str,
) -> Result<Option<String>, Error> {
    if !is_identifier(from) || !is_identifier(to) {
        return Err(Error::Parse(format!(
            "refusing to rename `{from}` -> `{to}`: both must be identifiers"
        )));
    }
    let parser = parser_for(path).ok_or_else(|| Error::UnsupportedLanguage(path.to_string()))?;
    if parser.symbols(source)?.declared.contains(from) {
        return Ok(None);
    }
    let ranges = parser.identifier_ranges(source, from)?;
    if ranges.is_empty() {
        return Ok(None);
    }
    let mut out = source.to_string();
    for range in ranges.into_iter().rev() {
        out.replace_range(range, to);
    }
    Ok(Some(out))
}

#[cfg(all(test, feature = "full"))]
mod tests {
    use super::*;

    #[test]
    fn renames_references_but_not_properties_or_strings() {
        let src = "import { fmt } from \"./lib\";\nconst a = fmt(user);\nconst b = obj.fmt;\nconst c = \"fmt\";\n";
        let out = rename_identifiers("app.ts", src, "fmt", "formatName")
            .expect("ok")
            .expect("changed");
        assert_eq!(
            out,
            "import { formatName } from \"./lib\";\nconst a = formatName(user);\nconst b = obj.fmt;\nconst c = \"fmt\";\n"
        );
    }

    #[test]
    fn python_calls_are_renamed() {
        let out = rename_identifiers(
            "a.py",
            "from lib import fmt\nprint(fmt(1))\n",
            "fmt",
            "format_name",
        )
        .expect("ok")
        .expect("changed");
        assert_eq!(out, "from lib import format_name\nprint(format_name(1))\n");
    }

    #[test]
    fn local_declarations_make_the_rewrite_unsafe() {
        let src = "def fmt(x):\n    return x\n\nfmt(1)\n";
        assert_eq!(
            rename_identifiers("a.py", src, "fmt", "format_name").expect("ok"),
            None
        );
    }

    #[test]
    fn nothing_to_do_and_bad_input() {
        assert_eq!(
            rename_identifiers("a.py", "x = 1\n", "fmt", "y").expect("ok"),
            None
        );
        assert!(rename_identifiers("a.py", "", "a b", "c").is_err());
        assert!(matches!(
            rename_identifiers("a.rb", "", "a", "b"),
            Err(Error::UnsupportedLanguage(_))
        ));
    }
}
