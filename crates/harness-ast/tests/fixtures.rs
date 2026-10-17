//! Fixture pairs: `tests/fixtures/<lang>/before.<ext>` + `after.<ext>` must
//! produce exactly the `AstChange` lines in `expected.txt`.
#![cfg(feature = "full")]

use std::path::Path;

use harness_ast::parser_for_path;

fn check(lang: &str, ext: &str) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(lang);
    let read = |name: &str| {
        std::fs::read_to_string(dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
    };
    let before = read(&format!("before.{ext}"));
    let after = read(&format!("after.{ext}"));
    let parser = parser_for_path(&format!("x.{ext}")).expect("parser registered");
    let actual: Vec<String> = parser
        .diff(&before, &after)
        .expect("diff")
        .iter()
        .filter(|c| c.is_reportable())
        .map(ToString::to_string)
        .collect();
    let expected: Vec<String> = read("expected.txt")
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    assert_eq!(actual, expected, "\nactual:\n{}\n", actual.join("\n"));
}

#[test]
fn typescript_fixture() {
    check("typescript", "ts");
}

#[test]
fn python_fixture() {
    check("python", "py");
}

#[test]
fn javascript_fixture() {
    check("javascript", "js");
}
